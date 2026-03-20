use num::BigUint;
use num::ToPrimitive;
use serde_json::json;
use std::collections::BTreeSet;
use surfer_translation_types::{ValueKind, VariableInfo};

use crate::displayed_item::{DisplayedFieldRef, DisplayedItem, DisplayedVariable};
use crate::translation::{TranslationResultExt, TranslatorList};
use crate::wave_container::WaveContainer;
use crate::wave_data::WaveData;

/// Minimum number of WaveDrom cycles to ensure the diagram has reasonable width.
const MIN_CYCLES: usize = 16;

struct SignalSample {
    value_str: String,
    kind: ValueKind,
}

/// Intermediate representation of a WaveDrom signal before rendering.
struct WavedromSignal {
    name: String,
    wave_str: String,
    data_fields: Vec<String>,
}

/// Collect all transition timestamps across the selected signals within the viewport range,
/// then ensure at least `MIN_CYCLES` evenly-spaced clock ticks exist so the diagram is
/// never too narrow.
fn collect_time_points(
    waves: &WaveData,
    time_left: &BigUint,
    time_right: &BigUint,
) -> Vec<BigUint> {
    let wave_container = match waves.inner.as_waves() {
        Some(w) => w,
        None => return Vec::new(),
    };

    let mut transitions = BTreeSet::new();
    // Always include the viewport boundaries
    transitions.insert(time_left.clone());
    transitions.insert(time_right.clone());

    for node in waves.items_tree.iter_visible_selected() {
        let Some(DisplayedItem::Variable(displayed_variable)) =
            waves.displayed_items.get(&node.item_ref)
        else {
            continue;
        };

        let signal_id = match wave_container.signal_id(&displayed_variable.variable_ref) {
            Ok(id) => id,
            Err(_) => continue,
        };
        if !wave_container.is_signal_loaded(&signal_id) {
            continue;
        }

        let accessor = match wave_container.signal_accessor(signal_id) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for (time, _) in accessor.iter_changes() {
            let t = BigUint::from(time);
            if t > *time_right {
                break;
            }
            if t >= *time_left {
                transitions.insert(t);
            }
        }
    }

    // If we have fewer than MIN_CYCLES points, add evenly-spaced clock ticks
    if transitions.len() < MIN_CYCLES {
        let range = time_right - time_left;
        if let Some(range_u64) = range.to_u64() {
            let step = range_u64 / (MIN_CYCLES as u64);
            if step > 0 {
                for i in 0..=MIN_CYCLES {
                    let t = time_left + BigUint::from(i as u64 * step);
                    if t <= *time_right {
                        transitions.insert(t);
                    }
                }
            }
        }
    }

    transitions.into_iter().collect()
}

/// Sample a signal at a given timestamp and return its translated display value.
fn sample_signal(
    wave_container: &WaveContainer,
    displayed_variable: &DisplayedVariable,
    meta: &crate::wave_container::VariableMeta,
    translator: &crate::translation::DynTranslator,
    translators: &TranslatorList,
    time: &BigUint,
) -> Option<SignalSample> {
    let query_result = wave_container
        .query_variable(&displayed_variable.variable_ref, time)
        .ok()??;
    let (_, val) = query_result.current?;

    let translation_result = translator.translate(meta, &val).ok()?;
    // Get the root-level flat translation (empty field path)
    let fields = translation_result.format_flat(
        &displayed_variable.format,
        &displayed_variable.field_formats,
        translators,
    );
    let root_field = fields.iter().find(|f| f.names.is_empty())?;
    root_field.value.as_ref().map(|tv| SignalSample {
        value_str: tv.value.clone(),
        kind: tv.kind,
    })
}

/// Determine if a signal is boolean-like (single-bit: Bool, Clock, Event)
fn is_bool_like(info: &VariableInfo) -> bool {
    matches!(
        info,
        VariableInfo::Bool | VariableInfo::Clock | VariableInfo::Event
    )
}

/// Build a WaveDrom cycle character for a bool-like signal.
fn bool_wave_char(sample: &SignalSample, prev: Option<&SignalSample>) -> char {
    match sample.kind {
        ValueKind::Undef | ValueKind::Warn | ValueKind::Error => 'x',
        ValueKind::HighImp => 'z',
        _ => {
            let ch = match sample.value_str.as_str() {
                "1" => '1',
                "0" => '0',
                _ => 'x',
            };
            if let Some(prev) = prev
                && prev.value_str == sample.value_str && prev.kind == sample.kind
            {
                return '.';
            }
            ch
        }
    }
}

/// Build a WaveDrom cycle character for a vector/multi-bit signal.
/// Returns the character and optionally a data label to push.
fn vector_wave_char(
    sample: &SignalSample,
    prev: Option<&SignalSample>,
    color_idx: &mut usize,
) -> (char, Option<String>) {
    match sample.kind {
        ValueKind::Undef | ValueKind::Warn | ValueKind::Error => ('x', None),
        ValueKind::HighImp => ('z', None),
        _ => {
            if let Some(prev) = prev
                && prev.value_str == sample.value_str && prev.kind == sample.kind
            {
                return ('.', None);
            }
            // Cycle through colors 2-9
            let colors = ['2', '3', '4', '5', '6', '7', '8', '9'];
            let ch = colors[*color_idx % colors.len()];
            *color_idx += 1;
            (ch, Some(sample.value_str.clone()))
        }
    }
}

/// Build the intermediate WaveDrom signal representations from selected signals.
/// Includes a synthetic clock signal for timing reference and minimum diagram width.
fn build_wavedrom_signals(
    waves: &WaveData,
    translators: &TranslatorList,
    viewport_idx: usize,
) -> Option<Vec<WavedromSignal>> {
    let wave_container = waves.inner.as_waves()?;
    let num_timestamps = waves.safe_num_timestamps();
    let viewport = waves.viewports.get(viewport_idx)?;

    let time_left_bigint = viewport.left_edge_time(&num_timestamps);
    let time_right_bigint = viewport.right_edge_time(&num_timestamps);

    // Clamp to non-negative
    let time_left = time_left_bigint
        .to_biguint()
        .unwrap_or_else(|| BigUint::from(0u32));
    let time_right = time_right_bigint
        .to_biguint()
        .unwrap_or_else(|| BigUint::from(0u32));

    if time_left >= time_right {
        return None;
    }

    let time_points = collect_time_points(waves, &time_left, &time_right);

    if time_points.is_empty() {
        return None;
    }

    let mut result = Vec::new();

    for node in waves.items_tree.iter_visible_selected() {
        let Some(DisplayedItem::Variable(displayed_variable)) =
            waves.displayed_items.get(&node.item_ref)
        else {
            continue;
        };

        let signal_id = match wave_container.signal_id(&displayed_variable.variable_ref) {
            Ok(id) => id,
            Err(_) => continue,
        };
        if !wave_container.is_signal_loaded(&signal_id) {
            continue;
        }

        let meta = match wave_container.variable_meta(&displayed_variable.variable_ref) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let displayed_field_ref: DisplayedFieldRef = node.item_ref.into();
        let translator =
            waves.variable_translator_with_meta(&displayed_field_ref, translators, &meta);
        let info = match translator.variable_info(&meta) {
            Ok(i) => i,
            Err(_) => continue,
        };

        let is_bool = is_bool_like(&info);
        let mut wave_str = String::new();
        let mut data_fields: Vec<String> = Vec::new();
        let mut prev_sample: Option<SignalSample> = None;
        let mut color_idx: usize = 0;

        for time in &time_points {
            let sample = sample_signal(
                wave_container,
                displayed_variable,
                &meta,
                translator,
                translators,
                time,
            );

            match sample {
                Some(s) => {
                    if is_bool {
                        wave_str.push(bool_wave_char(&s, prev_sample.as_ref()));
                    } else {
                        let (ch, data) = vector_wave_char(&s, prev_sample.as_ref(), &mut color_idx);
                        wave_str.push(ch);
                        if let Some(label) = data {
                            data_fields.push(label);
                        }
                    }
                    prev_sample = Some(s);
                }
                None => {
                    wave_str.push('x');
                    prev_sample = None;
                }
            }
        }

        let name = displayed_variable
            .manual_name
            .as_deref()
            .unwrap_or(&displayed_variable.display_name);

        result.push(WavedromSignal {
            name: name.to_string(),
            wave_str,
            data_fields,
        });
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Generate a WaveDrom JSON string from the selected signals in the given viewport.
/// This produces the standard WaveJSON format that can be pasted into wavedrom.com/editor.
pub fn generate_wavedrom_json(
    waves: &WaveData,
    translators: &TranslatorList,
    viewport_idx: usize,
) -> Option<String> {
    let wd_signals = build_wavedrom_signals(waves, translators, viewport_idx)?;

    let signal_array: Vec<serde_json::Value> = wd_signals
        .into_iter()
        .map(|s| {
            let mut obj = json!({
                "name": s.name,
                "wave": s.wave_str,
            });
            if !s.data_fields.is_empty() {
                obj["data"] = json!(s.data_fields);
            }
            obj
        })
        .collect();

    let wavedrom_json = json!({ "signal": signal_array });
    serde_json::to_string_pretty(&wavedrom_json).ok()
}
