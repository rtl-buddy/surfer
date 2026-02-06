//! Analog signal rendering: command generation and waveform drawing.

use crate::analog_signal_cache::{AnalogSignalCache, CacheQueryResult, is_nan_highimp};
use crate::displayed_item::{
    AnalogSettings, DisplayedFieldRef, DisplayedItemRef, DisplayedVariable,
};
use crate::drawing_canvas::{AnalogDrawingCommands, DrawingCommands, VariableDrawCommands};
use crate::message::Message;
use crate::translation::TranslatorList;
use crate::view::DrawingContext;
use crate::viewport::Viewport;
use crate::wave_data::WaveData;
use egui::{Color32, Pos2, Stroke, emath};
use epaint::PathShape;
use num::{BigInt, ToPrimitive};
use std::collections::HashMap;

pub enum AnalogDrawingCommand {
    /// Constant value from `start_px` to `end_px`.
    /// In Step mode: horizontal line at `start_val`, vertical transition to next.
    /// In Interpolated mode: line from (`start_px`, `start_val`) to (`end_px`, `end_val`).
    Flat {
        start_px: f32,
        start_val: f64,
        end_px: f32,
        end_val: f64,
    },
    /// Multiple transitions in one pixel (anti-aliased vertical bar).
    /// Rendered identically in both Step and Interpolated modes.
    Range { px: f32, min_val: f64, max_val: f64 },
}

/// Generate draw commands for a displayed analog variable.
/// Returns `None` if unrenderable, or a cache-build command if cache not ready.
pub(crate) fn variable_analog_draw_commands(
    displayed_variable: &DisplayedVariable,
    display_id: DisplayedItemRef,
    waves: &WaveData,
    translators: &TranslatorList,
    view_width: f32,
    viewport_idx: usize,
) -> Option<VariableDrawCommands> {
    let render_mode = displayed_variable.analog.as_ref()?;

    let wave_container = waves.inner.as_waves()?;
    let displayed_field_ref: DisplayedFieldRef = display_id.into();
    let translator = waves.variable_translator(&displayed_field_ref, translators);
    let viewport = &waves.viewports[viewport_idx];
    let num_timestamps = waves.safe_num_timestamps();

    let signal_id = wave_container
        .signal_id(&displayed_variable.variable_ref)
        .ok()?;
    let translator_name = translator.name();
    let cache_key = (signal_id, translator_name.clone());

    // Check if cache exists and is valid (correct generation and matching key)
    let cache = match &render_mode.cache {
        Some(entry)
            if entry.generation == waves.cache_generation && entry.cache_key == cache_key =>
        {
            if let Some(cache) = entry.get() {
                cache
            } else {
                // Cache is building, return loading state
                let mut local_commands = HashMap::new();
                local_commands.insert(
                    vec![],
                    DrawingCommands::Analog(AnalogDrawingCommands::Loading),
                );
                return Some(VariableDrawCommands {
                    clock_edges: vec![],
                    display_id,
                    local_commands,
                    local_msgs: vec![],
                });
            }
        }
        _ => {
            // Cache missing or stale - request build and show loading
            let mut local_commands = HashMap::new();
            local_commands.insert(
                vec![],
                DrawingCommands::Analog(AnalogDrawingCommands::Loading),
            );
            return Some(VariableDrawCommands {
                clock_edges: vec![],
                display_id,
                local_commands,
                local_msgs: vec![Message::BuildAnalogCache {
                    display_id,
                    cache_key,
                }],
            });
        }
    };

    let analog_commands = CommandBuilder::new(
        cache,
        viewport,
        &num_timestamps,
        view_width,
        render_mode.settings,
    )
    .build();

    let mut local_commands = HashMap::new();
    local_commands.insert(vec![], DrawingCommands::Analog(analog_commands));

    Some(VariableDrawCommands {
        clock_edges: vec![],
        display_id,
        local_commands,
        local_msgs: vec![],
    })
}

/// Render analog waveform from pre-computed commands.
pub fn draw_analog(
    analog_commands: &AnalogDrawingCommands,
    color: Color32,
    offset: f32,
    height_scaling_factor: f32,
    frame_width: f32,
    ctx: &mut DrawingContext,
) {
    let AnalogDrawingCommands::Ready {
        viewport_min,
        viewport_max,
        global_min,
        global_max,
        values,
        min_valid_pixel,
        max_valid_pixel,
        analog_settings,
    } = analog_commands
    else {
        draw_building_indicator(offset, height_scaling_factor, frame_width, ctx);
        return;
    };

    let (min_val, max_val) = select_value_range(
        *viewport_min,
        *viewport_max,
        *global_min,
        *global_max,
        analog_settings,
    );

    let render_ctx = RenderContext::new(
        color,
        min_val,
        max_val,
        *min_valid_pixel,
        *max_valid_pixel,
        offset,
        height_scaling_factor,
        ctx,
    );

    // Use the appropriate strategy based on settings
    match analog_settings.render_style {
        crate::displayed_item::AnalogRenderStyle::Step => {
            let mut strategy = StepStrategy::default();
            render_with_strategy(values, &render_ctx, &mut strategy, ctx);
        }
        crate::displayed_item::AnalogRenderStyle::Interpolated => {
            let mut strategy = InterpolatedStrategy::default();
            render_with_strategy(values, &render_ctx, &mut strategy, ctx);
        }
    }

    draw_amplitude_labels(&render_ctx, frame_width, ctx);
}

/// Draw a building indicator with animated dots while analog cache is being built.
fn draw_building_indicator(
    offset: f32,
    height_scaling_factor: f32,
    frame_width: f32,
    ctx: &mut DrawingContext,
) {
    // Animate dots: cycle through ".", "..", "..." every 333ms
    let elapsed = ctx.painter.ctx().input(|i| i.time);
    let dot_index = (elapsed / 0.333) as usize % 3;
    let text = ["Building.  ", "Building.. ", "Building..."][dot_index];

    let text_size = ctx.cfg.text_size;
    let row_height = ctx.cfg.line_height * height_scaling_factor;
    let center_y = offset + row_height / 2.0;
    let center_x = frame_width / 2.0;
    let pos = (ctx.to_screen)(center_x, center_y);

    ctx.painter.text(
        pos,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(text_size),
        ctx.theme.foreground.gamma_multiply(0.6),
    );
}

fn select_value_range(
    viewport_min: f64,
    viewport_max: f64,
    global_min: f64,
    global_max: f64,
    settings: &AnalogSettings,
) -> (f64, f64) {
    let (min, max) = match settings.y_axis_scale {
        crate::displayed_item::AnalogYAxisScale::Viewport => (viewport_min, viewport_max),
        crate::displayed_item::AnalogYAxisScale::Global => (global_min, global_max),
    };

    // Handle all-NaN case: min=INFINITY, max=NEG_INFINITY
    if !min.is_finite() || !max.is_finite() || min > max {
        return (-0.5, 0.5);
    }

    // Avoid division by zero
    if (min - max).abs() < f64::EPSILON {
        (min - 0.5, max + 0.5)
    } else {
        (min, max)
    }
}

/// Builds drawing commands by iterating viewport pixels.
struct CommandBuilder<'a> {
    cache: &'a AnalogSignalCache,
    viewport: &'a Viewport,
    num_timestamps: &'a BigInt,
    view_width: f32,
    min_valid_pixel: f32,
    max_valid_pixel: f32,
    output: CommandOutput,
    analog_settings: AnalogSettings,
}

/// Accumulates commands and tracks value bounds.
struct CommandOutput {
    commands: Vec<AnalogDrawingCommand>,
    pending_flat: Option<(f32, f64)>,
    viewport_min: f64,
    viewport_max: f64,
}

impl CommandOutput {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            pending_flat: None,
            viewport_min: f64::INFINITY,
            viewport_max: f64::NEG_INFINITY,
        }
    }

    fn update_bounds(&mut self, value: f64) {
        if value.is_finite() {
            self.viewport_min = self.viewport_min.min(value);
            self.viewport_max = self.viewport_max.max(value);
        }
    }

    fn emit_flat(&mut self, px: f32, value: f64) {
        match self.pending_flat {
            // Bit compare to distinguish different NaN payloads ( Undef / HighZ )
            Some((_, v)) if v.to_bits() == value.to_bits() => {
                // Same value, extend the flat region (no-op, end_px updated on flush)
            }
            Some((start, start_val)) => {
                // Value changed: flush previous flat
                let end_val = if start_val.is_finite() && value.is_finite() {
                    value
                } else {
                    start_val
                };
                self.commands.push(AnalogDrawingCommand::Flat {
                    start_px: start,
                    start_val,
                    end_px: px,
                    end_val,
                });
                self.pending_flat = Some((px, value));
            }
            None => self.pending_flat = Some((px, value)),
        }
    }

    fn emit_range(&mut self, px: f32, min: f64, max: f64, entry_val: f64, exit_val: f64) {
        // Flush pending flat - end_val is the first transition value (entry to range)
        // for correct interpolation in Interpolated mode
        if let Some((start, start_val)) = self.pending_flat.take() {
            let end_val = if start_val.is_finite() && entry_val.is_finite() {
                entry_val
            } else {
                start_val
            };
            self.commands.push(AnalogDrawingCommand::Flat {
                start_px: start,
                start_val,
                end_px: px,
                end_val,
            });
        }
        self.commands.push(AnalogDrawingCommand::Range {
            px,
            min_val: min,
            max_val: max,
        });
        // Start new flat from exit_val
        self.pending_flat = Some((px + 1.0, exit_val));
    }
}

impl<'a> CommandBuilder<'a> {
    fn new(
        cache: &'a AnalogSignalCache,
        viewport: &'a Viewport,
        num_timestamps: &'a BigInt,
        view_width: f32,
        analog_settings: AnalogSettings,
    ) -> Self {
        let min_valid_pixel =
            viewport.pixel_from_time(&BigInt::from(0), view_width, num_timestamps);
        let max_valid_pixel = viewport.pixel_from_time(num_timestamps, view_width, num_timestamps);

        Self {
            cache,
            viewport,
            num_timestamps,
            view_width,
            min_valid_pixel,
            max_valid_pixel,
            output: CommandOutput::new(),
            analog_settings,
        }
    }

    fn build(mut self) -> AnalogDrawingCommands {
        let end_px = self.view_width.floor().max(0.0) + 1.0;

        let before_px = self.add_before_viewport_sample();
        self.iterate_pixels(0.0, end_px);
        self.add_after_viewport_sample(end_px);

        self.finalize(before_px)
    }

    fn time_at_pixel(&self, px: f64) -> u64 {
        self.viewport
            .as_absolute_time(px, self.view_width, self.num_timestamps)
            .0
            .to_u64()
            .unwrap_or(0)
    }

    fn pixel_at_time(&self, time: u64) -> f32 {
        self.viewport
            .pixel_from_time(&BigInt::from(time), self.view_width, self.num_timestamps)
    }

    fn query(&self, time: u64) -> CacheQueryResult {
        self.cache.query_at_time(time)
    }

    /// Captures the most recent sample occurring before the visible viewport.
    /// This method ensures rendering continuity when a signal value extends from before
    /// the viewport into the visible area.
    fn add_before_viewport_sample(&mut self) -> Option<f32> {
        let query = self.query(self.time_at_pixel(0.0));

        if let Some((time, value)) = query.current {
            let px = self.pixel_at_time(time);
            if px < 0.0 {
                self.output.update_bounds(value);
                self.output.pending_flat = Some((px, value));
                return Some(px);
            }
        }
        None
    }

    fn iterate_pixels(&mut self, start_px: f32, end_px: f32) {
        let mut px = start_px as u32;
        let end = end_px as u32;
        let mut next_query_time: Option<u64> = None;
        let mut last_queried_time: Option<u64> = None;

        while px < end {
            // Track if we jumped to this pixel for a specific transition
            let jumped_to_transition = next_query_time.is_some();
            let t0 = next_query_time.unwrap_or_else(|| self.time_at_pixel(f64::from(px)));
            let t1 = self.time_at_pixel(f64::from(px) + 1.0);
            next_query_time = None;

            // Skip if we already queried this exact time (optimization for zoomed-out views
            // where multiple pixels map to the same integer time). Don't skip if we jumped
            // here for a specific transition.
            if !jumped_to_transition && last_queried_time == Some(t0) {
                px += 1;
                continue;
            }

            let query = self.query(t0);
            last_queried_time = Some(t0);
            let next_change = query.next;
            let is_flat = next_change.is_none_or(|nc| nc >= t1);

            if is_flat {
                px = self.process_flat(px, end, &query, next_change, &mut next_query_time);
            } else {
                self.process_range(px, t0, t1);
                px += 1;
            }
        }
    }

    fn process_flat(
        &mut self,
        px: u32,
        end: u32,
        query: &CacheQueryResult,
        next_change: Option<u64>,
        next_query_time: &mut Option<u64>,
    ) -> u32 {
        if let Some((_, value)) = query.current {
            self.output.update_bounds(value);
            self.output.emit_flat(px as f32, value);
        }

        // Skip ahead to next transition
        if let Some(next) = next_change {
            let next_px = self.pixel_at_time(next);
            if next_px.is_finite() {
                let jump = next_px.floor().max(0.0) as u32;
                if jump > px {
                    *next_query_time = Some(next);
                    return jump.min(end);
                }
            }
            (px + 1).min(end)
        } else {
            end
        }
    }

    fn process_range(&mut self, px: u32, t0: u64, t1: u64) {
        if let Some((min, max)) = self.cache.query_time_range(t0, t1.saturating_sub(1)) {
            self.output.update_bounds(min);
            self.output.update_bounds(max);

            // Query the value at the first transition within the pixel (entry value)
            // This is used as end_val for the preceding Flat in interpolated mode
            let t0_query = self.query(t0);
            let entry_val = match t0_query.current {
                // If t0 is exactly on a transition (jumped here via next_query_time),
                // the current value is already the first transition value
                Some((time, value)) if time == t0 => value,
                // Otherwise t0 is at pixel start, so first transition is at t0_query.next
                _ => {
                    if let Some(first_change) = t0_query.next {
                        self.query(first_change).current.map_or(min, |(_, v)| v)
                    } else {
                        min
                    }
                }
            };

            // Query the value at the end of the range (exit value)
            let exit_query = self.query(t1.saturating_sub(1));
            let exit_val = exit_query.current.map_or(max, |(_, v)| v);

            self.output
                .emit_range(px as f32, min, max, entry_val, exit_val);
        }
    }

    /// Extends rendering to include the first sample occurring after the visible viewport.
    fn add_after_viewport_sample(&mut self, end_px: f32) {
        let query = self.query(self.time_at_pixel(f64::from(end_px)));

        let Some(next_time) = query.next else {
            return;
        };

        let after_px = self.pixel_at_time(next_time);
        if after_px <= end_px {
            return;
        }

        let after_query = self.query(next_time);

        if let Some((_, value)) = after_query.current {
            self.output.update_bounds(value);

            if let Some((start, start_val)) = self.output.pending_flat.take() {
                self.output.commands.push(AnalogDrawingCommand::Flat {
                    start_px: start,
                    start_val,
                    end_px: after_px,
                    end_val: value,
                });
            }
        }
    }

    fn finalize(mut self, before_px: Option<f32>) -> AnalogDrawingCommands {
        // Flush remaining pending flat with same end_val (constant to end)
        if let Some((start, start_val)) = self.output.pending_flat.take() {
            self.output.commands.push(AnalogDrawingCommand::Flat {
                start_px: start,
                start_val,
                end_px: self.max_valid_pixel,
                end_val: start_val, // Signal stays constant
            });
        }

        // Extend first command to include before-viewport sample
        if let Some(before) = before_px
            && let Some(AnalogDrawingCommand::Flat { start_px, .. }) =
                self.output.commands.first_mut()
        {
            *start_px = (*start_px).min(before);
        }

        AnalogDrawingCommands::Ready {
            viewport_min: self.output.viewport_min,
            viewport_max: self.output.viewport_max,
            global_min: self.cache.global_min,
            global_max: self.cache.global_max,
            values: self.output.commands,
            min_valid_pixel: self.min_valid_pixel,
            max_valid_pixel: self.max_valid_pixel,
            analog_settings: self.analog_settings,
        }
    }
}

/// Rendering strategy for analog waveforms.
pub trait RenderStrategy {
    /// Reset state after encountering undefined values.
    fn reset_state(&mut self);

    /// Get the last rendered point (for Range connection).
    fn last_point(&self) -> Option<Pos2>;

    /// Set the last rendered point (after Range draws).
    fn set_last_point(&mut self, point: Pos2);

    /// Render a flat segment.
    /// Step: horizontal line at `start_val`, connect to next.
    /// Interpolated: line from (`start_px`, `start_val`) to (`end_px`, `end_val`).
    fn render_flat(
        &mut self,
        ctx: &mut DrawingContext,
        render_ctx: &RenderContext,
        start_px: f32,
        start_val: f64,
        end_px: f32,
        end_val: f64,
    );

    /// Render a range segment (default impl, same for both strategies).
    /// Draws vertical bar at px from `min_val` to `max_val`.
    fn render_range(
        &mut self,
        ctx: &mut DrawingContext,
        render_ctx: &RenderContext,
        px: f32,
        min_val: f64,
        max_val: f64,
    ) {
        if !min_val.is_finite() || !max_val.is_finite() {
            let nan = if min_val.is_finite() {
                max_val
            } else {
                min_val
            };
            render_ctx.draw_undefined(px, px + 1.0, nan, ctx);
            self.reset_state();
            return;
        }

        let p_min = render_ctx.to_screen(px, min_val, ctx);
        let p_max = render_ctx.to_screen(px, max_val, ctx);

        // Connect from previous to closer endpoint
        let (connect, other) = match self.last_point() {
            Some(prev) if (prev.y - p_min.y).abs() < (prev.y - p_max.y).abs() => (p_min, p_max),
            _ => (p_max, p_min),
        };

        if let Some(prev) = self.last_point() {
            render_ctx.draw_line(prev, connect, ctx);
        }

        // Vertical bar
        render_ctx.draw_line(connect, other, ctx);
        self.set_last_point(other);
    }
}

/// Coordinate transformation state shared between rendering strategies.
/// Invariant: `min_val` and `max_val` are always finite.
pub struct RenderContext {
    pub stroke: Stroke,
    pub min_val: f64,
    pub max_val: f64,
    /// Pixel position of timestamp 0 (start of signal data).
    pub min_valid_pixel: f32,
    /// Pixel position of last timestamp (end of signal data).
    pub max_valid_pixel: f32,
    pub offset: f32,
    pub height_scale: f32,
    pub line_height: f32,
}

impl RenderContext {
    #[allow(clippy::too_many_arguments)]
    fn new(
        color: Color32,
        min_val: f64,
        max_val: f64,
        min_valid_pixel: f32,
        max_valid_pixel: f32,
        offset: f32,
        height_scale: f32,
        ctx: &DrawingContext,
    ) -> Self {
        Self {
            stroke: Stroke::new(ctx.theme.linewidth, color),
            min_val,
            max_val,
            min_valid_pixel,
            max_valid_pixel,
            offset,
            height_scale,
            line_height: ctx.cfg.line_height,
        }
    }

    /// Normalize value to [0, 1].
    /// Invariant: `min_val` and `max_val` are always finite (guaranteed by `AnalogSignalCache`).
    #[must_use]
    pub fn normalize(&self, value: f64) -> f32 {
        debug_assert!(
            self.min_val.is_finite() && self.max_val.is_finite(),
            "RenderContext min_val and max_val must be finite"
        );
        let range = self.max_val - self.min_val;
        if range.abs() <= f64::EPSILON {
            0.5
        } else {
            ((value - self.min_val) / range) as f32
        }
    }

    /// Convert value to screen position.
    #[must_use]
    pub fn to_screen(&self, x: f32, y: f64, ctx: &DrawingContext) -> Pos2 {
        let y_norm = self.normalize(y);
        (ctx.to_screen)(
            x,
            (1.0 - y_norm) * self.line_height * self.height_scale + self.offset,
        )
    }

    /// Clamp x to valid pixel range (within VCD file bounds).
    #[must_use]
    pub fn clamp_x(&self, x: f32) -> f32 {
        x.clamp(self.min_valid_pixel, self.max_valid_pixel)
    }

    pub fn draw_line(&self, from: Pos2, to: Pos2, ctx: &mut DrawingContext) {
        ctx.painter
            .add(PathShape::line(vec![from, to], self.stroke));
    }

    pub fn draw_undefined(&self, start_x: f32, end_x: f32, value: f64, ctx: &mut DrawingContext) {
        let color = if value == f64::INFINITY {
            ctx.theme.accent_error.background
        } else if value == f64::NEG_INFINITY {
            ctx.theme.variable_dontcare
        } else if is_nan_highimp(value) {
            ctx.theme.variable_highimp
        } else {
            ctx.theme.variable_undef
        };
        let min = (ctx.to_screen)(start_x, self.offset);
        let max = (ctx.to_screen)(end_x, self.offset + self.line_height * self.height_scale);
        ctx.painter
            .rect_filled(egui::Rect::from_min_max(min, max), 0.0, color);
    }
}

/// Step-style rendering: horizontal segments with vertical transitions.
#[derive(Default)]
pub struct StepStrategy {
    last_point: Option<Pos2>,
}

impl RenderStrategy for StepStrategy {
    fn reset_state(&mut self) {
        self.last_point = None;
    }

    fn last_point(&self) -> Option<Pos2> {
        self.last_point
    }

    fn set_last_point(&mut self, point: Pos2) {
        self.last_point = Some(point);
    }

    fn render_flat(
        &mut self,
        ctx: &mut DrawingContext,
        render_ctx: &RenderContext,
        start_px: f32,
        start_val: f64,
        end_px: f32,
        _end_val: f64, // Ignored in Step mode
    ) {
        let start_px = render_ctx.clamp_x(start_px);
        let end_px = render_ctx.clamp_x(end_px);

        if !start_val.is_finite() {
            render_ctx.draw_undefined(start_px, end_px, start_val, ctx);
            self.reset_state();
            return;
        }

        let p1 = render_ctx.to_screen(start_px, start_val, ctx);
        let p2 = render_ctx.to_screen(end_px, start_val, ctx);

        // Vertical transition from previous
        if let Some(prev) = self.last_point {
            render_ctx.draw_line(Pos2::new(p1.x, prev.y), p1, ctx);
        }

        // Horizontal line
        render_ctx.draw_line(p1, p2, ctx);
        self.last_point = Some(p2);
    }
}

/// Interpolated rendering: diagonal lines connecting consecutive values.
#[derive(Default)]
pub struct InterpolatedStrategy {
    last_point: Option<Pos2>,
    started: bool,
}

impl RenderStrategy for InterpolatedStrategy {
    fn reset_state(&mut self) {
        self.last_point = None;
        self.started = true;
    }

    fn last_point(&self) -> Option<Pos2> {
        self.last_point
    }

    fn set_last_point(&mut self, point: Pos2) {
        self.last_point = Some(point);
        self.started = true;
    }

    fn render_flat(
        &mut self,
        ctx: &mut DrawingContext,
        render_ctx: &RenderContext,
        start_px: f32,
        start_val: f64,
        end_px: f32,
        end_val: f64,
    ) {
        let start_px = render_ctx.clamp_x(start_px);
        let end_px = render_ctx.clamp_x(end_px);

        if !start_val.is_finite() {
            render_ctx.draw_undefined(start_px, end_px, start_val, ctx);
            self.reset_state();
            return;
        }

        // If end_val is NaN but start_val is finite, render as flat line using start_val
        let end_val = if end_val.is_finite() {
            end_val
        } else {
            start_val
        };

        let p1 = render_ctx.to_screen(start_px, start_val, ctx);
        let p2 = render_ctx.to_screen(end_px, end_val, ctx);

        // Connect from previous point
        if let Some(prev) = self.last_point {
            render_ctx.draw_line(prev, p1, ctx);
        } else if !self.started {
            // Connect from viewport edge
            let edge = render_ctx.to_screen(render_ctx.min_valid_pixel.max(0.0), start_val, ctx);
            render_ctx.draw_line(edge, p1, ctx);
        }

        render_ctx.draw_line(p1, p2, ctx);
        self.last_point = Some(p2);
        self.started = true;
    }
}

/// Render commands using the given strategy.
fn render_with_strategy<S: RenderStrategy>(
    commands: &[AnalogDrawingCommand],
    render_ctx: &RenderContext,
    strategy: &mut S,
    ctx: &mut DrawingContext,
) {
    for cmd in commands {
        match cmd {
            AnalogDrawingCommand::Flat {
                start_px,
                start_val,
                end_px,
                end_val,
            } => {
                strategy.render_flat(ctx, render_ctx, *start_px, *start_val, *end_px, *end_val);
            }
            AnalogDrawingCommand::Range {
                px,
                min_val,
                max_val,
            } => {
                strategy.render_range(ctx, render_ctx, *px, *min_val, *max_val);
            }
        }
    }
}

/// Format amplitude value for display, using scientific notation for extreme values.
fn format_amplitude_value(value: f64) -> String {
    const SCIENTIFIC_THRESHOLD_HIGH: f64 = 1e4;
    const SCIENTIFIC_THRESHOLD_LOW: f64 = 1e-3;
    let abs_val = value.abs();
    if abs_val == 0.0 {
        "0.00".to_string()
    } else if !(SCIENTIFIC_THRESHOLD_LOW..SCIENTIFIC_THRESHOLD_HIGH).contains(&abs_val) {
        format!("{value:.2e}")
    } else {
        format!("{value:.2}")
    }
}

fn draw_amplitude_labels(render_ctx: &RenderContext, frame_width: f32, ctx: &mut DrawingContext) {
    const SPLIT_LABEL_HEIGHT_THRESHOLD: f32 = 2.0;
    const LABEL_ALPHA: f32 = 0.7;
    const BACKGROUND_ALPHA: u8 = 200;

    let text_size = ctx.cfg.text_size;

    let text_color = render_ctx.stroke.color.gamma_multiply(LABEL_ALPHA);
    let bg_color = Color32::from_rgba_unmultiplied(0, 0, 0, BACKGROUND_ALPHA);
    let font = egui::FontId::monospace(text_size);

    if render_ctx.height_scale < SPLIT_LABEL_HEIGHT_THRESHOLD {
        let combined_text = format!(
            "[{}, {}]",
            format_amplitude_value(render_ctx.min_val),
            format_amplitude_value(render_ctx.max_val)
        );
        let galley = ctx
            .painter
            .layout_no_wrap(combined_text.clone(), font.clone(), text_color);

        let label_x = frame_width - galley.size().x - 5.0;
        let label_pos = render_ctx.to_screen(
            label_x,
            f64::midpoint(render_ctx.min_val, render_ctx.max_val),
            ctx,
        );

        let rect = egui::Rect::from_min_size(
            Pos2::new(label_pos.x - 2.0, label_pos.y - galley.size().y / 2.0 - 2.0),
            egui::Vec2::new(galley.size().x + 4.0, galley.size().y + 4.0),
        );
        ctx.painter.rect_filled(rect, 2.0, bg_color);
        ctx.painter.text(
            Pos2::new(label_pos.x, label_pos.y - galley.size().y / 2.0),
            emath::Align2::LEFT_TOP,
            combined_text,
            font,
            text_color,
        );
    } else {
        let max_text = format_amplitude_value(render_ctx.max_val);
        let min_text = format_amplitude_value(render_ctx.min_val);

        let max_galley = ctx
            .painter
            .layout_no_wrap(max_text.clone(), font.clone(), text_color);
        let min_galley = ctx
            .painter
            .layout_no_wrap(min_text.clone(), font.clone(), text_color);

        let label_x = frame_width - max_galley.size().x.max(min_galley.size().x) - 5.0;

        let max_pos = render_ctx.to_screen(label_x, render_ctx.max_val, ctx);
        let max_rect = egui::Rect::from_min_size(
            Pos2::new(max_pos.x - 2.0, max_pos.y - 2.0),
            egui::Vec2::new(max_galley.size().x + 4.0, max_galley.size().y + 4.0),
        );
        ctx.painter.rect_filled(max_rect, 2.0, bg_color);
        ctx.painter.text(
            max_pos,
            emath::Align2::LEFT_TOP,
            max_text,
            font.clone(),
            text_color,
        );

        let min_pos = render_ctx.to_screen(label_x, render_ctx.min_val, ctx);
        let min_rect = egui::Rect::from_min_size(
            Pos2::new(min_pos.x - 2.0, min_pos.y - min_galley.size().y - 2.0),
            egui::Vec2::new(min_galley.size().x + 4.0, min_galley.size().y + 4.0),
        );
        ctx.painter.rect_filled(min_rect, 2.0, bg_color);
        ctx.painter.text(
            min_pos,
            emath::Align2::LEFT_BOTTOM,
            min_text,
            font,
            text_color,
        );
    }
}
