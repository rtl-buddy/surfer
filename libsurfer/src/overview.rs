use crate::message::Message;
use crate::view::{DrawConfig, DrawingContext};
use crate::viewport::Viewport;
use crate::{SystemState, wave_data::WaveData};
use egui::{Context, Frame, PointerButton, Sense, TopBottomPanel, Ui};
use emath::{Align2, Pos2, Rect, RectTransform};
use epaint::CornerRadius;

impl SystemState {
    pub fn add_overview_panel(&self, ctx: &Context, waves: &WaveData, msgs: &mut Vec<Message>) {
        TopBottomPanel::bottom("overview")
            .frame(Frame {
                fill: self.user.config.theme.primary_ui_color.background,
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.draw_overview(ui, waves, msgs);
            });
    }

    fn draw_overview(&self, ui: &mut Ui, waves: &WaveData, msgs: &mut Vec<Message>) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        let frame_size = response.rect.size();
        let frame_width = frame_size.x;
        let frame_height = frame_size.y;
        let cfg = DrawConfig::new(
            frame_height,
            frame_width,
            self.user.config.layout.waveforms_line_height,
            self.user.config.layout.waveforms_text_size,
        );
        let container_rect = Rect::from_min_size(Pos2::ZERO, frame_size);
        let to_screen = RectTransform::from_to(container_rect, response.rect);

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y)),
            theme: &self.user.config.theme,
        };

        let num_timestamps = waves.safe_num_timestamps();
        let viewport_all = waves.viewport_all();
        let fill_color = self
            .user
            .config
            .theme
            .canvas_colors
            .foreground
            .gamma_multiply(0.3);

        // Draw rectangles for each viewport
        waves
            .viewports
            .iter()
            .map(|viewport| get_viewport_rect(&ctx, &num_timestamps, &viewport_all, viewport))
            .for_each(|rect| {
                ctx.painter
                    .rect_filled(rect, CornerRadius::ZERO, fill_color);
            });

        // Draw cursor
        waves.draw_cursor(&self.user.config.theme, &mut ctx, &viewport_all);

        // Draw ticks
        let mut ticks = self.get_ticks_for_viewport(waves, &viewport_all, &cfg);

        if ticks.len() >= 2 {
            // Remove first and last tick
            ticks.pop();
            ticks.remove(0);
            // Draw ticks
            waves.draw_ticks(
                self.user.config.theme.foreground,
                &ticks,
                &ctx,
                frame_height * 0.5,
                Align2::CENTER_CENTER,
            );
        }

        // Draw markers
        waves.draw_markers(&self.user.config.theme, &mut ctx, &viewport_all);
        waves.draw_marker_number_boxes(&mut ctx, &self.user.config.theme, &viewport_all);

        // Handle dragging of the primary viewport
        response.dragged_by(PointerButton::Primary).then(|| {
            let pointer_pos_global = ui.input(|i| i.pointer.interact_pos());
            let pos = pointer_pos_global.map(|p| to_screen.inverse().transform_pos(p));
            if let Some(pos) = pos {
                let timestamp = viewport_all.as_time_bigint(pos.x, frame_width, &num_timestamps);
                msgs.push(Message::GoToTime(Some(timestamp), 0));
            }
        });
    }
}

fn get_viewport_rect(
    ctx: &DrawingContext<'_>,
    num_timestamps: &num::BigInt,
    viewport_all: &Viewport,
    viewport: &Viewport,
) -> Rect {
    let minx = viewport_all.pixel_from_absolute_time(
        viewport.curr_left.absolute(num_timestamps),
        ctx.cfg.canvas_width,
        num_timestamps,
    );
    let maxx = viewport_all.pixel_from_absolute_time(
        viewport.curr_right.absolute(num_timestamps),
        ctx.cfg.canvas_width,
        num_timestamps,
    );
    let min = (ctx.to_screen)(minx, 0.);
    let max = (ctx.to_screen)(maxx, ctx.cfg.canvas_height);
    Rect { min, max }
}
