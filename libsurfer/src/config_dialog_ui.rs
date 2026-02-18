// Configuration Dialog UI Integration
//
// This module provides the runtime utilities needed to build graphical configuration
// dialogs from the metadata. It handles UI rendering, form validation, and persisting
// changes back to the configuration.
//
// # Features
// - Dynamic UI generation based on FieldType
// - Form validation with constraints
// - Change tracking and serialization
// - Easy egui integration

use crate::config_dialog::*;
use std::collections::HashMap;

/// Represents the state of a configuration form being edited
pub struct ConfigFormState {
    /// Current values being edited (before saving)
    pub values: HashMap<String, FormValue>,
    /// Which fields have been modified
    pub dirty_fields: HashMap<String, bool>,
    /// Validation errors for fields
    pub errors: HashMap<String, String>,
}

/// A value in the configuration form
#[derive(Clone, Debug, PartialEq)]
pub enum FormValue {
    Bool(bool),
    Float(f32),
    Integer(i64),
    String(String),
    Enum(String),
    Color([u8; 3]),
    List(Vec<FormValue>),
}

impl FormValue {
    /// Convert to boolean representation
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FormValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Convert to float representation
    pub fn as_float(&self) -> Option<f32> {
        match self {
            FormValue::Float(f) => Some(*f),
            FormValue::Integer(i) => Some(*i as f32),
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn as_string(&self) -> Option<&str> {
        match self {
            FormValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Convert to enum representation
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            FormValue::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// Convert to color representation
    pub fn as_color(&self) -> Option<[u8; 3]> {
        match self {
            FormValue::Color(c) => Some(*c),
            _ => None,
        }
    }
}

impl ConfigFormState {
    /// Create a new form state with all fields marked as clean
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            dirty_fields: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    /// Initialize the form with metadata and default values
    pub fn with_metadata(metadata: &ConfigSectionMetadata) -> Self {
        let mut state = Self::new();
        for field in &metadata.fields {
            state.dirty_fields.insert(field.name.clone(), false);
        }
        state
    }

    /// Set a field value and mark it as dirty
    pub fn set_value(&mut self, name: impl Into<String>, value: FormValue) {
        let name = name.into();
        self.values.insert(name.clone(), value);
        self.dirty_fields.insert(name, true);
    }

    /// Get a field value
    pub fn get_value(&self, name: &str) -> Option<&FormValue> {
        self.values.get(name)
    }

    /// Mark a field as clean (no unsaved changes)
    pub fn mark_clean(&mut self, name: &str) {
        self.dirty_fields.insert(name.to_string(), false);
    }

    /// Mark all fields as clean
    pub fn mark_all_clean(&mut self) {
        for dirty in self.dirty_fields.values_mut() {
            *dirty = false;
        }
    }

    /// Check if any field has unsaved changes
    pub fn has_changes(&self) -> bool {
        self.dirty_fields.values().any(|&dirty| dirty)
    }

    /// Get all dirty field names
    pub fn dirty_fields(&self) -> Vec<&str> {
        self.dirty_fields
            .iter()
            .filter_map(|(name, &dirty)| if dirty { Some(name.as_str()) } else { None })
            .collect()
    }

    /// Add a validation error for a field
    pub fn add_error(&mut self, name: impl Into<String>, error: impl Into<String>) {
        self.errors.insert(name.into(), error.into());
    }

    /// Clear all errors
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Check if there are any validation errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get error for a specific field
    pub fn get_error(&self, name: &str) -> Option<&str> {
        self.errors.get(name).map(|e| e.as_str())
    }
}

impl Default for ConfigFormState {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates a form value against field constraints
pub struct FormValidator;

impl FormValidator {
    /// Validate a form value against a field's type constraints
    pub fn validate(field: &FieldMetadata, value: &FormValue) -> Result<(), String> {
        match (&field.field_type, value) {
            (FieldType::Bool, FormValue::Bool(_)) => Ok(()),
            (FieldType::Float { min, max, .. }, FormValue::Float(f)) => {
                if let Some(m) = min {
                    if f < m {
                        return Err(format!("Value must be at least {}", m));
                    }
                }
                if let Some(m) = max {
                    if f > m {
                        return Err(format!("Value must be at most {}", m));
                    }
                }
                Ok(())
            }
            (FieldType::USize { min, max }, FormValue::Integer(i)) => {
                if *i < 0 {
                    return Err("Value must be non-negative".to_string());
                }
                if let Some(m) = min {
                    if (*i as usize) < *m {
                        return Err(format!("Value must be at least {}", m));
                    }
                }
                if let Some(m) = max {
                    if (*i as usize) > *m {
                        return Err(format!("Value must be at most {}", m));
                    }
                }
                Ok(())
            }
            (FieldType::String { max_length, .. }, FormValue::String(s)) => {
                if let Some(max) = max_length {
                    if s.len() > *max {
                        return Err(format!("String must be at most {} characters", max));
                    }
                }
                Ok(())
            }
            (FieldType::Enum { variants }, FormValue::Enum(e)) => {
                if variants.iter().any(|v| &v.name == e) {
                    Ok(())
                } else {
                    Err(format!("'{}' is not a valid option", e))
                }
            }
            (FieldType::Color, FormValue::Color(_)) => Ok(()),
            _ => Err("Type mismatch between field and value".to_string()),
        }
    }

    /// Validate all values in a form state against metadata
    pub fn validate_form(
        metadata: &ConfigSectionMetadata,
        state: &ConfigFormState,
    ) -> Vec<(String, String)> {
        let mut errors = Vec::new();

        for field in &metadata.fields {
            if let Some(value) = state.get_value(&field.name) {
                if let Err(error) = Self::validate(field, value) {
                    errors.push((field.name.clone(), error));
                }
            } else if field.required {
                errors.push((field.name.clone(), "This field is required".to_string()));
            }
        }

        errors
    }
}

/// Suggested UI widget type based on field type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuggestedWidget {
    /// A checkbox for boolean values
    Checkbox,
    /// A text input field
    TextInput,
    /// A multi-line text editor
    TextArea,
    /// A dropdown selection
    Dropdown,
    /// A slider for numeric values
    Slider,
    /// A spinner (up/down buttons with text)
    Spinner,
    /// A color picker
    ColorPicker,
    /// A list/vector editor
    ListEditor,
    /// A nested form for structs
    NestedForm,
    /// Read-only label
    ReadOnly,
}

impl SuggestedWidget {
    /// Get the suggested widget type for a field
    pub fn for_field(field_type: &FieldType) -> Self {
        match field_type {
            FieldType::Bool => SuggestedWidget::Checkbox,
            FieldType::Float { min, max, .. } => {
                if min.is_some() && max.is_some() {
                    SuggestedWidget::Slider
                } else {
                    SuggestedWidget::TextInput
                }
            }
            FieldType::U16 { min, max } => {
                if min.is_some() && max.is_some() {
                    SuggestedWidget::Slider
                } else {
                    SuggestedWidget::Spinner
                }
            }
            FieldType::USize { min, max } => {
                if min.is_some() && max.is_some() {
                    SuggestedWidget::Slider
                } else {
                    SuggestedWidget::Spinner
                }
            }
            FieldType::I32 { min, max } => {
                if min.is_some() && max.is_some() {
                    SuggestedWidget::Slider
                } else {
                    SuggestedWidget::Spinner
                }
            }
            FieldType::String { multiline, .. } => {
                if *multiline {
                    SuggestedWidget::TextArea
                } else {
                    SuggestedWidget::TextInput
                }
            }
            FieldType::Color => SuggestedWidget::ColorPicker,
            FieldType::Enum { .. } => SuggestedWidget::Dropdown,
            FieldType::Vector { .. } => SuggestedWidget::ListEditor,
            FieldType::Struct { .. } => SuggestedWidget::NestedForm,
        }
    }
}

/// Helper for building configuration dialog forms
pub struct ConfigDialogBuilder {
    metadata: ConfigSectionMetadata,
}

impl ConfigDialogBuilder {
    /// Create a new dialog builder for a configuration section
    pub fn new(metadata: ConfigSectionMetadata) -> Self {
        Self { metadata }
    }

    /// Get the metadata
    pub fn metadata(&self) -> &ConfigSectionMetadata {
        &self.metadata
    }

    /// Get fields sorted by order
    pub fn fields_sorted(&self) -> Vec<&FieldMetadata> {
        let mut fields = self.metadata.fields.iter().collect::<Vec<_>>();
        fields.sort_by_key(|f| f.order);
        fields
    }

    /// Get fields grouped by category (prefix before first underscore)
    pub fn fields_grouped(&self) -> HashMap<String, Vec<&FieldMetadata>> {
        let mut groups: HashMap<String, Vec<&FieldMetadata>> = HashMap::new();

        for field in &self.metadata.fields {
            let category = field.name.split('_').next().unwrap_or("other").to_string();
            groups.entry(category).or_default().push(field);
        }

        // Sort fields within each group
        for fields in groups.values_mut() {
            fields.sort_by_key(|f| f.order);
        }

        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_value_conversions() {
        assert_eq!(FormValue::Bool(true).as_bool(), Some(true));
        assert_eq!(FormValue::Float(3.41).as_float(), Some(3.41));
        assert_eq!(FormValue::Integer(42).as_integer(), None);
        assert_eq!(
            FormValue::String("hello".to_string()).as_string(),
            Some("hello")
        );
    }

    #[test]
    fn test_form_state_tracking() {
        let mut state = ConfigFormState::new();
        state.set_value("field1", FormValue::Bool(true));

        assert!(state.has_changes());
        assert!(state.dirty_fields.contains_key("field1"));
        assert_eq!(state.get_value("field1"), Some(&FormValue::Bool(true)));

        state.mark_clean("field1");
        assert!(!state.has_changes());
    }

    #[test]
    fn test_form_validation() {
        let field = FieldMetadataBuilder::new(
            "test",
            FieldType::Float {
                min: Some(0.0),
                max: Some(100.0),
                step: None,
            },
        )
        .build();

        assert!(FormValidator::validate(&field, &FormValue::Float(50.0)).is_ok());
        assert!(FormValidator::validate(&field, &FormValue::Float(-10.0)).is_err());
        assert!(FormValidator::validate(&field, &FormValue::Float(150.0)).is_err());
    }

    #[test]
    fn test_suggested_widget() {
        assert_eq!(
            SuggestedWidget::for_field(&FieldType::Bool),
            SuggestedWidget::Checkbox
        );
        assert_eq!(
            SuggestedWidget::for_field(&FieldType::Color),
            SuggestedWidget::ColorPicker
        );
        assert_eq!(
            SuggestedWidget::for_field(&FieldType::Enum { variants: vec![] }),
            SuggestedWidget::Dropdown
        );
    }

    #[test]
    fn test_config_dialog_builder() {
        let metadata = ConfigSectionMetadata {
            name: "test".to_string(),
            label: "Test".to_string(),
            description: "Test section".to_string(),
            fields: vec![
                FieldMetadataBuilder::new("field1", FieldType::Bool)
                    .order(2)
                    .build(),
                FieldMetadataBuilder::new("field2", FieldType::Bool)
                    .order(1)
                    .build(),
            ],
        };

        let builder = ConfigDialogBuilder::new(metadata);
        let sorted = builder.fields_sorted();

        assert_eq!(sorted[0].name, "field2");
        assert_eq!(sorted[1].name, "field1");
    }
}

// Helper trait for FormValue
impl FormValue {
    /// Try to convert to integer
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            FormValue::Integer(i) => Some(*i),
            FormValue::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
}
