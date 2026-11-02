//! Writes the application icons of the Linux and macOS packages.
//!
//! ```text
//! cargo run -p ride-app --example export_icon
//! ```
//!
//! The icon is drawn in code (`src/icon.rs`), so the PNG files are generated from the very same
//! function the window icon uses — no binary asset is edited by hand. The files land in
//! `packaging/icons/{32,128,256,512,1024}.png`; `crates/ride-app/Dioxus.toml` (AppImage, .deb,
//! .desktop) and `packaging/macos/bundle.sh` (.icns) read them from there. They are committed,
//! so a release build never has to run this.

#[allow(dead_code)] // `window_icon()` needs a live desktop; the exporter only uses `render_rgba`
#[path = "../src/icon.rs"]
mod icon;

use std::path::{Path, PathBuf};

/// The sizes the bundle formats ask for: the AppImage/.desktop icon, the .deb hicolor theme
/// and the macOS .icns set (32 → 32x32, 128 → 128x128, 256 → 256x256, 512 → 512x512,
/// 1024 → 512x512@2x).
const SIZES: [u32; 5] = [32, 128, 256, 512, 1024];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packaging")
        .join("icons");
    std::fs::create_dir_all(&dir)?;
    for size in SIZES {
        let path = dir.join(format!("{size}.png"));
        write_png(&path, size, &icon::render_rgba(size))?;
        println!("{}", path.display());
    }
    Ok(())
}

/// One 8-bit RGBA PNG, top-down rows (exactly what `render_rgba` returns).
fn write_png(path: &Path, size: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}
