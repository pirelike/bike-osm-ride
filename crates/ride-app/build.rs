//! Copies `WebView2Loader.dll` next to the produced executables.
//!
//! On the GNU toolchain the exe imports `WebView2Loader.dll` dynamically (MSVC links it
//! statically). Without the DLL beside the exe the process exits immediately with
//! `0xC0000135` (STATUS_DLL_NOT_FOUND). The same is true for the unit-test executables
//! in `target/<profile>/deps/`, so the DLL is copied to every directory a binary of this
//! crate can end up in, for every profile.
//!
//! Where the DLL comes from (first hit wins):
//! 1. `target/<profile>/build/webview2-com-sys-*/out/x64/WebView2Loader.dll`
//!    (the newest such file, by modification time), produced by `webview2-com-sys`;
//! 2. the sources of the `webview2-com-sys` crate in the cargo registry
//!    (`.../webview2-com-sys-*/x64/WebView2Loader.dll`), which matters on a clean build
//!    because the build script of that dependency may not have run yet when this one does.
//!
//! The script never fails the build: a missing DLL or a locked destination (the app is
//! running and has the DLL mapped) only produces a `cargo:warning`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const DLL: &str = "WebView2Loader.dll";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let arch_dir = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return,
    };

    // OUT_DIR = <target>/<profile>/build/ride-app-<hash>/out  ->  <target>/<profile>
    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    let Some(profile_dir) = out_dir.ancestors().nth(3).map(Path::to_path_buf) else {
        return;
    };
    if !profile_dir.join("build").is_dir() {
        println!(
            "cargo:warning=WebView2Loader.dll: unexpected OUT_DIR layout ({}), not copying",
            out_dir.display()
        );
        return;
    }

    let destinations: Vec<PathBuf> = ["", "deps", "examples"]
        .iter()
        .map(|sub| profile_dir.join(sub))
        .collect();
    // Re-run when a destination copy disappears (e.g. after deleting the exe's folder).
    for dir in &destinations {
        println!("cargo:rerun-if-changed={}", dir.join(DLL).display());
    }

    let Some(source) = find_dll(&profile_dir, arch_dir) else {
        println!(
            "cargo:warning=WebView2Loader.dll was not found (searched {}/build/webview2-com-sys-*/out/{arch_dir}/ and the cargo registry); \
             the app will not start until the DLL is placed next to the exe",
            profile_dir.display()
        );
        return;
    };

    let Ok(bytes) = fs::read(&source) else {
        println!(
            "cargo:warning=WebView2Loader.dll: cannot read {}",
            source.display()
        );
        return;
    };

    for dir in destinations {
        if fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let dest = dir.join(DLL);
        // Skip identical files: overwriting a DLL that a running instance has mapped fails.
        if fs::read(&dest)
            .map(|existing| existing == bytes)
            .unwrap_or(false)
        {
            continue;
        }
        if let Err(err) = fs::write(&dest, &bytes) {
            println!(
                "cargo:warning=WebView2Loader.dll: cannot write {} ({err}); is the app running?",
                dest.display()
            );
        }
    }
}

fn find_dll(profile_dir: &Path, arch_dir: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. build output of webview2-com-sys inside this target dir.
    if let Ok(entries) = fs::read_dir(profile_dir.join("build")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("webview2-com-sys-") {
                let candidate = entry.path().join("out").join(arch_dir).join(DLL);
                if candidate.is_file() {
                    candidates.push(candidate);
                }
            }
        }
    }
    if let Some(best) = newest(&candidates) {
        return Some(best);
    }

    // 2. the crate sources in the cargo registry.
    let cargo_home = env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(|h| PathBuf::from(h).join(".cargo"))
    })?;
    if let Ok(registries) = fs::read_dir(cargo_home.join("registry").join("src")) {
        for registry in registries.flatten() {
            let Ok(crates) = fs::read_dir(registry.path()) else {
                continue;
            };
            for krate in crates.flatten() {
                if krate
                    .file_name()
                    .to_string_lossy()
                    .starts_with("webview2-com-sys-")
                {
                    let candidate = krate.path().join(arch_dir).join(DLL);
                    if candidate.is_file() {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }
    newest(&candidates)
}

/// The most recently modified file of `paths`.
fn newest(paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .max_by_key(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })
        .cloned()
}
