//! Utility functions, typically inlined, for more readable code

use ecolor::Color32;
use egui::Modifiers;

use crate::{
    SystemState,
    clock_highlighting::ClockHighlightType,
    config::{ArrowKeyBindings, AutoLoad, PrimaryMouseDrag, TransitionValue},
    displayed_item::DisplayedItem,
    hierarchy::{HierarchyStyle, ParameterDisplayLocation},
};

impl SystemState {
    #[inline]
    pub fn get_item_text_color(&self, item: &DisplayedItem) -> Color32 {
        item.color()
            .and_then(|color| self.user.config.theme.get_color(color))
            .unwrap_or(self.user.config.theme.primary_ui_color.foreground)
    }

    #[inline]
    pub fn show_statusbar(&self) -> bool {
        self.user.show_statusbar.unwrap_or_else(|| {
            (self.user.waves.is_some() || self.progress_tracker.is_some())
                && self.user.config.layout.show_statusbar()
        })
    }

    #[inline]
    pub fn show_toolbar(&self) -> bool {
        self.user
            .show_toolbar
            .unwrap_or_else(|| self.user.config.layout.show_toolbar())
    }

    #[inline]
    pub fn show_overview(&self) -> bool {
        self.user
            .show_overview
            .unwrap_or_else(|| self.user.config.layout.show_overview())
    }

    #[inline]
    pub fn show_hierarchy(&self) -> bool {
        self.user
            .show_hierarchy
            .unwrap_or_else(|| self.user.config.layout.show_hierarchy())
    }

    #[inline]
    pub fn show_tooltip(&self) -> bool {
        self.user
            .show_tooltip
            .unwrap_or_else(|| self.user.config.layout.show_tooltip())
    }

    #[inline]
    pub fn show_scope_tooltip(&self) -> bool {
        self.user
            .show_scope_tooltip
            .unwrap_or_else(|| self.user.config.layout.show_scope_tooltip())
    }

    #[inline]
    pub fn show_ticks(&self) -> bool {
        self.user
            .show_ticks
            .unwrap_or_else(|| self.user.config.layout.show_ticks())
    }

    #[inline]
    pub fn show_menu(&self) -> bool {
        self.user
            .show_menu
            .unwrap_or_else(|| self.user.config.layout.show_menu())
    }

    #[inline]
    pub fn show_variable_indices(&self) -> bool {
        self.user
            .show_variable_indices
            .unwrap_or_else(|| self.user.config.layout.show_variable_indices())
    }

    #[inline]
    pub fn show_variable_direction(&self) -> bool {
        self.user
            .show_variable_direction
            .unwrap_or_else(|| self.user.config.layout.show_variable_direction())
    }

    #[inline]
    pub fn ui_zoom_factor(&self) -> f32 {
        self.user
            .ui_zoom_factor
            .unwrap_or_else(|| self.user.config.layout.default_zoom_factor())
    }

    #[inline]
    pub fn show_empty_scopes(&self) -> bool {
        self.user
            .show_empty_scopes
            .unwrap_or_else(|| self.user.config.layout.show_empty_scopes())
    }

    #[inline]
    pub fn show_hierarchy_icons(&self) -> bool {
        self.user
            .show_hierarchy_icons
            .unwrap_or_else(|| self.user.config.layout.show_hierarchy_icons())
    }

    #[inline]
    pub fn show_default_timeline(&self) -> bool {
        self.user
            .show_default_timeline
            .unwrap_or_else(|| self.user.config.layout.show_default_timeline())
    }

    #[inline]
    pub fn highlight_focused(&self) -> bool {
        self.user
            .highlight_focused
            .unwrap_or_else(|| self.user.config.layout.highlight_focused())
    }

    #[inline]
    pub fn fill_high_values(&self) -> bool {
        self.user
            .fill_high_values
            .unwrap_or_else(|| self.user.config.layout.fill_high_values())
    }

    #[inline]
    pub fn animation_enabled(&self) -> bool {
        self.user
            .animation_enabled
            .unwrap_or_else(|| self.user.config.animation_enabled())
    }

    #[inline]
    pub fn show_divider_text(&self) -> bool {
        self.user.config.show_divider_text()
    }

    #[inline]
    pub fn primary_button_drag_behavior(&self) -> PrimaryMouseDrag {
        self.user
            .primary_button_drag_behavior
            .unwrap_or_else(|| self.user.config.behavior.primary_button_drag_behavior())
    }

    #[inline]
    /// Return true if the combination of `primary_button_drag_behavior` and
    /// `modifiers` results in a measure, false otherwise.
    pub fn do_measure(&self, modifiers: &Modifiers) -> bool {
        let drag_behavior = self.primary_button_drag_behavior();
        (drag_behavior == PrimaryMouseDrag::Measure && !modifiers.shift)
            || (drag_behavior == PrimaryMouseDrag::Cursor && modifiers.shift)
    }

    #[inline]
    pub fn arrow_key_bindings(&self) -> ArrowKeyBindings {
        self.user
            .arrow_key_bindings
            .unwrap_or_else(|| self.user.config.behavior.arrow_key_bindings())
    }

    #[inline]
    pub fn clock_highlight_type(&self) -> ClockHighlightType {
        self.user
            .clock_highlight_type
            .unwrap_or_else(|| self.user.config.default_clock_highlight_type())
    }

    #[inline]
    pub fn hierarchy_style(&self) -> HierarchyStyle {
        self.user
            .hierarchy_style
            .unwrap_or_else(|| self.user.config.layout.hierarchy_style())
    }

    #[inline]
    pub fn autoreload_files(&self) -> AutoLoad {
        self.user
            .autoreload_files
            .unwrap_or_else(|| self.user.config.autoreload_files())
    }

    #[inline]
    pub fn autoload_sibling_state_files(&self) -> AutoLoad {
        self.user
            .autoload_sibling_state_files
            .unwrap_or_else(|| self.user.config.autoload_sibling_state_files())
    }

    #[inline]
    pub fn parameter_display_location(&self) -> ParameterDisplayLocation {
        self.user
            .parameter_display_location
            .unwrap_or_else(|| self.user.config.layout.parameter_display_location())
    }

    #[inline]
    pub fn use_dinotrace_style(&self) -> bool {
        self.user
            .use_dinotrace_style
            .unwrap_or_else(|| self.user.config.layout.use_dinotrace_style())
    }

    #[inline]
    pub fn transition_value(&self) -> TransitionValue {
        self.user
            .transition_value
            .unwrap_or_else(|| self.user.config.layout.transition_value())
    }

    #[inline]
    pub fn align_names_right(&self) -> bool {
        self.user
            .align_names_right
            .unwrap_or_else(|| self.user.config.layout.align_names_right())
    }
}
