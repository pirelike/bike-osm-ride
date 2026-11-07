//! Collects the embedded workout libraries: `assets/workouts.json` is the Hungarian source, and
//! `assets/workouts.<code>.json` is one finished translation per language. Adding a language is
//! dropping a file in `assets/`; no Rust source has to change.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let assets = manifest.join("assets");

    let mut entries: Vec<(String, String)> =
        vec![("hu".to_string(), slash(&assets.join("workouts.json")))];
    if let Ok(read) = fs::read_dir(&assets) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // `workouts.json` is the source; `workouts.<code>.json` is a translation.
            let Some(code) = stem.strip_prefix("workouts.") else {
                continue;
            };
            entries.push((code.to_string(), slash(&path)));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out =
        String::from("/// Every embedded workout library: language code and its JSON text.\n");
    out.push_str("pub static WORKOUT_CATALOGS: &[(&str, &str)] = &[\n");
    for (code, path) in &entries {
        out.push_str(&format!("    ({code:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").expect("out dir")).join("workout_catalogs.rs");
    fs::write(&dest, out).expect("write workout_catalogs.rs");
}

/// `include_str!` paths are relative to the source file on every platform; forward slashes keep
/// Windows paths valid Rust string literals.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
