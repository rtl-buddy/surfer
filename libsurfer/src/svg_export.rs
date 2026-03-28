use std::fmt::Write as _;
use std::path::Path;

use ecolor::Color32;
use eyre::{Result, eyre};
use itertools::Itertools;

use crate::clock_highlighting::ClockHighlightType;
use crate::config::SurferTheme;
use crate::displayed_item::DisplayedItem;
use crate::drawing_canvas::{AnalogDrawingCommands, DigitalDrawingType, DrawingCommands};
use crate::graphics::{Anchor, Direction, Graphic};
use crate::translation::ValueKindExt;
use crate::view::{DrawConfig, ItemDrawingInfo};
use crate::{CachedDrawData, Message, SystemState};

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn color_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn color_opacity(c: Color32) -> f32 {
    f32::from(c.a()) / 255.0
}

fn push_line(svg: &mut String, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Color32, width: f32) {
    let _ = writeln!(
        svg,
        "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{width:.2}\" />",
        color_hex(stroke),
        color_opacity(stroke)
    );
}

fn push_rect(svg: &mut String, x: f32, y: f32, w: f32, h: f32, fill: Color32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let _ = writeln!(
        svg,
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"{}\" fill-opacity=\"{:.3}\" />",
        color_hex(fill),
        color_opacity(fill)
    );
}

fn push_text(
    svg: &mut String,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: Color32,
    anchor: &str,
) {
    let _ = writeln!(
        svg,
        "<text x=\"{x:.2}\" y=\"{y:.2}\" fill=\"{}\" fill-opacity=\"{:.3}\" font-size=\"{size:.2}\" font-family=\"monospace\" text-anchor=\"{anchor}\" dominant-baseline=\"middle\">{}</text>",
        color_hex(color),
        color_opacity(color),
        escape_xml(text)
    );
}

fn push_text_top(svg: &mut String, x: f32, y: f32, text: &str, size: f32, color: Color32) {
    let _ = writeln!(
        svg,
        "<text x=\"{x:.2}\" y=\"{y:.2}\" fill=\"{}\" fill-opacity=\"{:.3}\" font-size=\"{size:.2}\" font-family=\"monospace\" text-anchor=\"middle\" dominant-baseline=\"hanging\">{}</text>",
        color_hex(color),
        color_opacity(color),
        escape_xml(text)
    );
}

fn push_polyline(svg: &mut String, points: &[(f32, f32)], stroke: Color32, width: f32, fill: &str) {
    if points.is_empty() {
        return;
    }
    let mut pts = String::new();
    for (idx, (x, y)) in points.iter().enumerate() {
        if idx > 0 {
            pts.push(' ');
        }
        let _ = write!(pts, "{x:.2},{y:.2}");
    }
    let _ = writeln!(
        svg,
        "<polyline points=\"{pts}\" fill=\"{fill}\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{width:.2}\" />",
        color_hex(stroke),
        color_opacity(stroke)
    );
}

fn push_polygon(
    svg: &mut String,
    points: &[(f32, f32)],
    stroke: Color32,
    width: f32,
    fill: Color32,
) {
    if points.is_empty() {
        return;
    }
    let mut pts = String::new();
    for (idx, (x, y)) in points.iter().enumerate() {
        if idx > 0 {
            pts.push(' ');
        }
        let _ = write!(pts, "{x:.2},{y:.2}");
    }
    let _ = writeln!(
        svg,
        "<polygon points=\"{pts}\" fill=\"{}\" fill-opacity=\"{:.3}\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{width:.2}\" />",
        color_hex(fill),
        color_opacity(fill),
        color_hex(stroke),
        color_opacity(stroke)
    );
}

fn bool_height(value: &str) -> f32 {
    if value.trim() == "0" { 0.0 } else { 1.0 }
}

fn map_analog_to_y(v: f64, min: f64, max: f64, row_top: f32, row_height: f32) -> f32 {
    if !v.is_finite() || !min.is_finite() || !max.is_finite() || (max - min).abs() < f64::EPSILON {
        return row_top + row_height * 0.5;
    }
    let t = ((v - min) / (max - min)).clamp(0.0, 1.0) as f32;
    row_top + (1.0 - t) * row_height
}

fn get_item_y(
    waves: &crate::wave_data::WaveData,
    item: crate::displayed_item::DisplayedItemRef,
    anchor: &Anchor,
) -> Option<f32> {
    waves
        .items_tree
        .iter_visible()
        .zip(&waves.drawing_infos)
        .find(|(node, _)| node.item_ref == item)
        .map(|(_, info)| match anchor {
            Anchor::Top => info.top(),
            Anchor::Center => info.top() + (info.bottom() - info.top()) / 2.0,
            Anchor::Bottom => info.bottom(),
        })
        .map(|point| point - waves.top_item_draw_offset)
}

fn marker_color_for_idx(
    waves: &crate::wave_data::WaveData,
    idx: u8,
    theme: &SurferTheme,
) -> Color32 {
    waves
        .items_tree
        .iter()
        .find_map(|node| {
            if let Some(DisplayedItem::Marker(marker)) = waves.displayed_items.get(&node.item_ref)
                && marker.idx == idx
            {
                return marker
                    .color
                    .as_ref()
                    .and_then(|color| theme.get_color(color));
            }
            None
        })
        .unwrap_or(theme.cursor.color)
}

impl SystemState {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn dump_svg(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let waves = self
            .user
            .waves
            .as_ref()
            .ok_or_else(|| eyre!("No waveform loaded"))?;

        let viewport_idx = 0usize;
        if viewport_idx >= waves.viewports.len() {
            return Err(eyre!("No viewport available"));
        }

        let context = self
            .context
            .as_ref()
            .ok_or_else(|| eyre!("egui context is not initialized"))?;

        let viewport_size = context.viewport_rect().size();
        let size = if viewport_size.x > 0.0 && viewport_size.y > 0.0 {
            viewport_size
        } else {
            emath::Vec2::new(
                self.user.config.layout.window_width as f32,
                self.user.config.layout.window_height as f32,
            )
        };

        let cfg = match waves.inner {
            crate::data_container::DataContainer::Waves(_) => DrawConfig::new(
                size.y,
                size.x,
                self.user.config.layout.waveforms_line_height,
                self.user.config.layout.waveforms_text_size,
            ),
            crate::data_container::DataContainer::Transactions(_) => DrawConfig::new(
                size.y,
                size.x,
                self.user.config.layout.transactions_line_height,
                self.user.config.layout.waveforms_text_size,
            ),
            crate::data_container::DataContainer::Empty => return Err(eyre!("No data to export")),
        };

        let mut msgs = vec![];
        self.generate_draw_commands(&cfg, &mut msgs, viewport_idx);
        for msg in msgs {
            if matches!(msg, Message::BuildAnalogCache { .. }) {
                self.update(msg);
            }
        }
        while !self.analog_caches_ready() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let mut async_msgs = vec![];
            self.push_async_messages(&mut async_msgs);
            while let Some(msg) = async_msgs.pop() {
                self.update(msg);
            }
        }

        let mut post_msgs = vec![];
        self.generate_draw_commands(&cfg, &mut post_msgs, viewport_idx);

        let waves = self
            .user
            .waves
            .as_ref()
            .ok_or_else(|| eyre!("No waveform loaded"))?;
        if waves.drawing_infos.is_empty() {
            return Err(eyre!("No drawing layout available yet"));
        }

        let content_top = waves
            .drawing_infos
            .iter()
            .map(ItemDrawingInfo::top)
            .fold(f32::INFINITY, f32::min);
        let content_bottom = waves
            .drawing_infos
            .iter()
            .map(ItemDrawingInfo::bottom)
            .fold(f32::NEG_INFINITY, f32::max);
        let timeline_height = if self.show_default_timeline() {
            cfg.text_size + 6.0
        } else {
            0.0
        };
        let content_height = (content_bottom - content_top).max(1.0);
        let height = content_height + timeline_height;

        let ucursor = waves.cursor.as_ref().and_then(num::BigInt::to_biguint);

        let mut max_name_chars = 4usize;
        let mut max_value_chars = 4usize;
        for drawing_info in waves.drawing_infos.iter().sorted_by_key(|o| o.top() as i32) {
            if let ItemDrawingInfo::Variable(variable_info) = drawing_info
                && let Some(node) = waves.items_tree.get_visible(drawing_info.vidx())
                && let Some(displayed_item) = waves.displayed_items.get(&node.item_ref)
            {
                let name_text = if variable_info.field_ref.field.is_empty() {
                    displayed_item.name()
                } else {
                    variable_info
                        .field_ref
                        .field
                        .last()
                        .cloned()
                        .unwrap_or_else(|| displayed_item.name())
                };
                let name_chars = name_text.chars().count() + node.level as usize * 2;
                max_name_chars = max_name_chars.max(name_chars);

                if let Some(value) = self.get_variable_value(
                    waves,
                    &variable_info.displayed_field_ref,
                    ucursor.as_ref(),
                ) {
                    max_value_chars = max_value_chars.max(value.chars().count());
                }
            }
        }
        max_name_chars = max_name_chars.max("Time".chars().count());

        let mut svg = String::with_capacity(512 * 1024);
        let waveform_width = cfg.canvas_width.max(1.0);
        let char_width = cfg.text_size * (20.0 / 31.0);
        let name_col_width = (max_name_chars as f32 * char_width + 24.0).clamp(180.0, 520.0);
        let value_col_width = (max_value_chars as f32 * char_width + 24.0).clamp(120.0, 520.0);
        let panel_gap = 8.0;
        let left_panel_width = name_col_width + value_col_width + panel_gap * 2.0;
        let wave_x_offset = left_panel_width;
        let width = wave_x_offset + waveform_width;

        let _ = writeln!(
            svg,
            "<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.0} {:.0}\">",
            width, height, width, height
        );

        let bg = self.user.config.theme.canvas_colors.background;
        let _ = writeln!(
            svg,
            "<rect x=\"0\" y=\"0\" width=\"100%\" height=\"100%\" fill=\"{}\" fill-opacity=\"{:.3}\" />",
            color_hex(bg),
            color_opacity(bg)
        );

        // Left side panels: variable names and variable values.
        let name_bg = self.user.config.theme.primary_ui_color.background;
        let value_bg = self.user.config.theme.secondary_ui_color.background;
        push_rect(&mut svg, 0.0, 0.0, name_col_width, height, name_bg);
        push_rect(
            &mut svg,
            name_col_width + panel_gap,
            0.0,
            value_col_width,
            height,
            value_bg,
        );

        let separator = epaint::Stroke::from(&self.user.config.theme.viewport_separator);
        push_line(
            &mut svg,
            name_col_width,
            0.0,
            name_col_width,
            height,
            separator.color,
            separator.width,
        );
        push_line(
            &mut svg,
            wave_x_offset,
            0.0,
            wave_x_offset,
            height,
            separator.color,
            separator.width,
        );

        if let Some(CachedDrawData::WaveDrawData(draw_data)) = self
            .draw_data
            .borrow()
            .get(viewport_idx)
            .and_then(|o| o.as_ref())
        {
            if self.show_ticks() {
                let tick_stroke = epaint::Stroke::from(&self.user.config.theme.ticks.style);
                for (_, x) in &draw_data.ticks {
                    push_line(
                        &mut svg,
                        wave_x_offset + *x,
                        timeline_height,
                        wave_x_offset + *x,
                        height,
                        tick_stroke.color,
                        tick_stroke.width,
                    );
                }
            }

            match self.clock_highlight_type() {
                ClockHighlightType::Line => {
                    let stroke = epaint::Stroke::from(&self.user.config.theme.clock_highlight_line);
                    for x in &draw_data.clock_edges {
                        push_line(
                            &mut svg,
                            wave_x_offset + *x,
                            timeline_height,
                            wave_x_offset + *x,
                            height,
                            stroke.color,
                            stroke.width,
                        );
                    }
                }
                ClockHighlightType::Cycle => {
                    for chunk in draw_data.clock_edges.chunks(2) {
                        if let [x_start, x_end] = chunk {
                            push_rect(
                                &mut svg,
                                wave_x_offset + *x_start,
                                timeline_height,
                                *x_end - *x_start,
                                content_height,
                                self.user.config.theme.clock_highlight_cycle,
                            );
                        }
                    }
                }
                ClockHighlightType::None => {}
            }

            for drawing_info in waves.drawing_infos.iter().sorted_by_key(|o| o.top() as i32) {
                let y_offset = timeline_height + (drawing_info.top() - content_top);

                let displayed_item = waves
                    .items_tree
                    .get_visible(drawing_info.vidx())
                    .and_then(|node| waves.displayed_items.get(&node.item_ref));
                let color = displayed_item
                    .and_then(crate::displayed_item::DisplayedItem::color)
                    .and_then(|name| self.user.config.theme.get_color(name))
                    .unwrap_or(self.user.config.theme.variable_default);

                if let ItemDrawingInfo::Variable(variable_info) = drawing_info
                    && let Some(commands) = draw_data
                        .draw_commands
                        .get(&variable_info.displayed_field_ref)
                {
                    let row_top = y_offset;
                    let row_height = (drawing_info.bottom() - drawing_info.top()).max(1.0);

                    if let Some(node) = waves.items_tree.get_visible(drawing_info.vidx())
                        && let Some(displayed_item) = waves.displayed_items.get(&node.item_ref)
                    {
                        let name_text = if variable_info.field_ref.field.is_empty() {
                            displayed_item.name()
                        } else {
                            variable_info
                                .field_ref
                                .field
                                .last()
                                .cloned()
                                .unwrap_or_else(|| displayed_item.name())
                        };
                        let indent = (node.level as f32) * 10.0;
                        let name_color = displayed_item
                            .color()
                            .and_then(|name| self.user.config.theme.get_color(name))
                            .unwrap_or(self.user.config.theme.primary_ui_color.foreground);
                        push_text(
                            &mut svg,
                            8.0 + indent,
                            row_top + row_height * 0.5,
                            &name_text,
                            cfg.text_size,
                            name_color,
                            "start",
                        );
                    }

                    if let Some(value) = self.get_variable_value(
                        waves,
                        &variable_info.displayed_field_ref,
                        ucursor.as_ref(),
                    ) {
                        let value_color = self.user.config.theme.get_best_text_color(value_bg);
                        push_text(
                            &mut svg,
                            name_col_width + panel_gap + 8.0,
                            row_top + row_height * 0.5,
                            &value,
                            cfg.text_size,
                            value_color,
                            "start",
                        );
                    }

                    match commands {
                        DrawingCommands::Digital(digital) => match digital.drawing_type {
                            DigitalDrawingType::Bool | DigitalDrawingType::Clock => {
                                for (old, new) in
                                    digital.values.iter().zip(digital.values.iter().skip(1))
                                {
                                    if let (Some(old_result), Some(new_result)) =
                                        (&old.1.inner, &new.1.inner)
                                    {
                                        let line_color =
                                            old_result.kind.color(color, &self.user.config.theme);
                                        let old_h = bool_height(&old_result.value);
                                        let new_h = bool_height(&new_result.value);
                                        let y_old = row_top + (1.0 - old_h) * row_height;
                                        let y_new = row_top + (1.0 - new_h) * row_height;
                                        let points = vec![
                                            (wave_x_offset + old.0, y_old),
                                            (wave_x_offset + new.0, y_old),
                                            (wave_x_offset + new.0, y_new),
                                        ];
                                        push_polyline(
                                            &mut svg,
                                            &points,
                                            line_color,
                                            self.user.config.theme.linewidth,
                                            "none",
                                        );
                                    }
                                }
                            }
                            DigitalDrawingType::Event => {
                                for event in &digital.values {
                                    if event.1.inner.is_some() {
                                        let x = wave_x_offset + event.0;
                                        push_line(
                                            &mut svg,
                                            x,
                                            row_top,
                                            x,
                                            row_top + row_height,
                                            color,
                                            self.user.config.theme.linewidth,
                                        );
                                        let tri = [
                                            (x - 2.5, row_top + 0.2 * row_height),
                                            (x, row_top),
                                            (x + 2.5, row_top + 0.2 * row_height),
                                        ];
                                        push_polygon(
                                            &mut svg,
                                            &tri,
                                            color,
                                            self.user.config.theme.linewidth,
                                            color,
                                        );
                                    }
                                }
                            }
                            DigitalDrawingType::Vector => {
                                for (old, new) in
                                    digital.values.iter().zip(digital.values.iter().skip(1))
                                {
                                    let old_x = wave_x_offset + old.0;
                                    let new_x = wave_x_offset + new.0;
                                    let transition_width = (new_x - old_x)
                                        .min(self.user.config.theme.vector_transition_width);
                                    let line_color = old
                                        .1
                                        .inner
                                        .as_ref()
                                        .map(|v| v.kind.color(color, &self.user.config.theme))
                                        .unwrap_or(color);

                                    let points = vec![
                                        (old_x, row_top + row_height * 0.5),
                                        (old_x + transition_width / 2.0, row_top),
                                        (new_x - transition_width / 2.0, row_top),
                                        (new_x, row_top + row_height * 0.5),
                                        (new_x - transition_width / 2.0, row_top + row_height),
                                        (old_x + transition_width / 2.0, row_top + row_height),
                                        (old_x, row_top + row_height * 0.5),
                                    ];

                                    if self.user.config.theme.wide_opacity != 0.0 {
                                        let mut fill = line_color;
                                        fill = fill
                                            .gamma_multiply(self.user.config.theme.wide_opacity);
                                        push_polygon(&mut svg, &points, fill, 0.0, fill);
                                    }

                                    push_polyline(
                                        &mut svg,
                                        &points,
                                        line_color,
                                        self.user.config.theme.linewidth,
                                        "none",
                                    );

                                    if let Some(prev) = &old.1.inner {
                                        let text_size = cfg.text_size;
                                        let char_width = text_size * (20.0 / 31.0);
                                        let text_area = (new_x - old_x) - transition_width;
                                        let num_chars = (text_area / char_width).floor() as usize;
                                        if num_chars >= 1 {
                                            let content =
                                                if prev.value.len() > num_chars && num_chars > 1 {
                                                    prev.value
                                                        .chars()
                                                        .take(num_chars - 1)
                                                        .chain(['…'])
                                                        .collect::<String>()
                                                } else {
                                                    prev.value.clone()
                                                };

                                            let text_color =
                                                self.user.config.theme.get_best_text_color(
                                                    self.user.config.theme.canvas_colors.background,
                                                );
                                            push_text(
                                                &mut svg,
                                                old_x + transition_width,
                                                row_top + row_height * 0.5,
                                                &content,
                                                text_size,
                                                text_color,
                                                "start",
                                            );
                                        }
                                    }
                                }
                            }
                        },
                        DrawingCommands::Analog(analog) => {
                            if let AnalogDrawingCommands::Ready {
                                viewport_min,
                                viewport_max,
                                global_min,
                                global_max,
                                type_limits,
                                values,
                                analog_settings,
                                ..
                            } = analog
                            {
                                let (range_min, range_max) = match analog_settings.y_axis_scale {
                                    crate::displayed_item::AnalogYAxisScale::Viewport => {
                                        (*viewport_min, *viewport_max)
                                    }
                                    crate::displayed_item::AnalogYAxisScale::Global => {
                                        (*global_min, *global_max)
                                    }
                                    crate::displayed_item::AnalogYAxisScale::TypeLimits => {
                                        type_limits
                                            .as_ref()
                                            .map_or((*global_min, *global_max), |r| (r.min, r.max))
                                    }
                                };

                                for cmd in values {
                                    match cmd {
                                        crate::analog_renderer::AnalogDrawingCommand::Flat {
                                            start_px,
                                            start_val,
                                            end_px,
                                            end_val,
                                        } => {
                                            let y1 = map_analog_to_y(
                                                *start_val, range_min, range_max, row_top,
                                                row_height,
                                            );
                                            let y2 = map_analog_to_y(
                                                *end_val, range_min, range_max, row_top, row_height,
                                            );
                                            push_line(
                                                &mut svg,
                                                wave_x_offset + *start_px,
                                                y1,
                                                wave_x_offset + *end_px,
                                                y2,
                                                color,
                                                self.user.config.theme.linewidth,
                                            );
                                        }
                                        crate::analog_renderer::AnalogDrawingCommand::Range {
                                            px,
                                            min_val,
                                            max_val,
                                        } => {
                                            let y1 = map_analog_to_y(
                                                *min_val, range_min, range_max, row_top, row_height,
                                            );
                                            let y2 = map_analog_to_y(
                                                *max_val, range_min, range_max, row_top, row_height,
                                            );
                                            push_line(
                                                &mut svg,
                                                wave_x_offset + *px,
                                                y1,
                                                wave_x_offset + *px,
                                                y2,
                                                color,
                                                self.user.config.theme.linewidth,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let viewport = &waves.viewports[viewport_idx];
        let num_timestamps = waves.safe_num_timestamps();

        if let Some(cursor) = &waves.cursor {
            let x = viewport.pixel_from_time(cursor, cfg.canvas_width, &num_timestamps);
            push_line(
                &mut svg,
                wave_x_offset + x,
                timeline_height,
                wave_x_offset + x,
                height,
                self.user.config.theme.cursor.color,
                self.user.config.theme.cursor.width,
            );
        }

        for (idx, marker) in &waves.markers {
            let x = viewport.pixel_from_time(marker, cfg.canvas_width, &num_timestamps);
            push_line(
                &mut svg,
                wave_x_offset + x,
                timeline_height,
                wave_x_offset + x,
                height,
                marker_color_for_idx(waves, *idx, &self.user.config.theme),
                self.user.config.theme.cursor.width,
            );
        }

        for graphic in waves.graphics.values() {
            match graphic {
                Graphic::TextArrow {
                    from: (from_point, from_dir),
                    to: (to_point, to_dir),
                    text,
                } => {
                    let from_x = wave_x_offset
                        + viewport.pixel_from_time(
                            &from_point.x,
                            cfg.canvas_width,
                            &num_timestamps,
                        );
                    let to_x = wave_x_offset
                        + viewport.pixel_from_time(&to_point.x, cfg.canvas_width, &num_timestamps);
                    if let (Some(from_y_raw), Some(to_y_raw)) = (
                        get_item_y(waves, from_point.y.item, &from_point.y.anchor),
                        get_item_y(waves, to_point.y.item, &to_point.y.anchor),
                    ) {
                        let from_y = timeline_height + (from_y_raw - content_top);
                        let to_y = timeline_height + (to_y_raw - content_top);
                        let dir_vec = |d: &Direction| match d {
                            Direction::North => (0.0, -1.0),
                            Direction::East => (-1.0, 0.0),
                            Direction::South => (0.0, 1.0),
                            Direction::West => (1.0, 0.0),
                        };
                        let (fx, fy) = dir_vec(from_dir);
                        let (tx, ty) = dir_vec(to_dir);
                        let c1x = from_x + 30.0 * fx;
                        let c1y = from_y + 30.0 * fy;
                        let c2x = to_x + 30.0 * tx;
                        let c2y = to_y + 30.0 * ty;

                        let _ = writeln!(
                            svg,
                            "<path d=\"M {from_x:.2} {from_y:.2} C {c1x:.2} {c1y:.2}, {c2x:.2} {c2y:.2}, {to_x:.2} {to_y:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"3\" />",
                            color_hex(self.user.config.theme.variable_dontcare)
                        );
                        let _ = writeln!(
                            svg,
                            "<text x=\"{to_x:.2}\" y=\"{to_y:.2}\" fill=\"{}\" font-size=\"15\" font-family=\"monospace\">{}</text>",
                            color_hex(self.user.config.theme.variable_dontcare),
                            escape_xml(text)
                        );
                    }
                }
                Graphic::Text {
                    pos: (pos, _dir),
                    text,
                } => {
                    let x = wave_x_offset
                        + viewport.pixel_from_time(&pos.x, cfg.canvas_width, &num_timestamps);
                    if let Some(y_raw) = get_item_y(waves, pos.y.item, &pos.y.anchor) {
                        let y = timeline_height + (y_raw - content_top);
                        let _ = writeln!(
                            svg,
                            "<text x=\"{x:.2}\" y=\"{y:.2}\" fill=\"{}\" font-size=\"15\" font-family=\"monospace\">{}</text>",
                            color_hex(self.user.config.theme.variable_dontcare),
                            escape_xml(text)
                        );
                    }
                }
            }
        }

        if self.show_default_timeline() {
            let timeline_bg = self.user.config.theme.canvas_colors.background;
            push_rect(
                &mut svg,
                wave_x_offset,
                0.0,
                waveform_width,
                timeline_height,
                timeline_bg,
            );

            if let Some(CachedDrawData::WaveDrawData(draw_data)) = self
                .draw_data
                .borrow()
                .get(viewport_idx)
                .and_then(|o| o.as_ref())
            {
                for (tick_text, x) in &draw_data.ticks {
                    push_text_top(
                        &mut svg,
                        wave_x_offset + *x,
                        1.0,
                        tick_text,
                        cfg.text_size,
                        self.user.config.theme.foreground,
                    );
                }
            }

            push_text(
                &mut svg,
                8.0,
                timeline_height * 0.5,
                "Time",
                cfg.text_size,
                self.user.config.theme.primary_ui_color.foreground,
                "start",
            );
        }

        svg.push_str("</svg>\n");
        std::fs::write(path, svg)?;
        Ok(())
    }
}
