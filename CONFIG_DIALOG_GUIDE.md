// CONFIG DIALOG SYSTEM - COMPREHENSIVE GUIDE
//
// This document explains the graphical configuration dialog system and how to use it.
//
// ============================================================================
// OVERVIEW
// ============================================================================
//
// The configuration dialog system provides a complete framework for automatically
// generating graphical user interfaces for configuration settings. It includes:
//
// 1. Metadata Description System
//    - Describe field types, constraints, and UI hints
//    - Provide human-readable labels and explanations
//    - Detailed enum variant descriptions
//
// 2. Compile-Time Code Generation
//    - Automatically extract configuration structure
//    - Generate metadata at compile time
//    - Keep UI in sync with config changes
//
// 3. Runtime UI Integration
//    - Form state management
//    - Validation and error handling
//    - Change tracking
//    - egui integration helpers
//
// ============================================================================
// QUICK START
// ============================================================================
//
// Step 1: Define Your Configuration Structs (already done in config.rs)
//
//     #[derive(Debug, Deserialize)]
//     pub struct SurferLayout {
//         pub show_hierarchy: bool,
//         pub show_menu: bool,
//         pub waveforms_text_size: f32,
//     }
//
// Step 2: Create Metadata Using the Helper Functions
//
//     use libsurfer::config_dialog::*;
//     use libsurfer::config_dialog_metadata::*;
//
//     let layout_metadata = LayoutConfigMetadata::metadata();
//
// Step 3: Create a Form and Render It
//
//     let mut form_state = ConfigFormState::with_metadata(&layout_metadata);
//     form_state.set_value("show_hierarchy", FormValue::Bool(true));
//
// ============================================================================
// MODULE BREAKDOWN
// ============================================================================
//
// config_dialog.rs
// ----------------
// Core metadata structures and builders:
// - FieldMetadata: Describes a single configuration field
// - FieldType: Enum of all possible field types (Bool, Float, Enum, etc.)
// - EnumVariantMetadata: Information about enum variants with descriptions
// - ConfigSectionMetadata: A collection of related fields
// - Builders: FieldMetadataBuilder, EnumVariantBuilder for easy construction
// - ConfigMetadata trait: Implement to provide metadata for a struct
// - ConfigMetadataRegistry: Central registry of all config metadata
//
// config_dialog_metadata.rs
// -------------------------
// Pre-built metadata for existing config structures:
// - Helper functions for common field types
// - Metadata generators for each config section (LayoutConfigMetadata, etc.)
// - Enum variant descriptions with explanations
// - Test implementations showing the pattern
//
// config_dialog_ui.rs
// -------------------
// Runtime UI and form handling:
// - ConfigFormState: Manages form state, dirty tracking, validation
// - FormValue: The actual values being edited
// - FormValidator: Validates values against field constraints
// - SuggestedWidget: Recommends UI widget types for fields
// - ConfigDialogBuilder: Helper for organizing fields into UI
//
// ============================================================================
// FIELD TYPES
// ============================================================================
//
// The system supports the following field types:
//
// Bool
//   Rendered as: Checkbox
//   Example: show_hierarchy: bool
//
// Float
//   Rendered as: Text input or Slider (if min/max provided)
//   Constraints: min, max, step
//   Example: waveforms_text_size: f32
//
// U16, I32, USize
//   Rendered as: Spinner or Slider (if bounds provided)
//   Constraints: min, max
//   Example: snap_distance: u16
//
// String
//   Rendered as: Text input or Text area (if multiline)
//   Constraints: max_length, multiline flag
//   Example: window_title: String
//
// Color
//   Rendered as: Color picker
//   Example: foreground_color: Color32
//
// Enum
//   Rendered as: Dropdown menu
//   Features: Each variant can have display name, description, and explanation
//   Example: ArrowKeyBindings (Edge, Scroll)
//
// Vector
//   Rendered as: List editor (add/remove elements)
//   Example: zoom_factors: Vec<f32>
//
// Struct
//   Rendered as: Nested form or collapsible section
//   Example: Nested configuration groups
//
// ============================================================================
// CREATING METADATA FOR YOUR CONFIG
// ============================================================================
//
// Method 1: Using Helper Functions (Recommended for now)
//
//     use libsurfer::config_dialog::*;
//     use libsurfer::config_dialog_metadata::*;
//
//     impl ConfigMetadata for MyConfig {
//         fn metadata() -> ConfigSectionMetadata {
//             ConfigSectionMetadata {
//                 name: "my_config".to_string(),
//                 label: "My Configuration".to_string(),
//                 description: "Settings for my feature".to_string(),
//                 fields: vec![
//                     bool_field("enabled", "Enable this feature")
//                         .order(1)
//                         .build(),
//                     float_field("sensitivity", "Sensitivity level", Some(0.1), Some(10.0), Some(0.1))
//                         .order(2)
//                         .build(),
//                 ],
//             }
//         }
//     }
//
// Method 2: Using Builders Directly
//
//     FieldMetadataBuilder::new("field_name", FieldType::Bool)
//         .label("Field Label")
//         .description("What this field does")
//         .explanation("Longer explanation for help tooltips")
//         .order(10)
//         .build()
//
// Method 3: Using Derive Macro (Future Enhancement)
//
//     #[derive(ConfigMetadata)]
//     #[config(label = "Layout Settings", description = "UI layout options")]
//     pub struct LayoutConfig {
//         #[config_field(
//             label = "Show Hierarchy",
//             description = "Display the signal hierarchy panel",
//         )]
//         pub show_hierarchy: bool,
//
//         #[config_field(
//             description = "Text size in points",
//             min = 6.0,
//             max = 32.0,
//             step = 0.5,
//         )]
//         pub text_size: f32,
//     }
//
// ============================================================================
// ENUM VARIANTS WITH DESCRIPTIONS
// ============================================================================
//
// Each enum variant can have its own description and explanation:
//
//     EnumVariantBuilder::new("Edge")
//         .display_name("Step by Edge")
//         .description("Arrow keys step to the next edge")
//         .explanation(
//             "When enabled, left/right arrow keys will step the cursor to the \
//              next rising or falling edge in the selected signal. This is useful \
//              for quickly finding transitions."
//         )
//         .build()
//
// The display_name is what's shown in the UI, while name matches the Rust enum variant.
// If display_name is not provided, name is used instead.
//
// ============================================================================
// FORM STATE AND VALIDATION
// ============================================================================
//
// Working with forms:
//
//     // Create form state
//     let mut state = ConfigFormState::with_metadata(&metadata);
//
//     // Set values
//     state.set_value("show_hierarchy", FormValue::Bool(true));
//     state.set_value("text_size", FormValue::Float(12.0));
//
//     // Check for unsaved changes
//     if state.has_changes() {
//         println!("User has made changes");
//     }
//
//     // Validate values
//     let errors = FormValidator::validate_form(&metadata, &state);
//     if !errors.is_empty() {
//         for (field, error) in errors {
//             println!("Field '{}': {}", field, error);
//         }
//     }
//
//     // Mark as saved
//     state.mark_all_clean();
//
// ============================================================================
// UI WIDGET SELECTION
// ============================================================================
//
// The system recommends appropriate widgets for each field:
//
//     let widget = SuggestedWidget::for_field(&field.field_type);
//     match widget {
//         SuggestedWidget::Checkbox => { /* Render checkbox */ }
//         SuggestedWidget::Slider => { /* Render slider */ }
//         SuggestedWidget::Dropdown => { /* Render dropdown */ }
//         SuggestedWidget::TextInput => { /* Render text field */ }
//         SuggestedWidget::ColorPicker => { /* Render color picker */ }
//         // ... etc
//     }
//
// ============================================================================
// INTEGRATION WITH EGUI
// ============================================================================
//
// Here's a sketch of how to integrate with egui:
//
//     use egui::Ui;
//
//     pub fn render_config_dialog(ui: &mut Ui, state: &mut ConfigFormState, metadata: &ConfigSectionMetadata) {
//         ui.heading(&metadata.label);
//         ui.label(&metadata.description);
//
//         let builder = ConfigDialogBuilder::new(metadata.clone());
//         for field in builder.fields_sorted() {
//             ui.label(&field.label);
//
//             match SuggestedWidget::for_field(&field.field_type) {
//                 SuggestedWidget::Checkbox => {
//                     if let Some(FormValue::Bool(checked)) = state.get_value(&field.name) {
//                         let mut new_value = *checked;
//                         ui.checkbox(&mut new_value, &field.description);
//                         if new_value != *checked {
//                             state.set_value(&field.name, FormValue::Bool(new_value));
//                         }
//                     }
//                 }
//                 SuggestedWidget::TextInput => {
//                     if let Some(FormValue::String(text)) = state.get_value(&field.name) {
//                         let mut new_text = text.clone();
//                         ui.text_edit_singleline(&mut new_text);
//                         if new_text != *text {
//                             state.set_value(&field.name, FormValue::String(new_text));
//                         }
//                     }
//                 }
//                 // ... etc for other widgets
//                 _ => {}
//             }
//
//             if let Some(error) = state.get_error(&field.name) {
//                 ui.colored_label(egui::Color32::RED, error);
//             }
//         }
//     }
//
// ============================================================================
// COMPILE-TIME GENERATION
// ============================================================================
//
// The system is designed to work with build.rs scripts to:
//
// 1. Parse config structures at compile time
// 2. Generate Rust code with metadata
// 3. Create UI code snippets
// 4. Validate consistency between config and UI
//
// Future enhancement: A proc-macro can automatically derive ConfigMetadata
// from structures, removing the need for manual metadata definition.
//
// ============================================================================
// BEST PRACTICES
// ============================================================================
//
// 1. Always provide descriptions
//    - Make it clear what each setting does
//    - Use simple, understandable language
//
// 2. Use explanations for complex settings
//    - Explain the impact of the setting
//    - Provide guidance on choosing values
//
// 3. Set appropriate constraints
//    - Provide min/max for numeric fields
//    - Set reasonable defaults
//
// 4. Order fields logically
//    - Most important settings first
//    - Group related settings (ordering helps with this)
//
// 5. For enums, document each variant
//    - Explain differences between options
//    - Help users choose the right one
//
// 6. Keep field names consistent
//    - Use snake_case in Rust
//    - Display names can use spaces and capitals
//
// ============================================================================
// MIGRATION FROM MANUAL UI
// ============================================================================
//
// If you have existing manual config UI:
//
// 1. Extract field names and types
// 2. Create FieldMetadata entries
// 3. Add helper functions from config_dialog_metadata.rs
// 4. Gradually replace UI code with form-based generation
// 5. Benefit from automatic validation and change tracking
//
// ============================================================================
// TESTING METADATA
// ============================================================================
//
// You can test metadata consistency:
//
//     #[test]
//     fn test_layout_metadata() {
//         let metadata = LayoutConfigMetadata::metadata();
//         assert!(!metadata.fields.is_empty());
//
//         // Check all field names match actual struct
//         for field in &metadata.fields {
//             assert!(!field.description.is_empty(),
//                 "Field {} is missing description", field.name);
//         }
//     }
//
// ============================================================================

// This file is documentation only. See module files for implementation.
