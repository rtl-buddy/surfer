use ecolor::Color32;
use eyre::Result;
use num::{BigUint, One, ToPrimitive};
use surfer_translation_types::{
    TranslationPreference, VariableValue, kind_for_binary_representation,
};

use crate::translation::{BasicTranslator, ValueKind};
use crate::wave_container::{ScopeId, VarId, VariableMeta};

pub struct RGBTranslator {}

impl BasicTranslator<VarId, ScopeId> for RGBTranslator {
    fn name(&self) -> String {
        String::from("RGB")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let nibble_length = num_bits.div_ceil(3);
                let b = v % (BigUint::one() << nibble_length);
                let g = (v >> nibble_length) % (BigUint::one() << nibble_length);
                let r = (v >> (2 * nibble_length)) % (BigUint::one() << nibble_length);
                let (r_u8, g_u8, b_u8) = if nibble_length >= 8 {
                    let scale = nibble_length - 8;
                    (
                        (r >> scale).to_u8().unwrap_or(255),
                        (g >> scale).to_u8().unwrap_or(255),
                        (b >> scale).to_u8().unwrap_or(255),
                    )
                } else {
                    let scale = 8 - nibble_length;
                    (
                        (r << scale).to_u8().unwrap_or(255),
                        (g << scale).to_u8().unwrap_or(255),
                        (b << scale).to_u8().unwrap_or(255),
                    )
                };
                let s = format!("#{r_u8:02x}{g_u8:02x}{b_u8:02x}");
                (s, ValueKind::Custom(Color32::from_rgb(r_u8, g_u8, b_u8)))
            }
            VariableValue::String(s) => (s.clone(), kind_for_binary_representation(s)),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        if let Some(num_bits) = variable.num_bits {
            if num_bits.is_multiple_of(3u32) {
                Ok(TranslationPreference::Yes)
            } else {
                Ok(TranslationPreference::No)
            }
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct YCbCrTranslator {}

impl BasicTranslator<VarId, ScopeId> for YCbCrTranslator {
    fn name(&self) -> String {
        String::from("YCbCr")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let nibble_length = num_bits.div_ceil(3);
                let cr = v % (BigUint::one() << nibble_length);
                let cb = (v >> nibble_length) % (BigUint::one() << nibble_length);
                let y = (v >> (2 * nibble_length)) % (BigUint::one() << nibble_length);
                let (y_u8, cb_u8, cr_u8) = if nibble_length >= 8 {
                    let scale = nibble_length - 8;
                    (
                        (y >> scale).to_u8().unwrap_or(255),
                        (cb >> scale).to_u8().unwrap_or(255),
                        (cr >> scale).to_u8().unwrap_or(255),
                    )
                } else {
                    let scale = 8 - nibble_length;
                    (
                        (y << scale).to_u8().unwrap_or(255),
                        (cb << scale).to_u8().unwrap_or(255),
                        (cr << scale).to_u8().unwrap_or(255),
                    )
                };
                let s = format!("#{y_u8:02x}{cb_u8:02x}{cr_u8:02x}");
                let (r_u8, g_u8, b_u8) = ycbcr_to_rgb(y_u8, cb_u8, cr_u8);
                (s, ValueKind::Custom(Color32::from_rgb(r_u8, g_u8, b_u8)))
            }
            VariableValue::String(s) => (s.clone(), kind_for_binary_representation(s)),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        if let Some(num_bits) = variable.num_bits {
            if num_bits.is_multiple_of(3u32) {
                Ok(TranslationPreference::Yes)
            } else {
                Ok(TranslationPreference::No)
            }
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct GrayScaleTranslator {}

impl BasicTranslator<VarId, ScopeId> for GrayScaleTranslator {
    fn name(&self) -> String {
        String::from("Grayscale")
    }

    fn basic_translate(&self, num_bits: u32, value: &VariableValue) -> (String, ValueKind) {
        match value {
            VariableValue::BigUint(v) => {
                let g = if num_bits >= 8 {
                    let scale = num_bits - 8;
                    (v >> scale).to_u8().unwrap_or(255)
                } else {
                    let scale = 8 - num_bits;
                    (v << scale).to_u8().unwrap_or(255)
                };
                let s = format!("#{g:02x}");
                (s, ValueKind::Custom(Color32::from_gray(g)))
            }
            VariableValue::String(s) => (s.clone(), kind_for_binary_representation(s)),
        }
    }
}

// Convert YCbCr (BT.601) to RGB. Inputs and outputs are 8-bit.
// Uses floating-point coefficients with rounding and clamps to [0, 255].
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y_f = f32::from(y);
    let cb_i = (i32::from(cb) - 128) as f32;
    let cr_i = (i32::from(cr) - 128) as f32;

    let r = (y_f + 1.402_f32 * cr_i).round() as i32;
    let g = (y_f - 0.344136_f32 * cb_i - 0.714136_f32 * cr_i).round() as i32;
    let b = (y_f + 1.772_f32 * cb_i).round() as i32;

    fn clamp_u8(x: i32) -> u8 {
        x.clamp(0, 255) as u8
    }

    (clamp_u8(r), clamp_u8(g), clamp_u8(b))
}

#[cfg(test)]
mod test {
    use super::*;
    use num::BigUint;

    fn translate_rgb(num_bits: u32, value: u32) -> (String, ValueKind) {
        let translator = RGBTranslator {};
        let biguint_value = VariableValue::BigUint(BigUint::from(value));
        translator.basic_translate(num_bits, &biguint_value)
    }

    fn assert_color_value(result: &ValueKind, expected_r: u8, expected_g: u8, expected_b: u8) {
        if let ValueKind::Custom(color) = result {
            let actual_color = Color32::from_rgb(expected_r, expected_g, expected_b);
            assert_eq!(color, &actual_color, "Color mismatch");
        } else {
            panic!("Expected Custom color value, got {result:?}");
        }
    }

    #[test]
    fn rgb_translator_name() {
        let translator = RGBTranslator {};
        assert_eq!(translator.name(), "RGB");
    }

    // === 24-bit Color Tests ===
    #[test]
    fn rgb_translator_24bit_pure_red() {
        let (hex, kind) = translate_rgb(24, 0xFF0000);
        assert_eq!(hex, "#ff0000");
        assert_color_value(&kind, 255, 0, 0);
    }

    #[test]
    fn rgb_translator_24bit_pure_green() {
        let (hex, kind) = translate_rgb(24, 0x00FF00);
        assert_eq!(hex, "#00ff00");
        assert_color_value(&kind, 0, 255, 0);
    }

    #[test]
    fn rgb_translator_24bit_pure_blue() {
        let (hex, kind) = translate_rgb(24, 0x0000FF);
        assert_eq!(hex, "#0000ff");
        assert_color_value(&kind, 0, 0, 255);
    }

    #[test]
    fn rgb_translator_24bit_white() {
        let (hex, kind) = translate_rgb(24, 0xFFFFFF);
        assert_eq!(hex, "#ffffff");
        assert_color_value(&kind, 255, 255, 255);
    }

    #[test]
    fn rgb_translator_24bit_black() {
        let (hex, kind) = translate_rgb(24, 0x000000);
        assert_eq!(hex, "#000000");
        assert_color_value(&kind, 0, 0, 0);
    }

    #[test]
    fn rgb_translator_24bit_mixed_colors() {
        // 0xABCDEF: R=AB, G=CD, B=EF
        let (hex, kind) = translate_rgb(24, 0xABCDEF);
        assert_eq!(hex, "#abcdef");
        assert_color_value(&kind, 0xAB, 0xCD, 0xEF);
    }

    #[test]
    fn rgb_translator_24bit_low_values() {
        // 0x010203: R=01, G=02, B=03
        let (hex, kind) = translate_rgb(24, 0x010203);
        assert_eq!(hex, "#010203");
        assert_color_value(&kind, 0x01, 0x02, 0x03);
    }

    // === Edge Case: Different Bit Widths ===
    #[test]
    fn rgb_translator_3bit_max_value() {
        // 3-bit RGB (1 bit per channel)
        // Value: 0b111 = all channels at max
        let (hex, kind) = translate_rgb(3, 0x7);
        // 1-bit channel values scaled to 8-bit: 1 << 7 = 128
        assert_eq!(hex, "#808080");
        assert_color_value(&kind, 128, 128, 128);
    }

    #[test]
    fn rgb_translator_6bit_all_ones() {
        // 6-bit RGB (2 bits per channel)
        // All channels: 11 (3) scaled to 8-bit: 3 << 6 = 192
        let (hex, kind) = translate_rgb(6, 0x3F);
        assert_eq!(hex, "#c0c0c0");
        assert_color_value(&kind, 192, 192, 192);
    }

    #[test]
    fn rgb_translator_9bit_full_range() {
        // 9-bit RGB (3 bits per channel)
        // Each channel: 111 (7) scaled to 8-bit: 7 << 5 = 224
        let (hex, kind) = translate_rgb(9, 0x1FF);
        assert_eq!(hex, "#e0e0e0");
        assert_color_value(&kind, 224, 224, 224);
    }

    #[test]
    fn rgb_translator_12bit_mixed() {
        // 12-bit RGB (4 bits per channel)
        // R: 1111 (15), G: 1010 (10), B: 0101 (5)
        // Scaled: 15 << 4 = 240, 10 << 4 = 160, 5 << 4 = 80
        let (hex, kind) = translate_rgb(12, 0xFA5);
        assert_eq!(hex, "#f0a050");
        assert_color_value(&kind, 240, 160, 80);
    }

    #[test]
    fn rgb_translator_15bit_downscaling() {
        // 15-bit RGB (5 bits per channel)
        // All channels: 11111 (31) scaled down: 31 >> 0 = 31 << 3 = 248
        let (hex, kind) = translate_rgb(15, 0x7FFF);
        assert_eq!(hex, "#f8f8f8");
        assert_color_value(&kind, 248, 248, 248);
    }

    #[test]
    fn rgb_translator_18bit_downscaling() {
        // 18-bit RGB (6 bits per channel)
        // All channels: 111111 (63) scaled down: 63 >> 0 = 63 << 2 = 252
        let (hex, kind) = translate_rgb(18, 0x3FFFF);
        assert_eq!(hex, "#fcfcfc");
        assert_color_value(&kind, 252, 252, 252);
    }

    #[test]
    fn rgb_translator_21bit_downscaling() {
        // 21-bit RGB (7 bits per channel)
        // All channels: 1111111 (127) scaled down: 127 >> 0 = 127 << 1 = 254
        let (hex, kind) = translate_rgb(21, 0x1FFFFF);
        assert_eq!(hex, "#fefefe");
        assert_color_value(&kind, 254, 254, 254);
    }

    // === Asymmetric Color Values ===
    #[test]
    fn rgb_translator_24bit_high_red_low_others() {
        let (hex, kind) = translate_rgb(24, 0xFF0000);
        assert_eq!(hex, "#ff0000");
        assert_color_value(&kind, 255, 0, 0);
    }

    #[test]
    fn rgb_translator_24bit_medium_values() {
        // 0x808080: R=128, G=128, B=128 (mid-gray)
        let (hex, kind) = translate_rgb(24, 0x808080);
        assert_eq!(hex, "#808080");
        assert_color_value(&kind, 128, 128, 128);
    }

    #[test]
    fn rgb_translator_12bit_asymmetric() {
        // 12-bit RGB (4 bits per channel)
        // R: 1111 (15), G: 1000 (8), B: 0001 (1)
        // Scaled: 15 << 4 = 240, 8 << 4 = 128, 1 << 4 = 16
        let (hex, kind) = translate_rgb(12, 0xF81);
        assert_eq!(hex, "#f08010");
        assert_color_value(&kind, 240, 128, 16);
    }

    // === String Value Handling ===
    #[test]
    fn rgb_translator_string_value() {
        let translator = RGBTranslator {};
        let value = VariableValue::String("test_string".to_string());
        let (result, _kind) = translator.basic_translate(24, &value);
        assert_eq!(result, "test_string");
    }

    // === Grayscale Translator Tests ===
    fn translate_gray(num_bits: u32, value: u32) -> (String, ValueKind) {
        let translator = GrayScaleTranslator {};
        let biguint_value = VariableValue::BigUint(BigUint::from(value));
        translator.basic_translate(num_bits, &biguint_value)
    }

    fn assert_gray_value(result: &ValueKind, g: u8) {
        if let ValueKind::Custom(color) = result {
            let expected = Color32::from_gray(g);
            assert_eq!(color, &expected, "Gray color mismatch");
        } else {
            panic!("Expected Custom color value, got {result:?}");
        }
    }

    #[test]
    fn grayscale_translator_name() {
        let translator = GrayScaleTranslator {};
        assert_eq!(translator.name(), "Grayscale");
    }

    // 8-bit grayscale: identity mapping
    #[test]
    fn grayscale_8bit_black() {
        let (hex, kind) = translate_gray(8, 0x00);
        assert_eq!(hex, "#00");
        assert_gray_value(&kind, 0);
    }

    #[test]
    fn grayscale_8bit_mid_gray() {
        let (hex, kind) = translate_gray(8, 0x80);
        assert_eq!(hex, "#80");
        assert_gray_value(&kind, 128);
    }

    #[test]
    fn grayscale_8bit_white() {
        let (hex, kind) = translate_gray(8, 0xFF);
        assert_eq!(hex, "#ff");
        assert_gray_value(&kind, 255);
    }

    // < 8-bit: upscaling via left shift
    #[test]
    fn grayscale_1bit_max() {
        let (hex, kind) = translate_gray(1, 0x1);
        assert_eq!(hex, "#80");
        assert_gray_value(&kind, 128);
    }

    #[test]
    fn grayscale_4bit_max() {
        let (hex, kind) = translate_gray(4, 0xF);
        assert_eq!(hex, "#f0");
        assert_gray_value(&kind, 240);
    }

    #[test]
    fn grayscale_7bit_max() {
        let (hex, kind) = translate_gray(7, 0x7F);
        assert_eq!(hex, "#fe");
        assert_gray_value(&kind, 254);
    }

    // > 8-bit: downscaling via right shift
    #[test]
    fn grayscale_12bit_high_nibble() {
        let (hex, kind) = translate_gray(12, 0xF00);
        assert_eq!(hex, "#f0");
        assert_gray_value(&kind, 0xF0);
    }

    #[test]
    fn grayscale_12bit_mid_nibble() {
        let (hex, kind) = translate_gray(12, 0x0F0);
        assert_eq!(hex, "#0f");
        assert_gray_value(&kind, 0x0F);
    }

    #[test]
    fn grayscale_12bit_low_nibble() {
        let (hex, kind) = translate_gray(12, 0x00F);
        assert_eq!(hex, "#00");
        assert_gray_value(&kind, 0x00);
    }

    #[test]
    fn grayscale_16bit_high_byte() {
        let (hex, kind) = translate_gray(16, 0xFF00);
        assert_eq!(hex, "#ff");
        assert_gray_value(&kind, 0xFF);
    }

    #[test]
    fn grayscale_16bit_mid_value() {
        let (hex, kind) = translate_gray(16, 0x7F00);
        assert_eq!(hex, "#7f");
        assert_gray_value(&kind, 0x7F);
    }

    #[test]
    fn grayscale_16bit_low_byte() {
        let (hex, kind) = translate_gray(16, 0x00FF);
        assert_eq!(hex, "#00");
        assert_gray_value(&kind, 0x00);
    }

    // String value handling
    #[test]
    fn grayscale_string_value() {
        let translator = GrayScaleTranslator {};
        let value = VariableValue::String("gray_string".to_string());
        let (result, _kind) = translator.basic_translate(8, &value);
        assert_eq!(result, "gray_string");
    }

    // === YCbCr → RGB Helper Tests ===
    fn assert_rgb_approx(actual: (u8, u8, u8), expected: (u8, u8, u8), tol: u8) {
        let (ar, ag, ab) = actual;
        let (er, eg, eb) = expected;
        let dr = ar.abs_diff(er);
        let dg = ag.abs_diff(eg);
        let db = ab.abs_diff(eb);
        assert!(
            dr <= tol && dg <= tol && db <= tol,
            "actual={actual:?} expected={expected:?} tol={tol} diffs=({dr}, {dg}, {db})"
        );
    }

    #[test]
    fn ycbcr_gray_identity_low_mid_high() {
        // When Cb=Cr=128, RGB should equal Y for all channels
        assert_eq!(ycbcr_to_rgb(0, 128, 128), (0, 0, 0));
        assert_eq!(ycbcr_to_rgb(128, 128, 128), (128, 128, 128));
        assert_eq!(ycbcr_to_rgb(255, 128, 128), (255, 255, 255));
    }

    #[test]
    fn ycbcr_pure_red_sample_bt601() {
        // Typical BT.601 sample for pure red: Y=76, Cb=85, Cr=255
        let rgb = ycbcr_to_rgb(76, 85, 255);
        // Rounding may yield 254; allow small tolerance
        assert_rgb_approx(rgb, (255, 0, 0), 2);
    }

    #[test]
    fn ycbcr_pure_green_sample_bt601() {
        // Typical BT.601 sample for pure green: Y=150, Cb=44, Cr=21
        let rgb = ycbcr_to_rgb(150, 44, 21);
        assert_rgb_approx(rgb, (0, 255, 0), 2);
    }

    #[test]
    fn ycbcr_pure_blue_sample_bt601() {
        // Typical BT.601 sample for pure blue: Y=29, Cb=255, Cr=107
        let rgb = ycbcr_to_rgb(29, 255, 107);
        assert_rgb_approx(rgb, (0, 0, 255), 2);
    }

    // === YCbCr Translator Tests ===
    fn translate_ycbcr(num_bits: u32, y: u8, cb: u8, cr: u8) -> (String, ValueKind) {
        let translator = YCbCrTranslator {};
        let nibble_length = num_bits.div_ceil(3);
        let packed: u32 = (u32::from(y) << (2 * nibble_length))
            | (u32::from(cb) << nibble_length)
            | u32::from(cr);
        let value = VariableValue::BigUint(BigUint::from(packed));
        translator.basic_translate(num_bits, &value)
    }

    #[test]
    fn ycbcr_translator_name() {
        let translator = YCbCrTranslator {};
        assert_eq!(translator.name(), "YCbCr");
    }

    #[test]
    fn ycbcr_24bit_gray_identity_low_mid_high() {
        let (hex0, kind0) = translate_ycbcr(24, 0, 128, 128);
        assert_eq!(hex0, "#008080");
        assert_color_value(&kind0, 0, 0, 0);

        let (hexm, kindm) = translate_ycbcr(24, 128, 128, 128);
        assert_eq!(hexm, "#808080");
        assert_color_value(&kindm, 128, 128, 128);

        let (hexw, kindw) = translate_ycbcr(24, 255, 128, 128);
        assert_eq!(hexw, "#ff8080");
        assert_color_value(&kindw, 255, 255, 255);
    }

    #[test]
    fn ycbcr_24bit_pure_red_sample_bt601() {
        // Typical BT.601 sample for pure red: Y=76, Cb=85, Cr=255
        let (hex, kind) = translate_ycbcr(24, 76, 85, 255);
        assert_eq!(hex, "#4c55ff");
        let (er, eg, eb) = ycbcr_to_rgb(76, 85, 255);
        assert_color_value(&kind, er, eg, eb);
    }

    #[test]
    fn ycbcr_24bit_pure_green_sample_bt601() {
        // Typical BT.601 sample for pure green: Y=150, Cb=44, Cr=21
        let (hex, kind) = translate_ycbcr(24, 150, 44, 21);
        assert_eq!(hex, "#962c15");
        let (er, eg, eb) = ycbcr_to_rgb(150, 44, 21);
        assert_color_value(&kind, er, eg, eb);
    }

    #[test]
    fn ycbcr_24bit_pure_blue_sample_bt601() {
        // Typical BT.601 sample for pure blue: Y=29, Cb=255, Cr=107
        let (hex, kind) = translate_ycbcr(24, 29, 255, 107);
        assert_eq!(hex, "#1dff6b");
        let (er, eg, eb) = ycbcr_to_rgb(29, 255, 107);
        assert_color_value(&kind, er, eg, eb);
    }

    #[test]
    fn ycbcr_12bit_scaled_values() {
        // 12-bit (4 bits per component): Y=0xF, Cb=0x8, Cr=0x1
        // After scaling to 8-bit: Y=240, Cb=128, Cr=16
        let (hex, kind) = translate_ycbcr(12, 0xF, 0x8, 0x1);
        assert_eq!(hex, "#f08010");
        let (er, eg, eb) = ycbcr_to_rgb(240, 128, 16);
        assert_color_value(&kind, er, eg, eb);
    }

    #[test]
    fn ycbcr_3bit_scaled_values() {
        // 3-bit (1 bit per component): Y=1, Cb=1, Cr=1
        // After scaling to 8-bit: 128, 128, 128
        let (hex, kind) = translate_ycbcr(3, 1, 1, 1);
        assert_eq!(hex, "#808080");
        let (er, eg, eb) = ycbcr_to_rgb(128, 128, 128);
        assert_color_value(&kind, er, eg, eb);
    }
}
