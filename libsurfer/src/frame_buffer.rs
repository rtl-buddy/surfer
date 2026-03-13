use ecolor::Color32;
use egui::{CornerRadius, DragValue, Pos2, Rect, Sense, Stroke};
use serde::{Deserialize, Serialize};
use surfer_translation_types::VariableValue;

use crate::wave_container::VariableRefExt;
use crate::{Message, system_state::SystemState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrameBufferSettings {
    pub pixels_per_row: usize,
    pub square_pixels: bool,
    pub rgb_mode: bool,
    pub grayscale_bits: u8,
    pub r_bits: u8,
    pub g_bits: u8,
    pub b_bits: u8,
}

impl Default for FrameBufferSettings {
    fn default() -> Self {
        Self {
            pixels_per_row: 16,
            square_pixels: true,
            rgb_mode: false,
            grayscale_bits: 1,
            r_bits: 3,
            g_bits: 3,
            b_bits: 2,
        }
    }
}

struct FrameBufferSample {
    value: VariableValue,
    word_length: u32,
    variable_name: String,
    cursor: String,
}

impl SystemState {
    pub fn draw_frame_buffer_window(&mut self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let frame_buffer_value = self.selected_variable_for_frame_buffer();
        let mut open = true;
        egui::Window::new("Frame Buffer")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                let Some(sample) = frame_buffer_value.as_ref() else {
                    ui.label(
                        "Select a variable with context menu \"Show frame buffer\" and place the cursor.",
                    );
                    return;
                };

                let settings = &mut self.user.frame_buffer;

                ui.checkbox(
                    &mut settings.square_pixels,
                    "Square pixels",
                );

                ui.checkbox(&mut settings.rgb_mode, "RGB mode");

                if settings.rgb_mode {
                    ui.horizontal(|ui| {
                        ui.label("R bits");
                        ui.add(DragValue::new(&mut settings.r_bits).range(0..=8));
                        ui.label("G bits");
                        ui.add(DragValue::new(&mut settings.g_bits).range(0..=8));
                        ui.label("B bits");
                        ui.add(DragValue::new(&mut settings.b_bits).range(0..=8));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Grayscale bits");
                        ui.add(DragValue::new(&mut settings.grayscale_bits).range(1..=8));
                    });
                }

                ui.separator();

                let bits = frame_buffer_bits(&sample.value, sample.word_length as usize);
                if bits.is_empty() {
                    ui.label("No bits available");
                    return;
                }

                let (pixel_colors, bits_per_pixel) = if settings.rgb_mode {
                    let r_bits = settings.r_bits as usize;
                    let g_bits = settings.g_bits as usize;
                    let b_bits = settings.b_bits as usize;
                    let bits_per_pixel = r_bits + g_bits + b_bits;
                    if bits_per_pixel == 0 {
                        ui.label("Set at least one RGB channel bit count above zero.");
                        return;
                    }
                    (decode_rgb_pixels(&bits, r_bits, g_bits, b_bits), bits_per_pixel)
                } else {
                    let gray_bits = settings.grayscale_bits as usize;
                    (decode_grayscale_pixels(&bits, gray_bits), gray_bits)
                };

                if pixel_colors.is_empty() {
                    ui.label("No pixels to draw with current bit settings.");
                    return;
                }

                ui.label(format!(
                    "Var: {} | Cursor: {} | Bits: {} | Bits/px: {} | Pixels: {}",
                    sample.variable_name,
                    sample.cursor,
                    bits.len(),
                    bits_per_pixel,
                    pixel_colors.len()
                ));

                let max_columns = pixel_colors.len().max(1);
                settings.pixels_per_row = settings.pixels_per_row.clamp(1, max_columns);

                ui.horizontal(|ui| {
                    ui.label("Pixels in x-direction");
                    ui.add(
                        egui::Slider::new(&mut settings.pixels_per_row, 1..=max_columns)
                            .integer(),
                    );
                });

                ui.separator();

                let columns = settings.pixels_per_row.min(pixel_colors.len()).max(1);
                let rows = pixel_colors.len().div_ceil(columns);
                let available = ui.available_size_before_wrap();

                if available.x <= 0.0 || available.y <= 0.0 {
                    return;
                }

                let (pixel_width, pixel_height) = if settings.square_pixels {
                    let side = (available.x / columns as f32).min(available.y / rows as f32);
                    (side, side)
                } else {
                    (available.x / columns as f32, available.y / rows as f32)
                };

                let image_size =
                    egui::vec2(pixel_width * columns as f32, pixel_height * rows as f32);
                let (rect, _) = ui.allocate_exact_size(image_size, Sense::hover());
                let painter = ui.painter_at(rect);

                for (index, color) in pixel_colors.iter().copied().enumerate() {
                    let x = index % columns;
                    let y = index / columns;

                    let min = Pos2 {
                        x: rect.min.x + x as f32 * pixel_width,
                        y: rect.min.y + y as f32 * pixel_height,
                    };
                    let max = Pos2 {
                        x: min.x + pixel_width,
                        y: min.y + pixel_height,
                    };

                    painter.rect_filled(
                        Rect { min, max },
                        CornerRadius::ZERO,
                        color,
                    );
                }

                painter.rect_stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, ui.visuals().weak_text_color()),
                    egui::StrokeKind::Inside,
                );
            });

        if !open {
            msgs.push(Message::SetFrameBufferVisible(false));
        }
    }

    fn selected_variable_for_frame_buffer(&self) -> Option<FrameBufferSample> {
        let waves = self.user.waves.as_ref()?;
        let variable_ref = self.frame_buffer_variable.as_ref()?;
        let variable_name = variable_ref.full_path_string_no_index();
        let cursor_text = waves.cursor.as_ref()?.to_string();

        let cursor = waves.cursor.as_ref()?.to_biguint()?;
        let wave_container = waves.inner.as_waves()?;
        let meta = wave_container.variable_meta(variable_ref).ok()?;
        let word_length = meta.num_bits?;
        let query_result = wave_container
            .query_variable(variable_ref, &cursor)
            .ok()
            .flatten()?;

        let (_, value) = query_result.current?;
        Some(FrameBufferSample {
            value,
            word_length,
            variable_name,
            cursor: cursor_text,
        })
    }
}

fn frame_buffer_bits(value: &VariableValue, word_length: usize) -> Vec<bool> {
    let mut bits: Vec<bool> = match value {
        VariableValue::BigUint(v) => format!("{v:b}").chars().map(|c| c == '1').collect(),
        VariableValue::String(v) => v.chars().map(|c| c == '1').collect(),
    };

    if bits.len() < word_length {
        let mut padded = vec![false; word_length - bits.len()];
        padded.extend(bits);
        bits = padded;
    } else if bits.len() > word_length {
        bits = bits[bits.len() - word_length..].to_vec();
    }

    bits
}

fn decode_grayscale_pixels(bits: &[bool], grayscale_bits: usize) -> Vec<Color32> {
    let mut out = Vec::with_capacity(bits.len().div_ceil(grayscale_bits.max(1)));
    for start in (0..bits.len()).step_by(grayscale_bits.max(1)) {
        let gray = scale_to_u8(
            bits_to_u16_padded(bits, start, grayscale_bits),
            grayscale_bits,
        );
        out.push(Color32::from_rgb(gray, gray, gray));
    }
    out
}

fn decode_rgb_pixels(bits: &[bool], r_bits: usize, g_bits: usize, b_bits: usize) -> Vec<Color32> {
    let bits_per_pixel = r_bits + g_bits + b_bits;
    let mut out = Vec::with_capacity(bits.len().div_ceil(bits_per_pixel.max(1)));
    for start in (0..bits.len()).step_by(bits_per_pixel.max(1)) {
        let red = scale_to_u8(bits_to_u16_padded(bits, start, r_bits), r_bits);
        let green = scale_to_u8(bits_to_u16_padded(bits, start + r_bits, g_bits), g_bits);
        let blue = scale_to_u8(
            bits_to_u16_padded(bits, start + r_bits + g_bits, b_bits),
            b_bits,
        );
        out.push(Color32::from_rgb(red, green, blue));
    }
    out
}

fn bits_to_u16_padded(bits: &[bool], start: usize, len: usize) -> u16 {
    let mut value = 0u16;
    for offset in 0..len {
        value = (value << 1) | u16::from(bits.get(start + offset).copied().unwrap_or(false));
    }
    value
}

fn scale_to_u8(value: u16, bits: usize) -> u8 {
    if bits == 0 {
        return 0;
    }
    let max_in = (1u16 << bits) - 1;
    (((value as u32) * 255) / (max_in as u32)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    #[test]
    fn frame_buffer_bits_pads_to_word_length() {
        let bits = frame_buffer_bits(&VariableValue::BigUint(BigUint::from(0b101u8)), 5);
        assert_eq!(bits, vec![false, false, true, false, true]);
    }

    #[test]
    fn frame_buffer_bits_truncates_to_word_length() {
        let bits = frame_buffer_bits(&VariableValue::String("101101".to_string()), 4);
        assert_eq!(bits, vec![true, true, false, true]);
    }

    #[test]
    fn bits_to_u16_padded_reads_and_zero_pads() {
        let bits = vec![true, false, true];
        assert_eq!(bits_to_u16_padded(&bits, 0, 3), 0b101);
        assert_eq!(bits_to_u16_padded(&bits, 1, 4), 0b0100);
    }

    #[test]
    fn scale_to_u8_scales_full_range() {
        assert_eq!(scale_to_u8(0, 1), 0);
        assert_eq!(scale_to_u8(1, 1), 255);
        assert_eq!(scale_to_u8(7, 3), 255);
        assert_eq!(scale_to_u8(4, 3), 145);
    }

    #[test]
    fn decode_grayscale_pixels_uses_bit_groups() {
        let bits = vec![false, false, true, true];
        let pixels = decode_grayscale_pixels(&bits, 2);
        assert_eq!(pixels.len(), 2);
        assert_eq!(pixels[0], Color32::from_rgb(0, 0, 0));
        assert_eq!(pixels[1], Color32::from_rgb(255, 255, 255));
    }

    #[test]
    fn decode_rgb_pixels_supports_different_channel_widths() {
        let bits = vec![
            true, false, false, true, true, false, // R=10 G=01 B=10 with r=2,g=2,b=2
        ];
        let pixels = decode_rgb_pixels(&bits, 2, 2, 2);
        assert_eq!(pixels.len(), 1);
        assert_eq!(pixels[0], Color32::from_rgb(170, 85, 170));
    }
}
