// Configuration Dialog System
//
// This module provides a framework for automatically generating graphical configuration dialogs
// that reflect the current config structure. The system is designed to be regenerated at compile
// time to ensure the UI always matches the actual configuration.
//
// # Features
// - Automatic field discovery from config structs
// - Custom metadata for fields (descriptions, constraints, units, etc.)
// - Enum metadata with detailed descriptions and explanations
// - Type-aware UI generation (checkboxes for bools, sliders for numbers, etc.)
// - Compile-time code generation support

use std::collections::HashMap;

/// Metadata about a configuration field that is used to build the UI
#[derive(Clone, Debug, PartialEq)]
pub struct FieldMetadata {
    /// The name/identifier of the field
    pub name: String,
    /// Human-readable label for the field
    pub label: String,
    /// Description of what this field does
    pub description: String,
    /// Optional longer explanation of the field
    pub explanation: Option<String>,
    /// The type of the field
    pub field_type: FieldType,
    /// Whether this field is required
    pub required: bool,
    /// UI order (lower values appear first)
    pub order: usize,
}

/// Describes the type of a configuration field
#[derive(Clone, Debug, PartialEq)]
pub enum FieldType {
    /// A boolean flag
    Bool,
    /// A 32-bit floating point number
    Float {
        min: Option<f32>,
        max: Option<f32>,
        step: Option<f32>,
    },
    /// A 16-bit unsigned integer
    U16 { min: Option<u16>, max: Option<u16> },
    /// An unsigned integer (usize)
    USize {
        min: Option<usize>,
        max: Option<usize>,
    },
    /// A signed 32-bit integer
    I32 { min: Option<i32>, max: Option<i32> },
    /// A text string
    String {
        multiline: bool,
        max_length: Option<usize>,
    },
    /// A color value (RGBA)
    Color,
    /// An enumeration with named variants
    Enum { variants: Vec<EnumVariantMetadata> },
    /// A vector/list of values
    Vector {
        element_type: Box<FieldType>,
        min_length: Option<usize>,
        max_length: Option<usize>,
    },
    /// A nested structure
    Struct { fields: Vec<FieldMetadata> },
}

/// Metadata about an enumeration variant
#[derive(Clone, Debug, PartialEq)]
pub struct EnumVariantMetadata {
    /// The name of the variant (e.g., "Edge", "Scroll")
    pub name: String,
    /// Human-readable display name (can be different from `name`)
    pub display_name: Option<String>,
    /// Description of what this variant does
    pub description: Option<String>,
    /// Longer explanation of the variant
    pub explanation: Option<String>,
}

impl EnumVariantMetadata {
    /// Get the display name, falling back to the variant name if not specified
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

/// Describes a complete configuration section/struct
#[derive(Clone, Debug)]
pub struct ConfigSectionMetadata {
    /// Name of the section
    pub name: String,
    /// Human-readable label
    pub label: String,
    /// Description of the section
    pub description: String,
    /// All fields in this section
    pub fields: Vec<FieldMetadata>,
}

/// Builder for creating field metadata
pub struct FieldMetadataBuilder {
    name: String,
    label: Option<String>,
    description: String,
    explanation: Option<String>,
    field_type: FieldType,
    required: bool,
    order: usize,
}

impl FieldMetadataBuilder {
    /// Create a new field metadata builder
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        let name_str = name.into();
        Self {
            name: name_str.clone(),
            label: None,
            description: String::new(),
            explanation: None,
            field_type,
            required: true,
            order: 0,
        }
    }

    /// Set the human-readable label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set the longer explanation
    pub fn explanation(mut self, expl: impl Into<String>) -> Self {
        self.explanation = Some(expl.into());
        self
    }

    /// Set whether this field is required
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Set the UI order
    pub fn order(mut self, order: usize) -> Self {
        self.order = order;
        self
    }

    /// Build the metadata
    pub fn build(self) -> FieldMetadata {
        FieldMetadata {
            name: self.name.clone(),
            label: self.label.unwrap_or_else(|| {
                // Convert snake_case to Title Case
                self.name
                    .split('_')
                    .map(|s| {
                        s.chars()
                            .enumerate()
                            .map(|(i, c)| {
                                if i == 0 {
                                    c.to_uppercase().to_string()
                                } else {
                                    c.to_string()
                                }
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
            description: self.description,
            explanation: self.explanation,
            field_type: self.field_type,
            required: self.required,
            order: self.order,
        }
    }
}

/// Builder for enum variant metadata
pub struct EnumVariantBuilder {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    explanation: Option<String>,
}

impl EnumVariantBuilder {
    /// Create a new enum variant builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            description: None,
            explanation: None,
        }
    }

    /// Set the display name
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the explanation
    pub fn explanation(mut self, expl: impl Into<String>) -> Self {
        self.explanation = Some(expl.into());
        self
    }

    /// Build the metadata
    pub fn build(self) -> EnumVariantMetadata {
        EnumVariantMetadata {
            name: self.name,
            display_name: self.display_name,
            description: self.description,
            explanation: self.explanation,
        }
    }
}

/// A trait that config structs can implement to provide their metadata
/// This is automatically derived or can be manually implemented
pub trait ConfigMetadata {
    /// Get the metadata for this config or section
    fn metadata() -> ConfigSectionMetadata;

    /// Get a specific field's metadata by name
    fn field_metadata(name: &str) -> Option<FieldMetadata> {
        Self::metadata().fields.into_iter().find(|f| f.name == name)
    }
}

/// Helper to generate enum metadata from variant names
/// This can be extended at compile time to support more detailed metadata
pub fn create_enum_metadata(variants: Vec<EnumVariantMetadata>) -> FieldType {
    FieldType::Enum { variants }
}

/// A registry of all configuration metadata that can be queried at runtime
pub struct ConfigMetadataRegistry {
    sections: HashMap<String, ConfigSectionMetadata>,
}

impl ConfigMetadataRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            sections: HashMap::new(),
        }
    }

    /// Register a configuration section
    pub fn register<T: ConfigMetadata>(mut self) -> Self {
        let metadata = T::metadata();
        self.sections.insert(metadata.name.clone(), metadata);
        self
    }

    /// Get a section by name
    pub fn get_section(&self, name: &str) -> Option<&ConfigSectionMetadata> {
        self.sections.get(name)
    }

    /// Get all registered sections
    pub fn sections(&self) -> impl Iterator<Item = &ConfigSectionMetadata> {
        self.sections.values()
    }

    /// Get all sections sorted by some criteria
    pub fn sections_sorted(&self) -> Vec<&ConfigSectionMetadata> {
        let mut sections: Vec<_> = self.sections.values().collect();
        sections.sort_by(|a, b| a.name.cmp(&b.name));
        sections
    }
}

impl Default for ConfigMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_metadata_builder() {
        let metadata = FieldMetadataBuilder::new("test_field", FieldType::Bool)
            .label("Test Field")
            .description("A test boolean field")
            .explanation("This field controls test behavior")
            .order(5)
            .build();

        assert_eq!(metadata.name, "test_field");
        assert_eq!(metadata.label, "Test Field");
        assert_eq!(metadata.description, "A test boolean field");
        assert_eq!(
            metadata.explanation,
            Some("This field controls test behavior".to_string())
        );
        assert_eq!(metadata.order, 5);
    }

    #[test]
    fn test_field_metadata_label_conversion() {
        let metadata = FieldMetadataBuilder::new("show_hierarchy", FieldType::Bool)
            .description("Show hierarchy")
            .build();

        assert_eq!(metadata.label, "Show Hierarchy");
    }

    #[test]
    fn test_enum_variant_metadata() {
        let variant = EnumVariantBuilder::new("Edge")
            .display_name("Step by Edge")
            .description("Step to the next edge")
            .explanation("When enabled, arrow keys will step to the next rising or falling edge")
            .build();

        assert_eq!(variant.name, "Edge");
        assert_eq!(variant.display_name(), "Step by Edge");
        assert_eq!(
            variant.description,
            Some("Step to the next edge".to_string())
        );
    }

    #[test]
    fn test_enum_variant_fallback_display_name() {
        let variant = EnumVariantBuilder::new("Edge")
            .description("Step to the next edge")
            .build();

        assert_eq!(variant.display_name(), "Edge");
    }

    #[test]
    fn test_config_metadata_registry() {
        let registry = ConfigMetadataRegistry::new();

        // Registry should be created empty
        assert_eq!(registry.sections().count(), 0);
    }

    #[test]
    fn test_field_type_variants() {
        // Test that all FieldType variants can be created
        let _bool_type = FieldType::Bool;
        let _float_type = FieldType::Float {
            min: Some(0.0),
            max: Some(100.0),
            step: Some(0.1),
        };
        let _color_type = FieldType::Color;
        let _enum_type = FieldType::Enum {
            variants: vec![
                EnumVariantBuilder::new("Variant1").build(),
                EnumVariantBuilder::new("Variant2").build(),
            ],
        };
    }
}
