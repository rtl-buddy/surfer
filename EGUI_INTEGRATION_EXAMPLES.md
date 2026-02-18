# egui Configuration Dialog Examples

## Example 1: Simple Settings Dialog

A minimal example showing how to display a configuration dialog for a single section:

```rust
use libsurfer::config_metadata::*;
use libsurfer::config_dialog_ui::ConfigFormState;
use libsurfer::config_dialog_egui::{render_dialog, DialogConfig, DialogResult};
use egui::Context;

fn show_simple_settings(egui_ctx: &Context, app_state: &mut AppState) {
    let metadata = SurferLayoutMetadata::metadata();

    egui::Window::new("Settings")
        .resizable(true)
        .default_size([400.0, 600.0])
        .open(&mut app_state.settings_open)
        .show(egui_ctx, |ui| {
            let result = render_dialog(
                ui,
                "Layout Settings",
                &metadata,
                &mut app_state.layout_form,
                &DialogConfig::default(),
            );

            match result {
                DialogResult::Applied => {
                    apply_layout_changes(&app_state.layout_form);
                    app_state.settings_open = false;
                }
                DialogResult::Cancelled => {
                    app_state.settings_open = false;
                }
                DialogResult::Pending => {}
            }
        });
}

fn apply_layout_changes(form_state: &ConfigFormState) {
    for (field_name, value) in &form_state.values {
        println!("Applying {}: {:?}", field_name, value);
    }
}
```

## Example 2: Multi-Tab Settings Window

Display multiple configuration sections as collapsible tabs:

```rust
use libsurfer::config_metadata::*;
use libsurfer::config_dialog_ui::ConfigFormState;
use libsurfer::config_dialog_egui::{render_multi_section_dialog, DialogConfig, DialogResult};

struct SettingsWindow {
    open: bool,
    all_sections: Vec<ConfigSectionMetadata>,
    form_state: ConfigFormState,
}

impl SettingsWindow {
    fn new() -> Self {
        let sections = vec![
            SurferLayoutMetadata::metadata(),
            SurferBehaviorMetadata::metadata(),
            SurferThemeMetadata::metadata(),
        ];

        Self {
            open: true,
            all_sections: sections.clone(),
            form_state: ConfigFormState::new(),
        }
    }

    fn show(&mut self, egui_ctx: &egui::Context) {
        egui::Window::new("Settings")
            .open(&mut self.open)
            .resizable(true)
            .default_size([500.0, 800.0])
            .show(egui_ctx, |ui| {
                let result = render_multi_section_dialog(
                    ui,
                    "Surfer Settings",
                    &self.all_sections,
                    &mut self.form_state,
                    &DialogConfig::default(),
                );

                if result == DialogResult::Applied {
                    self.apply_all_changes();
                    self.open = false;
                }
            });
    }

    fn apply_all_changes(&self) {
        println!("Applying {} changes", self.form_state.dirty_fields.len());
        // Save configuration with self.form_state.values
    }
}
```

## Example 3: Real-Time Preview

Update preview as user changes settings:

```rust
use libsurfer::config_dialog_egui::*;

struct SettingsWithPreview {
    metadata: ConfigSectionMetadata,
    form_state: ConfigFormState,
    preview_enabled: bool,
}

impl SettingsWithPreview {
    fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Settings with Live Preview");
            if ui.checkbox(&mut self.preview_enabled, "Live Preview") {}
        });

        ui.separator();

        // Left side: settings
        ui.columns(2, |cols| {
            render_section(&mut cols[0], &self.metadata, &mut self.form_state, &DialogConfig::default());

            // Right side: preview
            cols[1].group(|ui| {
                ui.heading("Preview");
                if self.preview_enabled {
                    self.render_preview(ui);
                }
            });
        });
    }

    fn render_preview(&self, ui: &mut egui::Ui) {
        // Use current form values to render a preview
        if let Some(FormValue::Bool(show)) = self.form_state.values.get("show_hierarchy") {
            if *show {
                ui.label("✓ Hierarchy panel visible");
            } else {
                ui.label("✗ Hierarchy panel hidden");
            }
        }
    }
}
```

## Example 4: Progressive Form Validation

Show validation errors as user types:

```rust
use libsurfer::config_dialog_ui::*;
use libsurfer::config_dialog_egui::*;

struct ValidatingForm {
    metadata: ConfigSectionMetadata,
    form_state: ConfigFormState,
    config: DialogConfig,
}

impl ValidatingForm {
    fn render(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings with Validation");

        for field in &self.metadata.fields {
            // Render field
            if render_field(ui, field, &mut self.form_state, &self.config) {
                // Validate immediately after change
                let validator = FormValidator::new();
                if let Some(error) = validator.validate_field(field,
                    self.form_state.values.get(&field.name)) {
                    self.form_state.errors.insert(field.name.clone(), error);
                } else {
                    self.form_state.errors.remove(&field.name);
                }
            }
        }

        // Show validation summary
        if !self.form_state.errors.is_empty() {
            ui.colored_label(egui::Color32::RED,
                format!("{} validation errors", self.form_state.errors.len()));
        } else {
            ui.colored_label(egui::Color32::GREEN, "✓ All fields valid");
        }
    }
}
```

## Example 5: Conditional Field Visibility

Show/hide fields based on other field values:

```rust
use libsurfer::config_dialog_egui::*;

struct ConditionalSettings {
    metadata: ConfigSectionMetadata,
    form_state: ConfigFormState,
}

impl ConditionalSettings {
    fn render(&mut self, ui: &mut egui::Ui, config: &DialogConfig) {
        for field in &self.metadata.fields {
            // Example: Only show "hover_delay" if "show_tooltips" is true
            if field.name == "hover_delay" {
                if let Some(FormValue::Bool(false)) = self.form_state.values.get("show_tooltips") {
                    ui.label(format!("{} (disabled - tooltips are off)", field.label));
                    ui.disable(true, |ui| {
                        render_field(ui, field, &mut self.form_state, config);
                    });
                    ui.disable(false, |_| {});
                    continue;
                }
            }

            render_field(ui, field, &mut self.form_state, config);
        }
    }
}
```

## Example 6: Comparing Before/After Values

Show what changed before applying:

```rust
use libsurfer::config_dialog_egui::*;
use std::collections::HashMap;

struct ConfigComparison {
    original_values: HashMap<String, FormValue>,
    form_state: ConfigFormState,
}

impl ConfigComparison {
    fn show_changes(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("Changes Summary");

            let mut any_changes = false;
            for (field_name, new_value) in &self.form_state.values {
                if let Some(original) = self.original_values.get(field_name) {
                    if original != new_value {
                        any_changes = true;
                        ui.horizontal(|ui| {
                            ui.label(field_name);
                            ui.label(format!("{:?} → {:?}", original, new_value));
                        });
                    }
                }
            }

            if !any_changes {
                ui.label("No changes");
            }
        });
    }

    fn confirm_and_apply(&mut self) -> bool {
        // Require explicit confirmation if there are significant changes
        if self.form_state.dirty_fields.len() > 5 {
            // Show confirmation dialog
            return true; // After user confirms
        }
        true
    }
}
```

## Example 7: Import/Export Configuration

Load and save configuration to/from the dialog:

```rust
use libsurfer::config_dialog_egui::*;
use libsurfer::config_dialog_ui::FormValue;
use std::collections::HashMap;

fn export_form_as_json(form_state: &ConfigFormState) -> String {
    let mut json_obj = HashMap::new();

    for (name, value) in &form_state.values {
        let json_value = match value {
            FormValue::Bool(b) => format!("true/false: {}", b),
            FormValue::Float(f) => format!("{}", f),
            FormValue::Integer(i) => format!("{}", i),
            FormValue::String(s) => format!("\"{}\"", s),
            FormValue::Enum(e) => format!("\"{}\"", e),
            FormValue::Color(rgb) => format!("rgb({}, {}, {})", rgb[0], rgb[1], rgb[2]),
            FormValue::List(items) => format!("[{} items]", items.len()),
        };
        json_obj.insert(name.clone(), json_value);
    }

    format!("{:#?}", json_obj)
}

fn import_form_from_preset(form_state: &mut ConfigFormState, preset_name: &str) {
    // Load preset configuration based on name
    match preset_name {
        "dark_theme" => {
            form_state.values.insert("theme_color".to_string(), FormValue::Color([30, 30, 30]));
            form_state.values.insert("text_color".to_string(), FormValue::Color([255, 255, 255]));
        }
        "light_theme" => {
            form_state.values.insert("theme_color".to_string(), FormValue::Color([240, 240, 240]));
            form_state.values.insert("text_color".to_string(), FormValue::Color([0, 0, 0]));
        }
        _ => {}
    }
}
```

## Example 8: Hierarchical Settings

Organize settings by category in a tree view:

```rust
use libsurfer::config_dialog_egui::*;

struct HierarchicalSettings {
    sections_by_category: HashMap<String, Vec<ConfigSectionMetadata>>,
    expanded_categories: HashMap<String, bool>,
    form_state: ConfigFormState,
}

impl HierarchicalSettings {
    fn render(&mut self, ui: &mut egui::Ui, config: &DialogConfig) {
        for (category, sections) in &self.sections_by_category {
            let expanded = self.expanded_categories.entry(category.clone()).or_insert(true);

            if ui.collapsing(category, |ui| {
                for section in sections {
                    ui.group(|ui| {
                        render_section(ui, section, &mut self.form_state, config);
                    });
                    ui.separator();
                }
            }).header_response.clicked() {
                *expanded = !*expanded;
            }
        }
    }
}
```

## Example 9: Field Search/Filter

Allow users to search for specific settings:

```rust
use libsurfer::config_dialog_egui::*;

struct SearchableSettings {
    metadata: ConfigSectionMetadata,
    form_state: ConfigFormState,
    search_query: String,
}

impl SearchableSettings {
    fn render(&mut self, ui: &mut egui::Ui, config: &DialogConfig) {
        // Search box
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search_query);
        });

        ui.separator();

        // Filter and render fields
        let query = self.search_query.to_lowercase();
        for field in &self.metadata.fields {
            if query.is_empty()
                || field.name.to_lowercase().contains(&query)
                || field.label.to_lowercase().contains(&query)
                || field.description.to_lowercase().contains(&query) {
                render_field(ui, field, &mut self.form_state, config);
            }
        }
    }
}
```

## Example 10: Async Configuration Reload

Load configuration from file while showing dialog:

```rust
use libsurfer::config_dialog_egui::*;
use std::sync::mpsc::Receiver;

struct AsyncConfigDialog {
    form_state: ConfigFormState,
    metadata: ConfigSectionMetadata,
    loading_receiver: Option<Receiver<Result<ConfigFormState, String>>>,
    is_loading: bool,
}

impl AsyncConfigDialog {
    fn render(&mut self, ui: &mut egui::Ui) {
        if self.is_loading {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Loading configuration...");
            });

            // Check if loading finished
            if let Some(rx) = &self.loading_receiver {
                if let Ok(result) = rx.try_recv() {
                    self.is_loading = false;
                    match result {
                        Ok(new_form) => self.form_state = new_form,
                        Err(e) => eprintln!("Failed to load: {}", e),
                    }
                }
            }
        } else {
            let config = DialogConfig::default();
            render_section(ui, &self.metadata, &mut self.form_state, &config);

            if ui.button("Reload").clicked() {
                self.start_async_load();
            }
        }
    }

    fn start_async_load(&mut self) {
        // Spawn async task to load configuration
        self.is_loading = true;
    }
}
```

## Running These Examples

To use these examples:

1. Add to your application state:
```rust
struct MyApp {
    settings_window: SettingsWindow,
    // ... other fields
}
```

2. Create in `new()`:
```rust
impl MyApp {
    fn new() -> Self {
        Self {
            settings_window: SettingsWindow::new(),
            // ...
        }
    }
}
```

3. Render in update:
```rust
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.settings_window.show(ctx);
        // ... other UI
    }
}
```

All examples integrate seamlessly with the configuration metadata system and handle the complete lifecycle of configuration dialogs.
