use super::{TranslationPreference, ValueKind, VariableInfo};
use crate::wave_container::{ScopeId, VarId, VariableMeta};

use eyre::Result;
use itertools::Itertools;
use num::{One, Zero};
use surfer_translation_types::{
    BasicTranslator, NumericRange, VariableValue, check_vector_variable, extend_string,
    kind_for_binary_representation, parse_value_to_numeric,
};

use super::integer_numeric_range;

/// Splits a string into groups of `n` characters.
/// If the string length is not divisible by `n`, the first group will be shorter.
/// The string must only consist of ASCII characters.
#[must_use]
pub(crate) fn group_n_chars(s: &str, n: usize) -> Vec<&str> {
    let mut groups = Vec::new();
    let len = s.len();
    let rem = len % n;
    let mut start = 0;
    if rem > 0 {
        groups.push(&s[0..rem]);
        start = rem;
    }
    while start < len {
        let end = (start + n).min(len);
        groups.push(&s[start..end]);
        start += n;
    }
    groups
}

/// Map to radix-based representation, in practice hex or octal
fn map_to_radix(s: &str, radix: usize, num_bits: u32) -> (String, ValueKind) {
    let mut had_invalid_digit = false;
    let formatted = group_n_chars(
        &format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
        radix,
    )
    .into_iter()
    .map(|g| match g {
        g if g.contains('x') => "x".to_string(),
        g if g.contains('z') => "z".to_string(),
        g if g.contains('-') => "-".to_string(),
        g if g.contains('u') => "u".to_string(),
        g if g.contains('w') => "w".to_string(),
        g if g.contains('h') => "h".to_string(),
        g if g.contains('l') => "l".to_string(),
        g => {
            if let Ok(val) = u8::from_str_radix(g, 2) {
                format!("{val:x}")
            } else {
                had_invalid_digit = true;
                "?".to_string()
            }
        }
    })
    .join("");

    let kind = if had_invalid_digit {
        ValueKind::Error
    } else {
        kind_for_binary_representation(&formatted)
    };
    (formatted, kind)
}

fn check_wordlength(
    num_bits: Option<u32>,
    required: impl FnOnce(u32) -> bool,
) -> Result<TranslationPreference> {
    if let Some(num_bits) = num_bits {
        if required(num_bits) {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    } else {
        Ok(TranslationPreference::No)
    }
}

pub struct HexTranslator {}

impl BasicTranslator<VarId, ScopeId> for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                format!("{v:0width$x}", width = num_bits.div_ceil(4) as usize),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => map_to_radix(s, 4, num_bits),
        }
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, false)
    }
}

pub struct BitTranslator {}

impl BasicTranslator<VarId, ScopeId> for BitTranslator {
    fn name(&self) -> String {
        String::from("Bit")
    }

    fn basic_translate(&self, _num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                if (*v).is_zero() {
                    "0".to_string()
                } else if (*v).is_one() {
                    "1".to_string()
                } else {
                    "-".to_string()
                },
                ValueKind::Normal,
            ),
            VariableValue::String(s) => (s.clone(), kind_for_binary_representation(s)),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        if let Some(num_bits) = variable.num_bits {
            if num_bits == 1u32 {
                Ok(TranslationPreference::Prefer)
            } else {
                Ok(TranslationPreference::No)
            }
        } else {
            Ok(TranslationPreference::No)
        }
    }

    fn variable_info(&self, _variable: &VariableMeta) -> Result<VariableInfo> {
        Ok(VariableInfo::Bool)
    }
}

pub struct OctalTranslator {}

impl BasicTranslator<VarId, ScopeId> for OctalTranslator {
    fn name(&self) -> String {
        String::from("Octal")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                format!("{v:0width$o}", width = num_bits.div_ceil(3) as usize),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => map_to_radix(s, 3, num_bits),
        }
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, false)
    }
}

pub struct GroupingBinaryTranslator {}

impl BasicTranslator<VarId, ScopeId> for GroupingBinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary (with groups)")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        let (val, color) = match value {
            VariableValue::BigUint(v) => (
                format!("{v:0width$b}", width = num_bits as usize),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => (
                format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
                kind_for_binary_representation(s),
            ),
        };

        (group_n_chars(&val, 4).join(" "), color)
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, false)
    }
}

pub struct BinaryTranslator {}

impl BasicTranslator<VarId, ScopeId> for BinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                format!("{v:0width$b}", width = num_bits as usize),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => (
                format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, false)
    }
}

pub struct ASCIITranslator {}

impl BasicTranslator<VarId, ScopeId> for ASCIITranslator {
    fn name(&self) -> String {
        String::from("ASCII")
    }

    fn basic_translate(&self, _num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                v.to_bytes_be()
                    .into_iter()
                    .map(|val| (val as char).to_string())
                    .join(""),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => match check_vector_variable(s) {
                Some(v) => v,
                None => (
                    group_n_chars(s, 8)
                        .into_iter()
                        .map(|substr| {
                            format!(
                                "{cval}",
                                cval = u8::from_str_radix(substr, 2).unwrap_or_else(|_| panic!(
                                    "Found non-binary digit {substr} in value"
                                )) as char
                            )
                        })
                        .join(""),
                    ValueKind::Normal,
                ),
            },
        }
    }
}

fn decode_lebxxx(value: &num::BigUint) -> Result<num::BigUint, &'static str> {
    let bytes = value.to_bytes_be();
    match bytes.first() {
        Some(b) if b & 0x80 != 0 => return Err("invalid MSB"),
        _ => (),
    }

    let first: num::BigUint = bytes.first().copied().unwrap_or(0).into();
    bytes.iter().skip(1).try_fold(first, |result, b| {
        if (b & 0x80 == 0) == (result.is_zero()) {
            Ok((result << 7) + (*b & 0x7f))
        } else {
            Err("invalid flag")
        }
    })
}

pub struct LebTranslator {}

impl BasicTranslator<VarId, ScopeId> for LebTranslator {
    fn name(&self) -> String {
        "LEBxxx".to_string()
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        let decoded = match value {
            VariableValue::BigUint(v) => decode_lebxxx(v),
            VariableValue::String(s) => match check_vector_variable(s) {
                Some(v) => return v,
                None => match num::BigUint::parse_bytes(s.as_bytes(), 2) {
                    Some(bi) => decode_lebxxx(&bi),
                    None => return ("INVALID".to_owned(), ValueKind::Warn),
                },
            },
        };

        match decoded {
            Ok(decoded) => (decoded.to_str_radix(10), ValueKind::Normal),
            Err(s) => (
                s.to_owned()
                    + ": "
                    + &GroupingBinaryTranslator {}
                        .basic_translate(num_bits, value)
                        .0,
                ValueKind::Warn,
            ),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        check_wordlength(variable.num_bits, |n| (n.is_multiple_of(8)) && n > 0)
    }
}

pub struct NumberOfOnesTranslator {}

impl BasicTranslator<VarId, ScopeId> for NumberOfOnesTranslator {
    fn name(&self) -> String {
        String::from("Number of ones")
    }

    fn basic_translate(&self, _num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (v.count_ones().to_string(), ValueKind::Normal),
            VariableValue::String(s) => (
                s.bytes().filter(|b| *b == b'1').count().to_string(),
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| v.count_ones() as f64))
    }
}

pub struct TrailingOnesTranslator {}

impl BasicTranslator<VarId, ScopeId> for TrailingOnesTranslator {
    fn name(&self) -> String {
        String::from("Trailing ones")
    }

    fn basic_translate(&self, _num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (v.trailing_ones().to_string(), ValueKind::Normal),
            VariableValue::String(s) => (
                s.bytes()
                    .rev()
                    .take_while(|b| *b == b'1')
                    .count()
                    .to_string(),
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| v.trailing_ones() as f64))
    }
}

pub struct TrailingZerosTranslator {}

impl BasicTranslator<VarId, ScopeId> for TrailingZerosTranslator {
    fn name(&self) -> String {
        String::from("Trailing zeros")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => (
                v.trailing_zeros()
                    .unwrap_or(u64::from(num_bits))
                    .to_string(),
                ValueKind::Normal,
            ),
            VariableValue::String(s) => (
                (extend_string(s, num_bits) + s)
                    .bytes()
                    .rev()
                    .take_while(|b| *b == b'0')
                    .count()
                    .to_string(),
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_translate_numeric(&self, num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            v.trailing_zeros().unwrap_or(u64::from(num_bits)) as f64
        }))
    }
}

pub struct LeadingOnesTranslator {}

impl BasicTranslator<VarId, ScopeId> for LeadingOnesTranslator {
    fn name(&self) -> String {
        String::from("Leading ones")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let s = format!("{v:0width$b}", width = num_bits as usize);
                self.basic_translate(num_bits, &VariableValue::String(s))
            }
            VariableValue::String(s) => (
                if s.len() == (num_bits as usize) {
                    s.bytes().take_while(|b| *b == b'1').count().to_string()
                } else {
                    "0".to_string()
                },
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_translate_numeric(&self, num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            leading_ones(v, u64::from(num_bits)) as f64
        }))
    }
}

pub struct LeadingZerosTranslator {}

impl BasicTranslator<VarId, ScopeId> for LeadingZerosTranslator {
    fn name(&self) -> String {
        String::from("Leading zeros")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let s = format!("{v:0width$b}", width = num_bits as usize);
                self.basic_translate(num_bits, &VariableValue::String(s))
            }
            VariableValue::String(s) => (
                (extend_string(s, num_bits) + s)
                    .bytes()
                    .take_while(|b| *b == b'0')
                    .count()
                    .to_string(),
                kind_for_binary_representation(s),
            ),
        }
    }

    fn basic_translate_numeric(&self, num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            u64::from(num_bits).saturating_sub(v.bits()) as f64
        }))
    }
}

/// Counts leading ones in a `BigUint` value with a given bit width.
/// Returns 0 if the value has leading zeros (i.e., `v.bits() < num_bits`).
fn leading_ones(v: &num::BigUint, num_bits: u64) -> u64 {
    if v.bits() < num_bits {
        0
    } else {
        let mask = (num::BigUint::one() << num_bits) - 1u32;
        num_bits.saturating_sub((mask ^ v).bits())
    }
}

pub struct IdenticalMSBsTranslator {}

impl BasicTranslator<VarId, ScopeId> for IdenticalMSBsTranslator {
    fn name(&self) -> String {
        String::from("Identical MSBs")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let s = format!("{v:0width$b}", width = num_bits as usize);
                self.basic_translate(num_bits, &VariableValue::String(s))
            }
            VariableValue::String(s) => {
                let extended_string = extend_string(s, num_bits) + s;
                let zeros = extended_string.bytes().take_while(|b| *b == b'0').count();
                let ones = extended_string.bytes().take_while(|b| *b == b'1').count();
                let count = ones.max(zeros);
                (count.to_string(), kind_for_binary_representation(s))
            }
        }
    }

    fn basic_translate_numeric(&self, num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            let num_bits = u64::from(num_bits);
            let lz = num_bits.saturating_sub(v.bits());
            let lo = leading_ones(v, num_bits);
            lz.max(lo) as f64
        }))
    }
}

#[cfg(test)]
mod test {

    use num::BigUint;

    use super::*;

    #[test]
    fn group_n_chars_splits_with_remainder() {
        assert_eq!(group_n_chars("101010", 4), vec!["10", "1010"]);
    }

    #[test]
    fn group_n_chars_splits_evenly() {
        assert_eq!(group_n_chars("10101010", 4), vec!["1010", "1010"]);
    }

    #[test]
    fn group_n_chars_handles_empty_input() {
        assert_eq!(group_n_chars("", 4), Vec::<&str>::new());
    }

    #[test]
    fn map_to_radix_formats_binary_groups() {
        let (formatted, kind) = map_to_radix("10000", 4, 5);
        assert_eq!(formatted, "10");
        assert_eq!(kind, ValueKind::Normal);
    }

    #[test]
    fn map_to_radix_prefers_special_digits_per_group() {
        let (formatted, kind) = map_to_radix("zx01", 4, 4);
        assert_eq!(formatted, "x");
        assert_eq!(kind, kind_for_binary_representation("x"));
    }

    #[test]
    fn map_to_radix_marks_invalid_binary_digit_as_error() {
        let (formatted, kind) = map_to_radix("1002", 4, 4);
        assert_eq!(formatted, "?");
        assert_eq!(kind, ValueKind::Error);
    }

    #[test]
    fn check_wordlength_returns_yes_when_predicate_matches() {
        let preference = check_wordlength(Some(16), |n| n.is_multiple_of(8)).unwrap();
        assert_eq!(preference, TranslationPreference::Yes);
    }

    #[test]
    fn check_wordlength_returns_no_for_missing_or_nonmatching_wordlength() {
        let missing = check_wordlength(None, |n| n.is_multiple_of(8)).unwrap();
        assert_eq!(missing, TranslationPreference::No);

        let nonmatching = check_wordlength(Some(7), |n| n.is_multiple_of(8)).unwrap();
        assert_eq!(nonmatching, TranslationPreference::No);
    }

    #[test]
    fn decode_lebxxx_decodes_valid_value() {
        let decoded = decode_lebxxx(&BigUint::from(0b01011010_11101111u16)).unwrap();
        assert_eq!(decoded, BigUint::from(11_631u32));
    }

    #[test]
    fn decode_lebxxx_rejects_invalid_msb() {
        let err = decode_lebxxx(&BigUint::from(0b10000000u8)).unwrap_err();
        assert_eq!(err, "invalid MSB");
    }

    #[test]
    fn hexadecimal_translation_groups_digits_correctly_string() {
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "10"
        );

        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::String("1000".to_string()))
                .0,
            "08"
        );

        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::String("100000".to_string()))
                .0,
            "20"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(10, &VariableValue::String("1z00x0".to_string()))
                .0,
            "0zx"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(10, &VariableValue::String("z0110".to_string()))
                .0,
            "zz6"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(24, &VariableValue::String("xz0110".to_string()))
                .0,
            "xxxxx6"
        );
    }

    #[test]
    fn hexadecimal_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b10000u32)))
                .0,
            "10"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b1000u32)))
                .0,
            "08"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0u32)))
                .0,
            "00"
        );
    }

    #[test]
    fn octal_translation_groups_digits_correctly_string() {
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "20"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &VariableValue::String("100".to_string()))
                .0,
            "04"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "xx4"
        );
    }

    #[test]
    fn octal_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b10000u32)))
                .0,
            "20"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "04"
        );
    }

    #[test]
    fn grouping_binary_translation_groups_digits_correctly_string() {
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(5, &VariableValue::String("1000w".to_string()))
                .0,
            "1 000w"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(8, &VariableValue::String("100l00".to_string()))
                .0,
            "0010 0l00"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &VariableValue::String("10x00".to_string()))
                .0,
            "001 0x00"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &VariableValue::String("z10x00".to_string()))
                .0,
            "zz1 0x00"
        );
    }

    #[test]
    fn grouping_binary_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &VariableValue::BigUint(BigUint::from(0b100000u32)))
                .0,
            "010 0000"
        );
    }

    #[test]
    fn binary_translation_groups_digits_correctly_string() {
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "10000"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(8, &VariableValue::String("100h00".to_string()))
                .0,
            "00100h00"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &VariableValue::String("10x0-".to_string()))
                .0,
            "0010x0-"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &VariableValue::String("z10x00".to_string()))
                .0,
            "zz10x00"
        );
    }

    #[test]
    fn binary_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &VariableValue::BigUint(BigUint::from(0b100000u32)))
                .0,
            "0100000"
        );
    }

    #[test]
    fn ascii_translation_from_biguint() {
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(
                    15,
                    &VariableValue::BigUint(BigUint::from(0b100111101001011u32))
                )
                .0,
            "OK"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(72, &VariableValue::BigUint(BigUint::from(0b010011000110111101101110011001110010000001110100011001010111001101110100u128)))
                .0,
            "Long test"
        );
    }

    #[test]
    fn ascii_translation_from_string() {
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(15, &VariableValue::String("100111101001011".to_string()))
                .0,
            "OK"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(
                    72,
                    &VariableValue::String(
                        "010011000110111101101110011001110010000001110100011001010111001101110100"
                            .to_string()
                    )
                )
                .0,
            "Long test"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(16, &VariableValue::String("010x111101001011".to_string()))
                .0,
            "UNDEF"
        );
        // string too short for 2 characters, pads with 0
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(15, &VariableValue::String("11000001001011".to_string()))
                .0,
            "0K"
        );
    }

    #[test]
    fn bit_translation_from_biguint() {
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &VariableValue::BigUint(BigUint::from(0b1u8)))
                .0,
            "1"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &VariableValue::BigUint(BigUint::from(0b0u8)))
                .0,
            "0"
        );
    }

    #[test]
    fn bit_translation_from_string() {
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &VariableValue::String("1".to_string()))
                .0,
            "1"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &VariableValue::String("0".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &VariableValue::String("x".to_string()))
                .0,
            "x"
        );
    }

    #[test]
    fn bit_translator_with_invalid_data() {
        assert_eq!(
            BitTranslator {}
                .basic_translate(2, &VariableValue::BigUint(BigUint::from(3u8)))
                .0,
            "-"
        );
    }

    #[test]
    fn leb_translation_from_biguint() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::BigUint(0b01011010_11101111u16.into()))
                .0,
            "11631"
        );
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::BigUint(0b00000000_00000001u16.into()))
                .0,
            "1"
        );
        assert_eq!(
            LebTranslator{}.basic_translate(64, &VariableValue::BigUint(0b01001010_11110111_11101000_10100000_10111010_11110110_11100001_10011001u64.into())).0, "42185246214303897"
        );
    }
    #[test]
    fn leb_translation_from_string() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::String("0111110011100010".to_owned()))
                .0,
            "15970"
        );
    }
    #[test]
    fn leb_translation_invalid_msb() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::BigUint(0b1000000010000000u16.into()))
                .0,
            "invalid MSB: 1000 0000 1000 0000"
        );
    }
    #[test]
    fn leb_translation_invalid_continuation() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::BigUint(0b0111111101111111u16.into()))
                .0,
            "invalid flag: 0111 1111 0111 1111"
        );
    }

    #[test]
    fn leb_tranlator_input_not_multiple_of_8() {
        // act as if padded with 0s
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &VariableValue::BigUint(0b00001111111u16.into()))
                .0,
            "127"
        );
    }

    #[test]
    fn number_of_ones_translation_string() {
        assert_eq!(
            NumberOfOnesTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "1"
        );
        assert_eq!(
            NumberOfOnesTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "2"
        );
        assert_eq!(
            NumberOfOnesTranslator {}
                .basic_translate(9, &VariableValue::String("1x100".to_string()))
                .0,
            "2"
        );
    }

    #[test]
    fn number_of_ones_translation_bigint() {
        assert_eq!(
            NumberOfOnesTranslator {}
                .basic_translate(17, &VariableValue::BigUint(BigUint::from(0b101110000u32)))
                .0,
            "4"
        );
        assert_eq!(
            NumberOfOnesTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "1"
        );
    }

    #[test]
    fn trailing_ones_translation_string() {
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(5, &VariableValue::String("10111".to_string()))
                .0,
            "3"
        );
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "1"
        );
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "0"
        );
    }

    #[test]
    fn trailing_ones_translation_bigint() {
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(17, &VariableValue::BigUint(BigUint::from(0b101111111u32)))
                .0,
            "7"
        );
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "0"
        );
        assert_eq!(
            TrailingOnesTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b11111u32)))
                .0,
            "5"
        );
    }

    #[test]
    fn trailing_zeros_translation_string() {
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "4"
        );
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "2"
        );
    }

    #[test]
    fn trailing_zeros_translation_bigint() {
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(17, &VariableValue::BigUint(BigUint::from(0b101111111u32)))
                .0,
            "0"
        );
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "2"
        );
        assert_eq!(
            TrailingZerosTranslator {}
                .basic_translate(16, &VariableValue::BigUint(BigUint::from(0b0u32)))
                .0,
            "16"
        );
    }

    #[test]
    fn leading_ones_translation_string() {
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(5, &VariableValue::String("11101".to_string()))
                .0,
            "3"
        );
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "0"
        );
    }

    #[test]
    fn leading_ones_translation_bigint() {
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(11, &VariableValue::BigUint(BigUint::from(0b11111111100u32)))
                .0,
            "9"
        );
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "0"
        );
        assert_eq!(
            LeadingOnesTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b11111u32)))
                .0,
            "5"
        );
    }

    #[test]
    fn leading_zeros_translation_string() {
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "2"
        );
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "0"
        );
    }

    #[test]
    fn leading_zeros_translation_bigint() {
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(17, &VariableValue::BigUint(BigUint::from(0b101111111u32)))
                .0,
            "8"
        );
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "37"
        );
        assert_eq!(
            LeadingZerosTranslator {}
                .basic_translate(16, &VariableValue::BigUint(BigUint::from(0b0u32)))
                .0,
            "16"
        );
    }

    #[test]
    fn signbits_translation_string() {
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "1"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(7, &VariableValue::String("0".to_string()))
                .0,
            "7"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(5, &VariableValue::String("101".to_string()))
                .0,
            "2"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(9, &VariableValue::String("x100".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(5, &VariableValue::String("11101".to_string()))
                .0,
            "3"
        );
    }

    #[test]
    fn signbits_translation_bigint() {
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(17, &VariableValue::BigUint(BigUint::from(0b101111111u32)))
                .0,
            "8"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(40, &VariableValue::BigUint(BigUint::from(0b00100u32)))
                .0,
            "37"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(16, &VariableValue::BigUint(BigUint::from(0b0u32)))
                .0,
            "16"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(11, &VariableValue::BigUint(BigUint::from(0b11111111100u32)))
                .0,
            "9"
        );
        assert_eq!(
            IdenticalMSBsTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b11111u32)))
                .0,
            "5"
        );
    }
}
