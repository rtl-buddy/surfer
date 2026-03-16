use crate::message::Message;
use crate::translation::fixed_point::{big_uint_to_sfixed, big_uint_to_ufixed};
use crate::variable_meta::VariableMetaExt;
use crate::wave_container::{ScopeId, VarId};
use eyre::Result;
use half::{bf16, f16};
use num::{BigUint, One};
use softposit::{P8E0, P16E1, P32E2, Q8E0, Q16E1};
use surfer_translation_types::{
    BasicTranslator, NumericRange, TranslationResult, Translator, ValueKind, ValueRepr,
    VariableInfo, VariableMeta, VariableValue, biguint_to_f64, parse_value_to_numeric,
    translates_all_bit_types,
};

use super::{TranslationPreference, check_single_wordlength, integer_numeric_range};

#[inline]
fn shortest_float_representation<T: std::fmt::LowerExp + std::fmt::Display>(v: T) -> String {
    let dec = format!("{v}");
    let exp = format!("{v:e}");
    if dec.len() > exp.len() { exp } else { dec }
}

/// If `value` is a biguint or consists only of 1 or 0, translates the value using
/// `biguint_translator`. If `value` contains other values such as X, Z etc. the result
/// is the corresponding `ValueKind`
fn translate_numeric(
    biguint_translator: impl Fn(&BigUint) -> String,
    value: &VariableValue,
) -> (String, ValueKind) {
    match value.parse_biguint() {
        Ok(v) => (biguint_translator(&v), ValueKind::Normal),
        Err((v, k)) => (v, k),
    }
}

pub struct UnsignedTranslator {}

impl BasicTranslator<VarId, ScopeId> for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(std::string::ToString::to_string, v)
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, biguint_to_f64))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        if variable.has_unsigned_integer_type_name() {
            Ok(TranslationPreference::Prefer)
        } else {
            translates_all_bit_types(variable)
        }
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, false)
    }
}

pub struct SignedTranslator {}

impl BasicTranslator<VarId, ScopeId> for SignedTranslator {
    fn name(&self) -> String {
        String::from("Signed")
    }

    fn basic_translate(&self, num_bits: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(|val| compute_signed_value(val, num_bits), v)
    }

    fn basic_translate_numeric(&self, num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            let signweight = BigUint::one() << (num_bits - 1);
            if v < &signweight {
                biguint_to_f64(v)
            } else {
                let v2 = (&signweight << 1) - v;
                -biguint_to_f64(&v2)
            }
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        if variable.is_integer_type() || variable.has_signed_integer_type_name() {
            Ok(TranslationPreference::Prefer)
        } else {
            translates_all_bit_types(variable)
        }
    }

    fn basic_numeric_range(&self, num_bits: u32) -> Option<NumericRange> {
        integer_numeric_range(num_bits, true)
    }
}

/// Computes the signed value string for a given `BigUint` and bit width.
fn compute_signed_value(v: &BigUint, num_bits: u32) -> String {
    let signweight = BigUint::one() << (num_bits - 1);
    if v < &signweight {
        v.to_string()
    } else {
        let v2 = (signweight << 1) - v;
        format!("-{v2}")
    }
}

pub struct SinglePrecisionTranslator {}

impl BasicTranslator<VarId, ScopeId> for SinglePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 32-bit IEEE 754")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                shortest_float_representation(f32::from_bits(
                    v.iter_u32_digits().next().unwrap_or(0),
                ))
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            f64::from(f32::from_bits(v.iter_u32_digits().next().unwrap_or(0)))
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 32)
    }
}

pub struct DoublePrecisionTranslator {}

impl BasicTranslator<VarId, ScopeId> for DoublePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 64-bit IEEE 754")
    }
    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                shortest_float_representation(f64::from_bits(
                    v.iter_u64_digits().next().unwrap_or(0),
                ))
            },
            v,
        )
    }
    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            f64::from_bits(v.iter_u64_digits().next().unwrap_or(0))
        }))
    }
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        if variable.is_real() {
            Ok(TranslationPreference::Prefer)
        } else {
            check_single_wordlength(variable.num_bits, 64)
        }
    }
}

#[cfg(feature = "f128")]
pub struct QuadPrecisionTranslator {}

#[cfg(feature = "f128")]
impl BasicTranslator<VarId, ScopeId> for QuadPrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 128-bit IEEE 754")
    }
    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                let mut digits = v.iter_u64_digits();
                let lsb = digits.next().unwrap_or(0);
                let msb = if digits.len() > 0 {
                    digits.next().unwrap_or(0)
                } else {
                    0
                };
                let val = lsb as u128 | (msb as u128) << 64;
                f128::f128::from_bits(val).to_string()
            },
            v,
        )
    }
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 128)
    }
}

pub struct HalfPrecisionTranslator {}

impl BasicTranslator<VarId, ScopeId> for HalfPrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 16-bit IEEE 754")
    }
    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                shortest_float_representation(f16::from_bits(
                    v.iter_u32_digits().next().unwrap_or(0) as u16,
                ))
            },
            v,
        )
    }
    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            f64::from(f16::from_bits(
                v.iter_u32_digits().next().unwrap_or(0) as u16
            ))
        }))
    }
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 16)
    }

    fn basic_numeric_range(&self, _num_bits: u32) -> Option<NumericRange> {
        Some(NumericRange {
            min: f64::from(f16::MIN),
            max: f64::from(f16::MAX),
        })
    }
}

pub struct BFloat16Translator {}

impl BasicTranslator<VarId, ScopeId> for BFloat16Translator {
    fn name(&self) -> String {
        String::from("FP: bfloat16")
    }
    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                shortest_float_representation(bf16::from_bits(
                    v.iter_u32_digits().next().unwrap_or(0) as u16,
                ))
            },
            v,
        )
    }
    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            f64::from(bf16::from_bits(
                v.iter_u32_digits().next().unwrap_or(0) as u16
            ))
        }))
    }
    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 16)
    }

    fn basic_numeric_range(&self, _num_bits: u32) -> Option<NumericRange> {
        Some(NumericRange {
            min: f64::from(bf16::MIN),
            max: f64::from(bf16::MAX),
        })
    }
}

pub struct Posit32Translator {}

impl BasicTranslator<VarId, ScopeId> for Posit32Translator {
    fn name(&self) -> String {
        String::from("Posit: 32-bit (two exponent bits)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                format!(
                    "{p}",
                    p = P32E2::from_bits(v.iter_u32_digits().next().unwrap_or(0))
                )
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            P32E2::from_bits(v.iter_u32_digits().next().unwrap_or(0)).into()
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 32)
    }
}

pub struct Posit16Translator {}

impl BasicTranslator<VarId, ScopeId> for Posit16Translator {
    fn name(&self) -> String {
        String::from("Posit: 16-bit (one exponent bit)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                format!(
                    "{p}",
                    p = P16E1::from_bits(v.iter_u32_digits().next().unwrap_or(0) as u16)
                )
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            P16E1::from_bits(v.iter_u32_digits().next().unwrap_or(0) as u16).into()
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 16)
    }
}

pub struct Posit8Translator {}

impl BasicTranslator<VarId, ScopeId> for Posit8Translator {
    fn name(&self) -> String {
        String::from("Posit: 8-bit (no exponent bit)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                format!(
                    "{p}",
                    p = P8E0::from_bits(v.iter_u32_digits().next().unwrap_or(0) as u8)
                )
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            P8E0::from_bits(v.iter_u32_digits().next().unwrap_or(0) as u8).into()
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 8)
    }
}

pub struct PositQuire8Translator {}

impl BasicTranslator<VarId, ScopeId> for PositQuire8Translator {
    fn name(&self) -> String {
        String::from("Posit: quire for 8-bit (no exponent bit)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                format!(
                    "{p}",
                    p = Q8E0::from_bits(v.iter_u32_digits().next().unwrap_or(0))
                )
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            let q = Q8E0::from_bits(v.iter_u32_digits().next().unwrap_or(0));
            P8E0::from(q).into()
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 32)
    }
}

pub struct PositQuire16Translator {}

impl BasicTranslator<VarId, ScopeId> for PositQuire16Translator {
    fn name(&self) -> String {
        String::from("Posit: quire for 16-bit (one exponent bit)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| {
                let mut digits = v.iter_u64_digits();
                let lsb = digits.next().unwrap_or(0);
                let msb = digits.next().unwrap_or(0);
                let val = u128::from(lsb) | (u128::from(msb) << 64);
                format!("{}", Q16E1::from_bits(val))
            },
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            let mut digits = v.iter_u64_digits();
            let lsb = digits.next().unwrap_or(0);
            let msb = digits.next().unwrap_or(0);
            let val = u128::from(lsb) | (u128::from(msb) << 64);
            P16E1::from(Q16E1::from_bits(val)).into()
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 128)
    }
}

/// Format an f64 value as a string for FP8 display.
///
/// Handles special cases: NaN, infinity (with sign), and signed zero.
/// Normal values use the shortest representation (decimal or scientific).
fn format_fp8_value(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v.is_sign_negative() {
            "-∞".to_string()
        } else {
            "∞".to_string()
        }
    } else if v == 0.0 {
        if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        }
    } else {
        shortest_float_representation(v as f32)
    }
}

/// Decode u8 as 8-bit float with five exponent bits and two mantissa bits, returning f64.
#[allow(clippy::excessive_precision)]
fn decode_e5m2_f64(v: u8) -> f64 {
    let mant = v & 3;
    let exp = (v >> 2) & 31;
    let sign: f64 = if (v >> 7) != 0 { -1.0 } else { 1.0 };
    match (exp, mant) {
        (31, 0) => sign * f64::INFINITY,
        (31, ..) => f64::NAN,
        (0, 0) => sign * 0.0,
        (0, ..) => sign * f64::from(mant) * 0.0000152587890625f64, // 2^-16
        _ => sign * f64::from(4 + mant) * 2.0f64.powi(i32::from(exp) - 17),
    }
}

/// Decode u8 as 8-bit float with four exponent bits and three mantissa bits, returning f64.
fn decode_e4m3_f64(v: u8) -> f64 {
    let mant = v & 7;
    let exp = (v >> 3) & 15;
    let sign: f64 = if (v >> 7) != 0 { -1.0 } else { 1.0 };
    match (exp, mant) {
        (15, 7) => f64::NAN,
        (0, 0) => sign * 0.0,
        (0, ..) => sign * f64::from(mant) * 0.001953125f64, // 2^-9
        _ => sign * f64::from(8 + mant) * 2.0f64.powi(i32::from(exp) - 10),
    }
}

/// Decode u8 as 8-bit float with five exponent bits and two mantissa bits.
fn decode_e5m2(v: u8) -> String {
    format_fp8_value(decode_e5m2_f64(v))
}

pub struct E5M2Translator {}

impl BasicTranslator<VarId, ScopeId> for E5M2Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E5M2)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| decode_e5m2(v.iter_u32_digits().next().unwrap_or(0) as u8),
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            decode_e5m2_f64(v.iter_u32_digits().next().unwrap_or(0) as u8)
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 8)
    }

    fn basic_numeric_range(&self, _num_bits: u32) -> Option<NumericRange> {
        Some(NumericRange {
            min: -57344.0,
            max: 57344.0,
        })
    }
}

/// Decode u8 as 8-bit float with four exponent bits and three mantissa bits.
fn decode_e4m3(v: u8) -> String {
    format_fp8_value(decode_e4m3_f64(v))
}

pub struct E4M3Translator {}

impl BasicTranslator<VarId, ScopeId> for E4M3Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E4M3)")
    }

    fn basic_translate(&self, _: u32, v: &VariableValue) -> (String, ValueKind) {
        translate_numeric(
            |v| decode_e4m3(v.iter_u32_digits().next().unwrap_or(0) as u8),
            v,
        )
    }

    fn basic_translate_numeric(&self, _num_bits: u32, value: &VariableValue) -> Option<f64> {
        Some(parse_value_to_numeric(value, |v| {
            decode_e4m3_f64(v.iter_u32_digits().next().unwrap_or(0) as u8)
        }))
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, 8)
    }

    fn basic_numeric_range(&self, _num_bits: u32) -> Option<NumericRange> {
        Some(NumericRange {
            min: -448.0,
            max: 448.0,
        })
    }
}

pub struct UnsignedFixedPointTranslator;

impl Translator<VarId, ScopeId, Message> for UnsignedFixedPointTranslator {
    fn name(&self) -> String {
        "Unsigned fixed point".into()
    }

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        let (string, value_kind) = if let Some(idx) = &variable.index {
            translate_numeric(|v| big_uint_to_ufixed(v, -idx.lsb), value)
        } else {
            translate_numeric(std::string::ToString::to_string, value)
        };
        Ok(TranslationResult {
            kind: value_kind,
            val: ValueRepr::String(string),
            subfields: vec![],
        })
    }

    fn variable_info(&self, _: &VariableMeta<VarId, ScopeId>) -> Result<VariableInfo> {
        Ok(VariableInfo::Bits)
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        if variable.has_unsigned_fixedpoint_type_name() {
            Ok(TranslationPreference::Prefer)
        } else {
            translates_all_bit_types(variable)
        }
    }
}

pub struct SignedFixedPointTranslator;

impl Translator<VarId, ScopeId, Message> for SignedFixedPointTranslator {
    fn name(&self) -> String {
        "Signed fixed point".into()
    }

    fn translate(
        &self,
        variable: &VariableMeta<VarId, ScopeId>,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        let (string, value_kind) = if let Some(idx) = &variable.index {
            translate_numeric(
                |v| big_uint_to_sfixed(v, u64::from(variable.num_bits.unwrap_or(0)), -idx.lsb),
                value,
            )
        } else {
            translate_numeric(std::string::ToString::to_string, value)
        };
        Ok(TranslationResult {
            kind: value_kind,
            val: ValueRepr::String(string),
            subfields: vec![],
        })
    }

    fn variable_info(&self, _: &VariableMeta<VarId, ScopeId>) -> Result<VariableInfo> {
        Ok(VariableInfo::Bits)
    }

    fn translates(&self, variable: &VariableMeta<VarId, ScopeId>) -> Result<TranslationPreference> {
        if variable.has_signed_fixedpoint_type_name() {
            Ok(TranslationPreference::Prefer)
        } else {
            translates_all_bit_types(variable)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use surfer_translation_types::VariableValue;

    #[test]
    fn signed_translation_from_string() {
        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "-16"
        );

        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &VariableValue::String("01000".to_string()))
                .0,
            "8"
        );
    }

    #[test]
    fn signed_translation_from_biguint() {
        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b10011u32)))
                .0,
            "-13"
        );

        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b01000u32)))
                .0,
            "8"
        );
        assert_eq!(
            SignedTranslator {}
                .basic_translate(2, &VariableValue::BigUint(BigUint::from(0u32)))
                .0,
            "0"
        );
    }

    #[test]
    fn unsigned_translation_from_string() {
        assert_eq!(
            UnsignedTranslator {}
                .basic_translate(5, &VariableValue::String("10000".to_string()))
                .0,
            "16"
        );

        assert_eq!(
            UnsignedTranslator {}
                .basic_translate(5, &VariableValue::String("01000".to_string()))
                .0,
            "8"
        );
    }

    #[test]
    fn unsigned_translation_from_biguint() {
        assert_eq!(
            UnsignedTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b10011u32)))
                .0,
            "19"
        );

        assert_eq!(
            UnsignedTranslator {}
                .basic_translate(5, &VariableValue::BigUint(BigUint::from(0b01000u32)))
                .0,
            "8"
        );
        assert_eq!(
            UnsignedTranslator {}
                .basic_translate(2, &VariableValue::BigUint(BigUint::from(0u32)))
                .0,
            "0"
        );
    }

    #[test]
    fn e4m3_translation_from_biguint() {
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::BigUint(BigUint::from(0b10001000u8)))
                .0,
            "-0.015625"
        );
    }

    #[test]
    fn e4m3_translation_from_string() {
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("11111111".to_string()))
                .0,
            "NaN"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("00000011".to_string()))
                .0,
            "0.005859375"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("10000000".to_string()))
                .0,
            "-0"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("00000000".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("11110111".to_string()))
                .0,
            "-240"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &VariableValue::String("01000000".to_string()))
                .0,
            "2"
        );
    }

    #[test]
    fn e5m2_translation_from_biguint() {
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::BigUint(BigUint::from(0b10000100u8)))
                .0,
            "-6.1035156e-5"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::BigUint(BigUint::from(0b11111100u8)))
                .0,
            "-∞"
        );
    }

    #[test]
    fn e5m2_translation_from_string() {
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("11111111".to_string()))
                .0,
            "NaN"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("00000011".to_string()))
                .0,
            "4.5776367e-5"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("10000000".to_string()))
                .0,
            "-0"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("00000000".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("11111011".to_string()))
                .0,
            "-57344"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &VariableValue::String("01000000".to_string()))
                .0,
            "2"
        );
    }

    #[test]
    fn posit8_translation_from_biguint() {
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &VariableValue::BigUint(BigUint::from(0b10001000u8)))
                .0,
            "-8"
        );
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &VariableValue::BigUint(BigUint::from(0u8)))
                .0,
            "0"
        );
    }

    #[test]
    fn posit8_translation_from_string() {
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &VariableValue::String("11111111".to_string()))
                .0,
            "-0.015625"
        );
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &VariableValue::String("00000011".to_string()))
                .0,
            "0.046875"
        );
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &VariableValue::String("10000000".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn posit16_translation_from_biguint() {
        assert_eq!(
            Posit16Translator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1010101010001000u16))
                )
                .0,
            "-2.68359375"
        );
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &VariableValue::BigUint(BigUint::from(0u16)))
                .0,
            "0"
        );
    }

    #[test]
    fn posit16_translation_from_string() {
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &VariableValue::String("1111111111111111".to_string()))
                .0,
            "-0.000000003725290298461914"
        );
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &VariableValue::String("0111000000000011".to_string()))
                .0,
            "16.046875"
        );
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &VariableValue::String("1000000000000000".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn posit32_translation_from_biguint() {
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b1010101010001000u16))
                )
                .0,
            "0.0000000000000000023056236824262055"
        );
        assert_eq!(
            Posit32Translator {}
                .basic_translate(32, &VariableValue::BigUint(BigUint::from(0u32)))
                .0,
            "0"
        );
    }

    #[test]
    fn posit32_translation_from_string() {
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &VariableValue::String("10000111000000001111111111111111".to_string())
                )
                .0,
            "-8176.000244140625"
        );
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &VariableValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "257.75"
        );
    }

    #[test]
    fn quire8_translation_from_biguint() {
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b1010101010001000u16))
                )
                .0,
            "10"
        );
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(32, &VariableValue::BigUint(BigUint::from(0u16)))
                .0,
            "0"
        );
    }

    #[test]
    fn quire8_translation_from_string() {
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &VariableValue::String("10000111000000001111111111111111".to_string())
                )
                .0,
            "-64"
        );
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &VariableValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "64"
        );
    }

    #[test]
    fn quire16_translation_from_biguint() {
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(128, &VariableValue::BigUint(BigUint::from(0b10101010100010001010101010001000101010101000100010101010100010001010101010001000101010101000100010101010100010001010101010001000u128)))
                .0,
            "-268435456"
        );
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(128, &VariableValue::BigUint(BigUint::from(7u8)))
                .0,
            "0.000000003725290298461914"
        );
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(128, &VariableValue::BigUint(BigUint::from(0u8)))
                .0,
            "0"
        );
    }

    #[test]
    fn quire16_translation_from_string() {
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(
                    128,
                    &VariableValue::String(
                        "1000011100000000111111111111111101110000000000111000000000000000"
                            .to_string()
                    )
                )
                .0,
            "135"
        );
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(
                    128,
                    &VariableValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "0.000000029802322387695313"
        );
    }

    #[test]
    fn bloat16_translation_from_string() {
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("0100100011100011".to_string()))
                .0,
            "464896"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("1000000000000000".to_string()))
                .0,
            "-0"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("1111111111111111".to_string()))
                .0,
            "NaN"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001z0011100011".to_string()))
                .0,
            "HIGHIMP"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001q0011100011".to_string()))
                .0,
            "UNKNOWN VALUES"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001-0011100011".to_string()))
                .0,
            "DON'T CARE"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001w0011100011".to_string()))
                .0,
            "UNDEF WEAK"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001h0011100011".to_string()))
                .0,
            "WEAK"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &VariableValue::String("01001u0011100011".to_string()))
                .0,
            "UNDEF"
        );
    }

    #[test]
    fn bloat16_translation_from_bigunit() {
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1010101010001000u16))
                )
                .0,
            "-2.4158453e-13"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1000000000000000u16))
                )
                .0,
            "-0"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b0000000000000000u16))
                )
                .0,
            "0"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1111111111111111u16))
                )
                .0,
            "NaN"
        );
    }

    #[test]
    fn half_translation_from_biguint() {
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1000000000000000u16))
                )
                .0,
            "-0"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b0000000000000000u16))
                )
                .0,
            "0"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(
                    16,
                    &VariableValue::BigUint(BigUint::from(0b1111111111111111u16))
                )
                .0,
            "NaN"
        );
    }

    #[test]
    fn half_translation_from_string() {
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &VariableValue::String("0100100011100011".to_string()))
                .0,
            "9.7734375"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &VariableValue::String("1000000000000000".to_string()))
                .0,
            "-0"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &VariableValue::String("1111111111111111".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn single_translation_from_bigunit() {
        assert_eq!(
            SinglePrecisionTranslator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b01010101010001001010101010001000u32))
                )
                .0,
            "1.3514794e13"
        );
        assert_eq!(
            SinglePrecisionTranslator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b10000000000000000000000000000000u32))
                )
                .0,
            "-0"
        );
        assert_eq!(
            SinglePrecisionTranslator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b00000000000000000000000000000000u32))
                )
                .0,
            "0"
        );
        assert_eq!(
            SinglePrecisionTranslator {}
                .basic_translate(
                    32,
                    &VariableValue::BigUint(BigUint::from(0b11111111111111111111111111111111u32))
                )
                .0,
            "NaN"
        );
    }

    #[test]
    fn double_translation_from_bigunit() {
        assert_eq!(
            DoublePrecisionTranslator {}
                .basic_translate(
                    64,
                    &VariableValue::BigUint(BigUint::from(
                        0b0101010101000100101010101000100001010101010001001010101010001000u64
                    ))
                )
                .0,
            "5.785860578429741e102"
        );
        assert_eq!(
            DoublePrecisionTranslator {}
                .basic_translate(
                    64,
                    &VariableValue::BigUint(BigUint::from(
                        0b1000000000000000000000000000000000000000000000000000000000000000u64
                    ))
                )
                .0,
            "-0"
        );
        assert_eq!(
            DoublePrecisionTranslator {}
                .basic_translate(
                    64,
                    &VariableValue::BigUint(BigUint::from(
                        0b0000000000000000000000000000000000000000000000000000000000000000u64
                    ))
                )
                .0,
            "0"
        );
        assert_eq!(
            DoublePrecisionTranslator {}
                .basic_translate(
                    64,
                    &VariableValue::BigUint(BigUint::from(
                        0b1111111111111111111111111111111111111111111111111111111111111111u64
                    ))
                )
                .0,
            "NaN"
        );
    }
}
