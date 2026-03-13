use ecolor::Color32;
use egui::{CornerRadius, DragValue, Pos2, Rect, Sense, Stroke};
use surfer_translation_types::VariableValue;

use crate::{Message, system_state::SystemState};

impl SystemState {
    pub fn draw_frame_buffer_window(&mut self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let frame_buffer_value = self.selected_variable_for_frame_buffer();
        let mut open = true;
        egui::Window::new("Frame Buffer")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                let Some((value, word_length)) = frame_buffer_value.as_ref() else {
                    ui.label(
                        "Select a variable with context menu \"Show frame buffer\" and place the cursor.",
                    );
                    return;
                };

                ui.checkbox(
                    &mut self.frame_buffer_square_pixels,
                    "Square pixels (off = stretch to window)",
                );

                ui.checkbox(&mut self.frame_buffer_rgb_mode, "RGB mode (unchecked = grayscale)");

                if self.frame_buffer_rgb_mode {
                    ui.horizontal(|ui| {
                        ui.label("R bits");
                        ui.add(DragValue::new(&mut self.frame_buffer_r_bits).range(0..=8));
                        ui.label("G bits");
                        ui.add(DragValue::new(&mut self.frame_buffer_g_bits).range(0..=8));
                        ui.label("B bits");
                        ui.add(DragValue::new(&mut self.frame_buffer_b_bits).range(0..=8));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Grayscale bits");
                        ui.add(DragValue::new(&mut self.frame_buffer_grayscale_bits).range(1..=8));
                    });
                }

                ui.separator();

                let bits = frame_buffer_bits(value, *word_length as usize);
                if bits.is_empty() {
                    ui.label("No bits available");
                    return;
                }

                let pixel_colors = if self.frame_buffer_rgb_mode {
                    let r_bits = self.frame_buffer_r_bits as usize;
                    let g_bits = self.frame_buffer_g_bits as usize;
                    let b_bits = self.frame_buffer_b_bits as usize;
                    let bits_per_pixel = r_bits + g_bits + b_bits;
                    if bits_per_pixel == 0 {
                        ui.label("Set at least one RGB channel bit count above zero.");
                        return;
                    }
                    decode_rgb_pixels(&bits, r_bits, g_bits, b_bits)
                } else {
                    let gray_bits = self.frame_buffer_grayscale_bits as usize;
                    decode_grayscale_pixels(&bits, gray_bits)
                };

                if pixel_colors.is_empty() {
                    ui.label("No pixels to draw with current bit settings.");
                    return;
                }

                let max_columns = pixel_colors.len().max(1);
                self.frame_buffer_pixels_per_row =
                    self.frame_buffer_pixels_per_row.clamp(1, max_columns);

                ui.horizontal(|ui| {
                    ui.label("Pixels in x-direction");
                    ui.add(
                        egui::Slider::new(&mut self.frame_buffer_pixels_per_row, 1..=max_columns)
                            .integer(),
                    );
                });

                ui.separator();

                let columns = self.frame_buffer_pixels_per_row.min(pixel_colors.len()).max(1);
                let rows = pixel_colors.len().div_ceil(columns);
                let available = ui.available_size_before_wrap();

                if available.x <= 0.0 || available.y <= 0.0 {
                    return;
                }

                let (pixel_width, pixel_height) = if self.frame_buffer_square_pixels {
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

    fn selected_variable_for_frame_buffer(&self) -> Option<(VariableValue, u32)> {
        let waves = self.user.waves.as_ref()?;
        let variable_ref = self.frame_buffer_variable.as_ref()?;

        let cursor = waves.cursor.as_ref()?.to_biguint()?;
        let wave_container = waves.inner.as_waves()?;
        let meta = wave_container.variable_meta(variable_ref).ok()?;
        let word_length = meta.num_bits?;
        let query_result = wave_container
            .query_variable(variable_ref, &cursor)
            .ok()
            .flatten()?;

        let (_, value) = query_result.current?;
        Some((value, word_length))
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
    bits_to_chunks(bits, grayscale_bits)
        .map(|chunk| {
            let gray = scale_to_u8(bits_to_u16(&chunk), grayscale_bits);
            Color32::from_rgb(gray, gray, gray)
        })
        .collect()
}

fn decode_rgb_pixels(bits: &[bool], r_bits: usize, g_bits: usize, b_bits: usize) -> Vec<Color32> {
    let bits_per_pixel = r_bits + g_bits + b_bits;
    bits_to_chunks(bits, bits_per_pixel)
        .map(|chunk| {
            let mut offset = 0;
            let red = {
                let val = scale_to_u8(bits_to_u16(&chunk[offset..offset + r_bits]), r_bits);
                offset += r_bits;
                val
            };
            let green = {
                let val = scale_to_u8(bits_to_u16(&chunk[offset..offset + g_bits]), g_bits);
                offset += g_bits;
                val
            };
            let blue = scale_to_u8(bits_to_u16(&chunk[offset..offset + b_bits]), b_bits);
            Color32::from_rgb(red, green, blue)
        })
        .collect()
}

fn bits_to_chunks(bits: &[bool], chunk_size: usize) -> impl Iterator<Item = Vec<bool>> + '_ {
    (0..bits.len()).step_by(chunk_size).map(move |start| {
        (0..chunk_size)
            .map(|offset| bits.get(start + offset).copied().unwrap_or(false))
            .collect::<Vec<_>>()
    })
}

fn bits_to_u16(bits: &[bool]) -> u16 {
    bits.iter()
        .fold(0u16, |acc, bit| (acc << 1) | u16::from(*bit))
}

fn scale_to_u8(value: u16, bits: usize) -> u8 {
    if bits == 0 {
        return 0;
    }
    let max_in = (1u16 << bits) - 1;
    (((value as u32) * 255) / (max_in as u32)) as u8
}
