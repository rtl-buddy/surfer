//! Filtering of the variable list.
use derive_more::Display;
use egui::collapsing_header::CollapsingState;
use egui::{Button, Layout, RichText, TextEdit, Ui};
use egui_remixicon::icons;
use emath::{Align, Vec2};
use enum_iterator::Sequence;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use itertools::Itertools;
use regex::{Regex, RegexBuilder, escape};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

use crate::data_container::DataContainer::Transactions;
use crate::transaction_container::{StreamScopeRef, TransactionStreamRef};
use crate::variable_direction::VariableDirectionExt;
use crate::wave_container::{VariableRefExt, WaveContainer};
use crate::wave_data::ScopeType;
use crate::{SystemState, message::Message, wave_container::VariableRef};
use surfer_translation_types::VariableDirection;

use std::cmp::Ordering;

#[derive(Debug, Display, PartialEq, Serialize, Deserialize, Sequence)]
pub enum VariableNameFilterType {
    #[display("Fuzzy")]
    Fuzzy,

    #[display("Regular expression")]
    Regex,

    #[display("Variable starts with")]
    Start,

    #[display("Variable contains")]
    Contain,
}

#[derive(Serialize, Deserialize)]
pub struct VariableFilter {
    pub(crate) name_filter_type: VariableNameFilterType,
    pub(crate) name_filter_str: String,
    pub(crate) name_filter_case_insensitive: bool,

    pub(crate) include_inputs: bool,
    pub(crate) include_outputs: bool,
    pub(crate) include_inouts: bool,
    pub(crate) include_others: bool,

    pub(crate) group_by_direction: bool,
    #[serde(skip)]
    cache: RefCell<VariableFilterRegexCache>,
}

// Lightweight cache for compiled regex and fuzzy matcher to avoid repeated compilation
#[derive(Default)]
struct VariableFilterRegexCache {
    // For regex-based filters (Regex, Start, Contain)
    regex_pattern: Option<String>,
    regex_case_insensitive: bool,
    regex: Option<Regex>,
    regex_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub enum VariableIOFilterType {
    Input,
    Output,
    InOut,
    Other,
}

impl Default for VariableFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableFilter {
    #[must_use]
    pub fn new() -> VariableFilter {
        VariableFilter {
            name_filter_type: VariableNameFilterType::Contain,
            name_filter_str: String::new(),
            name_filter_case_insensitive: true,

            include_inputs: true,
            include_outputs: true,
            include_inouts: true,
            include_others: true,

            group_by_direction: false,
            cache: RefCell::new(Default::default()),
        }
    }

    fn name_filter_fn(&self) -> Box<dyn FnMut(&str) -> bool> {
        if self.name_filter_str.is_empty() {
            if self.name_filter_type == VariableNameFilterType::Regex {
                // Clear cached regex when filter string is empty
                let mut cache = self.cache.borrow_mut();
                cache.regex_pattern = None;
                cache.regex = None;
                cache.regex_error = None;
            }
            return Box::new(|_var_name| true);
        }

        // Copy the decisions/inputs out of self so the borrow of self.cache can be short-lived.
        let filter_type = &self.name_filter_type;
        let filter_str = self.name_filter_str.clone();
        let case_insensitive = self.name_filter_case_insensitive;

        // Prepare owned clones that we will move into the returned closure.
        let mut owned_regex: Option<Regex> = None;

        if *filter_type != VariableNameFilterType::Fuzzy
        // Short-lived borrow of the cache to potentially rebuild and to clone out owned values.
        {
            let mut cache = self.cache.borrow_mut();

            let pat = match filter_type {
                VariableNameFilterType::Regex => filter_str.clone(),
                VariableNameFilterType::Start => format!("^{}", escape(&filter_str)),
                VariableNameFilterType::Contain => escape(&filter_str),
                VariableNameFilterType::Fuzzy => unreachable!(),
            };
            let rebuild = (cache.regex_pattern.as_ref() != Some(&pat))
                || cache.regex_case_insensitive != case_insensitive
                || cache.regex.is_none();

            if rebuild {
                cache.regex_pattern = Some(pat.clone());
                cache.regex_case_insensitive = case_insensitive;
                match RegexBuilder::new(&pat)
                    .case_insensitive(case_insensitive)
                    .build()
                {
                    Ok(r) => {
                        cache.regex = Some(r);
                        cache.regex_error = None;
                    }
                    Err(e) => {
                        cache.regex = None;
                        cache.regex_error = Some(e.to_string());
                    }
                }
            }

            if let Some(r) = cache.regex.as_ref() {
                owned_regex = Some(r.clone());
            }
        } // cache borrow ends here

        // Now build the closure using only owned values (no borrow of cache/self remains).
        match filter_type {
            VariableNameFilterType::Fuzzy => {
                let mut matcher = SkimMatcherV2::default();
                matcher = if case_insensitive {
                    matcher.ignore_case()
                } else {
                    matcher.respect_case()
                };
                let pat = filter_str;
                Box::new(move |var_name| matcher.fuzzy_match(var_name, &pat).is_some())
            }
            VariableNameFilterType::Regex
            | VariableNameFilterType::Start
            | VariableNameFilterType::Contain => {
                if let Some(regex) = owned_regex {
                    Box::new(move |var_name| regex.is_match(var_name))
                } else {
                    Box::new(|_var_name| false)
                }
            }
        }
    }

    fn kind_filter(&self, vr: &VariableRef, wave_container_opt: Option<&WaveContainer>) -> bool {
        match get_variable_direction(vr, wave_container_opt) {
            VariableDirection::Input => self.include_inputs,
            VariableDirection::Output => self.include_outputs,
            VariableDirection::InOut => self.include_inouts,
            _ => self.include_others,
        }
    }

    pub fn matching_variables(
        &self,
        variables: &[VariableRef],
        wave_container_opt: Option<&WaveContainer>,
        full_path: bool,
    ) -> Vec<VariableRef> {
        let mut name_filter = self.name_filter_fn();
        if full_path {
            variables
                .iter()
                .filter(|&vr| self.kind_filter(vr, wave_container_opt))
                .filter(|&vr| name_filter(&vr.full_path_string()))
                .cloned()
                .collect_vec()
        } else {
            variables
                .iter()
                .filter(|&vr| self.kind_filter(vr, wave_container_opt))
                .filter(|&vr| name_filter(&vr.name))
                .cloned()
                .collect_vec()
        }
    }

    /// Returns true if the current `name_filter_type` is `Regex` and the cached
    /// compiled regex is invalid.
    pub fn is_regex_and_invalid(&self) -> bool {
        if self.name_filter_type != VariableNameFilterType::Regex {
            return false;
        }
        let cache = self.cache.borrow();
        cache.regex_error.is_some()
    }

    /// Returns the regex error message if the current filter type is Regex and
    /// the regex compilation failed.
    pub fn regex_error(&self) -> Option<String> {
        if self.name_filter_type != VariableNameFilterType::Regex {
            return None;
        }
        let cache = self.cache.borrow();
        cache.regex_error.clone()
    }
}

impl SystemState {
    pub fn draw_variable_filter_edit(
        &mut self,
        ui: &mut Ui,
        msgs: &mut Vec<Message>,
        full_path: bool,
    ) {
        ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
            CollapsingState::load_with_default_open(
                ui.ctx(),
                ui.make_persistent_id("variable_filter"),
                false,
            )
            .show_header(ui, |ui| {
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    let default_padding = ui.spacing().button_padding;
                    ui.spacing_mut().button_padding = Vec2 {
                        x: 0.,
                        y: default_padding.y,
                    };
                    if ui
                        .button(icons::ADD_FILL)
                        .on_hover_text("Add all variables from active Scope")
                        .clicked()
                    {
                        self.add_filtered_variables(msgs, full_path);
                    }
                    if ui
                        .add_enabled(
                            !self.user.variable_filter.name_filter_str.is_empty(),
                            Button::new(icons::CLOSE_FILL),
                        )
                        .on_hover_text("Clear filter")
                        .clicked()
                    {
                        self.user.variable_filter.name_filter_str.clear();
                    }

                    // Create text edit with isolated style for invalid regex
                    let is_invalid = self.user.variable_filter.is_regex_and_invalid();
                    let error_msg = self.user.variable_filter.regex_error();

                    // Save original style to restore after
                    let original_bg = ui.style().visuals.extreme_bg_color;

                    if is_invalid {
                        ui.style_mut().visuals.extreme_bg_color =
                            self.user.config.theme.accent_error.background;
                    }

                    let mut response = ui.add(
                        TextEdit::singleline(&mut self.user.variable_filter.name_filter_str)
                            .hint_text("Filter"),
                    );

                    // Restore original style immediately after rendering
                    ui.style_mut().visuals.extreme_bg_color = original_bg;

                    // Add hover text with error message if regex is invalid
                    if let Some(err) = error_msg {
                        response = response.on_hover_ui(|ui| {
                            ui.label("Invalid regex:");
                            // Use monospace font for error details as it contains position information
                            ui.label(RichText::new(err).family(epaint::FontFamily::Monospace));
                        });
                    }

                    // Handle focus
                    if response.gained_focus() {
                        msgs.push(Message::SetFilterFocused(true));
                    }
                    if response.lost_focus() {
                        msgs.push(Message::SetFilterFocused(false));
                    }
                    ui.spacing_mut().button_padding = default_padding;
                });
            })
            .body(|ui| self.variable_filter_type_menu(ui, msgs));
        });
    }

    fn add_filtered_variables(&mut self, msgs: &mut Vec<Message>, full_path: bool) {
        if let Some(waves) = self.user.waves.as_ref() {
            if full_path {
                let variables = waves.inner.as_waves().unwrap().variables();
                msgs.push(Message::AddVariables(
                    self.filtered_variables(&variables, false),
                ));
            } else {
                // Iterate over the reversed list to get
                // waves in the same order as the variable
                // list
                if let Some(active_scope) = waves.active_scope.as_ref() {
                    match active_scope {
                        ScopeType::WaveScope(active_scope) => {
                            let variables = waves
                                .inner
                                .as_waves()
                                .unwrap()
                                .variables_in_scope(active_scope);
                            msgs.push(Message::AddVariables(
                                self.filtered_variables(&variables, false),
                            ));
                        }
                        ScopeType::StreamScope(active_scope) => {
                            if let Transactions(inner) = &waves.inner {
                                match active_scope {
                                    StreamScopeRef::Root => {
                                        for stream in inner.get_streams() {
                                            msgs.push(Message::AddStreamOrGenerator(
                                                TransactionStreamRef::new_stream(
                                                    stream.id,
                                                    stream.name.clone(),
                                                ),
                                            ));
                                        }
                                    }
                                    StreamScopeRef::Stream(s) => {
                                        for gen_id in
                                            &inner.get_stream(s.stream_id).unwrap().generators
                                        {
                                            let generator = inner.get_generator(*gen_id).unwrap();

                                            msgs.push(Message::AddStreamOrGenerator(
                                                TransactionStreamRef::new_gen(
                                                    generator.stream_id,
                                                    generator.id,
                                                    generator.name.clone(),
                                                ),
                                            ));
                                        }
                                    }
                                    StreamScopeRef::Empty(_) => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn variable_filter_type_menu(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        // Checkbox wants a mutable bool reference but we don't have mutable self to give it a
        // mutable 'group_by_direction' directly. Plus we want to update things via a message. So
        // make a copy of the flag here that can be mutable and just ensure we update the actual
        // flag on a click.
        let mut name_filter_case_insensitive =
            self.user.variable_filter.name_filter_case_insensitive;

        if ui
            .checkbox(&mut name_filter_case_insensitive, "Case insensitive")
            .clicked()
        {
            msgs.push(Message::SetVariableNameFilterCaseInsensitive(
                !self.user.variable_filter.name_filter_case_insensitive,
            ));
        }

        ui.separator();

        for filter_type in enum_iterator::all::<VariableNameFilterType>() {
            if ui
                .radio(
                    self.user.variable_filter.name_filter_type == filter_type,
                    filter_type.to_string(),
                )
                .clicked()
            {
                msgs.push(Message::SetVariableNameFilterType(filter_type));
            }
        }

        ui.separator();

        // Checkbox wants a mutable bool reference but we don't have mutable self to give it a
        // mutable 'group_by_direction' directly. Plus we want to update things via a message. So
        // make a copy of the flag here that can be mutable and just ensure we update the actual
        // flag on a click.
        let mut group_by_direction = self.user.variable_filter.group_by_direction;

        if ui
            .checkbox(&mut group_by_direction, "Group by direction")
            .clicked()
        {
            msgs.push(Message::SetVariableGroupByDirection(
                !self.user.variable_filter.group_by_direction,
            ));
        }

        ui.separator();

        ui.horizontal(|ui| {
            let input = VariableDirection::Input;
            let output = VariableDirection::Output;
            let inout = VariableDirection::InOut;

            if ui
                .add(
                    Button::new(input.get_icon().unwrap())
                        .selected(self.user.variable_filter.include_inputs),
                )
                .on_hover_text("Show inputs")
                .clicked()
            {
                msgs.push(Message::SetVariableIOFilter(
                    VariableIOFilterType::Input,
                    !self.user.variable_filter.include_inputs,
                ));
            }

            if ui
                .add(
                    Button::new(output.get_icon().unwrap())
                        .selected(self.user.variable_filter.include_outputs),
                )
                .on_hover_text("Show outputs")
                .clicked()
            {
                msgs.push(Message::SetVariableIOFilter(
                    VariableIOFilterType::Output,
                    !self.user.variable_filter.include_outputs,
                ));
            }

            if ui
                .add(
                    Button::new(inout.get_icon().unwrap())
                        .selected(self.user.variable_filter.include_inouts),
                )
                .on_hover_text("Show inouts")
                .clicked()
            {
                msgs.push(Message::SetVariableIOFilter(
                    VariableIOFilterType::InOut,
                    !self.user.variable_filter.include_inouts,
                ));
            }

            if ui
                .add(
                    Button::new(icons::GLOBAL_LINE)
                        .selected(self.user.variable_filter.include_others),
                )
                .on_hover_text("Show others")
                .clicked()
            {
                msgs.push(Message::SetVariableIOFilter(
                    VariableIOFilterType::Other,
                    !self.user.variable_filter.include_others,
                ));
            }
        });
    }

    pub fn variable_cmp(
        &self,
        a: &VariableRef,
        b: &VariableRef,
        wave_container: Option<&WaveContainer>,
    ) -> Ordering {
        // Fast path: if not grouping by direction, just compare names
        if !self.user.variable_filter.group_by_direction {
            return numeric_sort::cmp(&a.name, &b.name);
        }

        let a_direction = get_variable_direction(a, wave_container);
        let b_direction = get_variable_direction(b, wave_container);

        if a_direction == b_direction {
            numeric_sort::cmp(&a.name, &b.name)
        } else if a_direction < b_direction {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    pub fn filtered_variables(
        &self,
        variables: &[VariableRef],
        full_path: bool,
    ) -> Vec<VariableRef> {
        let wave_container = match &self.user.waves {
            Some(wd) => wd.inner.as_waves(),
            None => None,
        };

        self.user
            .variable_filter
            .matching_variables(variables, wave_container, full_path)
            .iter()
            .sorted_by(|a, b| self.variable_cmp(a, b, wave_container))
            .cloned()
            .collect_vec()
    }
}

fn get_variable_direction(
    vr: &VariableRef,
    wave_container_opt: Option<&WaveContainer>,
) -> VariableDirection {
    match wave_container_opt {
        Some(wave_container) => wave_container
            .variable_meta(vr)
            .map_or(VariableDirection::Unknown, |m| {
                m.direction.unwrap_or(VariableDirection::Unknown)
            }),
        None => VariableDirection::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_filter_matches_all() {
        let filter = VariableFilter::new();
        assert!(filter.name_filter_str.is_empty());

        let mut filter_fn = filter.name_filter_fn();
        // Empty filter should match everything
        assert!(filter_fn("test"));
        assert!(filter_fn("anything"));
        assert!(filter_fn(""));
    }

    #[test]
    fn test_contain_filter_basic() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Contain;
        filter.name_filter_str = "clock".to_string();
        filter.name_filter_case_insensitive = false;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("clock"));
        assert!(filter_fn("my_clock"));
        assert!(filter_fn("clock_signal"));
        assert!(filter_fn("sys_clock_div"));
        assert!(!filter_fn("clk"));
        assert!(!filter_fn("CLOCK")); // Case sensitive
    }

    #[test]
    fn test_contain_filter_case_insensitive() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Contain;
        filter.name_filter_str = "Clock".to_string();
        filter.name_filter_case_insensitive = true;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("clock"));
        assert!(filter_fn("CLOCK"));
        assert!(filter_fn("ClOcK"));
        assert!(filter_fn("my_Clock_signal"));
        assert!(!filter_fn("clk"));
    }

    #[test]
    fn test_start_filter() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Start;
        filter.name_filter_str = "sys".to_string();
        filter.name_filter_case_insensitive = false;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("sys"));
        assert!(filter_fn("sys_clock"));
        assert!(filter_fn("system"));
        assert!(!filter_fn("my_sys"));
        assert!(!filter_fn("SYS")); // Case sensitive
    }

    #[test]
    fn test_start_filter_case_insensitive() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Start;
        filter.name_filter_str = "Sys".to_string();
        filter.name_filter_case_insensitive = true;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("sys"));
        assert!(filter_fn("SYS_CLOCK"));
        assert!(filter_fn("System"));
        assert!(!filter_fn("my_sys"));
    }

    #[test]
    fn test_regex_filter_valid() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Regex;
        filter.name_filter_str = r"^clk_\d+$".to_string();
        filter.name_filter_case_insensitive = false;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("clk_0"));
        assert!(filter_fn("clk_123"));
        assert!(!filter_fn("clk_"));
        assert!(!filter_fn("clk_abc"));
        assert!(!filter_fn("my_clk_0"));
    }

    #[test]
    fn test_regex_filter_invalid() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Regex;
        filter.name_filter_str = "[invalid(".to_string(); // Invalid regex
        filter.name_filter_case_insensitive = false;

        // Should not match anything when regex is invalid
        let mut filter_fn = filter.name_filter_fn();
        assert!(!filter_fn("anything"));
        assert!(!filter_fn("test"));

        // Should report as invalid
        assert!(filter.is_regex_and_invalid());

        // Should have an error message
        let error = filter.regex_error();
        assert!(error.is_some());
        assert!(error.unwrap().contains("unclosed"));
    }

    #[test]
    fn test_is_regex_and_invalid_only_for_regex_type() {
        let mut filter = VariableFilter::new();
        filter.name_filter_str = "[invalid(".to_string();

        // Not regex type, so should return false even with invalid pattern
        filter.name_filter_type = VariableNameFilterType::Contain;
        // Cache rebuild
        let _ = filter.name_filter_fn();
        assert!(!filter.is_regex_and_invalid());

        filter.name_filter_type = VariableNameFilterType::Start;
        // Cache rebuild
        let _ = filter.name_filter_fn();
        assert!(!filter.is_regex_and_invalid());

        filter.name_filter_type = VariableNameFilterType::Fuzzy;
        // Cache rebuild
        let _ = filter.name_filter_fn();
        assert!(!filter.is_regex_and_invalid());

        // Only regex type should check validity
        filter.name_filter_type = VariableNameFilterType::Regex;
        // Cache rebuild
        let _ = filter.name_filter_fn();
        assert!(filter.is_regex_and_invalid());
    }

    #[test]
    fn test_regex_error_only_for_regex_type() {
        let mut filter = VariableFilter::new();
        filter.name_filter_str = "[invalid(".to_string();

        // Force cache rebuild
        filter.name_filter_type = VariableNameFilterType::Regex;
        let _ = filter.name_filter_fn();

        // Now switch to non-regex types
        filter.name_filter_type = VariableNameFilterType::Contain;
        assert!(filter.regex_error().is_none());

        filter.name_filter_type = VariableNameFilterType::Start;
        assert!(filter.regex_error().is_none());

        // Back to regex should show error
        filter.name_filter_type = VariableNameFilterType::Regex;
        assert!(filter.regex_error().is_some());
    }

    #[test]
    fn test_fuzzy_filter() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Fuzzy;
        filter.name_filter_str = "clk".to_string();
        filter.name_filter_case_insensitive = true;

        let mut filter_fn = filter.name_filter_fn();
        // Fuzzy should match with characters in order
        assert!(filter_fn("clock"));
        assert!(filter_fn("c_l_k"));
        assert!(filter_fn("call_lock"));
        assert!(!filter_fn("kclc")); // Wrong order
    }

    #[test]
    fn test_special_chars_escaped_in_contain() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Contain;
        // These are regex special chars that should be escaped
        filter.name_filter_str = "sig[0]".to_string();
        filter.name_filter_case_insensitive = false;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("sig[0]"));
        assert!(filter_fn("my_sig[0]_data"));
        assert!(!filter_fn("sig0")); // Should require literal brackets
        assert!(!filter_fn("siga")); // [0] is escaped, not a regex char class
    }

    #[test]
    fn test_special_chars_escaped_in_start() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Start;
        filter.name_filter_str = "data.value".to_string();
        filter.name_filter_case_insensitive = false;

        let mut filter_fn = filter.name_filter_fn();
        assert!(filter_fn("data.value"));
        assert!(filter_fn("data.value_out"));
        assert!(!filter_fn("dataxvalue")); // Dot should be literal
        assert!(!filter_fn("my_data.value")); // Must start with pattern
    }

    #[test]
    fn test_cache_reuses_compiled_regex() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Regex;
        filter.name_filter_str = r"\d+".to_string();
        filter.name_filter_case_insensitive = false;

        // First call compiles
        let mut fn1 = filter.name_filter_fn();
        assert!(fn1("123"));

        // Second call should reuse cached regex
        let mut fn2 = filter.name_filter_fn();
        assert!(fn2("456"));

        // Verify cache has the pattern
        let cache = filter.cache.borrow();
        assert_eq!(cache.regex_pattern.as_ref().unwrap(), r"\d+");
        assert!(cache.regex.is_some());
    }

    #[test]
    fn test_cache_rebuilds_on_pattern_change() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Contain;
        filter.name_filter_str = "old".to_string();
        filter.name_filter_case_insensitive = false;

        let mut fn1 = filter.name_filter_fn();
        assert!(fn1("old_value"));

        // Change pattern
        filter.name_filter_str = "new".to_string();
        let mut fn2 = filter.name_filter_fn();
        assert!(fn2("new_value"));
        assert!(!fn2("old_value"));
    }

    #[test]
    fn test_cache_rebuilds_on_case_sensitivity_change() {
        let mut filter = VariableFilter::new();
        filter.name_filter_type = VariableNameFilterType::Contain;
        filter.name_filter_str = "Test".to_string();
        filter.name_filter_case_insensitive = false;

        let mut fn1 = filter.name_filter_fn();
        assert!(!fn1("test")); // Case sensitive

        // Change case sensitivity
        filter.name_filter_case_insensitive = true;
        let mut fn2 = filter.name_filter_fn();
        assert!(fn2("test")); // Now case insensitive
    }

    #[test]
    fn test_default_filter_settings() {
        let filter = VariableFilter::new();

        assert_eq!(filter.name_filter_type, VariableNameFilterType::Contain);
        assert_eq!(filter.name_filter_str, "");
        assert!(filter.name_filter_case_insensitive);

        assert!(filter.include_inputs);
        assert!(filter.include_outputs);
        assert!(filter.include_inouts);
        assert!(filter.include_others);

        assert!(!filter.group_by_direction);
    }
}
