use std::error::Error;
use std::path::Path;
use vergen_gitcl::{BuildBuilder, Emitter, GitclBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    // describe with tags=true so that v0.4.0 gets picked up (not annotated)
    let git = GitclBuilder::default()
        .all()
        .describe(true, true, None)
        .build()?;
    let build = BuildBuilder::all_build()?;
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;

    println!("cargo:rerun-if-changed=assets/com.gitlab.surferproject.surfer.png");

    // Load and process the icon at build time
    let icon_bytes = include_bytes!("assets/com.gitlab.surferproject.surfer.png");
    let icon = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png)
        .expect("Failed to load icon")
        .to_rgba8();
    let (width, height) = icon.dimensions();
    let rgba_data = icon.into_raw();

    let icon_data = egui::viewport::IconData {
        rgba: rgba_data,
        width,
        height,
    };

    let serialized = bincode::serialize(&icon_data).expect("Failed to serialize icon data");

    // Write binary icon data for compile-time inclusion
    let out_dir = std::env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("icon_data.bin");
    std::fs::write(&dest_path, serialized)?;

    Ok(())
}
