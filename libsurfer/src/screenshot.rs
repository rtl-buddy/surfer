use arboard::Clipboard;
use egui::{CentralPanel, Frame, Id, Margin, RichText, ScrollArea, SidePanel, TextWrapMode};
use egui::containers::panel::PanelState;
use egui_skia_renderer::{EncodedImageFormat, RasterizeOptions, create_surface, draw_onto_surface};
use emath::Vec2;
use tracing::{error, info};

use crate::{Message, SystemState, setup_custom_font};

const DEFAULT_WIDTH: f32 = 1280.0;
const DEFAULT_HEIGHT: f32 = 720.0;

/// Read the current width of a named panel from the live egui context.
fn read_panel_width(ctx: &egui::Context, panel_name: &str, fallback: f32) -> f32 {
    PanelState::load(ctx, Id::new(panel_name))
        .map(|s| s.rect.width())
        .unwrap_or(fallback)
}

pub fn copy_viewport_as_png(state: &mut SystemState) {
    // Read panel widths from the live context so the screenshot matches
    // what the user sees (including any manual resizes).
    let (name_width, value_width) = state
        .context
        .as_ref()
        .map(|ctx| {
            (
                read_panel_width(ctx, "variable list", 100.0),
                read_panel_width(ctx, "variable values", 100.0),
            )
        })
        .unwrap_or((100.0, 100.0));

    let size = Vec2::new(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let size_i = (size.x as i32, size.y as i32);

    let mut surface = create_surface(size_i);
    surface
        .canvas()
        .clear(egui_skia_renderer::Color::BLACK);

    draw_onto_surface(
        &mut surface,
        |ctx| {
            ctx.set_visuals(state.get_visuals());
            setup_custom_font(ctx);

            let mut msgs = Vec::new();

            if let Some(waves) = &state.user.waves {
                let scroll_offset = waves.scroll_offset;

                if waves.any_displayed() {
                    // Signal name list panel — use width from the live UI
                    SidePanel::left("screenshot_variable_list")
                        .default_width(name_width)
                        .exact_width(name_width)
                        .show(ctx, |ui| {
                            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                            if state.show_default_timeline() {
                                ui.label(RichText::new("Time").italics());
                            }
                            ScrollArea::both()
                                .auto_shrink([false; 2])
                                .vertical_scroll_offset(scroll_offset)
                                .show(ui, |ui| {
                                    state.draw_item_list(&mut msgs, ui, ctx);
                                });
                        });

                    // Variable values panel — use width from the live UI
                    SidePanel::left("screenshot_variable_values")
                        .frame(
                            Frame::default()
                                .inner_margin(0)
                                .outer_margin(0)
                                .fill(state.user.config.theme.secondary_ui_color.background),
                        )
                        .default_width(value_width)
                        .exact_width(value_width)
                        .show(ctx, |ui| {
                            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                            ScrollArea::both()
                                .auto_shrink([false; 2])
                                .vertical_scroll_offset(scroll_offset)
                                .show(ui, |ui| state.draw_var_values(ui, &mut msgs));
                        });
                }
            }

            // Waveform canvas
            CentralPanel::default()
                .frame(Frame {
                    inner_margin: Margin::ZERO,
                    outer_margin: Margin::ZERO,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    state.draw_items(ctx, &mut msgs, ui, 0);
                });

            // Process analog cache messages like snapshot tests do
            for msg in msgs {
                if matches!(msg, Message::BuildAnalogCache { .. }) {
                    state.update(msg);
                }
            }
        },
        Some(RasterizeOptions {
            frames_before_screenshot: 5,
            ..Default::default()
        }),
    );

    let data = surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("Failed to encode image");

    let img = image::load_from_memory(&data).expect("Failed to decode PNG");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    match Clipboard::new() {
        Ok(mut clipboard) => {
            let img_data = arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: rgba.into_raw().into(),
            };
            match clipboard.set_image(img_data) {
                Ok(()) => info!("Viewport PNG copied to clipboard"),
                Err(e) => error!("Failed to copy image to clipboard: {e}"),
            }
        }
        Err(e) => error!("Failed to open clipboard: {e}"),
    }
}
