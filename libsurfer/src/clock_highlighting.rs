//! Drawing and handling of clock highlighting.
use derive_more::{Display, FromStr};
use egui::Ui;
use emath::{Pos2, Rect};
use enum_iterator::Sequence;
use epaint::{CornerRadius, Stroke};
use serde::{Deserialize, Serialize};

use crate::{config::SurferConfig, message::Message, view::DrawingContext};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Display, FromStr, Sequence, Serialize)]
pub enum ClockHighlightType {
    /// Draw a line at every posedge of the clocks
    Line,

    /// Highlight every other cycle
    Cycle,

    /// No highlighting
    None,
}

pub fn draw_clock_edge_marks(
    clock_edges: &[f32],
    ctx: &mut DrawingContext,
    config: &SurferConfig,
    clock_highlight_type: ClockHighlightType,
) {
    match clock_highlight_type {
        ClockHighlightType::Line => {
            let stroke = Stroke::from(&config.theme.clock_highlight_line);

            for x in clock_edges {
                let Pos2 {
                    x: x_pos,
                    y: y_start,
                } = (ctx.to_screen)(*x, 0.);
                ctx.painter
                    .vline(x_pos, (y_start)..=(y_start + ctx.cfg.canvas_height), stroke);
            }
        }
        ClockHighlightType::Cycle => {
            // Process clock edges in pairs: every other cycle gets highlighted
            let fill_color = config.theme.clock_highlight_cycle;

            for chunk in clock_edges.chunks(2) {
                if let [x_start, x_end] = chunk {
                    let Pos2 {
                        x: x_end_screen,
                        y: y_start,
                    } = (ctx.to_screen)(*x_end, 0.);
                    ctx.painter.rect_filled(
                        Rect {
                            min: (ctx.to_screen)(*x_start, 0.),
                            max: Pos2 {
                                x: x_end_screen,
                                y: ctx.cfg.canvas_height + y_start,
                            },
                        },
                        CornerRadius::ZERO,
                        fill_color,
                    );
                }
            }
        }
        ClockHighlightType::None => (),
    }
}

pub fn clock_highlight_type_menu(
    ui: &mut Ui,
    msgs: &mut Vec<Message>,
    clock_highlight_type: ClockHighlightType,
) {
    for highlight_type in enum_iterator::all::<ClockHighlightType>() {
        if ui
            .radio(
                highlight_type == clock_highlight_type,
                highlight_type.to_string(),
            )
            .clicked()
        {
            msgs.push(Message::SetClockHighlightType(highlight_type));
        }
    }
}
