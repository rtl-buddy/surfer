use camino::Utf8Path;
use ecolor::Color32;
use num::BigUint;
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use surfer_translation_types::{
    BasicTranslator, TranslationPreference, ValueKind, VariableValue, extend_string,
    kind_for_binary_representation,
};
use thiserror::Error;

use crate::{
    translation::check_single_wordlength,
    wave_container::{ScopeId, VarId, VariableMeta},
};

static KIND_COLOR_KEYWORDS: OnceLock<HashMap<&'static str, ValueKind>> = OnceLock::new();

fn kind_color_keywords() -> &'static HashMap<&'static str, ValueKind> {
    KIND_COLOR_KEYWORDS.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("normal", ValueKind::Normal);
        m.insert("default", ValueKind::Normal);
        m.insert("undef", ValueKind::Undef);
        m.insert("highimp", ValueKind::HighImp);
        m.insert("warn", ValueKind::Warn);
        m.insert("dontcare", ValueKind::DontCare);
        m.insert("weak", ValueKind::Weak);
        m.insert("error", ValueKind::Error);
        m.insert("black", ValueKind::Custom(Color32::BLACK));
        m.insert("white", ValueKind::Custom(Color32::WHITE));
        m.insert("red", ValueKind::Custom(Color32::RED));
        m.insert("green", ValueKind::Custom(Color32::GREEN));
        m.insert("blue", ValueKind::Custom(Color32::BLUE));
        m.insert("yellow", ValueKind::Custom(Color32::YELLOW));
        m.insert("cyan", ValueKind::Custom(Color32::CYAN));
        m.insert("magenta", ValueKind::Custom(Color32::MAGENTA));
        m.insert("gray", ValueKind::Custom(Color32::GRAY));
        m.insert("grey", ValueKind::Custom(Color32::GRAY));
        m.insert("light_gray", ValueKind::Custom(Color32::LIGHT_GRAY));
        m.insert("light_grey", ValueKind::Custom(Color32::LIGHT_GRAY));
        m.insert("dark_gray", ValueKind::Custom(Color32::DARK_GRAY));
        m.insert("dark_grey", ValueKind::Custom(Color32::DARK_GRAY));
        m.insert("brown", ValueKind::Custom(Color32::BROWN));
        m.insert("dark_red", ValueKind::Custom(Color32::DARK_RED));
        m.insert("light_red", ValueKind::Custom(Color32::LIGHT_RED));
        m.insert("orange", ValueKind::Custom(Color32::ORANGE));
        m.insert("light_yellow", ValueKind::Custom(Color32::LIGHT_YELLOW));
        m.insert("khaki", ValueKind::Custom(Color32::KHAKI));
        m.insert("dark_green", ValueKind::Custom(Color32::DARK_GREEN));
        m.insert("light_green", ValueKind::Custom(Color32::LIGHT_GREEN));
        m.insert("dark_blue", ValueKind::Custom(Color32::DARK_BLUE));
        m.insert("light_blue", ValueKind::Custom(Color32::LIGHT_BLUE));
        m.insert("purple", ValueKind::Custom(Color32::PURPLE));
        m.insert("gold", ValueKind::Custom(Color32::GOLD));
        m
    })
}

#[derive(Debug, Clone, PartialEq)]
struct MappingEntry {
    label: String,
    kind: ValueKind,
}

#[derive(Debug, Clone)]
/// A mapping of variable values to their string entries.
struct MappingTranslatorMap {
    name: String,
    bits: u32,
    entries: HashMap<VariableValue, MappingEntry>,
}

pub struct MappingTranslator {
    map: MappingTranslatorMap,
}

#[derive(Debug, Error)]
pub enum MappingParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid bits value: {0}")]
    InvalidBitsValue(String),

    #[error("Invalid hex number: {0}")]
    InvalidHex(String),

    #[error("Invalid octal number: {0}")]
    InvalidOctal(String),

    #[error("Unknown kind/color: {0}")]
    UnknownKindColor(String),

    #[error(
        "Binary string '{value}' requires {required} bits, but only {specified} bits specified"
    )]
    BinaryTooWide {
        value: String,
        required: u32,
        specified: u32,
    },

    #[error(
        "String '{value}' has {value_len} characters, expected {expected} characters to match bit width"
    )]
    StringLengthMismatch {
        value: String,
        value_len: u32,
        expected: u32,
    },

    #[error(
        "String '{value}' contains invalid characters. Only 01zx-hlwu are allowed. Maybe you are missing a 0b prefix for a binary string?"
    )]
    InvalidStringCharacters { value: String },

    #[error("Missing mapping")]
    MissingMapping,

    #[error("Empty line")]
    EmptyLine,

    #[error("Line {line}: {message}\n  Content: {content}")]
    LineError {
        line: usize,
        content: String,
        message: String,
    },
}

impl MappingTranslator {
    pub fn new_from_file<P: AsRef<Utf8Path>>(path: P) -> Result<Self, MappingParseError> {
        Ok(MappingTranslator {
            map: MappingTranslatorMap::new(path)?,
        })
    }

    pub fn bits(&self) -> u32 {
        self.map.bits
    }
}

impl BasicTranslator<VarId, ScopeId> for MappingTranslator {
    fn name(&self) -> String {
        self.map.name.clone()
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        // Extend the value if it's a string to match num_bits
        let lookup_value = match value {
            VariableValue::BigUint(_) => value.clone(),
            VariableValue::String(s) => {
                let extended = format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits));
                VariableValue::String(extended)
            }
        };

        if let Some(entry) = self.map.entries.get(&lookup_value) {
            return (entry.label.clone(), entry.kind);
        }

        let var_string = key_display(&lookup_value, num_bits);

        let val_kind = kind_for_binary_representation(&var_string);
        (var_string, val_kind)
    }

    fn translates(&self, variable: &VariableMeta) -> eyre::Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, self.map.bits)
    }
}

impl MappingTranslatorMap {
    pub fn new<P: AsRef<Utf8Path>>(file: P) -> Result<Self, MappingParseError> {
        parse_file(file)
    }
}

/// Parse a file and return a MappingTranslatorMap.
fn parse_file<P: AsRef<Utf8Path>>(path: P) -> Result<MappingTranslatorMap, MappingParseError> {
    let utf = path.as_ref();
    let content = fs::read_to_string(utf.as_std_path())?;

    // Extract filename (without extension) for default name
    let default_name = utf.file_stem().map(|s| s.to_string());

    parse_content_with_default_name(&content, default_name)
}

/// Parse content of file and return a MappingTranslatorMap, falling back to default_name if no name is found in the file.
fn parse_content_with_default_name(
    content: &str,
    default_name: Option<String>,
) -> Result<MappingTranslatorMap, MappingParseError> {
    let mut name = None;
    let mut bits = None;
    let mut raw_entries = Vec::new();

    // Process all lines and determine a temporary list of entries
    for (line_num, line_str) in content.lines().enumerate() {
        let processed = line_str.trim();

        // Skip empty lines and comments (only lines starting with '#')
        if processed.is_empty() || processed.starts_with('#') {
            continue;
        }

        // Check for Name line if we haven't found one yet
        if let Some(name_value) = parse_specifier(processed, "name") {
            if name.is_some() {
                return Err(MappingParseError::LineError {
                    line: line_num + 1,
                    content: line_str.to_string(),
                    message: "Multiple Name specifiers found".to_string(),
                });
            }
            name = Some(name_value);
            continue;
        }

        // Check for Bits line if we haven't found one yet
        if let Some(bits_value) = parse_specifier(processed, "bits") {
            if bits.is_some() {
                return Err(MappingParseError::LineError {
                    line: line_num + 1,
                    content: line_str.to_string(),
                    message: "Multiple Bits specifiers found".to_string(),
                });
            }
            let bits_num = bits_value
                .parse::<u32>()
                .map_err(|_| MappingParseError::InvalidBitsValue(bits_value.to_string()))?;
            bits = Some(bits_num);
            continue;
        }

        match parse_line(processed) {
            Ok(entry) => raw_entries.push(entry),
            Err(e) => {
                return Err(MappingParseError::LineError {
                    line: line_num + 1,
                    content: line_str.to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Determine bit width if not provided
    let bit_width = bits.unwrap_or_else(|| {
        // Find the largest value length among entries
        raw_entries.iter().map(|entry| entry.3).max().unwrap_or(0)
    });

    // Validate and normalize all entries, building HashMap
    let mut entries = HashMap::new();
    for (value, label, kind, value_len) in raw_entries {
        let key = normalize_first_column(&value, value_len, bit_width)?;

        if entries.contains_key(&key) {
            tracing::warn!(
                "Duplicate mapping key '{}' encountered; keeping first occurrence",
                key_display(&key, bit_width)
            );
            continue;
        }
        entries.insert(key, MappingEntry { label, kind });
    }

    // Use default_name if no name was found in the file
    let final_name = name
        .or(default_name)
        .unwrap_or_else(|| "Unknown mapping".to_string());

    Ok(MappingTranslatorMap {
        name: final_name,
        bits: bit_width,
        entries,
    })
}

/// Normalize the first column value according to its type and specified bit width.
///
/// - Check that numeric values fit within the specified bit width.
/// - Check that strings fit within the specified bit width and extend them if they are shorter.
/// - Convert string values that are binary strings to numeric values.
fn normalize_first_column(
    value: &VariableValue,
    value_len: u32,
    bit_width: u32,
) -> Result<VariableValue, MappingParseError> {
    match value {
        VariableValue::BigUint(v) => {
            if value_len > bit_width {
                return Err(MappingParseError::BinaryTooWide {
                    value: format!("{v:0width$b}", width = value_len as usize),
                    required: value_len,
                    specified: bit_width,
                });
            }
            Ok(VariableValue::BigUint(v.clone()))
        }
        VariableValue::String(s) => {
            // Check if the first column contains only '0' and '1' (binary string)
            if s.chars().all(|c| c == '0' || c == '1') {
                if value_len > bit_width {
                    return Err(MappingParseError::BinaryTooWide {
                        value: s.to_string(),
                        required: value_len,
                        specified: bit_width,
                    });
                }

                let value = BigUint::parse_bytes(s.as_bytes(), 2)
                    .expect("binary string should parse as BigUint");
                Ok(VariableValue::BigUint(value))
            } else {
                // Check if larger than specified bit width
                if s.len() > bit_width as usize {
                    return Err(MappingParseError::StringLengthMismatch {
                        value: s.to_string(),
                        value_len: s.len() as u32,
                        expected: bit_width,
                    });
                }

                let extended = format!(
                    "{extra}{body}",
                    extra = extend_string(s, bit_width),
                    body = s
                );
                Ok(VariableValue::String(extended))
            }
        }
    }
}

#[inline]
fn key_display(key: &VariableValue, bit_width: u32) -> String {
    match key {
        VariableValue::String(s) => s.clone(),
        VariableValue::BigUint(v) => format!("{v:0width$b}", width = bit_width as usize),
    }
}

/// Parse a line and extract variable value, label, value kind, and length of the value.
fn parse_line(line: &str) -> Result<(VariableValue, String, ValueKind, u32), MappingParseError> {
    let mut chars = line.char_indices().peekable();
    while let Some((_, ch)) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }

    let start = if let Some((idx, _)) = chars.peek().copied() {
        idx
    } else {
        return Err(MappingParseError::EmptyLine);
    };

    let mut first_end = line.len();
    for (idx, ch) in chars {
        if ch.is_whitespace() {
            first_end = idx;
            break;
        }
    }

    let first_token = &line[start..first_end];
    let remainder = line[first_end..].trim_start();

    if first_token.is_empty() {
        return Err(MappingParseError::EmptyLine);
    }

    let (first, kind, first_len) = parse_key_with_kind(first_token)?;

    if remainder.is_empty() {
        return Err(MappingParseError::MissingMapping);
    }

    Ok((first, remainder.to_string(), kind, first_len))
}

/// Parse the value part of the first item into a [`VariableValue`]` and its bit width.
///
/// For binary strings and nine-value strings, the bit width is determined by the string length.
/// For hex, octal, and decimal numbers, the bit width is determined by the number of bits required to represent the actual numeric value.
fn parse_key_value(token: &str) -> Result<(VariableValue, u32), MappingParseError> {
    // Support underscore separators
    let cleaned = token.replace('_', "");

    // Binary prefix (0b or 0B)
    if cleaned.starts_with("0b") || cleaned.starts_with("0B") {
        let bin_str = &cleaned[2..];
        if let Some(val) = BigUint::parse_bytes(bin_str.as_bytes(), 2) {
            // Preserve the binary string length (e.g., 0b0101 => 4 bits, not 3)
            let bits = bin_str.len() as u32;
            return Ok((VariableValue::BigUint(val), bits));
        }
        // String literal
        let lower = bin_str.to_lowercase();

        // Validate that string contains only valid characters: 01zx-hlwu
        if lower
            .chars()
            .all(|c| matches!(c, '0' | '1' | 'z' | 'x' | '-' | 'h' | 'l' | 'w' | 'u'))
        {
            return Ok((VariableValue::String(lower.to_string()), lower.len() as u32));
        }

        return Err(MappingParseError::InvalidBitsValue(token.to_string()));
    }

    // Octal prefix (0o or 0O)
    if cleaned.starts_with("0o") || cleaned.starts_with("0O") {
        let oct_str = &cleaned[2..];
        let num = BigUint::parse_bytes(oct_str.as_bytes(), 8)
            .ok_or_else(|| MappingParseError::InvalidOctal(token.to_string()))?;
        // Actual number of bits required to represent the octal number
        // We do not want a multiple of 3 here, but the actual bit width
        let bits = num.bits() as u32;
        return Ok((VariableValue::BigUint(num), bits));
    }

    // Hex prefix (0x or 0X)
    if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        let hex_str = &cleaned[2..];
        let num = BigUint::parse_bytes(hex_str.as_bytes(), 16)
            .ok_or_else(|| MappingParseError::InvalidHex(token.to_string()))?;
        // Actual number of bits required to represent the hex number
        // We do not want a multiple of 4 here, but the actual bit width
        let bits = num.bits() as u32;
        return Ok((VariableValue::BigUint(num), bits));
    }

    // Decimal (default for numeric strings)
    if let Some(num) = BigUint::parse_bytes(cleaned.as_bytes(), 10) {
        let bits = num.bits() as u32;
        return Ok((VariableValue::BigUint(num), bits));
    }

    Err(MappingParseError::InvalidStringCharacters {
        value: token.to_string(),
    })
}

/// Parse the first item into a [`VariableValue`], a [`ValueKind`], and its bit width.
fn parse_key_with_kind(token: &str) -> Result<(VariableValue, ValueKind, u32), MappingParseError> {
    // Check if token contains [kind] notation: value[kind]
    if let Some(bracket_pos) = token.find('[')
        && let Some(close_bracket) = token.find(']')
        && close_bracket > bracket_pos
    {
        let value_part = &token[..bracket_pos];
        let kind_part = &token[bracket_pos + 1..close_bracket];

        let (value, len) = parse_key_value(value_part)?;
        let kind = parse_color_kind(kind_part)?;

        return Ok((value, kind, len));
    }

    // No [kind] notation, default to Normal
    let (value, len) = parse_key_value(token)?;
    Ok((value, ValueKind::Normal, len))
}

/// Parse a color or value kind from a token string.
///
/// This function supports:
/// - Hex color codes in the format #RRGGBB or RRGGBB
/// - Named colors from [`ecolor::Color32`]
/// - Value kinds from [`surfer_translation_types::ValueKind`]
fn parse_color_kind(token: &str) -> Result<ValueKind, MappingParseError> {
    // Try hex color (#RRGGBB or RRGGBB)
    let hex_str = token.strip_prefix('#').unwrap_or(token);

    if hex_str.len() == 6 && hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex_str[0..2], 16)
            .map_err(|_| MappingParseError::InvalidHex(token.to_string()))?;
        let g = u8::from_str_radix(&hex_str[2..4], 16)
            .map_err(|_| MappingParseError::InvalidHex(token.to_string()))?;
        let b = u8::from_str_radix(&hex_str[4..6], 16)
            .map_err(|_| MappingParseError::InvalidHex(token.to_string()))?;
        return Ok(ValueKind::Custom(Color32::from_rgb(r, g, b)));
    }

    // Try value kinds and ecolor::Color32 named colors using lookup table
    let lower = token.to_lowercase();
    kind_color_keywords()
        .get(lower.as_str())
        .copied()
        .ok_or_else(|| MappingParseError::UnknownKindColor(token.to_string()))
}

// Check if line starts with keyword (case-insensitive) and parse value after '='
fn parse_specifier(line: &str, keyword: &str) -> Option<String> {
    // Check if line starts with keyword (case-insensitive)
    if !(line.len() >= keyword.len() && line[..keyword.len()].eq_ignore_ascii_case(keyword)) {
        return None;
    }

    let after_keyword = &line[keyword.len()..];

    // Skip optional whitespace and look for '='
    if let Some(rest) = after_keyword.trim_start().strip_prefix('=') {
        // Found valid separator, extract and return the value
        let value = rest.trim().to_string();
        return Some(value);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use num::BigUint;

    fn color_eq(a: Color32, b: Color32) -> bool {
        a.r() == b.r() && a.g() == b.g() && a.b() == b.b() && a.a() == b.a()
    }

    #[test]
    fn parse_empty_content_uses_default_name_and_no_bits() {
        let map = parse_content_with_default_name("", Some("DefaultName".to_string())).unwrap();
        assert_eq!(map.name, "DefaultName".to_string());
        assert_eq!(map.bits, 0); // empty content => no bits
        assert!(map.entries.is_empty());
    }

    #[test]
    fn parse_with_name_bits_and_entries_binary_and_hex_and_colors() {
        let content = "Name = MyMap\nBits = 4\n0[red] ZERO\n1[#00FF00] ONE\n0xA[blue] TEN";
        let map = parse_content_with_default_name(content, None).unwrap();
        assert_eq!(map.name, "MyMap".to_string());
        assert_eq!(map.bits, 4);
        // Expect numeric keys stored as BigUint
        let zero = map
            .entries
            .get(&VariableValue::BigUint(BigUint::from(0u32)))
            .expect("ZERO entry");
        assert_eq!(zero.label, "ZERO");
        assert_eq!(zero.kind, ValueKind::Custom(Color32::RED));
        let one = map
            .entries
            .get(&VariableValue::BigUint(BigUint::from(1u32)))
            .expect("ONE entry");
        assert_eq!(one.label, "ONE");
        // #00FF00 => green
        assert_eq!(one.kind, ValueKind::Custom(Color32::GREEN));
        let ten = map
            .entries
            .get(&VariableValue::BigUint(BigUint::from(10u32)))
            .expect("TEN entry");
        assert_eq!(ten.label, "TEN");
        assert_eq!(ten.kind, ValueKind::Custom(Color32::BLUE));
    }

    #[test]
    fn duplicate_keys_keep_first_and_log() {
        let content = "Bits = 4\n0[red] ZERO\n0[blue] ZERO_DUP\n1[green] ONE";
        let map = parse_content_with_default_name(content, None).unwrap();
        // Only one entry for key 0000
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(0u32)))
                .unwrap()
                .label,
            "ZERO"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(1u32)))
                .unwrap()
                .label,
            "ONE"
        );
        assert_eq!(map.entries.len(), 2);
    }

    #[test]
    fn infer_bit_width_from_longest_entry() {
        // Longest entry after normalization should determine width
        let content = "3 THREE\n15 FIFTEEN"; // 3 => 11, 15 => 1111 so width=4
        let map = parse_content_with_default_name(content, Some("Numbers".to_string())).unwrap();
        assert_eq!(map.bits, 4);
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(3u32)))
        );
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(15u32)))
        );
    }

    #[test]
    fn error_on_mismatched_string_length() {
        // Bits = 3 but entry has 4 chars in first column
        let content = "Bits =  3\n0bxxuu LABEL";
        let err = parse_content_with_default_name(content, None).unwrap_err();
        match err {
            MappingParseError::StringLengthMismatch { expected, .. } => {
                assert_eq!(expected, 3)
            }
            other => panic!("Unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_line_allows_unquoted_label_with_spaces() {
        let line = "0b0101 Label With Spaces";
        let (val, label, kind, len) = parse_line(line).expect("parse line");
        assert_eq!(label, "Label With Spaces");
        assert_eq!(len, 4);
        assert!(matches!(kind, ValueKind::Normal));
        match val {
            VariableValue::BigUint(v) => assert_eq!(v, BigUint::from(0b0101u32)),
            other => panic!("unexpected value parsed: {other:?}"),
        }
    }

    #[test]
    fn parse_hex_and_decimal_numbers() {
        let content = "Bits =  5\n0x1F[blue] HEXVAL\n7[green] DECVAL"; // 0x1F => 11111, 7 => 00111
        let map = parse_content_with_default_name(content, None).unwrap();
        assert_eq!(map.bits, 5);
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(31u32)))
        );
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(7u32)))
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(31u32)))
                .unwrap()
                .label,
            "HEXVAL"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(7u32)))
                .unwrap()
                .label,
            "DECVAL"
        );
    }

    #[test]
    fn mix_binary_and_decimal_without_bits_line_infers_width() {
        // Binary token with 0b prefix plus decimal numbers; longest normalized width should be 4
        let content = "0b0101 BINLABEL\n7 DECSEVEN\n13 DECTHIRTEEN"; // 0b0101 => 0101, 7 => 111, 13 => 1101
        let map = parse_content_with_default_name(content, Some("Mixed".to_string())).unwrap();
        assert_eq!(map.name, "Mixed".to_string());
        assert_eq!(map.bits, 4);
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(5u32)))
        ); // binary preserved
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(7u32)))
        ); // 7 padded to 4 bits
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(13u32)))
        ); // 13 already 4 bits
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(5u32)))
                .unwrap()
                .label,
            "BINLABEL"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(7u32)))
                .unwrap()
                .label,
            "DECSEVEN"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(13u32)))
                .unwrap()
                .label,
            "DECTHIRTEEN"
        );
    }

    #[test]
    fn parse_binary_octal_hex_decimal_prefixes() {
        // Test all number formats: 0b (binary), 0o (octal), 0x (hex), and decimal
        let content =
            "Bits =  8\n0b1111[red] BINARY\n0o17[green] OCTAL\n0xFF[blue] HEX\n255[yellow] DECIMAL";
        let map = parse_content_with_default_name(content, None).unwrap();
        assert_eq!(map.bits, 8);
        // 0b1111 => 00001111
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(15u32)))
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(15u32)))
                .unwrap()
                .label,
            "BINARY"
        );
        // 0o17 => 15 decimal => 00001111 (same as 0b1111, duplicate so keeps first)
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(15u32)))
                .unwrap()
                .label,
            "BINARY"
        );
        // 0xFF => 255 decimal => 11111111
        assert!(
            map.entries
                .contains_key(&VariableValue::BigUint(BigUint::from(255u32)))
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(255u32)))
                .unwrap()
                .label,
            "HEX"
        );
        // 255 decimal => 11111111 (same as 0xFF, duplicate so keeps first)
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(255u32)))
                .unwrap()
                .label,
            "HEX"
        );
    }

    #[test]
    fn file_based_translator_basic_translate_and_fallback() {
        use std::time::{SystemTime, UNIX_EPOCH};
        // Unique temp file path
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut pathbuf = std::env::temp_dir();
        pathbuf.push(format!("mapping_test_{}.txt", ts));
        let path = Utf8PathBuf::from_path_buf(pathbuf).expect("temp path must be UTF-8");

        let content = "Name = ExampleMapping\nBits =  4\n0[red] ZERO\n1[green] ONE\n2[blue] TWO";
        std::fs::write(path.as_std_path(), content).expect("write mapping file");

        let translator = MappingTranslator::new_from_file(&path)
            .ok()
            .expect("create translator");
        assert_eq!(translator.name(), "ExampleMapping");

        // Match padded binary for string value
        let (label_one, kind_one) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(1u32)));
        assert_eq!(label_one, "ONE");
        match kind_one {
            ValueKind::Custom(c) => assert!(color_eq(c, Color32::GREEN)),
            _ => panic!("expected custom green"),
        }

        // Integer value translation (BigUint) should pad and map to label TWO
        let (label_two, kind_two) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(2u32)));
        assert_eq!(label_two, "TWO");
        match kind_two {
            ValueKind::Custom(c) => assert!(color_eq(c, Color32::BLUE)),
            _ => panic!("expected custom blue"),
        }

        // Fallback for value not in map (all-defined binary -> Normal)
        let (label_unknown, kind_unknown) =
            translator.basic_translate(4, &VariableValue::String("0011".into()));
        assert_eq!(label_unknown, "0011");
        assert!(matches!(kind_unknown, ValueKind::Normal));

        // Value with x should return Error
        let (label_undef, kind_undef) =
            translator.basic_translate(4, &VariableValue::String("00x1".into()));
        assert_eq!(label_undef, "00x1");
        assert!(matches!(kind_undef, ValueKind::Undef));

        // Value with u char should return Undef
        let (label_undef, kind_undef) =
            translator.basic_translate(4, &VariableValue::String("00u1".into()));
        assert_eq!(label_undef, "00u1");
        assert!(matches!(kind_undef, ValueKind::Undef));
    }

    #[test]
    fn filename_derived_name_when_no_name_line_present() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();
        let stem = format!("auto_file_name_{}", ts);
        let mut pathbuf = std::env::temp_dir();
        pathbuf.push(format!("{stem}.mapping"));

        // No Name line, first line is Bits so default name should be file stem
        let content = "Bits=3\n0[red] ZERO\n1[green] ONE\n2[blue] TWO";
        let path = Utf8PathBuf::from_path_buf(pathbuf).expect("temp path must be UTF-8");
        std::fs::write(path.as_std_path(), content).expect("write mapping file");
        let translator = MappingTranslator::new_from_file(&path)
            .ok()
            .expect("create translator");
        assert_eq!(translator.name(), stem); // derived from filename
        assert_eq!(translator.map.bits, 3);
        // Translate a BigUint value 2 => binary 10 padded to 3 bits -> 010 maps to TWO
        let (label_two, kind_two) =
            translator.basic_translate(3, &VariableValue::BigUint(BigUint::from(2u32)));
        assert_eq!(label_two, "TWO");
        match kind_two {
            ValueKind::Custom(c) => assert!(color_eq(c, Color32::BLUE)),
            _ => panic!("expected custom blue"),
        }

        // Value not present (binary 111) should fallback to Normal
        let (fallback, vk_fb) = translator.basic_translate(3, &VariableValue::String("111".into()));
        assert_eq!(fallback, "111");
        assert!(matches!(vk_fb, ValueKind::Normal));

        // Value with z should be HighImp
        let (high_imp_val, high_imp_kind) =
            translator.basic_translate(3, &VariableValue::String("1z1".into()));
        assert_eq!(high_imp_val, "1z1");
        assert!(matches!(high_imp_kind, ValueKind::HighImp));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let content = "# top comment\n   # another comment\n\nName=Commented\n# mid\nBits =  3\n# entry comment\n0[red] ZERO\n# Hash comment\n\n1[green] ONE\n   # trailing comment line\n2[blue] TWO";
        let map = parse_content_with_default_name(content, None).unwrap();
        assert_eq!(map.name, "Commented".to_string());
        assert_eq!(map.bits, 3);
        assert_eq!(map.entries.len(), 3);
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(0u32)))
                .unwrap()
                .label,
            "ZERO"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(1u32)))
                .unwrap()
                .label,
            "ONE"
        );
        assert_eq!(
            map.entries
                .get(&VariableValue::BigUint(BigUint::from(2u32)))
                .unwrap()
                .label,
            "TWO"
        );
    }

    #[test]
    fn parse_and_translate_valuekind_keywords() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut pathbuf = std::env::temp_dir();
        pathbuf.push(format!("mapping_kinds_{}.txt", ts));
        let path = Utf8PathBuf::from_path_buf(pathbuf).expect("temp path must be UTF-8");

        // Define entries with various ValueKind keywords
        let content = "Bits =  4\n0[warn] ZERO\n1[undef] ONE\n2[highimp] TWO\n3[dontcare] THREE\n4[weak] FOUR\n5[error] FIVE\n6[normal] SIX";
        std::fs::write(path.as_std_path(), content).expect("write mapping file");

        let translator = MappingTranslator::new_from_file(&path)
            .ok()
            .expect("create translator");
        assert_eq!(translator.map.bits, 4);

        // Test each ValueKind keyword
        let (label_zero, kind_zero) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(0u32)));
        assert_eq!(label_zero, "ZERO");
        assert!(matches!(kind_zero, ValueKind::Warn));

        let (label_one, kind_one) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(1u32)));
        assert_eq!(label_one, "ONE");
        assert!(matches!(kind_one, ValueKind::Undef));

        let (label_two, kind_two) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(2u32)));
        assert_eq!(label_two, "TWO");
        assert!(matches!(kind_two, ValueKind::HighImp));

        let (label_three, kind_three) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(3u32)));
        assert_eq!(label_three, "THREE");
        assert!(matches!(kind_three, ValueKind::DontCare));

        let (label_four, kind_four) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(4u32)));
        assert_eq!(label_four, "FOUR");
        assert!(matches!(kind_four, ValueKind::Weak));

        let (label_five, kind_five) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(5u32)));
        assert_eq!(label_five, "FIVE");
        assert!(matches!(kind_five, ValueKind::Error));

        let (label_six, kind_six) =
            translator.basic_translate(4, &VariableValue::BigUint(BigUint::from(6u32)));
        assert_eq!(label_six, "SIX");
        assert!(matches!(kind_six, ValueKind::Normal));
    }

    #[test]
    fn error_variants_coverage() {
        // InvalidHex via parse_first_column
        match parse_key_value("0xZZ") {
            Err(MappingParseError::InvalidHex(tok)) => assert_eq!(tok, "0xZZ"),
            other => panic!("expected InvalidHex, got: {:?}", other),
        }

        // UnknownColor via parse_color_kind
        match parse_color_kind("not_a_color") {
            Err(MappingParseError::UnknownKindColor(s)) => assert_eq!(s, "not_a_color"),
            other => panic!("expected UnknownColor, got: {:?}", other),
        }

        // BinaryTooWide
        match normalize_first_column(&VariableValue::String("1111".into()), 4, 3) {
            Err(MappingParseError::BinaryTooWide {
                value,
                required,
                specified,
            }) => {
                assert_eq!(value, "1111");
                assert!(required > specified);
            }
            other => panic!("expected BinaryTooWide, got: {:?}", other),
        }

        // StringLengthMismatch
        match normalize_first_column(&VariableValue::String("ABCD".into()), 4, 3) {
            Err(MappingParseError::StringLengthMismatch {
                value,
                value_len,
                expected,
            }) => {
                assert_eq!(value, "ABCD");
                assert_eq!(value_len, 4);
                assert_eq!(expected, 3);
            }
            other => panic!("expected StringLengthMismatch, got: {:?}", other),
        }

        // MissingSecondColumn and EmptyLine via parse_line
        match parse_line("") {
            Err(MappingParseError::EmptyLine) => {}
            other => panic!("expected EmptyLine, got: {:?}", other),
        }

        match parse_line("0101") {
            Err(MappingParseError::MissingMapping) => {}
            other => panic!("expected MissingSecondColumn, got: {:?}", other),
        }

        // LineError produced by parse_content_with_default_name when an entry line is invalid
        let content = "Bits =  4\n0 ZERO red\nBADLINE";
        match parse_content_with_default_name(content, None) {
            Err(MappingParseError::LineError {
                line,
                content,
                message: _,
            }) => {
                assert_eq!(line, 3);
                assert_eq!(content, "BADLINE");
            }
            other => panic!("expected LineError, got: {:?}", other),
        }

        // Io error via MappingTranslatorMap::new with non-existent file
        let pathbuf = std::path::PathBuf::from("/this/path/should/not/exist/mapping.tmp");
        let path = Utf8PathBuf::from_path_buf(pathbuf).expect("path must be UTF-8");
        match MappingTranslatorMap::new(path) {
            Err(MappingParseError::Io(_)) => {}
            other => panic!("expected Io, got: {:?}", other),
        }
    }
}
