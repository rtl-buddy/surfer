use ecolor::Color32;
use egui::{CornerRadius, DragValue, Pos2, Rect, Sense, Stroke};
use serde::{Deserialize, Serialize};
use surfer_translation_types::VariableValue;

use crate::wave_container::{ScopeRef, ScopeRefExt, VariableRef, VariableRefExt, WaveContainer};
use crate::{Message, system_state::SystemState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct FrameBufferSettings {
    pub pixels_per_row: usize,
    pub square_pixels: bool,
    #[serde(flatten)]
    pub color_settings: PixelColorSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PixelColorSettings {
    pub rgb_mode: bool,
    pub grayscale_bits: u8,
    pub r_bits: u8,
    pub g_bits: u8,
    pub b_bits: u8,
}

impl Default for PixelColorSettings {
    fn default() -> Self {
        Self {
            rgb_mode: false,
            grayscale_bits: 1,
            r_bits: 3,
            g_bits: 3,
            b_bits: 2,
        }
    }
}

impl Default for FrameBufferSettings {
    fn default() -> Self {
        Self {
            pixels_per_row: 16,
            square_pixels: true,
            color_settings: PixelColorSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArrayLevel {
    pub min_index: i64,
    pub max_index: i64,
    pub first_index: i64,
    pub last_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameBufferContentCacheKey {
    pub content: FrameBufferContent,
    pub cursor_position: num::BigUint,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameBufferArrayCache {
    pub key: FrameBufferContentCacheKey,
    pub cached_value: Option<(String, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameBufferPixelCacheKey {
    pub array_key: FrameBufferContentCacheKey,
    pub settings: PixelColorSettings,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameBufferPixelCache {
    pub key: FrameBufferPixelCacheKey,
    pub pixel_colors: Vec<Color32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameBufferContent {
    Array {
        scope_ref: ScopeRef,
        /// One range-selector per level of array nesting.
        /// The last level always applies to variables.
        levels: Vec<ArrayLevel>,
    },
    Variable(VariableRef),
}

impl SystemState {
    pub fn draw_frame_buffer_window(&mut self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Frame Buffer")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                let frame_buffer_value = self.selected_variable_for_frame_buffer();
                let Some((value, word_length, variable_name)) = frame_buffer_value.as_ref() else {
                    ui.label("Place the cursor.");
                    return;
                };

                let color_settings_key = {
                    let settings = &mut self.user.frame_buffer;
                    let color_settings = &mut settings.color_settings;

                    ui.checkbox(&mut settings.square_pixels, "Square pixels");
                    ui.checkbox(&mut color_settings.rgb_mode, "RGB mode");

                    if color_settings.rgb_mode {
                        ui.horizontal(|ui| {
                            ui.label("R bits");
                            ui.add(DragValue::new(&mut color_settings.r_bits).range(0..=8));
                            ui.label("G bits");
                            ui.add(DragValue::new(&mut color_settings.g_bits).range(0..=8));
                            ui.label("B bits");
                            ui.add(DragValue::new(&mut color_settings.b_bits).range(0..=8));
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Grayscale bits");
                            ui.add(DragValue::new(&mut color_settings.grayscale_bits).range(1..=8));
                        });
                    }

                    color_settings.clone()
                };

                ui.separator();

                let bits = frame_buffer_bits(value, *word_length as usize);
                if bits.is_empty() {
                    ui.label("No bits available");
                    return;
                }

                let Some(pixel_cache_key) =
                    self.current_frame_buffer_array_cache_key()
                        .map(|array_key| FrameBufferPixelCacheKey {
                            array_key,
                            settings: color_settings_key.clone(),
                        })
                else {
                    ui.label("Place the cursor.");
                    return;
                };

                let pixel_colors = if let Some(cache) = self
                    .frame_buffer_pixel_cache
                    .as_ref()
                    .filter(|cache| cache.key == pixel_cache_key)
                {
                    cache.pixel_colors.clone()
                } else {
                    let decoded = if color_settings_key.rgb_mode {
                        let r_bits = color_settings_key.r_bits as usize;
                        let g_bits = color_settings_key.g_bits as usize;
                        let b_bits = color_settings_key.b_bits as usize;
                        let bits_per_pixel = r_bits + g_bits + b_bits;
                        if bits_per_pixel == 0 {
                            ui.label("Set at least one RGB channel bit count above zero.");
                            return;
                        }
                        decode_rgb_pixels(&bits, r_bits, g_bits, b_bits)
                    } else {
                        let gray_bits = color_settings_key.grayscale_bits as usize;
                        decode_grayscale_pixels(&bits, gray_bits)
                    };

                    self.frame_buffer_pixel_cache = Some(FrameBufferPixelCache {
                        key: pixel_cache_key,
                        pixel_colors: decoded.clone(),
                    });
                    decoded
                };

                if pixel_colors.is_empty() {
                    ui.label("No pixels to draw with current bit settings.");
                    return;
                }

                let settings = &mut self.user.frame_buffer;
                let columns = settings.pixels_per_row.min(pixel_colors.len()).max(1);
                let rows = pixel_colors.len().div_ceil(columns);
                ui.horizontal(|ui| {
                    ui.label(format!("Var: {variable_name} | {columns}×{rows}"));

                    if ui.button("Copy image").clicked() {
                        let total = columns * rows;
                        let mut padded = pixel_colors.clone();
                        padded.resize(total, Color32::BLACK);
                        ui.ctx().copy_image(egui::ColorImage {
                            size: [columns, rows],
                            pixels: padded,
                            source_size: egui::vec2(columns as f32, rows as f32),
                        });
                    }
                });
                self.draw_array_index_range(ui);

                let settings = &mut self.user.frame_buffer;
                let max_columns = pixel_colors.len().max(1);
                settings.pixels_per_row = settings.pixels_per_row.clamp(1, max_columns);

                ui.horizontal(|ui| {
                    ui.label("Pixels in x-direction");
                    ui.add(
                        egui::Slider::new(&mut settings.pixels_per_row, 1..=max_columns).integer(),
                    );
                });

                ui.separator();

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

                    painter.rect_filled(Rect { min, max }, CornerRadius::ZERO, color);
                }

                painter.rect_stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, ui.visuals().weak_text_color()),
                    egui::StrokeKind::Inside,
                );
            });

        if !open {
            msgs.push(Message::SetFrameBufferVisibleVariable(None));
        }
    }

    fn draw_array_index_range(&mut self, ui: &mut egui::Ui) {
        let Some(FrameBufferContent::Array {
            scope_ref: _,
            levels,
        }) = self.frame_buffer_content.as_mut()
        else {
            return;
        };

        if levels.is_empty() {
            return;
        }

        let total_levels = levels.len();

        for (i, level) in levels.iter_mut().enumerate() {
            let (min, max) = (level.min_index, level.max_index);
            level.first_index = level.first_index.clamp(min, max);
            level.last_index = level.last_index.clamp(min, max);
            if level.first_index > level.last_index {
                level.last_index = level.first_index;
            }
            ui.horizontal(|ui| {
                if total_levels == 1 {
                    ui.label("First array index");
                } else {
                    ui.label(format!("Level {} first index", i + 1));
                }
                ui.add(DragValue::new(&mut level.first_index).range(min..=max));
                if total_levels == 1 {
                    ui.label("Last array index");
                } else {
                    ui.label(format!("Level {} last index", i + 1));
                }
                ui.add(DragValue::new(&mut level.last_index).range(min..=max));
            });
            if level.first_index > level.last_index {
                level.first_index = level.last_index;
            }
        }
    }

    fn selected_variable_for_frame_buffer(&mut self) -> Option<(VariableValue, u32, String)> {
        let waves = self.user.waves.as_ref()?;
        let cursor = waves.cursor.as_ref()?.to_biguint()?;
        let wave_container = waves.inner.as_waves()?;
        let content = self.frame_buffer_content.clone()?;
        let cache_key = FrameBufferContentCacheKey {
            content: content.clone(),
            cursor_position: cursor.clone(),
        };
        let cached = self
            .frame_buffer_array_cache
            .as_ref()
            .filter(|cache| cache.key == cache_key)
            .cloned();

        let cached = if let Some(cached) = cached {
            cached
        } else {
            let cached = match &content {
                FrameBufferContent::Variable(variable_ref) => build_variable_frame_buffer_cache(
                    wave_container,
                    variable_ref,
                    &cursor,
                    cache_key,
                )?,
                FrameBufferContent::Array { scope_ref, levels } => {
                    if levels.is_empty() {
                        return None;
                    }

                    let sorted_variables =
                        resolve_leaf_scopes_and_variables(wave_container, scope_ref, levels)?;
                    let cached_value =
                        build_cached_variable_value(wave_container, &sorted_variables, &cursor);
                    FrameBufferArrayCache {
                        key: cache_key,
                        cached_value,
                    }
                }
            };
            self.frame_buffer_array_cache = Some(cached.clone());
            cached
        };

        let (concat_bits, total_bits) = cached.cached_value.as_ref()?;

        let variable_name = match &content {
            FrameBufferContent::Variable(variable_ref) => variable_ref.full_path_string_no_index(),
            FrameBufferContent::Array { scope_ref, .. } => scope_ref.full_name(),
        };

        Some((
            VariableValue::String(concat_bits.clone()),
            *total_bits,
            variable_name,
        ))
    }

    fn current_frame_buffer_array_cache_key(&self) -> Option<FrameBufferContentCacheKey> {
        let waves = self.user.waves.as_ref()?;
        let cursor_position = waves.cursor.as_ref()?.to_biguint()?;
        let content = self.frame_buffer_content.clone()?;
        Some(FrameBufferContentCacheKey {
            content,
            cursor_position,
        })
    }
}

fn build_cached_variable_value(
    wave_container: &WaveContainer,
    sorted_variables: &[VariableRef],
    cursor: &num::BigUint,
) -> Option<(String, u32)> {
    let mut concat_bits = String::new();
    let mut total_bits: u32 = 0;
    for var_ref in sorted_variables {
        let meta = wave_container.variable_meta(var_ref).ok()?;
        let bits = meta.num_bits? as usize;
        total_bits += bits as u32;
        let query_result = wave_container
            .query_variable(var_ref, cursor)
            .ok()
            .flatten()?;
        let (_, value) = query_result.current?;
        let bit_str = match &value {
            VariableValue::BigUint(v) => format!("{v:b}"),
            VariableValue::String(s) => s.clone(),
        };
        let padded = if bit_str.len() < bits {
            format!("{bit_str:0>bits$}")
        } else {
            bit_str[bit_str.len() - bits..].to_string()
        };
        concat_bits.push_str(&padded);
    }
    if total_bits == 0 {
        None
    } else {
        Some((concat_bits, total_bits))
    }
}

fn build_variable_frame_buffer_cache(
    wave_container: &WaveContainer,
    variable_ref: &VariableRef,
    cursor: &num::BigUint,
    key: FrameBufferContentCacheKey,
) -> Option<FrameBufferArrayCache> {
    let meta = wave_container.variable_meta(variable_ref).ok()?;
    let word_length = meta.num_bits? as usize;
    let query_result = wave_container
        .query_variable(variable_ref, cursor)
        .ok()
        .flatten()?;
    let (_, value) = query_result.current?;
    let bits = match value {
        VariableValue::BigUint(v) => format!("{v:b}"),
        VariableValue::String(s) => s,
    };
    let padded = if bits.len() < word_length {
        format!("{bits:0>word_length$}")
    } else {
        bits[bits.len() - word_length..].to_string()
    };

    Some(FrameBufferArrayCache {
        key,
        cached_value: Some((padded, word_length as u32)),
    })
}

fn resolve_leaf_scopes_and_variables(
    wave_container: &WaveContainer,
    scope_ref: &ScopeRef,
    levels: &[ArrayLevel],
) -> Option<Vec<VariableRef>> {
    let (scope_levels, var_level) = levels.split_at(levels.len() - 1);
    let var_level = &var_level[0];

    let mut current_scopes = vec![scope_ref.clone()];
    for level in scope_levels {
        let clamped_first = level.first_index.clamp(level.min_index, level.max_index);
        let clamped_last = level.last_index.clamp(level.min_index, level.max_index);
        let mut next_scopes = Vec::new();
        for scope in &current_scopes {
            let mut selected: Vec<ScopeRef> = wave_container
                .child_scopes(scope)
                .unwrap_or_default()
                .into_iter()
                .filter(|s| {
                    let idx = scope_array_index(s);
                    idx >= clamped_first && idx <= clamped_last
                })
                .collect();
            selected.sort_by_key(scope_array_index);
            next_scopes.extend(selected);
        }
        current_scopes = next_scopes;
    }

    if current_scopes.is_empty() {
        return None;
    }

    let clamped_first = var_level
        .first_index
        .clamp(var_level.min_index, var_level.max_index);
    let clamped_last = var_level
        .last_index
        .clamp(var_level.min_index, var_level.max_index);
    if clamped_first > clamped_last {
        return None;
    }

    let mut sorted_variables = Vec::new();
    for leaf_scope in &current_scopes {
        let mut variables = wave_container.variables_in_scope(leaf_scope);
        variables.sort_by_key(variable_array_index);
        sorted_variables.extend(variables.into_iter().filter(|var_ref| {
            let idx = variable_array_index(var_ref);
            idx >= clamped_first && idx <= clamped_last
        }));
    }

    Some(sorted_variables)
}

/// Analyses the scope hierarchy rooted at `scope_ref` and returns:
/// - `levels`: one `ArrayLevel` per nesting level, where the last level is for variables
/// - `all_leaf_vars`: every variable reachable from the root (for pre-loading)
///
/// Returns `None` when `scope_ref` is not found in the hierarchy.
pub(crate) fn build_frame_buffer_content(
    wave_container: &WaveContainer,
    scope_ref: &ScopeRef,
) -> Option<(Vec<ArrayLevel>, Vec<VariableRef>)> {
    // Probe the hierarchy by following the min-index child at each level.
    // Stop when we reach a leaf scope that has no child scopes.
    let mut levels: Vec<ArrayLevel> = Vec::new();
    let mut probe = scope_ref.clone();
    loop {
        let children = wave_container.child_scopes(&probe).unwrap_or_default();
        if children.is_empty() {
            break;
        }
        let indices: Vec<i64> = children.iter().map(scope_array_index).collect();
        let min_idx = *indices.iter().min().unwrap_or(&0);
        let max_idx = *indices.iter().max().unwrap_or(&0);
        levels.push(ArrayLevel {
            min_index: min_idx,
            max_index: max_idx,
            first_index: min_idx,
            last_index: max_idx,
        });
        probe = children.into_iter().min_by_key(scope_array_index).unwrap();
    }

    // Determine the variable index range from the representative leaf scope.
    let leaf_vars = wave_container.variables_in_scope(&probe);
    let var_indices: Vec<i64> = leaf_vars
        .iter()
        .map(variable_array_index)
        .filter(|&i| i != i64::MAX)
        .collect();
    let (var_min, var_max) = if var_indices.is_empty() {
        (0, 0)
    } else {
        (
            *var_indices.iter().min().unwrap(),
            *var_indices.iter().max().unwrap(),
        )
    };
    levels.push(ArrayLevel {
        min_index: var_min,
        max_index: var_max,
        first_index: var_min,
        last_index: var_max,
    });

    // Walk every path to collect all leaf variables for pre-loading.
    let depth = levels.len().saturating_sub(1);
    let mut leaf_scopes = vec![scope_ref.clone()];
    for _ in 0..depth {
        leaf_scopes = leaf_scopes
            .iter()
            .flat_map(|s| wave_container.child_scopes(s).unwrap_or_default())
            .collect();
    }
    let all_leaf_vars: Vec<VariableRef> = leaf_scopes
        .iter()
        .flat_map(|s| wave_container.variables_in_scope(s))
        .collect();

    Some((levels, all_leaf_vars))
}

fn scope_array_index(scope_ref: &ScopeRef) -> i64 {
    let name = scope_ref.name();
    name.parse::<i64>()
        .ok()
        .or_else(|| {
            name.strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<i64>().ok())
        })
        .unwrap_or(i64::MAX)
}

fn variable_array_index(var_ref: &VariableRef) -> i64 {
    fn parse_index_name(name: &str) -> Option<i64> {
        name.parse::<i64>().ok().or_else(|| {
            name.strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<i64>().ok())
        })
    }

    var_ref
        .index
        .or_else(|| parse_index_name(&var_ref.name))
        .unwrap_or(i64::MAX)
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
    ((u32::from(value) * 255) / u32::from(max_in)) as u8
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

    #[test]
    fn variable_array_index_parses_bracketed_name() {
        let var_ref = VariableRef::new(ScopeRef::empty(), "[2]".to_string());
        assert_eq!(variable_array_index(&var_ref), 2);
    }

    #[test]
    fn variable_array_index_parses_plain_numeric_name() {
        let var_ref = VariableRef::new(ScopeRef::empty(), "7".to_string());
        assert_eq!(variable_array_index(&var_ref), 7);
    }

    #[test]
    fn variable_array_index_prefers_explicit_index() {
        let var_ref = VariableRef::new_with_id_and_index(
            ScopeRef::empty(),
            "[2]".to_string(),
            Default::default(),
            Some(9),
        );
        assert_eq!(variable_array_index(&var_ref), 9);
    }

    #[test]
    fn variable_array_index_falls_back_to_max_for_non_numeric_names() {
        let var_ref = VariableRef::new(ScopeRef::empty(), "data".to_string());
        assert_eq!(variable_array_index(&var_ref), i64::MAX);
    }
}
