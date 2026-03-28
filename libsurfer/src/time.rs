//! Time handling and formatting.
use derive_more::Display;
use ecolor::Color32;
use egui::{Button, Key, RichText, Ui};
use egui_remixicon::icons;
use emath::{Align2, Pos2};
use enum_iterator::Sequence;
use epaint::{FontId, Stroke};
use ftr_parser::types::Timescale;
use itertools::Itertools;
use num::{BigInt, BigRational, ToPrimitive, Zero};
use pure_rust_locales::{Locale, locale_match};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use sys_locale::get_locale;

use crate::viewport::Viewport;
use crate::wave_data::WaveData;
use crate::{
    Message, SystemState,
    translation::group_n_chars,
    view::{DrawConfig, DrawingContext},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct TimeScale {
    pub unit: TimeUnit,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, Copy, Display, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeUnit {
    #[display("zs")]
    ZeptoSeconds,

    #[display("as")]
    AttoSeconds,

    #[display("fs")]
    FemtoSeconds,

    #[display("ps")]
    PicoSeconds,

    #[display("ns")]
    NanoSeconds,

    #[display("μs")]
    MicroSeconds,

    #[display("ms")]
    MilliSeconds,

    #[display("s")]
    Seconds,

    #[display("No unit")]
    None,

    /// Use the largest time unit feasible for each time.
    #[display("Auto")]
    Auto,
}

pub const DEFAULT_TIMELINE_NAME: &str = "Time";
const THIN_SPACE: &str = "\u{2009}";

/// Candidate multipliers used to choose tick spacing.
pub const TICK_STEPS: [f64; 8] = [1., 2., 2.5, 5., 10., 20., 25., 50.];

/// Cached locale-specific formatting properties.
struct LocaleFormatCache {
    grouping: &'static [i64],
    thousands_sep: String,
    decimal_point: String,
}

static LOCALE_FORMAT_CACHE: OnceLock<LocaleFormatCache> = OnceLock::new();

/// Get the cached locale formatting properties.
fn get_locale_format_cache() -> &'static LocaleFormatCache {
    LOCALE_FORMAT_CACHE.get_or_init(|| {
        let locale = get_locale()
            .unwrap_or_else(|| "en-US".to_string())
            .as_str()
            .try_into()
            .unwrap_or(Locale::en_US);
        create_cache(locale)
    })
}

fn create_cache(locale: Locale) -> LocaleFormatCache {
    let grouping = locale_match!(locale => LC_NUMERIC::GROUPING);
    let thousands_sep =
        locale_match!(locale => LC_NUMERIC::THOUSANDS_SEP).replace('\u{202f}', THIN_SPACE);
    let decimal_point = locale_match!(locale => LC_NUMERIC::DECIMAL_POINT).to_string();

    LocaleFormatCache {
        grouping,
        thousands_sep,
        decimal_point,
    }
}

impl From<wellen::TimescaleUnit> for TimeUnit {
    fn from(timescale: wellen::TimescaleUnit) -> Self {
        match timescale {
            wellen::TimescaleUnit::ZeptoSeconds => TimeUnit::ZeptoSeconds,
            wellen::TimescaleUnit::AttoSeconds => TimeUnit::AttoSeconds,
            wellen::TimescaleUnit::FemtoSeconds => TimeUnit::FemtoSeconds,
            wellen::TimescaleUnit::PicoSeconds => TimeUnit::PicoSeconds,
            wellen::TimescaleUnit::NanoSeconds => TimeUnit::NanoSeconds,
            wellen::TimescaleUnit::MicroSeconds => TimeUnit::MicroSeconds,
            wellen::TimescaleUnit::MilliSeconds => TimeUnit::MilliSeconds,
            wellen::TimescaleUnit::Seconds => TimeUnit::Seconds,
            wellen::TimescaleUnit::Unknown => TimeUnit::None,
        }
    }
}

impl From<ftr_parser::types::Timescale> for TimeUnit {
    fn from(timescale: Timescale) -> Self {
        match timescale {
            Timescale::Fs => TimeUnit::FemtoSeconds,
            Timescale::Ps => TimeUnit::PicoSeconds,
            Timescale::Ns => TimeUnit::NanoSeconds,
            Timescale::Us => TimeUnit::MicroSeconds,
            Timescale::Ms => TimeUnit::MilliSeconds,
            Timescale::S => TimeUnit::Seconds,
            Timescale::Unit => TimeUnit::None,
            Timescale::None => TimeUnit::None,
        }
    }
}

impl TimeUnit {
    /// Get the power-of-ten exponent for a time unit.
    fn exponent(self) -> i8 {
        match self {
            TimeUnit::ZeptoSeconds => -21,
            TimeUnit::AttoSeconds => -18,
            TimeUnit::FemtoSeconds => -15,
            TimeUnit::PicoSeconds => -12,
            TimeUnit::NanoSeconds => -9,
            TimeUnit::MicroSeconds => -6,
            TimeUnit::MilliSeconds => -3,
            TimeUnit::Seconds => 0,
            TimeUnit::None => 0,
            TimeUnit::Auto => 0,
        }
    }
    /// Convert a power-of-ten exponent to a time unit.
    fn from_exponent(exponent: i8) -> Option<Self> {
        match exponent {
            -21 => Some(TimeUnit::ZeptoSeconds),
            -18 => Some(TimeUnit::AttoSeconds),
            -15 => Some(TimeUnit::FemtoSeconds),
            -12 => Some(TimeUnit::PicoSeconds),
            -9 => Some(TimeUnit::NanoSeconds),
            -6 => Some(TimeUnit::MicroSeconds),
            -3 => Some(TimeUnit::MilliSeconds),
            0 => Some(TimeUnit::Seconds),
            _ => None,
        }
    }
}

/// Create menu for selecting preferred time unit.
pub fn timeunit_menu(ui: &mut Ui, msgs: &mut Vec<Message>, wanted_timeunit: &TimeUnit) {
    for timeunit in enum_iterator::all::<TimeUnit>() {
        if ui
            .radio(*wanted_timeunit == timeunit, timeunit.to_string())
            .clicked()
        {
            msgs.push(Message::SetTimeUnit(timeunit));
        }
    }
}

/// How to format the time stamps.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TimeFormat {
    /// How to format the numeric part of the time string.
    format: TimeStringFormatting,
    /// Insert a space between number and unit.
    show_space: bool,
    /// Display time unit.
    show_unit: bool,
}

impl Default for TimeFormat {
    fn default() -> Self {
        TimeFormat {
            format: TimeStringFormatting::No,
            show_space: true,
            show_unit: true,
        }
    }
}

impl TimeFormat {
    /// Create a new `TimeFormat` with custom settings.
    #[must_use]
    pub fn new(format: TimeStringFormatting, show_space: bool, show_unit: bool) -> Self {
        TimeFormat {
            format,
            show_space,
            show_unit,
        }
    }

    /// Set the format type.
    #[must_use]
    pub fn with_format(mut self, format: TimeStringFormatting) -> Self {
        self.format = format;
        self
    }

    /// Set whether to show space between number and unit.
    #[must_use]
    pub fn with_space(mut self, show_space: bool) -> Self {
        self.show_space = show_space;
        self
    }

    /// Set whether to show the time unit.
    #[must_use]
    pub fn with_unit(mut self, show_unit: bool) -> Self {
        self.show_unit = show_unit;
        self
    }
}

/// Draw the menu for selecting the time format.
pub fn timeformat_menu(ui: &mut Ui, msgs: &mut Vec<Message>, current_timeformat: &TimeFormat) {
    for time_string_format in enum_iterator::all::<TimeStringFormatting>() {
        if ui
            .radio(
                current_timeformat.format == time_string_format,
                if time_string_format == TimeStringFormatting::Locale {
                    format!(
                        "{time_string_format} ({locale})",
                        locale = get_locale().unwrap_or_else(|| "unknown".to_string())
                    )
                } else {
                    time_string_format.to_string()
                },
            )
            .clicked()
        {
            msgs.push(Message::SetTimeStringFormatting(Some(time_string_format)));
        }
    }
}

/// How to format the numeric part of the time string.
#[derive(Debug, Clone, Copy, Display, Eq, PartialEq, Serialize, Deserialize, Sequence)]
pub enum TimeStringFormatting {
    /// No additional formatting.
    No,

    /// Use the current locale to determine decimal separator, thousands separator, and grouping
    Locale,

    /// Use the SI standard: split into groups of three digits, unless there are exactly four
    /// for both integer and fractional part. Use space as group separator.
    SI,
}

/// Get rid of trailing zeros if the string contains a ., i.e., being fractional
/// If the resulting string ends with ., remove that as well.
fn strip_trailing_zeros_and_period(time: String) -> String {
    if !time.contains('.') {
        return time;
    }
    time.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Format number based on [`TimeStringFormatting`], i.e., possibly group digits together
/// and use correct separator for each group.
fn split_and_format_number(time: &str, format: TimeStringFormatting) -> String {
    match format {
        TimeStringFormatting::No => time.to_string(),
        TimeStringFormatting::Locale => format_locale(time, get_locale_format_cache()),
        TimeStringFormatting::SI => format_si(time),
    }
}

fn format_si(time: &str) -> String {
    if let Some((integer_part, fractional_part)) = time.split_once('.') {
        let integer_result = if integer_part.len() > 4 {
            group_n_chars(integer_part, 3).join(THIN_SPACE)
        } else {
            integer_part.to_string()
        };
        if fractional_part.len() > 4 {
            let reversed = fractional_part.chars().rev().collect::<String>();
            let reversed_fractional_parts = group_n_chars(&reversed, 3).join(THIN_SPACE);
            let fractional_result = reversed_fractional_parts.chars().rev().collect::<String>();
            format!("{integer_result}.{fractional_result}")
        } else {
            format!("{integer_result}.{fractional_part}")
        }
    } else if time.len() > 4 {
        group_n_chars(time, 3).join(THIN_SPACE)
    } else {
        time.to_string()
    }
}

fn format_locale(time: &str, cache: &LocaleFormatCache) -> String {
    if cache.grouping[0] > 0 {
        if let Some((integer_part, fractional_part)) = time.split_once('.') {
            let integer_result = group_n_chars(integer_part, cache.grouping[0] as usize)
                .join(cache.thousands_sep.as_str());
            format!(
                "{integer_result}{decimal_point}{fractional_part}",
                decimal_point = &cache.decimal_point
            )
        } else {
            group_n_chars(time, cache.grouping[0] as usize).join(cache.thousands_sep.as_str())
        }
    } else {
        time.to_string()
    }
}

/// Heuristically find a suitable time unit for the given time.
fn find_auto_scale(time: &BigInt, timescale: &TimeScale) -> TimeUnit {
    // In case of seconds, nothing to do as it is the largest supported unit
    // (unless we want to support minutes etc...)
    if matches!(timescale.unit, TimeUnit::Seconds) {
        return TimeUnit::Seconds;
    }
    let multiplier_digits = timescale.multiplier.unwrap_or(1).ilog10();
    let start_digits = -timescale.unit.exponent();
    for e in (3..=start_digits).step_by(3).rev() {
        if (time % pow10(e as u32 - multiplier_digits)).is_zero()
            && let Some(unit) = TimeUnit::from_exponent(e - start_digits)
        {
            return unit;
        }
    }
    timescale.unit
}

/// Formatter for time strings with caching of computed values.
/// Enables efficient formatting of multiple time values with the same timescale and format settings.
pub struct TimeFormatter {
    timescale: TimeScale,
    wanted_unit: TimeUnit,
    time_format: TimeFormat,
    /// Cached exponent difference (wanted - data)
    exponent_diff: i8,
    /// Cached unit string (empty if `show_unit` is false)
    unit_string: String,
    /// Cached space string (empty if `show_space` is false)
    space_string: String,
}

impl TimeFormatter {
    /// Create a new `TimeFormatter` with the given settings.
    #[must_use]
    pub fn new(timescale: &TimeScale, wanted_unit: &TimeUnit, time_format: &TimeFormat) -> Self {
        // Note: For Auto unit, we defer resolution to format() time since it depends on the value
        let (exponent_diff, unit_string) = if *wanted_unit == TimeUnit::Auto {
            // Use placeholder values for Auto - will be computed per-format call
            (0i8, String::new())
        } else {
            let wanted_exponent = wanted_unit.exponent();
            let data_exponent = timescale.unit.exponent();
            let exponent_diff = wanted_exponent - data_exponent;

            let unit_string = if time_format.show_unit {
                wanted_unit.to_string()
            } else {
                String::new()
            };

            (exponent_diff, unit_string)
        };

        TimeFormatter {
            timescale: timescale.clone(),
            wanted_unit: *wanted_unit,
            time_format: time_format.clone(),
            exponent_diff,
            unit_string,
            space_string: if time_format.show_space {
                " ".to_string()
            } else {
                String::new()
            },
        }
    }

    /// Format a single time value.
    #[must_use]
    pub fn format(&self, time: &BigInt) -> String {
        if self.wanted_unit == TimeUnit::None {
            return split_and_format_number(&time.to_string(), self.time_format.format);
        }

        // Handle Auto unit by resolving it for this specific time value
        let (exponent_diff, unit_string) = if self.wanted_unit == TimeUnit::Auto {
            let auto_unit = find_auto_scale(time, &self.timescale);
            let wanted_exponent = auto_unit.exponent();
            let data_exponent = self.timescale.unit.exponent();
            let exp_diff = wanted_exponent - data_exponent;

            let unit_str = if self.time_format.show_unit {
                auto_unit.to_string()
            } else {
                String::new()
            };

            (exp_diff, unit_str)
        } else {
            (self.exponent_diff, self.unit_string.clone())
        };

        let timestring = if exponent_diff >= 0 {
            let precision = exponent_diff as usize;
            strip_trailing_zeros_and_period(format!(
                "{scaledtime:.precision$}",
                scaledtime = BigRational::new(
                    time * self.timescale.multiplier.unwrap_or(1),
                    pow10(exponent_diff as u32)
                )
                .to_f64()
                .unwrap_or(f64::NAN)
            ))
        } else {
            (time * self.timescale.multiplier.unwrap_or(1) * pow10(-exponent_diff as u32))
                .to_string()
        };

        format!(
            "{scaledtime}{space}{unit}",
            scaledtime = split_and_format_number(&timestring, self.time_format.format),
            space = if unit_string.is_empty() {
                ""
            } else {
                &self.space_string
            },
            unit = &unit_string
        )
    }
}

/// Helper to compute powers of 10 efficiently.
/// Returns precomputed values for exponents 0-21, or computes on-demand for others.
fn pow10(exp: u32) -> BigInt {
    match exp {
        0 => BigInt::from(1),
        1 => BigInt::from(10),
        2 => BigInt::from(100),
        3 => BigInt::from(1000),
        6 => BigInt::from(1_000_000),
        9 => BigInt::from(1_000_000_000),
        12 => BigInt::from(1_000_000_000_000i64),
        15 => BigInt::from(1_000_000_000_000_000i64),
        18 => BigInt::from(1_000_000_000_000_000_000i64),
        21 => BigInt::from(1_000_000_000_000_000_000_000i128),
        _ => BigInt::from(10).pow(exp),
    }
}

/// Format the time string taking all settings into account.
/// This function delegates to `TimeFormatter` which handles the Auto timeunit.
#[must_use]
pub fn time_string(
    time: &BigInt,
    timescale: &TimeScale,
    wanted_timeunit: &TimeUnit,
    wanted_time_format: &TimeFormat,
) -> String {
    let formatter = TimeFormatter::new(timescale, wanted_timeunit, wanted_time_format);
    formatter.format(time)
}

/// Parse a time string and extract numeric value and unit (if present).
///
/// Parses strings like "100", "100ps", "100 ps", "1.5ms", etc.
/// Returns (numeric_value_str, optional_unit)
fn parse_time_input(input: &str) -> (String, Option<TimeUnit>) {
    let sorted_units =
        // Must be sorted by descending length to ensure correct matching (e.g., "ms" before "s")
        [
            ("zs", TimeUnit::ZeptoSeconds),
            ("as", TimeUnit::AttoSeconds),
            ("fs", TimeUnit::FemtoSeconds),
            ("ps", TimeUnit::PicoSeconds),
            ("ns", TimeUnit::NanoSeconds),
            ("μs", TimeUnit::MicroSeconds),
            ("us", TimeUnit::MicroSeconds), // Alternative spelling
            ("ms", TimeUnit::MilliSeconds),
            ("s", TimeUnit::Seconds),
        ];

    let trimmed = input.trim();

    for (unit_str, unit) in sorted_units {
        // Check if trimmed ends with this unit
        if trimmed.ends_with(unit_str) {
            let after_number = trimmed.len() - unit_str.len();

            // Support both adjacent and whitespace-separated units: "100ns", "100 ns", "100\tms".
            if after_number > 0 {
                let numeric = trimmed[..after_number].trim_end();
                if let Some(last_char) = numeric.chars().next_back()
                    && (last_char.is_ascii_digit() || last_char == '.')
                {
                    return (numeric.to_string(), Some(unit));
                }
            }
        }
    }

    (trimmed.to_string(), None)
}

/// Split a numeric string into integer and fractional parts.
///
/// Accepts "123", "123.", ".5", "123.456". Rejects negatives and non-digits.
fn split_numeric_parts(numeric_str: &str) -> Result<(String, String), String> {
    let trimmed = numeric_str.trim();
    if trimmed.is_empty() {
        return Err("Empty input".to_string());
    }
    if trimmed.starts_with('-') {
        return Err("Negative numbers not supported".to_string());
    }
    let normalized = trimmed.strip_prefix('+').unwrap_or(trimmed);
    let mut parts = normalized.split('.');
    let integer_part = parts.next().unwrap_or("");
    let fractional_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err("Invalid number: multiple decimal points".to_string());
    }

    // Validate all characters in one pass by chaining iterators
    let all_valid =
        (integer_part.chars().chain(fractional_part.chars())).all(|c| c.is_ascii_digit());
    if !all_valid {
        return Err(format!("Failed to parse '{numeric_str}' as number"));
    }

    let integer = if integer_part.is_empty() {
        "0".to_string()
    } else {
        integer_part.to_string()
    };
    Ok((integer, fractional_part.to_string()))
}

/// Convert a numeric string into an integer BigInt and normalized TimeUnit.
///
/// Decimal inputs are scaled by selecting a smaller unit so the numeric part is an integer.
/// Example: "1.5" with unit ns -> 1500 ps.
fn normalize_numeric_with_unit(
    numeric_str: &str,
    unit: TimeUnit,
) -> Result<(BigInt, TimeUnit), String> {
    let (integer_part, mut fractional_part) = split_numeric_parts(numeric_str)?;

    // Trim trailing zeros in fractional part to minimize scaling
    while fractional_part.ends_with('0') {
        fractional_part.pop();
    }

    if fractional_part.is_empty() {
        let value =
            BigInt::parse_bytes(integer_part.as_bytes(), 10).unwrap_or_else(|| BigInt::from(0));
        return Ok((value, unit));
    }

    let fractional_len = fractional_part.len();
    // Prevent excessively long fractional parts that exceed available time units
    if fractional_len > 21 {
        return Err("Too many decimal places (max 21 supported)".to_string());
    }

    let steps = fractional_len.div_ceil(3) as i8; // ceil(fractional_len / 3)
    let new_exponent = unit.exponent() - (steps * 3);
    let new_unit = TimeUnit::from_exponent(new_exponent)
        .ok_or_else(|| "Too much precision for available time units".to_string())?;

    let mut combined = integer_part;
    combined.push_str(&fractional_part);
    let mut value = BigInt::parse_bytes(combined.as_bytes(), 10).unwrap_or_else(|| BigInt::from(0));

    let extra_zeros = (steps as usize * 3).saturating_sub(fractional_len);
    if extra_zeros > 0 {
        let scale = pow10(extra_zeros as u32);
        value *= scale;
    }

    Ok((value, new_unit))
}

// Conversion helper lives on TimeInputState to avoid leaking conversion detail.

/// State for the time input widget.
#[derive(Clone, Debug)]
pub struct TimeInputState {
    /// User's text input
    input_text: String,
    /// Parsed time value (if valid)
    parsed_value: Option<BigInt>,
    /// Unit extracted from input (if present)
    input_unit: Option<TimeUnit>,
    /// Normalized unit after handling decimals
    normalized_unit: Option<TimeUnit>,
    /// Selected unit from dropdown (used when no unit in input)
    selected_unit: TimeUnit,
    /// Error message (if any)
    error: Option<String>,
}

impl Default for TimeInputState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            parsed_value: None,
            input_unit: None,
            normalized_unit: None,
            selected_unit: TimeUnit::NanoSeconds,
            error: None,
        }
    }
}

impl TimeInputState {
    /// Create a new time input state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the input text and parse it.
    pub fn update_input(&mut self, input: String) {
        self.input_text = input;
        let (numeric_str, unit) = parse_time_input(&self.input_text);
        self.input_unit = unit;

        let base_unit = self.input_unit.unwrap_or(self.selected_unit);
        match normalize_numeric_with_unit(&numeric_str, base_unit) {
            Ok((val, normalized_unit)) => {
                self.parsed_value = Some(val);
                self.normalized_unit = Some(normalized_unit);
                self.error = None;
            }
            Err(e) => {
                self.parsed_value = None;
                self.normalized_unit = None;
                self.error = Some(e);
            }
        }
    }

    /// Get the effective unit (from input or dropdown).
    pub fn effective_unit(&self) -> TimeUnit {
        self.normalized_unit
            .or(self.input_unit)
            .unwrap_or(self.selected_unit)
    }

    /// Convert the parsed value into timescale ticks.
    ///
    /// When conversion is not exact, this truncates toward zero.
    pub fn to_timescale_ticks(&self, timescale: &TimeScale) -> Option<BigInt> {
        let value = self.parsed_value.clone()?;
        let unit = self.effective_unit();
        let base_unit = if unit == TimeUnit::None {
            timescale.unit
        } else {
            unit
        };
        let unit_exp = base_unit.exponent();
        let data_exp = timescale.unit.exponent();
        let diff = unit_exp - data_exp;

        let mut result = value;
        if diff > 0 {
            let scale = pow10(diff as u32);
            result *= scale;
        } else if diff < 0 {
            let scale = pow10((-diff) as u32);
            result /= scale;
        }

        let multiplier = timescale.multiplier.unwrap_or(1);
        if multiplier != 1 {
            let mult = BigInt::from(multiplier);
            result /= mult;
        }

        Some(result)
    }
}

/// Render a time input widget in egui.
///
/// Shows a text input field and a unit selector dropdown (only enabled when no unit in input).
///
/// # Example
/// ```ignore
/// let mut time_state = TimeInputState::default();
///
/// time_input_widget(ui, "Enter time:", &mut time_state);
///
/// if let Some((value, timescale)) = time_state.to_timescale() {
///     println!("Time: {value} {:?}", timescale);
/// }
/// ```
pub fn time_input_widget(
    ui: &mut Ui,
    waves: &WaveData,
    msgs: &mut Vec<Message>,
    state: &mut TimeInputState,
    request_focus: bool,
) {
    ui.horizontal(|ui| {
        // Text input field
        let mut input = state.input_text.clone();
        let text_response = ui.add(
            egui::TextEdit::singleline(&mut input)
                .desired_width(100.0)
                .hint_text("e.g., 1.5ms"),
        );

        if request_focus && !text_response.has_focus() {
            text_response.request_focus();
            msgs.push(Message::SetRequestTimeEditFocus(false));
        }

        if text_response.changed() {
            state.update_input(input);
        }

        // Unit dropdown (only enabled if no unit in input)
        let dropdown_enabled = state.input_unit.is_none();

        if dropdown_enabled {
            egui::ComboBox::new("Unit", "")
                .width(32.0)
                .selected_text(state.selected_unit.to_string())
                .show_ui(ui, |ui| {
                    for unit in enum_iterator::all::<TimeUnit>() {
                        // Filter out Auto and None units from the dropdown
                        if !matches!(unit, TimeUnit::Auto | TimeUnit::None) {
                            ui.selectable_value(&mut state.selected_unit, unit, unit.to_string());
                        }
                    }
                });
        }

        // Handle focus
        if text_response.gained_focus() {
            msgs.push(Message::SetTimeEditFocused(true));
        }
        if text_response.lost_focus() {
            if text_response.ctx.input(|i| i.key_pressed(Key::Enter)) {
                // Enter pressed - trigger action if input is valid
                if let Some(time_stamp) =
                    state.to_timescale_ticks(&waves.inner.metadata().timescale)
                {
                    msgs.push(Message::GoToTime(Some(time_stamp), 0));
                }
            }
            msgs.push(Message::SetTimeEditFocused(false));
        }

        // Buttons
        let button_enabled = state.parsed_value.is_some();
        let goto_button = Button::new(RichText::new(icons::TARGET_FILL).heading()).frame(false);
        if ui
            .add_enabled(button_enabled, goto_button)
            .on_hover_text("Go to time")
            .clicked()
            && let Some(time_stamp) = state.to_timescale_ticks(&waves.inner.metadata().timescale)
        {
            msgs.push(Message::GoToTime(Some(time_stamp), 0));
        }
        let cursor_button = Button::new(RichText::new(icons::CURSOR_FILL).heading()).frame(false);
        if ui
            .add_enabled(button_enabled, cursor_button)
            .on_hover_text("Set cursor at time")
            .clicked()
            && let Some(time_stamp) = state.to_timescale_ticks(&waves.inner.metadata().timescale)
        {
            msgs.push(Message::CursorSet(time_stamp));
        }
    });
}

impl WaveData {
    pub fn draw_tick_line(&self, x: f32, ctx: &mut DrawingContext, stroke: &Stroke) {
        let Pos2 {
            x: x_pos,
            y: y_start,
        } = (ctx.to_screen)(x, 0.);
        ctx.painter.vline(
            x_pos,
            (y_start)..=(y_start + ctx.cfg.canvas_height),
            *stroke,
        );
    }
    /// Draw the text for each tick location.
    pub fn draw_ticks(
        &self,
        color: Color32,
        ticks: &[(String, f32)],
        ctx: &DrawingContext<'_>,
        y_offset: f32,
        align: Align2,
    ) {
        for (tick_text, x) in ticks {
            ctx.painter.text(
                (ctx.to_screen)(*x, y_offset),
                align,
                tick_text,
                FontId::proportional(ctx.cfg.text_size),
                color,
            );
        }
    }
}

impl SystemState {
    pub fn get_time_format(&self) -> TimeFormat {
        let time_format = self.user.config.default_time_format.clone();
        if let Some(time_string_format) = self.user.time_string_format {
            time_format.with_format(time_string_format)
        } else {
            time_format
        }
    }

    pub fn get_ticks_for_viewport_idx(
        &self,
        waves: &WaveData,
        viewport_idx: usize,
        cfg: &DrawConfig,
    ) -> Vec<(String, f32)> {
        self.get_ticks_for_viewport(waves, &waves.viewports[viewport_idx], cfg)
    }

    pub fn get_ticks_for_viewport(
        &self,
        waves: &WaveData,
        viewport: &Viewport,
        cfg: &DrawConfig,
    ) -> Vec<(String, f32)> {
        get_ticks_internal(
            viewport,
            &waves.inner.metadata().timescale,
            cfg.canvas_width,
            cfg.text_size,
            &self.user.wanted_timeunit,
            &self.get_time_format(),
            self.user.config.theme.ticks.density,
            &waves.safe_num_timestamps(),
        )
    }
}

/// Get suitable tick locations for the current view port.
/// The method is based on guessing the length of the time string and
/// is inspired by the corresponding code in Matplotlib.
#[allow(clippy::too_many_arguments)]
#[must_use]
fn get_ticks_internal(
    viewport: &Viewport,
    timescale: &TimeScale,
    frame_width: f32,
    text_size: f32,
    wanted_timeunit: &TimeUnit,
    time_format: &TimeFormat,
    density: f32,
    num_timestamps: &BigInt,
) -> Vec<(String, f32)> {
    let char_width = text_size * (20. / 31.);
    let rightexp = viewport
        .curr_right
        .absolute(num_timestamps)
        .inner()
        .abs()
        .log10()
        .round() as i16;
    let leftexp = viewport
        .curr_left
        .absolute(num_timestamps)
        .inner()
        .abs()
        .log10()
        .round() as i16;
    let max_labelwidth = f32::from(rightexp.max(leftexp) + 3) * char_width;
    let max_labels = ((frame_width * density) / max_labelwidth).floor() + 2.;
    let scale = 10.0f64.powf(
        ((viewport.curr_right - viewport.curr_left)
            .absolute(num_timestamps)
            .inner()
            / f64::from(max_labels))
        .log10()
        .floor(),
    );

    let mut ticks: Vec<(String, f32)> = [].to_vec();
    for step in &TICK_STEPS {
        let scaled_step = scale * step;
        let rounded_min_label_time =
            (viewport.curr_left.absolute(num_timestamps).inner() / scaled_step).floor()
                * scaled_step;
        let high = ((viewport.curr_right.absolute(num_timestamps).inner() - rounded_min_label_time)
            / scaled_step)
            .ceil() as f32
            + 1.;
        if high <= max_labels {
            let time_formatter = TimeFormatter::new(timescale, wanted_timeunit, time_format);
            ticks = (0..high as i16)
                .map(|v| {
                    BigInt::from((f64::from(v) * scaled_step + rounded_min_label_time) as i128)
                })
                .unique()
                .map(|tick| {
                    (
                        // Time string
                        time_formatter.format(&tick),
                        // X position
                        viewport.pixel_from_time(&tick, frame_width, num_timestamps),
                    )
                })
                .collect::<Vec<(String, f32)>>();
            break;
        }
    }
    ticks
}

#[cfg(test)]
mod test {
    use num::BigInt;

    use crate::time::{TimeFormat, TimeScale, TimeStringFormatting, TimeUnit, time_string};

    #[test]
    fn print_time_standard() {
        assert_eq!(
            time_string(
                &BigInt::from(103),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::FemtoSeconds
                },
                &TimeUnit::FemtoSeconds,
                &TimeFormat::default()
            ),
            "103 fs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat::default()
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MilliSeconds,
                &TimeFormat::default()
            ),
            "2.2 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::NanoSeconds,
                &TimeFormat::default()
            ),
            "2200000 ns"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::NanoSeconds
                },
                &TimeUnit::PicoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: false,
                    show_unit: true
                }
            ),
            "2200000ps"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: false,
                    show_unit: false
                }
            ),
            "22000"
        );
    }
    #[test]
    fn print_time_si() {
        assert_eq!(
            time_string(
                &BigInt::from(123456789010i128),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Seconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "123\u{2009}456.789\u{2009}01 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(1456789100i128),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Seconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "1456.7891 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "22\u{2009}200 μs"
        );
    }
    #[test]
    fn print_time_auto() {
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "2200 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "22 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(1500000000),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::PicoSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "1500 μs"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220 ms"
        );
        assert_eq!(
            time_string(
                &BigInt::from(220000),
                &TimeScale {
                    multiplier: Some(100),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "22 s"
        );
        assert_eq!(
            time_string(
                &BigInt::from(22000),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::Seconds
                },
                &TimeUnit::Auto,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220000 s"
        );
    }
    #[test]
    fn print_time_none() {
        assert_eq!(
            time_string(
                &BigInt::from(2200),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::None,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "2200"
        );
        assert_eq!(
            time_string(
                &BigInt::from(220),
                &TimeScale {
                    multiplier: Some(10),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::None,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "220"
        );
    }

    #[test]
    fn test_strip_trailing_zeros_and_period() {
        use crate::time::strip_trailing_zeros_and_period;

        assert_eq!(strip_trailing_zeros_and_period("123.000".into()), "123");
        assert_eq!(strip_trailing_zeros_and_period("123.450".into()), "123.45");
        assert_eq!(strip_trailing_zeros_and_period("123.456".into()), "123.456");
        assert_eq!(strip_trailing_zeros_and_period("123.".into()), "123");
        assert_eq!(strip_trailing_zeros_and_period("123".into()), "123");
        assert_eq!(strip_trailing_zeros_and_period("0.000".into()), "0");
        assert_eq!(strip_trailing_zeros_and_period("0.100".into()), "0.1");
        assert_eq!(strip_trailing_zeros_and_period(String::new()), "");
    }

    #[test]
    fn test_format_si() {
        use crate::time::format_si;

        // 4-digit rule: no grouping for 4 digits or less
        assert_eq!(format_si("1234.56"), "1234.56");
        assert_eq!(format_si("123.4"), "123.4");

        // Grouping for 5+ digits
        assert_eq!(format_si("12345.67"), "12\u{2009}345.67");
        assert_eq!(format_si("1234567.89"), "1\u{2009}234\u{2009}567.89");
        // No decimal part
        assert_eq!(format_si("12345"), "12\u{2009}345");
        assert_eq!(format_si("123"), "123");

        // Empty inputs
        assert_eq!(format_si("0.123"), "0.123");
        assert_eq!(format_si(""), "");

        // Decimal grouping
        assert_eq!(format_si("123.4567890"), "123.456\u{2009}789\u{2009}0");
    }

    #[test]
    fn test_time_unit_exponent() {
        // Test exponent method
        assert_eq!(TimeUnit::Seconds.exponent(), 0);
        assert_eq!(TimeUnit::MilliSeconds.exponent(), -3);
        assert_eq!(TimeUnit::MicroSeconds.exponent(), -6);
        assert_eq!(TimeUnit::NanoSeconds.exponent(), -9);
        assert_eq!(TimeUnit::PicoSeconds.exponent(), -12);
        assert_eq!(TimeUnit::FemtoSeconds.exponent(), -15);
        assert_eq!(TimeUnit::AttoSeconds.exponent(), -18);
        assert_eq!(TimeUnit::ZeptoSeconds.exponent(), -21);

        // Test from_exponent roundtrip
        for unit in [
            TimeUnit::Seconds,
            TimeUnit::MilliSeconds,
            TimeUnit::MicroSeconds,
            TimeUnit::NanoSeconds,
            TimeUnit::PicoSeconds,
            TimeUnit::FemtoSeconds,
            TimeUnit::AttoSeconds,
            TimeUnit::ZeptoSeconds,
        ] {
            assert_eq!(TimeUnit::from_exponent(unit.exponent()), Some(unit));
        }

        // Invalid exponents
        assert_eq!(TimeUnit::from_exponent(-5), None);
        assert_eq!(TimeUnit::from_exponent(1), None);
    }

    #[test]
    fn test_time_string_zero() {
        // Test zero values
        assert_eq!(
            time_string(
                &BigInt::from(0),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::MicroSeconds
                },
                &TimeUnit::MicroSeconds,
                &TimeFormat::default()
            ),
            "0 μs"
        );

        assert_eq!(
            time_string(
                &BigInt::from(0),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::Seconds
                },
                &TimeUnit::Auto,
                &TimeFormat::default()
            ),
            "0 s"
        );
    }

    #[test]
    fn test_time_string_large_numbers() {
        // Test very large numbers with SI formatting
        assert_eq!(
            time_string(
                &BigInt::from(999_999_999_999i64),
                &TimeScale {
                    multiplier: Some(1),
                    unit: TimeUnit::NanoSeconds
                },
                &TimeUnit::Seconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "999.999\u{2009}999\u{2009}999 s"
        );
    }

    #[test]
    fn test_time_string_no_multiplier() {
        // Test with None multiplier (raw ticks)
        assert_eq!(
            time_string(
                &BigInt::from(1234),
                &TimeScale {
                    multiplier: None,
                    unit: TimeUnit::NanoSeconds
                },
                &TimeUnit::NanoSeconds,
                &TimeFormat::default()
            ),
            "1234 ns"
        );
    }

    #[test]
    fn test_time_format_variations() {
        let value = BigInt::from(123456);
        let scale = TimeScale {
            multiplier: Some(1),
            unit: TimeUnit::NanoSeconds,
        };

        // Test all format variations
        assert_eq!(
            time_string(
                &value,
                &scale,
                &TimeUnit::NanoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: true
                }
            ),
            "123456 ns"
        );

        assert_eq!(
            time_string(
                &value,
                &scale,
                &TimeUnit::NanoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: false,
                    show_unit: true
                }
            ),
            "123456ns"
        );

        assert_eq!(
            time_string(
                &value,
                &scale,
                &TimeUnit::NanoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::No,
                    show_space: true,
                    show_unit: false
                }
            ),
            "123456"
        );

        assert_eq!(
            time_string(
                &value,
                &scale,
                &TimeUnit::NanoSeconds,
                &TimeFormat {
                    format: TimeStringFormatting::SI,
                    show_space: true,
                    show_unit: true
                }
            ),
            "123\u{2009}456 ns"
        );
    }

    #[test]
    fn test_find_auto_scale_seconds_passthrough() {
        use crate::time::find_auto_scale;

        let ts = TimeScale {
            unit: TimeUnit::Seconds,
            multiplier: Some(1),
        };
        assert_eq!(find_auto_scale(&BigInt::from(1), &ts), TimeUnit::Seconds);
        assert_eq!(
            find_auto_scale(&BigInt::from(1_234_567), &ts),
            TimeUnit::Seconds
        );
    }

    #[test]
    fn test_find_auto_scale_nanoseconds() {
        use crate::time::find_auto_scale;

        let ts = TimeScale {
            unit: TimeUnit::NanoSeconds,
            multiplier: Some(1),
        };

        // Divisible by 10^9 -> seconds
        assert_eq!(
            find_auto_scale(&BigInt::from(1_000_000_000i64), &ts),
            TimeUnit::Seconds
        );
        // Divisible by 10^6 -> milliseconds
        assert_eq!(
            find_auto_scale(&BigInt::from(1_000_000), &ts),
            TimeUnit::MilliSeconds
        );
        // Divisible by 10^3 -> microseconds
        assert_eq!(
            find_auto_scale(&BigInt::from(1_000), &ts),
            TimeUnit::MicroSeconds
        );
        // Not divisible by 10^3 -> stay at nanos
        assert_eq!(
            find_auto_scale(&BigInt::from(1234), &ts),
            TimeUnit::NanoSeconds
        );
    }

    #[test]
    fn test_find_auto_scale_microseconds_with_multiplier() {
        use crate::time::find_auto_scale;

        // multiplier: None (treated as 1)
        let ts_none = TimeScale {
            unit: TimeUnit::MicroSeconds,
            multiplier: None,
        };
        assert_eq!(
            find_auto_scale(&BigInt::from(1_000_000), &ts_none),
            TimeUnit::Seconds
        );
        assert_eq!(
            find_auto_scale(&BigInt::from(1_000), &ts_none),
            TimeUnit::MilliSeconds
        );
        assert_eq!(
            find_auto_scale(&BigInt::from(123), &ts_none),
            TimeUnit::MicroSeconds
        );

        // multiplier: Some(10) -> reduces required divisibility by 10^1
        let ts_mul10 = TimeScale {
            unit: TimeUnit::MicroSeconds,
            multiplier: Some(10),
        };
        assert_eq!(
            find_auto_scale(&BigInt::from(100_000), &ts_mul10),
            TimeUnit::Seconds
        );
        assert_eq!(
            find_auto_scale(&BigInt::from(100), &ts_mul10),
            TimeUnit::MilliSeconds
        );
        assert_eq!(
            find_auto_scale(&BigInt::from(123), &ts_mul10),
            TimeUnit::MicroSeconds
        );
    }

    #[test]
    fn test_find_auto_scale_femtoseconds() {
        use crate::time::find_auto_scale;

        let ts = TimeScale {
            unit: TimeUnit::FemtoSeconds,
            multiplier: Some(1),
        };
        // 10^15 fs = 1 s
        assert_eq!(
            find_auto_scale(&BigInt::from(10_i128.pow(15)), &ts),
            TimeUnit::Seconds
        );
        // 10^12 fs = 1 ms
        assert_eq!(
            find_auto_scale(&BigInt::from(10_i128.pow(12)), &ts),
            TimeUnit::MilliSeconds
        );
        // 10^9 fs = 1 μs
        assert_eq!(
            find_auto_scale(&BigInt::from(10_i128.pow(9)), &ts),
            TimeUnit::MicroSeconds
        );
        // 10^6 fs = 1 ns
        assert_eq!(
            find_auto_scale(&BigInt::from(10_i128.pow(6)), &ts),
            TimeUnit::NanoSeconds
        );
        // 10^3 fs = 1 ps
        assert_eq!(
            find_auto_scale(&BigInt::from(10_i128.pow(3)), &ts),
            TimeUnit::PicoSeconds
        );
        // Not divisible by 10^3 -> stay at fs
        assert_eq!(
            find_auto_scale(&BigInt::from(1), &ts),
            TimeUnit::FemtoSeconds
        );
    }

    #[test]
    fn test_locale_cache_en_us() {
        use crate::time::{create_cache, format_locale};
        use pure_rust_locales::Locale;

        let locale = Locale::en_US;
        let cache = create_cache(locale);

        // en_US uses period as decimal point and comma as thousands separator
        let result = format_locale("1234567.89", &cache);
        assert_eq!(result, "1,234,567.89");
    }

    #[test]
    fn test_locale_cache_de_de() {
        use crate::time::{create_cache, format_locale};
        use pure_rust_locales::Locale;

        let locale = Locale::de_DE;
        let cache = create_cache(locale);

        let result = format_locale("1234567.89", &cache);
        assert_eq!(result, "1.234.567,89");
    }

    #[test]
    fn test_locale_cache_fr_fr() {
        use crate::time::{create_cache, format_locale};
        use pure_rust_locales::Locale;

        let locale = Locale::fr_FR;
        let cache = create_cache(locale);

        // fr_FR typically uses space/thin_space and comma
        let result = format_locale("1234567.89", &cache);
        // Verify it produces valid output
        assert_eq!(result, "1\u{2009}234\u{2009}567,89");
    }

    #[test]
    fn test_locale_cache_small_numbers() {
        use crate::time::{create_cache, format_locale};
        use pure_rust_locales::Locale;

        let locale = Locale::en_US;
        let cache = create_cache(locale);

        // Numbers smaller than grouping threshold should remain unchanged
        assert_eq!(format_locale("123", &cache), "123");
        assert_eq!(format_locale("12.34", &cache), "12.34");
        assert_eq!(format_locale("0", &cache), "0");
    }

    #[test]
    fn test_locale_cache_consistency_across_locales() {
        use crate::time::create_cache;
        use pure_rust_locales::Locale;

        // Verify that creating cache for the same locale twice produces consistent results
        let cache1 = create_cache(Locale::en_US);
        let cache2 = create_cache(Locale::en_US);

        assert_eq!(cache1.thousands_sep, cache2.thousands_sep);
        assert_eq!(cache1.decimal_point, cache2.decimal_point);
        assert_eq!(cache1.grouping, cache2.grouping);
    }

    #[test]
    fn test_create_cache_from_various_locales() {
        use crate::time::{create_cache, format_locale};
        use pure_rust_locales::Locale;

        // Test that create_cache works for many Locale variants without panicking
        let locales = vec![
            Locale::en_US,
            Locale::de_DE,
            Locale::fr_FR,
            Locale::es_ES,
            Locale::it_IT,
            Locale::pt_BR,
            Locale::pt_PT,
            Locale::ja_JP,
            Locale::zh_CN,
            Locale::zh_TW,
            Locale::ru_RU,
            Locale::ko_KR,
            Locale::pl_PL,
            Locale::tr_TR,
            Locale::nl_NL,
            Locale::sv_SE,
            Locale::da_DK,
            Locale::fi_FI,
            Locale::el_GR,
            Locale::hu_HU,
            Locale::cs_CZ,
            Locale::ro_RO,
            Locale::th_TH,
            Locale::vi_VN,
            Locale::ar_SA,
            Locale::he_IL,
            Locale::id_ID,
            Locale::uk_UA,
            Locale::en_GB,
            Locale::en_AU,
            Locale::en_CA,
            Locale::en_NZ,
            Locale::en_IN,
            Locale::fr_CA,
            Locale::de_AT,
            Locale::de_CH,
            Locale::fr_CH,
            Locale::it_CH,
            Locale::es_MX,
            Locale::es_AR,
        ];

        for locale in locales {
            let cache = create_cache(locale);
            // Check so that it is not empty for a sample number
            assert!(
                !format_locale("1234567.89", &cache).is_empty(),
                "Failed for {locale:?}"
            );
        }
    }
}

#[cfg(test)]
mod get_ticks_tests {
    use super::*;
    use itertools::Itertools;
    use num::BigInt;

    // Basic smoke test: ensure we get at least one tick and that returned
    // pixel coordinates lie within the frame width.
    #[test]
    fn get_ticks_basic() {
        let vp = crate::viewport::Viewport::default();
        let timescale = TimeScale {
            unit: TimeUnit::MicroSeconds,
            multiplier: Some(1),
        };
        let frame_width = 800.0_f32;
        let text_size = 12.0_f32;
        let wanted = TimeUnit::MicroSeconds;
        let time_format = TimeFormat::default();
        let config = crate::config::SurferConfig::default();
        let num_timestamps = BigInt::from(1_000_000i64);

        let ticks = get_ticks_internal(
            &vp,
            &timescale,
            frame_width,
            text_size,
            &wanted,
            &time_format,
            config.theme.ticks.density,
            &num_timestamps,
        );

        assert!(!ticks.is_empty(), "expected at least one tick");

        // Check monotonic x positions and collect labels for uniqueness check
        let mut last_x = -1.0_f32;
        let mut labels: Vec<String> = Vec::with_capacity(ticks.len());
        for (label, x) in &ticks {
            assert!(
                *x >= last_x,
                "tick x not monotonic: {x} < {last_x} for label {label}"
            );
            last_x = *x;
            assert!(*x >= 0.0, "tick x < 0: {x}");
            assert!(
                *x <= frame_width,
                "tick x > frame_width: {x} > {frame_width}"
            );
            labels.push(label.clone());
        }
        // Labels should be unique
        let unique_labels = labels.iter().unique().count();
        assert_eq!(labels.len(), unique_labels, "duplicate tick labels found");
    }

    // Ensure tick generation produces a reasonable number of ticks when
    // viewport is zoomed and density is high.
    #[test]
    fn get_ticks_respects_frame_width_and_density() {
        let mut vp = crate::viewport::Viewport::default();
        // zoom to a narrower view
        vp.curr_left = crate::viewport::Relative(0.0);
        vp.curr_right = crate::viewport::Relative(0.1);

        let timescale = TimeScale {
            unit: TimeUnit::NanoSeconds,
            multiplier: Some(1),
        };
        let frame_width = 200.0_f32;
        let text_size = 10.0_f32;
        let wanted = TimeUnit::Auto;
        let time_format = TimeFormat {
            format: TimeStringFormatting::SI,
            show_space: true,
            show_unit: true,
        };

        let mut config = crate::config::SurferConfig::default();
        // make ticks dense
        config.theme.ticks.density = 1.0;

        let num_timestamps = BigInt::from(1_000_000i64);

        let ticks = get_ticks_internal(
            &vp,
            &timescale,
            frame_width,
            text_size,
            &wanted,
            &time_format,
            config.theme.ticks.density,
            &num_timestamps,
        );

        assert!(!ticks.is_empty(), "expected ticks even for narrow view");
        // expect a sane upper bound (protects against accidental infinite loops)
        assert!(ticks.len() < 200, "too many ticks: {}", ticks.len());

        // monotonic x positions and unique labels
        let mut last_x = -1.0_f32;
        let mut labels: Vec<String> = Vec::with_capacity(ticks.len());
        for (label, x) in &ticks {
            assert!(
                *x >= last_x,
                "tick x not monotonic: {x} < {last_x} for label {label}"
            );
            last_x = *x;
            assert!(*x >= 0.0, "tick x < 0: {x}");
            assert!(
                *x <= frame_width,
                "tick x > frame_width: {x} > {frame_width}"
            );
            labels.push(label.clone());
        }
        let unique_labels = labels.iter().unique().count();
        assert_eq!(labels.len(), unique_labels, "duplicate tick labels found");
    }
}

#[cfg(test)]
mod time_input_tests {
    use super::*;

    #[test]
    fn test_parse_time_input_simple() {
        let (num, unit) = parse_time_input("100");
        assert_eq!(num, "100");
        assert_eq!(unit, None);
    }

    #[test]
    fn test_parse_time_input_unit_only_no_panic() {
        // Unit-only input should not panic and should be treated as plain text.
        let (num, unit) = parse_time_input("ns");
        assert_eq!(num, "ns");
        assert_eq!(unit, None);
    }

    #[test]
    fn test_parse_time_input_with_unit_no_space() {
        let (num, unit) = parse_time_input("100ns");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::NanoSeconds));

        let (num, unit) = parse_time_input("50ps");
        assert_eq!(num, "50");
        assert_eq!(unit, Some(TimeUnit::PicoSeconds));

        let (num, unit) = parse_time_input("1.5ms");
        assert_eq!(num, "1.5");
        assert_eq!(unit, Some(TimeUnit::MilliSeconds));
    }

    #[test]
    fn test_parse_time_input_with_unit_space() {
        let (num, unit) = parse_time_input("100 ns");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::NanoSeconds));

        let (num, unit) = parse_time_input("1.5 ms");
        assert_eq!(num, "1.5");
        assert_eq!(unit, Some(TimeUnit::MilliSeconds));

        let (num, unit) = parse_time_input("100\tms");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::MilliSeconds));

        let (num, unit) = parse_time_input("100    ns");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::NanoSeconds));
    }

    #[test]
    fn test_parse_time_input_microseconds_unicode() {
        let (num, unit) = parse_time_input("100μs");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::MicroSeconds));

        let (num, unit) = parse_time_input("50 μs");
        assert_eq!(num, "50");
        assert_eq!(unit, Some(TimeUnit::MicroSeconds));
    }

    #[test]
    fn test_parse_time_input_microseconds_ascii() {
        // Support "us" as alternative to "μs"
        let (num, unit) = parse_time_input("100us");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::MicroSeconds));
    }

    #[test]
    fn test_parse_time_input_seconds() {
        let (num, unit) = parse_time_input("10s");
        assert_eq!(num, "10");
        assert_eq!(unit, Some(TimeUnit::Seconds));

        let (num, unit) = parse_time_input("0.5s");
        assert_eq!(num, "0.5");
        assert_eq!(unit, Some(TimeUnit::Seconds));
    }

    #[test]
    fn test_parse_time_input_femtoseconds() {
        let (num, unit) = parse_time_input("1000000fs");
        assert_eq!(num, "1000000");
        assert_eq!(unit, Some(TimeUnit::FemtoSeconds));
    }

    #[test]
    fn test_parse_time_input_with_whitespace() {
        let (num, unit) = parse_time_input("  100ns  ");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::NanoSeconds));
    }

    #[test]
    fn test_split_numeric_parts() {
        assert_eq!(
            split_numeric_parts("100").ok(),
            Some(("100".to_string(), "".to_string()))
        );
        assert_eq!(
            split_numeric_parts("100.").ok(),
            Some(("100".to_string(), "".to_string()))
        );
        assert_eq!(
            split_numeric_parts(".5").ok(),
            Some(("0".to_string(), "5".to_string()))
        );
        assert_eq!(
            split_numeric_parts("1.5").ok(),
            Some(("1".to_string(), "5".to_string()))
        );
    }

    #[test]
    fn test_split_numeric_parts_invalid() {
        assert!(split_numeric_parts("").is_err());
        assert!(split_numeric_parts("abc").is_err());
        assert!(split_numeric_parts("12.34.56").is_err());
        assert!(split_numeric_parts("-1").is_err());
    }

    #[test]
    fn test_normalize_numeric_with_unit_integer() {
        let (val, unit) = normalize_numeric_with_unit("100", TimeUnit::NanoSeconds).unwrap();
        assert_eq!(val, BigInt::from(100));
        assert_eq!(unit, TimeUnit::NanoSeconds);
    }

    #[test]
    fn test_normalize_numeric_with_unit_decimal_single_step() {
        let (val, unit) = normalize_numeric_with_unit("1.5", TimeUnit::NanoSeconds).unwrap();
        assert_eq!(val, BigInt::from(1500));
        assert_eq!(unit, TimeUnit::PicoSeconds);
    }

    #[test]
    fn test_normalize_numeric_with_unit_decimal_multi_step() {
        let (val, unit) = normalize_numeric_with_unit("1.2345", TimeUnit::NanoSeconds).unwrap();
        assert_eq!(val, BigInt::from(1_234_500));
        assert_eq!(unit, TimeUnit::FemtoSeconds);
    }

    #[test]
    fn test_time_input_state_default() {
        let state = TimeInputState::default();
        assert_eq!(state.input_text, "");
        assert_eq!(state.parsed_value, None);
        assert_eq!(state.input_unit, None);
        assert_eq!(state.normalized_unit, None);
        assert_eq!(state.selected_unit, TimeUnit::NanoSeconds);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_time_input_state_update_valid() {
        let mut state = TimeInputState::new();
        state.update_input("100ns".to_string());

        assert_eq!(state.input_text, "100ns");
        assert_eq!(state.parsed_value, Some(BigInt::from(100)));
        assert_eq!(state.input_unit, Some(TimeUnit::NanoSeconds));
        assert_eq!(state.normalized_unit, Some(TimeUnit::NanoSeconds));
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_time_input_state_update_invalid() {
        let mut state = TimeInputState::new();
        state.update_input("abc".to_string());

        assert_eq!(state.parsed_value, None);
        assert!(state.error.is_some());
    }

    #[test]
    fn test_time_input_state_update_decimal_unit_normalization() {
        let mut state = TimeInputState::new();
        state.update_input("1.5ns".to_string());

        assert_eq!(state.parsed_value, Some(BigInt::from(1500)));
        assert_eq!(state.input_unit, Some(TimeUnit::NanoSeconds));
        assert_eq!(state.normalized_unit, Some(TimeUnit::PicoSeconds));
        assert_eq!(state.effective_unit(), TimeUnit::PicoSeconds);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_time_input_state_to_timescale_ticks_invalid() {
        let state = TimeInputState::new();
        let timescale = TimeScale {
            unit: TimeUnit::NanoSeconds,
            multiplier: None,
        };

        assert_eq!(state.to_timescale_ticks(&timescale), None);
    }

    #[test]
    fn test_time_input_comprehensive_example() {
        // User types "2.5 ms"
        let mut state = TimeInputState::new();
        state.update_input("2.5 ms".to_string());

        // Parse should succeed
        assert_eq!(state.parsed_value, Some(BigInt::from(2500)));
        assert_eq!(state.input_unit, Some(TimeUnit::MilliSeconds));
        assert_eq!(state.effective_unit(), TimeUnit::MicroSeconds);
        assert_eq!(state.error, None);

        // Should be able to convert to timescale ticks
        let timescale = TimeScale {
            unit: TimeUnit::MicroSeconds,
            multiplier: None,
        };
        assert_eq!(
            state.to_timescale_ticks(&timescale),
            Some(BigInt::from(2500))
        );
    }

    #[test]
    fn test_parse_time_longest_match_first() {
        // "ms" should match before "s"
        let (num, unit) = parse_time_input("100ms");
        assert_eq!(num, "100");
        assert_eq!(unit, Some(TimeUnit::MilliSeconds));

        // Not just the "s"
        let (_, unit) = parse_time_input("100s");
        assert_ne!(unit, Some(TimeUnit::MilliSeconds));
    }

    #[test]
    fn test_parse_time_no_false_positives() {
        // These should not match units
        let (num, unit) = parse_time_input("mass");
        assert_eq!(num, "mass");
        assert_eq!(unit, None);

        let (num, unit) = parse_time_input("uses");
        assert_eq!(num, "uses");
        assert_eq!(unit, None);
    }

    #[test]
    fn test_time_input_state_clear() {
        let mut state = TimeInputState::new();
        state.update_input("100ns".to_string());
        assert!(state.parsed_value.is_some());

        // Clear by updating with empty string
        state.update_input(String::new());
        assert!(state.error.is_some());
    }
}
