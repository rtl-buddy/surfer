use std::error::Error;
use std::fs::File;
use std::io::Write;
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

    // Load and process the icon at build time
    let icon_bytes = include_bytes!("assets/com.gitlab.surferproject.surfer.png");
    let icon = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png)
        .expect("Failed to load icon")
        .to_rgba8();
    let (width, height) = icon.dimensions();
    let rgba_data = icon.into_raw();

    // Generate Rust source file with the icon data
    let out_dir = std::env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("icon_data.rs");
    let mut f = File::create(&dest_path)?;

    writeln!(f, "// Auto-generated icon data. Do not edit manually.")?;
    writeln!(f, "pub const ICON_WIDTH: u32 = {};", width)?;
    writeln!(f, "pub const ICON_HEIGHT: u32 = {};", height)?;
    writeln!(f, "pub const ICON_RGBA: &[u8] = &{:?};", rgba_data)?;

    Ok(())
}
