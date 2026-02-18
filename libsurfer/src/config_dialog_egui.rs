// egui Integration for Configuration Dialogs
//
// This module provides ready-to-use egui widgets for rendering configuration dialogs.
// It integrates with ConfigFormState and FieldMetadata to automatically generate the UI.

use crate::config_dialog::*;
use crate::config_dialog_ui::*;
use egui::{Color32, RichText, Ui};

/// Configuration for how the dialog should render
#[derive(Clone, Debug)]
pub struct DialogConfig {
    /// Width of the dialog (None = auto)
    pub width: Option<f32>,
    /// Show field descriptions below labels
    pub show_descriptions: bool,
    /// Show validation errors
    pub show_errors: bool,
    /// Spacing between fields
    pub spacing: f32,
}

impl Default for DialogConfig {
    fn default() -> Self {
        Self {
            width: None,
            show_descriptions: true,
            show_errors: true,
            spacing: 8.0,
        }
    }
}

/// Dialog action result after user interaction
#[derive(Clone, Debug, PartialEq)]
pub enum DialogResult {
    /// User pressed OK/Save
    Applied,
    /// User pressed Cancel/Close
    Cancelled,
    /// Dialog is still open
    Pending,
}

/// Renders a single field in the given UI context
/// Returns true if the field value changed
pub fn render_field(
    ui: &mut Ui,
    field: &FieldMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool {
    let mut changed = false;

    // Field label
    let _label_text = if config.show_descriptions && !field.description.is_empty() {
        format!("{}\n({})", field.label, field.description)
    } else {
        field.label.clone()
    };

    // Get or initialize the value
    let value = form_state
        .values
        .entry(field.name.clone())
        .or_insert_with(|| FormValue::String(String::new()))
        .clone();

    match (&field.field_type, value) {
        (FieldType::Bool, FormValue::Bool(current)) => {
            let mut new_value = current;
            if ui.checkbox(&mut new_value, &field.label).changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Bool(new_value));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::Float { min, max, step }, FormValue::Float(current)) => {
            let mut new_value = current;
            ui.label(&field.label);

            let response = if let (Some(min_val), Some(max_val)) = (min, max) {
                ui.add(
                    egui::Slider::new(&mut new_value, *min_val..=*max_val)
                        .step_by(step.map(|s| s as f64).unwrap_or(0.01)),
                )
            } else {
                let mut text = current.to_string();
                ui.text_edit_singleline(&mut text)
            };

            if response.changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Float(new_value));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::U16 { min, max }, FormValue::Integer(current)) => {
            let mut new_value = current as u16;
            ui.label(&field.label);

            let response = if let (Some(min_val), Some(max_val)) = (min, max) {
                let min_i = *min_val as i64;
                let max_i = *max_val as i64;
                let mut val_i = new_value as i64;
                let resp = ui.add(egui::Slider::new(&mut val_i, min_i..=max_i));
                new_value = val_i as u16;
                resp
            } else {
                let mut text = new_value.to_string();
                ui.text_edit_singleline(&mut text)
            };

            if response.changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Integer(new_value as i64));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::USize { min, max }, FormValue::Integer(current)) => {
            let mut new_value = current as usize;
            ui.label(&field.label);

            let response = if let (Some(min_val), Some(max_val)) = (min, max) {
                let min_i = *min_val as i64;
                let max_i = *max_val as i64;
                let mut val_i = new_value as i64;
                let resp = ui.add(egui::Slider::new(&mut val_i, min_i..=max_i));
                new_value = val_i as usize;
                resp
            } else {
                let mut text = new_value.to_string();
                ui.text_edit_singleline(&mut text)
            };

            if response.changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Integer(new_value as i64));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::I32 { min, max }, FormValue::Integer(current)) => {
            let mut new_value = current as i32;
            ui.label(&field.label);

            let response = if let (Some(min_val), Some(max_val)) = (min, max) {
                let mut val_i = new_value as i64;
                let resp = ui.add(egui::Slider::new(
                    &mut val_i,
                    *min_val as i64..=*max_val as i64,
                ));
                new_value = val_i as i32;
                resp
            } else {
                let mut text = new_value.to_string();
                ui.text_edit_singleline(&mut text)
            };

            if response.changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Integer(new_value as i64));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (
            FieldType::String {
                multiline,
                max_length: _,
            },
            FormValue::String(current),
        ) => {
            ui.label(&field.label);
            let mut new_value = current.clone();

            let response = if *multiline {
                ui.text_edit_multiline(&mut new_value)
            } else {
                ui.text_edit_singleline(&mut new_value)
            };

            if response.changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::String(new_value));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::Color, FormValue::Color(current)) => {
            ui.label(&field.label);
            let mut color = [current[0], current[1], current[2]];

            if ui.color_edit_button_srgb(&mut color).changed() {
                form_state
                    .values
                    .insert(field.name.clone(), FormValue::Color(color));
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        (FieldType::Enum { variants }, FormValue::Enum(current)) => {
            ui.label(&field.label);

            // Show as dropdown
            let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
            let selected_idx = variant_names
                .iter()
                .position(|n| n == &current)
                .unwrap_or(0);

            let mut new_idx = selected_idx;
            if egui::ComboBox::from_label("")
                .selected_text(current.as_str())
                .show_index(ui, &mut new_idx, variant_names.len(), |i| {
                    let name = &variant_names[i];
                    let variant = &variants[i];

                    match &variant.description {
                        Some(desc) if !desc.is_empty() => {
                            format!("{} ({})", name, desc)
                        }
                        _ => name.clone(),
                    }
                })
                .changed()
            {
                let new_variant = &variants[new_idx];
                form_state.values.insert(
                    field.name.clone(),
                    FormValue::Enum(new_variant.name.clone()),
                );
                form_state.dirty_fields.insert(field.name.clone(), true);
                changed = true;
            }
        }

        _ => {
            // Fallback: show field name and type mismatch
            ui.label(RichText::new(&field.label).color(Color32::RED));
            ui.label(format!("Type mismatch for {}", field.name));
        }
    }

    // Show validation error if present
    if config.show_errors {
        if let Some(error) = form_state.errors.get(&field.name) {
            ui.colored_label(Color32::RED, error);
        }
    }

    ui.separator();
    changed
}

/// Renders all fields in a form section
pub fn render_section(
    ui: &mut Ui,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool {
    let mut any_changed = false;

    // Section header
    ui.heading(&section.label);
    if !section.description.is_empty() {
        ui.label(RichText::new(&section.description).weak());
    }

    for field in &section.fields {
        if render_field(ui, field, form_state, config) {
            any_changed = true;
        }
    }

    any_changed
}

/// Renders a complete configuration dialog with OK/Cancel buttons
/// Returns the DialogResult indicating user action
///
/// # Example
/// ```ignore
/// let mut form_state = ConfigFormState::with_metadata(&metadata);
/// let mut config = DialogConfig::default();
///
/// loop {
///     egui::CentralPanel::default().show(&egui_ctx, |ui| {
///         let result = render_dialog(
///             ui,
///             "Settings",
///             &metadata,
///             &mut form_state,
///             &config,
///         );
///
///         match result {
///             DialogResult::Applied => { /* save changes */ break; }
///             DialogResult::Cancelled => { /* discard */ break; }
///             DialogResult::Pending => {} // keep rendering
///         }
///     });
/// }
/// ```
pub fn render_dialog(
    ui: &mut Ui,
    title: &str,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult {
    ui.heading(title);
    ui.separator();

    // Render all fields
    render_section(ui, section, form_state, config);

    ui.separator();

    // Action buttons
    let mut result = DialogResult::Pending;
    ui.horizontal(|ui| {
        if ui.button("OK").clicked() {
            result = DialogResult::Applied;
        }
        if ui.button("Cancel").clicked() {
            result = DialogResult::Cancelled;
        }
    });

    result
}

/// Easy wrapper for rendering multiple sections in a dialog
pub fn render_multi_section_dialog(
    ui: &mut Ui,
    title: &str,
    sections: &[ConfigSectionMetadata],
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult {
    ui.heading(title);
    ui.separator();

    // Render all sections with collapsible headers
    for section in sections {
        ui.collapsing(&section.label, |ui| {
            render_section(ui, section, form_state, config);
        });
    }

    ui.separator();

    // Action buttons
    let mut result = DialogResult::Pending;
    ui.horizontal(|ui| {
        if ui.button("OK").clicked() {
            result = DialogResult::Applied;
        }
        if ui.button("Cancel").clicked() {
            result = DialogResult::Cancelled;
        }
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_config_default() {
        let config = DialogConfig::default();
        assert_eq!(config.spacing, 8.0);
        assert!(config.show_descriptions);
        assert!(config.show_errors);
    }

    #[test]
    fn test_dialog_result_eq() {
        assert_eq!(DialogResult::Applied, DialogResult::Applied);
        assert_eq!(DialogResult::Cancelled, DialogResult::Cancelled);
        assert_eq!(DialogResult::Pending, DialogResult::Pending);
        assert_ne!(DialogResult::Applied, DialogResult::Cancelled);
    }
}
