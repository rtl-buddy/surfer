# Quick Reference: Config Dialog System

## 30-Second Integration

```rust
// 1. Get metadata
let meta = libsurfer::config_metadata::SurferLayoutMetadata::metadata();

// 2. Create form (do this once)
let mut form = ConfigFormState::with_metadata(&meta);

// 3. In egui render loop
use libsurfer::config_dialog_egui::{render_dialog, DialogConfig, DialogResult};

match render_dialog(ui, "Settings", &meta, &mut form, &DialogConfig::default()) {
    DialogResult::Applied => apply_form_changes(&form),
    DialogResult::Cancelled => {},
    _ => {}
}

// 4. Apply changes
fn apply_form_changes(form: &ConfigFormState) {
    for (name, value) in &form.values {
        println!("Setting {} = {:?}", name, value);
    }
}
```

## Available Metadata

```rust
libsurfer::config_metadata::{
    SurferLayoutMetadata       // Layout/window configuration
    SurferBehaviorMetadata     // User interaction preferences
    SurferConfigMetadata       // General configuration
    SurferGestureMetadata      // Mouse gesture settings
    SurferThemeMetadata        // Visual theme settings
    WcpConfigMetadata          // WCP protocol settings
}
```

## Core Functions

| Function | Purpose | Returns |
|----------|---------|---------|
| `render_field()` | Single field | bool (changed?) |
| `render_section()` | All fields in section | bool (any changed?) |
| `render_dialog()` | Complete dialog | DialogResult |
| `render_multi_section_dialog()` | Multiple sections | DialogResult |

## DialogResult

```rust
pub enum DialogResult {
    Applied,        // User clicked OK → apply changes
    Cancelled,      // User clicked Cancel → discard changes
    Pending,        // Dialog still open → keep rendering
}
```

## FormValue Enum

```rust
pub enum FormValue {
    Bool(bool),              // true/false
    Float(f32),              // floating point
    Integer(i64),            // integer value
    String(String),          // text
    Enum(String),            // enum variant name
    Color([u8; 3]),          // RGB [0-255]
    List(Vec<FormValue>),    // array
}
```

## ConfigFormState

```rust
pub struct ConfigFormState {
    pub values: HashMap<String, FormValue>,
    pub dirty_fields: HashMap<String, bool>,  // Which fields changed
    pub errors: HashMap<String, String>,      // Validation errors
}

// Methods:
impl ConfigFormState {
    pub fn with_metadata(meta: &ConfigSectionMetadata) -> Self { ... }
    pub fn new() -> Self { ... }
    pub fn get_value(&self, name: &str) -> Option<&FormValue> { ... }
    pub fn set_value(&mut self, name: String, value: FormValue) { ... }
}
```

## DialogConfig

```rust
pub struct DialogConfig {
    pub width: Option<f32>,           // None = auto
    pub show_descriptions: bool,       // Show field descriptions
    pub show_errors: bool,             // Show validation errors
    pub spacing: f32,                  // Between fields (default 8.0)
}

// Quick create:
let config = DialogConfig::default();

// Custom:
let config = DialogConfig {
    width: Some(400.0),
    show_descriptions: true,
    show_errors: true,
    spacing: 12.0,
};
```

## Field Types & Widgets

```
Bool            → Checkbox                    [x]
Float           → Slider (with min/max) OR   ◀─●─▶
U16, USize, I32   TextEdit
String          → TextEdit (or multiline)    ┌──────────┐
                                              │          │
                                              └──────────┘
Color           → Color Picker                  ■ (click)
Enum            → ComboBox/Dropdown           ▼ option ▲
```

## Complete Minimal Example

```rust
use libsurfer::{
    config_metadata::SurferLayoutMetadata,
    config_dialog_ui::ConfigFormState,
    config_dialog_egui::*,
};

struct MyApp {
    show_settings: bool,
    form: ConfigFormState,
}

impl MyApp {
    fn new() -> Self {
        let meta = SurferLayoutMetadata::metadata();
        Self {
            show_settings: false,
            form: ConfigFormState::with_metadata(&meta),
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .open(&mut self.show_settings)
            .show(ctx, |ui| {
                let meta = SurferLayoutMetadata::metadata();
                let result = render_dialog(
                    ui,
                    "Layout Settings",
                    &meta,
                    &mut self.form,
                    &DialogConfig::default(),
                );

                if result == DialogResult::Applied {
                    self.save_changes();
                    self.show_settings = false;
                }
                if result == DialogResult::Cancelled {
                    self.show_settings = false;
                }
            });
    }

    fn save_changes(&self) {
        for (name, value) in &self.form.values {
            println!("{} = {:?}", name, value);
        }
    }
}
```

## Common Patterns

### Get Changed Fields
```rust
for (field_name, _) in &form.dirty_fields {
    println!("User changed: {}", field_name);
}
```

### Check Validation Errors
```rust
if form.errors.is_empty() {
    println!("No errors, can save");
} else {
    for (field, error) in &form.errors {
        println!("{}: {}", field, error);
    }
}
```

### Get Specific Value
```rust
if let Some(FormValue::Bool(val)) = form.values.get("show_hierarchy") {
    println!("show_hierarchy = {}", val);
}
```

### Multi-Section Dialog
```rust
let sections = vec![
    SurferLayoutMetadata::metadata(),
    SurferBehaviorMetadata::metadata(),
];

let result = render_multi_section_dialog(
    ui,
    "Settings",
    &sections,
    &mut form,
    &DialogConfig::default(),
);
```

## Adding Config Fields

1. Add to `config.rs`:
```rust
/// Documentation
pub field_name: FieldType,
```

2. Update `build.rs` `is_config_struct()` if adding new struct

3. Rebuild (metadata auto-generates)

4. Use new metadata:
```rust
let meta = MyStructMetadata::metadata();
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Metadata not generated | Check struct in `is_config_struct()` |
| Field not showing | Add doc comment to field |
| Values not saving | Use mutable `form` in closure |
| Colors wrong | Ensure RGB values 0-255 |
| Dropdown not working | Check enum variant names match |
| Performance slow | Use collapsible sections, not all at once |

## Documentation Links

- **Full Guide:** EGUI_INTEGRATION_GUIDE.md
- **10 Examples:** EGUI_INTEGRATION_EXAMPLES.md
- **Architecture:** SYSTEM_OVERVIEW.md
- **Overview:** IMPLEMENTATION_COMPLETE.md

## Testing

```bash
# Check compilation
cargo check --lib

# Run tests
cargo test config_dialog --lib

# Build
cargo build --lib
```

## API Reference

### render_field
```rust
pub fn render_field(
    ui: &mut Ui,
    field: &FieldMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool  // true if changed
```

### render_section
```rust
pub fn render_section(
    ui: &mut Ui,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> bool  // true if any changed
```

### render_dialog
```rust
pub fn render_dialog(
    ui: &mut Ui,
    title: &str,
    section: &ConfigSectionMetadata,
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult
```

### render_multi_section_dialog
```rust
pub fn render_multi_section_dialog(
    ui: &mut Ui,
    title: &str,
    sections: &[ConfigSectionMetadata],
    form_state: &mut ConfigFormState,
    config: &DialogConfig,
) -> DialogResult
```

## Need More?

- See EGUI_INTEGRATION_EXAMPLES.md for 10 real-world examples
- See EGUI_INTEGRATION_GUIDE.md for detailed API documentation
- See SYSTEM_OVERVIEW.md for architecture details
