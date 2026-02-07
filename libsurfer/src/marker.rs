use ecolor::Color32;
use egui::{Context, RichText, WidgetText, Window};
use egui_extras::{Column, TableBuilder};
use emath::{Align2, Pos2, Rect};
use epaint::{CornerRadius, FontId, Stroke};
use itertools::Itertools;
use num::BigInt;

use crate::SystemState;
use crate::drawing_canvas::draw_vertical_line;
use crate::{
    config::SurferTheme,
    displayed_item::{DisplayedItem, DisplayedItemRef, DisplayedMarker},
    message::Message,
    time::TimeFormatter,
    view::{DrawingContext, ItemDrawingInfo},
    viewport::Viewport,
    wave_data::WaveData,
};

pub const DEFAULT_MARKER_NAME: &str = "Marker";
const MAX_MARKERS: usize = 255;
const MAX_MARKER_INDEX: u8 = 254;
const CURSOR_MARKER_IDX: u8 = 255;

impl WaveData {
    /// Get the color for a marker by its index, falling back to cursor color if not found
    fn get_marker_color(&self, idx: u8, theme: &SurferTheme) -> Color32 {
        self.items_tree
            .iter()
            .find_map(|node| {
                if let Some(DisplayedItem::Marker(marker)) =
                    self.displayed_items.get(&node.item_ref)
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

    pub fn draw_cursor(&self, theme: &SurferTheme, ctx: &mut DrawingContext, viewport: &Viewport) {
        if let Some(marker) = &self.cursor {
            let num_timestamps = self.safe_num_timestamps();
            let x = viewport.pixel_from_time(marker, ctx.cfg.canvas_width, &num_timestamps);
            draw_vertical_line(x, ctx, &theme.cursor);
        }
    }

    pub fn draw_markers(&self, theme: &SurferTheme, ctx: &mut DrawingContext, viewport: &Viewport) {
        let num_timestamps = self.safe_num_timestamps();
        for (idx, marker) in &self.markers {
            let color = self.get_marker_color(*idx, theme);
            let stroke = Stroke {
                color,
                width: theme.cursor.width,
            };
            let x = viewport.pixel_from_time(marker, ctx.cfg.canvas_width, &num_timestamps);
            draw_vertical_line(x, ctx, stroke);
        }
    }

    #[must_use]
    pub fn can_add_marker(&self) -> bool {
        self.markers.len() < MAX_MARKERS
    }

    pub fn add_marker(
        &mut self,
        location: &BigInt,
        name: Option<String>,
        move_focus: bool,
    ) -> Option<DisplayedItemRef> {
        if !self.can_add_marker() {
            return None;
        }

        let Some(idx) = (0..=MAX_MARKER_INDEX).find(|idx| !self.markers.contains_key(idx)) else {
            // This shouldn't happen since can_add_marker() was already checked,
            // but handle it gracefully
            return None;
        };

        let item_ref = self.insert_item(
            DisplayedItem::Marker(DisplayedMarker {
                color: None,
                background_color: None,
                name,
                idx,
            }),
            None,
            move_focus,
        );
        self.markers.insert(idx, location.clone());

        Some(item_ref)
    }

    pub fn remove_marker(&mut self, idx: u8) {
        if let Some(&marker_item_ref) =
            self.displayed_items
                .iter()
                .find_map(|(id, item)| match item {
                    DisplayedItem::Marker(marker) if marker.idx == idx => Some(id),
                    _ => None,
                })
        {
            self.remove_displayed_item(marker_item_ref);
        }
    }

    /// Set the marker with the specified id to the location. If the marker doesn't exist already,
    /// it will be created
    pub fn set_marker_position(&mut self, idx: u8, location: &BigInt) {
        if !self.markers.contains_key(&idx) {
            self.insert_item(
                DisplayedItem::Marker(DisplayedMarker {
                    color: None,
                    background_color: None,
                    name: None,
                    idx,
                }),
                None,
                true,
            );
        }
        self.markers.insert(idx, location.clone());
    }

    pub fn move_marker_to_cursor(&mut self, idx: u8) {
        if let Some(location) = self.cursor.clone() {
            self.set_marker_position(idx, &location);
        }
    }

    /// Draw text with background box at the specified position
    /// Returns the text and its background rectangle info for reuse if needed
    #[allow(clippy::too_many_arguments)]
    fn draw_text_with_background(
        ctx: &mut DrawingContext,
        x: f32,
        y: f32,
        text: &str,
        text_size: f32,
        background_color: Color32,
        foreground_color: Color32,
        padding: f32,
    ) {
        // Measure text first
        let rect = ctx.painter.text(
            (ctx.to_screen)(x, y),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(text_size),
            foreground_color,
        );

        // Background rectangle with padding
        let min = Pos2::new(rect.min.x - padding, rect.min.y - padding);
        let max = Pos2::new(rect.max.x + padding, rect.max.y + padding);

        ctx.painter
            .rect_filled(Rect { min, max }, CornerRadius::ZERO, background_color);

        // Draw text on top of background
        ctx.painter.text(
            (ctx.to_screen)(x, y),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(text_size),
            foreground_color,
        );
    }

    pub fn draw_marker_number_boxes(
        &self,
        ctx: &mut DrawingContext,
        theme: &SurferTheme,
        viewport: &Viewport,
    ) {
        let text_size = ctx.cfg.text_size;

        for displayed_item in self
            .items_tree
            .iter_visible()
            .map(|node| self.displayed_items.get(&node.item_ref))
            .filter_map(|item| match item {
                Some(DisplayedItem::Marker(marker)) => Some(marker),
                _ => None,
            })
        {
            let item = DisplayedItem::Marker(displayed_item.clone());
            let background_color = get_marker_background_color(&item, theme);

            let x =
                self.numbered_marker_location(displayed_item.idx, viewport, ctx.cfg.canvas_width);
            let idx_string = displayed_item.idx.to_string();

            Self::draw_text_with_background(
                ctx,
                x,
                ctx.cfg.canvas_height * 0.5,
                &idx_string,
                text_size,
                background_color,
                theme.foreground,
                2.0,
            );
        }
    }
}

impl SystemState {
    pub fn draw_marker_window(&self, waves: &WaveData, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;

        // Construct markers list: cursor first (if present), then numbered markers
        let markers: Vec<(u8, &BigInt, WidgetText)> = waves
            .cursor
            .as_ref()
            .into_iter()
            .map(|cursor| {
                (
                    CURSOR_MARKER_IDX,
                    cursor,
                    WidgetText::RichText(RichText::new("Primary").into()),
                )
            })
            .chain(
                waves
                    .items_tree
                    .iter()
                    .filter_map(|node| waves.displayed_items.get(&node.item_ref))
                    .filter_map(|displayed_item| match displayed_item {
                        DisplayedItem::Marker(marker) => {
                            let text_color = self.get_item_text_color(displayed_item);
                            Some((
                                marker.idx,
                                waves.numbered_marker_time(marker.idx),
                                marker.marker_text(text_color),
                            ))
                        }
                        _ => None,
                    })
                    .sorted_by(|a, b| Ord::cmp(&a.0, &b.0)),
            )
            .collect();

        Window::new("Markers")
            .collapsible(true)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    // Table of markers: header row then rows of time differences.
                    let row_height = ui.text_style_height(&egui::TextStyle::Body);
                    TableBuilder::new(ui)
                        .striped(true)
                        .cell_layout(egui::Layout::right_to_left(emath::Align::TOP))
                        .columns(Column::auto().resizable(true), markers.len() + 1)
                        .auto_shrink(emath::Vec2b::new(false, true))
                        .header(row_height, |mut header| {
                            header.col(|ui| {
                                ui.label("");
                            });
                            for (marker_idx, _, widget_text) in &markers {
                                header.col(|ui| {
                                    if ui.label(widget_text.clone()).clicked() {
                                        msgs.push(marker_click_message(
                                            *marker_idx,
                                            waves.cursor.as_ref(),
                                        ));
                                    }
                                });
                            }
                        })
                        .body(|mut body| {
                            let time_formatter = TimeFormatter::new(
                                &waves.inner.metadata().timescale,
                                &self.user.wanted_timeunit,
                                &self.get_time_format(),
                            );
                            for (marker_idx, row_marker_time, row_widget_text) in &markers {
                                body.row(row_height, |mut row| {
                                    row.col(|ui| {
                                        if ui.label(row_widget_text.clone()).clicked() {
                                            msgs.push(marker_click_message(
                                                *marker_idx,
                                                waves.cursor.as_ref(),
                                            ));
                                        }
                                    });
                                    for (_, col_marker_time, _) in &markers {
                                        let diff = time_formatter
                                            .format(&(*row_marker_time - *col_marker_time));
                                        row.col(|ui| {
                                            ui.label(diff);
                                        });
                                    }
                                });
                            }
                        });
                    ui.add_space(15.);
                    if ui.button("Close").clicked() {
                        msgs.push(Message::SetCursorWindowVisible(false));
                    }
                });
            });
        if !open {
            msgs.push(Message::SetCursorWindowVisible(false));
        }
    }

    pub fn draw_marker_boxes(
        &self,
        waves: &WaveData,
        ctx: &mut DrawingContext,
        gap: f32,
        viewport: &Viewport,
        y_zero: f32,
    ) {
        let text_size = ctx.cfg.text_size;

        let time_formatter = TimeFormatter::new(
            &waves.inner.metadata().timescale,
            &self.user.wanted_timeunit,
            &self.get_time_format(),
        );
        for drawing_info in waves.drawing_infos.iter().filter_map(|item| match item {
            ItemDrawingInfo::Marker(marker) => Some(marker),
            _ => None,
        }) {
            let Some(item) = waves
                .items_tree
                .get_visible(drawing_info.vidx)
                .and_then(|node| waves.displayed_items.get(&node.item_ref))
            else {
                return;
            };

            // We draw in absolute coords, but the variable offset in the y
            // direction is also in absolute coordinates, so we need to
            // compensate for that
            let y_offset = drawing_info.top - y_zero;
            let y_bottom = drawing_info.bottom - y_zero;

            let background_color = get_marker_background_color(item, &self.user.config.theme);

            let x =
                waves.numbered_marker_location(drawing_info.idx, viewport, ctx.cfg.canvas_width);

            // Time string
            let time = time_formatter.format(
                waves
                    .markers
                    .get(&drawing_info.idx)
                    .unwrap_or(&BigInt::from(0)),
            );

            let text_color = self.user.config.theme.get_best_text_color(background_color);

            // Create galley
            let galley =
                ctx.painter
                    .layout_no_wrap(time, FontId::proportional(text_size), text_color);
            let offset_width = galley.rect.width() * 0.5 + 2. * gap;

            // Background rectangle
            let min = (ctx.to_screen)(x - offset_width, y_offset - gap);
            let max = (ctx.to_screen)(x + offset_width, y_bottom + gap);

            ctx.painter
                .rect_filled(Rect { min, max }, CornerRadius::ZERO, background_color);

            // Draw actual text on top of rectangle
            ctx.painter.galley(
                (ctx.to_screen)(
                    x - galley.rect.width() * 0.5,
                    (y_offset + y_bottom - galley.rect.height()) * 0.5,
                ),
                galley,
                text_color,
            );
        }
    }
}

/// Get the background color for a marker or cursor, with fallback to theme cursor color
fn get_marker_background_color(item: &DisplayedItem, theme: &SurferTheme) -> Color32 {
    item.color()
        .and_then(|color| theme.get_color(color))
        .unwrap_or(theme.cursor.color)
}

/// Generate the message for a marker click based on its index
fn marker_click_message(marker_idx: u8, cursor: Option<&BigInt>) -> Message {
    if marker_idx < CURSOR_MARKER_IDX {
        Message::GoToMarkerPosition(marker_idx, 0)
    } else {
        Message::GoToTime(cursor.cloned(), 0)
    }
}
