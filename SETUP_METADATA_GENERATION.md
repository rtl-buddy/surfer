# Graphical Configuration Dialog System - Automatic Build-Time Generation

## Overview

The configuration dialog system for Surfer automatically generates metadata **at compile time** by parsing `config.rs`. This ensures the UI is always in sync with the current configuration structure without manual template maintenance.

## File Structure

### Core Framework Files
- `libsurfer/src/config_dialog.rs` - Core metadata types and builders
- `libsurfer/src/config_dialog_ui.rs` - UI integration, form state, and validation
- `libsurfer/src/config_dialog_metadata.rs` - Re-exports generated metadata (for convenience)

### Build-Time Generation
- `libsurfer/build.rs` - **Parses `config.rs` and generates metadata automatically**
- `libsurfer/target/OUT_DIR/config_metadata.rs` - Generated module (created during build)

### Documentation
- `CONFIG_DIALOG_GUIDE.md` - Comprehensive usage guide

## How to Use

### 1. Add Documentation to Config Fields

The build script automatically extracts doc comments from your config structs. Document your fields with `///` comments:

**In `src/config.rs`:**

```rust
#[derive(Debug, Deserialize)]
pub struct SurferLayout {
    /// Display the signal hierarchy panel
    pub show_hierarchy: bool,

    /// Text size for waveform values in points
    pub waveforms_text_size: f32,
}
```

The doc comments are automatically used as field descriptions in the generated metadata.

### 2. Rebuild to Update Metadata

Simply rebuild your project - metadata regenerates automatically:

```bash
cargo build
```

The build.rs script will:
1. Parse `config.rs`
2. Extract all supported config structs
3. Generate metadata from their fields and doc comments
4. Write to `target/OUT_DIR/config_metadata.rs`

### 3. Accessing Metadata at Runtime

```rust
use libsurfer::config_metadata::{SurferLayoutMetadata, ConfigMetadata};

// Get metadata for a section
let layout_meta = SurferLayoutMetadata::metadata();
for field in &layout_meta.fields {
    println!("{}: {}", field.label, field.description);
}

// Create and manage forms
let mut form = ConfigFormState::with_metadata(&layout_meta);
form.set_value("show_hierarchy", FormValue::Bool(true));

// Validate
let errors = FormValidator::validate_form(&layout_meta, &form);
```

## Architecture

### Build Process

```
src/config.rs (with /// doc comments)
    ↓ (build.rs with syn crate parser)
Parse structs, fields, types, docs
    ↓ (code generation with quote crate)
target/OUT_DIR/config_metadata.rs
    ↓ (include! macro in lib.rs)
config_metadata module at runtime
```

### How It Works

1. **Parse Phase**: `build.rs` uses `syn` to parse Rust syntax from config.rs
2. **Extract Phase**: Identifies config structs (SurferLayout, SurferBehavior, etc.)
3. **Document Phase**: Extracts `///` doc comments as field descriptions
4. **Type Phase**: Analyzes field types to determine widget types
5. **Generate Phase**: Creates ConfigMetadata implementations
6. **Include Phase**: Generated code included at compile time

### Module Structure

```
libsurfer crate
├── config_dialog        - Core types & builders
│   ├── FieldMetadata
│   ├── EnumVariantMetadata
│   ├── FieldType (Bool, Float, Enum, etc.)
│   ├── ConfigSectionMetadata
│   └── ConfigMetadata trait
│
├── config_dialog_ui     - Form handling & UI
│   ├── ConfigFormState
│   ├── FormValue
│   ├── FormValidator
│   └── SuggestedWidget
│
└── config_metadata     - **Generated at compile time**
    ├── LayoutConfigMetadata
    ├── BehaviorConfigMetadata
    ├── GeneralConfigMetadata
    ├── arrow_key_bindings_variants()
    ├── primary_mouse_drag_variants()
    ├── auto_load_variants()
    ├── transition_value_variants()
    └── [More generated metadata...]
```

## Key Design Decisions

1. **Automatic Parsing**: `build.rs` uses `syn` to parse config.rs - no manual templates
2. **Doc Comment Driven**: Field descriptions come from `///` doc comments
3. **Type-Safe**: Full Rust type checking on all metadata
4. **Zero Runtime Cost**: All generation happens at compile time
5. **Single Source of Truth**: Only edit `config.rs`, everything else is generated

## Benefits

✅ **Always in sync** - Changes to config.rs automatically regenerate metadata
✅ **Type-safe** - Compiler errors if metadata doesn't match config
✅ **Zero runtime cost** - All generation happens at compile time
✅ **Easy to extend** - Add fields and doc comments, rebuild automatically
✅ **No manual updates** - Doc comments are the single source of truth
✅ **IDE-friendly** - All metadata is actual Rust code with full intellisense

## Next Steps

To integrate with egui dialogs:

1. Call `ConfigDialogBuilder::new()` with metadata
2. Use `SuggestedWidget::for_field()` to pick UI components
3. Use `ConfigFormState` for form management
4. Call `FormValidator::validate_form()` before saving

Example integration framework is in `CONFIG_DIALOG_GUIDE.md`.

## Maintenance

- **To add/remove fields**: Edit `src/config.rs` struct definition
- **To change field descriptions**: Update the `///` doc comments in `src/config.rs`
- **To change field types**: Edit the field type in `src/config.rs`, metadata auto-updates
- **To add new field types**: Extend `FieldType` enum in `config_dialog.rs`
- **To change validation rules**: Update `FormValidator` in `config_dialog_ui.rs`
- **To modify parser logic**: Edit `build.rs` - changes take effect on next build

All compile-time changes take effect automatically on next `cargo build`.
No manual metadata files to edit!
