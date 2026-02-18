# egui Integration Guide

The `config_dialog_egui` module provides ready-to-use egui widgets for rendering configuration dialogs. It automatically handles UI rendering, state management, and validation for all field types.

## Quick Start

```rust
use libsurfer::config_dialog::{ConfigMetadata, ConfigSectionMetadata};
use libsurfer::config_dialog_ui::ConfigFormState;
use libsurfer::config_dialog_egui::{render_dialog, DialogConfig, DialogResult};

// Get metadata for your config struct
let metadata = libsurfer::config_metadata::SurferLayoutMetadata::metadata();

// Create form state
let mut form_state = ConfigFormState::with_metadata(&metadata);

// In your egui UI loop:
egui::CentralPanel::default().show(&egui_ctx, |ui| {
    let result = render_dialog(
        ui,
        "Settings",
        &metadata,
        &mut form_state,
        &DialogConfig::default(),
    );

    match result {
        DialogResult::Applied => {
            // Save changes to config
        }
        DialogResult::Cancelled => {
            // Discard changes
        }
        DialogResult::Pending => {
            // Dialog still open, keep rendering
        }
    }
});
```

## Core Functions

### `render_field()`

Renders a single configuration field with appropriate UI widget:

```rust
pub fn render_field(
    ui: &mut Ui,
    field: &FieldMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool  // Returns true if value changed
```

**Example:**
```rust
for field in &section.fields {
    if render_field(ui, field, &mut form_state, &config) {
        println!("Field {} changed", field.name);
    }
}
```

### `render_section()`

Renders all fields in a section with header:

```rust
pub fn render_section(
    ui: &mut Ui,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool  // Returns true if any field changed
```

**Example:**
```rust
egui::Window::new("Config").show(&egui_ctx, |ui| {
    render_section(ui, &metadata, &mut form_state, &config);
});
```

### `render_dialog()`

Renders a complete dialog with OK/Cancel buttons:

```rust
pub fn render_dialog(
    ui: &mut Ui,
    title: &str,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult
```

**Returns:**
- `DialogResult::Applied` - User clicked OK
- `DialogResult::Cancelled` - User clicked Cancel
- `DialogResult::Pending` - Dialog is still open

### `render_multi_section_dialog()`

Renders multiple sections in a single dialog with collapsible sections:

```rust
pub fn render_multi_section_dialog(
    ui: &mut Ui,
    title: &str,
    sections: &[ConfigSectionMetadata],
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult
```

## Field Type Rendering

Each `FieldType` is rendered with an appropriate egui widget:

| FieldType | Widget | Notes |
|-----------|--------|-------|
| `Bool` | Checkbox | Yes/No toggle |
| `Float {min, max, step}` | Slider or TextEdit | Slider if min/max provided |
| `U16 {min, max}` | Slider or TextEdit | Limited to 16-bit unsigned |
| `USize {min, max}` | Slider or TextEdit | Unsigned integer |
| `I32 {min, max}` | Slider or TextEdit | Signed 32-bit integer |
| `String {multiline, max_length}` | TextEdit | Multi-line if configured |
| `Color` | Color picker | RGB color selector |
| `Enum {variants}` | ComboBox | Shows variant descriptions |
| `Vector` | Not implemented | Future feature |
| `Struct` | Not implemented | Future feature |

## DialogConfig

Configure how dialogs should render:

```rust
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

// Default configuration
let config = DialogConfig::default();
// width: None (auto)
// show_descriptions: true
// show_errors: true
// spacing: 8.0

// Custom configuration
let config = DialogConfig {
    width: Some(400.0),
    show_descriptions: false,
    show_errors: true,
    spacing: 12.0,
};
```

## Complete Example: Settings Window

```rust
use libsurfer::config_dialog::{ConfigMetadata, ConfigSectionMetadata};
use libsurfer::config_dialog_ui::ConfigFormState;
use libsurfer::config_dialog_egui::{render_dialog, DialogConfig, DialogResult};

struct AppState {
    show_settings: bool,
    form_state: ConfigFormState,
}

impl AppState {
    fn new() -> Self {
        let metadata = libsurfer::config_metadata::SurferLayoutMetadata::metadata();
        let form_state = ConfigFormState::with_metadata(&metadata);

        Self {
            show_settings: false,
            form_state,
        }
    }

    fn show_settings_window(&mut self, egui_ctx: &egui::Context) {
        let metadata = libsurfer::config_metadata::SurferLayoutMetadata::metadata();
        let config = DialogConfig::default();

        egui::Window::new("Settings")
            .open(&mut self.show_settings)
            .resizable(true)
            .show(egui_ctx, |ui| {
                let result = render_dialog(
                    ui,
                    "Application Settings",
                    &metadata,
                    &mut self.form_state,
                    &config,
                );

                match result {
                    DialogResult::Applied => {
                        // Collect changes
                        let changes = self.form_state.get_changed_values();
                        println!("Saving: {:?}", changes);

                        // TODO: Apply changes to actual config
                        self.show_settings = false;
                    }
                    DialogResult::Cancelled => {
                        // Discard changes
                        self.form_state.reset();
                        self.show_settings = false;
                    }
                    DialogResult::Pending => {
                        // Keep rendering
                    }
                }
            });
    }
}
```

## Multi-Section Example

```rust
use libsurfer::config_metadata::*;
use libsurfer::config_dialog_ui::ConfigFormState;
use libsurfer::config_dialog_egui::{render_multi_section_dialog, DialogConfig, DialogResult};

let sections = vec![
    SurferLayoutMetadata::metadata(),
    SurferBehaviorMetadata::metadata(),
    SurferThemeMetadata::metadata(),
];

let mut form_state = ConfigFormState::new();

egui::Window::new("Advanced Settings").show(&egui_ctx, |ui| {
    let result = render_multi_section_dialog(
        ui,
        "Advanced Settings",
        &sections,
        &mut form_state,
        &DialogConfig::default(),
    );

    if result == DialogResult::Applied {
        // Save all sections
    }
});
```

## Field Rendering Details

### Boolean Fields

```rust
// Renders as: [x] Show Hierarchy
// Clicking toggles the value
```

### Numeric Fields (Float, U16, USize, I32)

**With min/max constraints:**
```rust
// Renders as: slider from min to max
// ◀─●──▶ with numeric display
```

**Without constraints:**
```rust
// Renders as: text input
// User types numeric value
```

### String Fields

**Single-line:**
```rust
// Renders as text input field
// Perfect for short strings
```

**Multi-line:**
```rust
// Renders as text area
// For longer text content
```

### Color Fields

```rust
// Renders as color picker
// Click to open color picker dialog
// Displays current RGB color
```

### Enum Fields

```rust
// Renders as dropdown/combobox
// Shows variant name and description
// Example: "Edge (Trigger on rising edge)"
```

## Validation & Error Display

Validation errors are shown in red below each field:

```rust
// If validation fails:
// Field Error: Value out of range (expected 0-100, got 150)
```

Control error display with `DialogConfig::show_errors`:

```rust
let config = DialogConfig {
    show_errors: false,  // Hide errors
    ..Default::default()
};
```

## Integration with ConfigFormState

The form state tracks changes:

```rust
// Check if field changed
if form_state.dirty_fields.contains_key("show_hierarchy") {
    println!("show_hierarchy was changed");
}

// Get specific value
if let Some(value) = form_state.values.get("show_hierarchy") {
    println!("Current value: {:?}", value);
}

// Get all changed values
let changed: Vec<_> = form_state.dirty_fields.keys().collect();
println!("Changed fields: {:?}", changed);
```

## Styling & Appearance

egui's built-in styling applies to all dialogs. Customize appearance through egui context:

```rust
// Modify context visuals for darker/lighter theme
let mut style = egui::Style::default();
style.visuals.dark_mode = true;
egui_ctx.set_style(style);

// Higher DPI scaling
egui_ctx.set_zoom_factor(1.5);
```

## Performance Considerations

- **Lazy rendering**: Only visible fields are rendered
- **Change tracking**: Only changed fields are validated
- **No clones**: Values are references until changed
- **Collapsible sections**: Use for large dialogs

## Future Enhancements

- [ ] Vector/list field rendering with add/remove/reorder
- [ ] Nested struct field rendering
- [ ] Custom widgets via attributes
- [ ] Field constraints extraction (min/max from attributes)
- [ ] Advanced validation rules
- [ ] Field dependencies (show field A only if field B is X)
- [ ] Search/filter fields
- [ ] Field grouping and tabs
- [ ] Undo/redo history
- [ ] Keyboard shortcuts for OK/Cancel

## Troubleshooting

**Colors not displaying correctly:**
- Ensure RGB values are 0-255
- Check egui color space settings

**Text truncated in fields:**
- Increase `DialogConfig::width`
- Use multiline for long strings

**Dropdown values not updating:**
- Ensure form_state is mutable
- Check that variant names match exactly

**Changes not saving:**
- Call `get_changed_values()` before resetting form
- Ensure you're validating before applying
