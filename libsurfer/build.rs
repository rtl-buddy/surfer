use std::error::Error;
use std::fs;
use std::path::Path;
use syn::{Attribute, Item, ItemStruct, Meta, parse_file};
use vergen_gitcl::{BuildBuilder, Emitter, GitclBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    // Generate version info
    let git = GitclBuilder::default()
        .all()
        .describe(true, true, None)
        .build()?;
    let build = BuildBuilder::all_build()?;
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;

    // Generate config metadata from config.rs
    generate_config_metadata_from_source()?;

    Ok(())
}

fn generate_config_metadata_from_source() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let config_path = Path::new("src/config.rs");

    // Read and parse config.rs
    let config_content = fs::read_to_string(config_path)?;
    let file = parse_file(&config_content)?;

    // Start with imports
    let mut metadata_code = String::from(
        "// AUTO-GENERATED at build time from config.rs\n\
         use crate::config_dialog::*;\n\n",
    );

    for item in &file.items {
        if let Item::Struct(item_struct) = item {
            if is_config_struct(item_struct) {
                metadata_code
                    .push_str(&generate_metadata_for_struct(item_struct, &config_content)?);
            }
        }
    }

    // Write generated file
    let output_path = Path::new(&out_dir).join("config_metadata.rs");
    fs::write(&output_path, metadata_code)?;

    println!("cargo:rerun-if-changed=src/config.rs");

    Ok(())
}

fn is_config_struct(item_struct: &ItemStruct) -> bool {
    let name = item_struct.ident.to_string();
    // Match key config structs
    matches!(
        name.as_str(),
        "SurferLayout"
            | "SurferBehavior"
            | "SurferConfig"
            | "SurferGesture"
            | "SurferTheme"
            | "WcpConfig"
    )
}

fn generate_metadata_for_struct(
    item_struct: &ItemStruct,
    _source: &str,
) -> Result<String, Box<dyn Error>> {
    let struct_name = &item_struct.ident;
    let metadata_struct_name = format!("{}Metadata", struct_name);

    // Extract documentation for the struct
    let doc = extract_doc_comments(&item_struct.attrs);

    // Generate field metadata
    let mut field_metadata = String::new();

    if let syn::Fields::Named(fields) = &item_struct.fields {
        for field in &fields.named {
            let field_name = field.ident.as_ref().unwrap().to_string();
            let field_doc = extract_doc_comments(&field.attrs);
            let field_label = humanize_field_name(&field_name);

            // Get field type code from the Type
            let field_type_code = get_field_type_code(&field.ty, &field_doc);

            field_metadata.push_str(&format!(
                r#"                FieldMetadataBuilder::new("{}", {})
                    .label("{}")
                    .description("{}")
                    .order({})
                    .build(),
"#,
                field_name,
                field_type_code,
                field_label,
                field_doc,
                field_metadata.lines().count() // simple ordering
            ));
        }
    }

    let label = humanize_field_name(&struct_name.to_string());
    let description = if doc.is_empty() {
        "Configuration settings".to_string()
    } else {
        doc
    };

    Ok(format!(
        r#"
/// Metadata for {struct_name}
pub struct {metadata_struct_name};

impl ConfigMetadata for {metadata_struct_name} {{
    fn metadata() -> ConfigSectionMetadata {{
        ConfigSectionMetadata {{
            name: "{name_lower}".to_string(),
            label: "{label}".to_string(),
            description: "{description}".to_string(),
            fields: vec![
{field_metadata}            ],
        }}
    }}
}}

"#,
        struct_name = struct_name,
        metadata_struct_name = metadata_struct_name,
        name_lower = struct_name.to_string().to_lowercase(),
        label = label,
        description = description.replace('"', "\\\""),
        field_metadata = field_metadata
    ))
}

fn extract_doc_comments(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let Meta::NameValue(nv) = &attr.meta {
                    if let syn::Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let doc = lit_str.value();
                            return Some(doc.trim().to_string());
                        }
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn humanize_field_name(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn get_field_type_code(ty: &syn::Type, _doc: &str) -> String {
    // Convert syn::Type to string for pattern matching
    let type_str = match ty {
        syn::Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };

    // Parse basic type patterns
    match type_str.as_str() {
        "bool" => "FieldType::Bool".to_string(),
        "f32" => "FieldType::Float { min: None, max: None, step: None }".to_string(),
        "f64" => "FieldType::Float { min: None, max: None, step: None }".to_string(),
        "u16" => "FieldType::U16 { min: None, max: None }".to_string(),
        "usize" => "FieldType::USize { min: None, max: None }".to_string(),
        "i32" => "FieldType::I32 { min: None, max: None }".to_string(),
        "String" => "FieldType::String { multiline: false, max_length: None }".to_string(),
        "Color32" => "FieldType::Color".to_string(),
        _ => "FieldType::String { multiline: false, max_length: None }".to_string(),
    }
}
