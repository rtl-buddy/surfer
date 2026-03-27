use std::sync::Mutex;

use extism_pdk::{Error, FnResult, Json, plugin_fn};
pub use surfer_translation_types::plugin_types::TranslateParams;
use surfer_translation_types::{
    SubFieldTranslationResult, TranslationPreference, TranslationResult, ValueKind, VariableInfo,
    VariableMeta, VariableValue,
    translator::{TrueName, VariableNameInfo, WaveSource},
};

static WAVE_FILE_BYTES: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[plugin_fn]
pub fn new() -> FnResult<()> {
    Ok(())
}

#[plugin_fn]
pub fn name() -> FnResult<String> {
    Ok("Wasm Example Translator".to_string())
}

#[plugin_fn]
pub fn translate(
    TranslateParams { variable, value }: TranslateParams,
) -> FnResult<TranslationResult> {
    let wave_file_bytes = WAVE_FILE_BYTES.lock().unwrap();
    let wave_file_bytes = wave_file_bytes
        .as_ref()
        .ok_or_else(|| Error::msg("wave source file not loaded"))?;
    if wave_file_bytes.first() != Some(&b'$') {
        return Err(Error::msg("unexpected wave file contents").into());
    }

    let binary_digits = match value {
        VariableValue::BigUint(big_uint) => {
            let raw = format!("{big_uint:b}");
            let padding = (0..((variable.num_bits.unwrap_or_default() as usize)
                .saturating_sub(raw.len())))
                .map(|_| "0")
                .collect::<Vec<_>>()
                .join("");

            format!("{padding}{raw}")
        }
        VariableValue::String(v) => v.clone(),
    };

    let digits = binary_digits.chars().collect::<Vec<_>>();

    Ok(TranslationResult {
        val: surfer_translation_types::ValueRepr::Tuple,
        subfields: {
            digits
                .chunks(4)
                .enumerate()
                .map(|(i, chunk)| SubFieldTranslationResult {
                    name: format!("[{i}]"),
                    result: TranslationResult {
                        val: surfer_translation_types::ValueRepr::Bits(4, chunk.iter().collect()),
                        subfields: vec![],
                        kind: ValueKind::Normal,
                    },
                })
                .collect()
        },
        kind: ValueKind::Normal,
    })
}

#[plugin_fn]
pub fn variable_info(variable: VariableMeta<(), ()>) -> FnResult<VariableInfo> {
    Ok(VariableInfo::Compound {
        subfields: (0..(variable.num_bits.unwrap_or_default() / 4 + 1))
            .map(|i| (format!("[{i}]"), VariableInfo::Bits))
            .collect(),
    })
}

#[plugin_fn]
pub fn translates(_variable: VariableMeta<(), ()>) -> FnResult<TranslationPreference> {
    Ok(TranslationPreference::Yes)
}

#[plugin_fn]
pub fn set_wave_source(Json(wave_source): Json<Option<WaveSource>>) -> FnResult<()> {
    let wave_file_bytes = match wave_source {
        Some(WaveSource::File(path)) => Some(
            std::fs::read(&path)
                .map_err(|e| Error::msg(format!("failed to read wave source {path}: {e}")))?,
        ),
        _ => None,
    };

    *WAVE_FILE_BYTES.lock().unwrap() = wave_file_bytes;
    Ok(())
}

#[plugin_fn]
pub fn variable_name_info(
    Json(variable): Json<VariableMeta<(), ()>>,
) -> FnResult<Option<VariableNameInfo>> {
    let result = match variable.var.name.as_str() {
        "trace_data" => Some(VariableNameInfo {
            true_name: Some(TrueName::SourceCode {
                line_number: 1,
                before: "ab".to_string(),
                this: "cde".to_string(),
                after: "ef".to_string(),
            }),
            priority: Some(2),
        }),
        "trace_file" => Some(VariableNameInfo {
            true_name: Some(TrueName::SourceCode {
                line_number: 2,
                before: "this is a very long start of line".to_string(),
                this: "short".to_string(),
                after: "a".to_string(),
            }),
            priority: Some(0),
        }),
        "trace_valid" => Some(VariableNameInfo {
            true_name: Some(TrueName::SourceCode {
                line_number: 3,
                before: "a".to_string(),
                this: "trace_valid".to_string(),
                after: "this is a very long end of line".to_string(),
            }),
            priority: Some(0),
        }),
        "resetn" => Some(VariableNameInfo {
            true_name: Some(TrueName::SourceCode {
                line_number: 4,
                before: "this is a very long start of line".to_string(),
                this: "resetn".to_string(),
                after: "this is a very long end of line".to_string(),
            }),
            priority: Some(-1),
        }),
        "clk" => Some(VariableNameInfo {
            true_name: Some(TrueName::SourceCode {
                line_number: 555,
                before: "this is a very long start of line".to_string(),
                this: "clk is a very long signal name that stretches".to_string(),
                after: "this is a very long end of line".to_string(),
            }),
            priority: Some(0),
        }),
        _ => None,
    };
    Ok(result)
}
