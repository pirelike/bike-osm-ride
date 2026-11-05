//! Where the app keeps its files (tile cache, BLE hex log, WebView profile).
//!
//! The root is [`ride_route::store::data_dir`] — the very directory the settings, the recent
//! routes and the processed routes live in — so the whole application has exactly one data
//! folder:
//!
//! | platform | root |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\bike-osm-ride` |
//! | macOS | `~/Library/Application Support/bike-osm-ride` |
//! | Linux | `$XDG_DATA_HOME/bike-osm-ride`, else `~/.local/share/bike-osm-ride` |
//!
//! `RIDE_DATA_DIR` overrides it (tests, portable installs, and the private files folder of the
//! Android app, see [`init_android_data_dir`]).

use std::path::PathBuf;

/// Data root of the application: the store's directory.
pub fn data_dir() -> PathBuf {
    ride_route::store::data_dir()
}

/// Disk cache of the map tiles: `<root>/tiles/<style>/<z>/<x>/<y>.png`.
pub fn tiles_dir() -> PathBuf {
    data_dir().join("tiles")
}

/// Target of `RIDE_BLE_LOG=1`.
pub fn ble_log_path() -> PathBuf {
    data_dir().join("ble.log")
}

/// Android gives every app a private files folder but no `LOCALAPPDATA`/`HOME`; pointing
/// `RIDE_DATA_DIR` at it makes every crate use it. Called first thing in `main`, before any
/// thread exists.
///
/// The folder is asked from the activity (`getFilesDir()`), because the path is not the same on
/// every phone: with a second user, a work profile, Secure Folder or "dual apps" it lives under
/// `/data/user/<n>/<package>`, while `/data/data/<package>` points at user 0's folder, which this
/// process may not touch ("Permission denied"). The guessed paths only back that call up, and
/// every candidate has to pass a write probe before it is used.
#[cfg(target_os = "android")]
pub fn init_android_data_dir() {
    if std::env::var_os("RIDE_DATA_DIR").is_some_and(|v| !v.is_empty()) {
        return;
    }
    if let Some(dir) = android_candidate_dirs().into_iter().find(|d| writable(d)) {
        std::env::set_var("RIDE_DATA_DIR", &dir);
        std::env::set_var("HOME", &dir);
    }
}

/// The places the app's private files folder can be, best first.
#[cfg(target_os = "android")]
fn android_candidate_dirs() -> Vec<PathBuf> {
    use std::os::unix::fs::MetadataExt;

    let mut dirs = Vec::new();
    if let Some(dir) = crate::android_picker::files_dir() {
        dirs.push(dir);
    }
    let package = std::fs::read("/proc/self/cmdline")
        .ok()
        .and_then(|raw| {
            let name = raw.split(|b| *b == 0).next()?.to_vec();
            String::from_utf8(name).ok()
        })
        .map(|s| s.split(':').next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty());
    if let Some(package) = package {
        // Android user id = uid / 100000 (user 0 is the owner).
        if let Ok(meta) = std::fs::metadata("/proc/self") {
            let user = meta.uid() / 100_000;
            dirs.push(
                PathBuf::from("/data/user")
                    .join(user.to_string())
                    .join(&package)
                    .join("files"),
            );
        }
        dirs.push(PathBuf::from("/data/data").join(package).join("files"));
    }
    dirs
}

/// True when `dir` exists (or can be created) and a file can be written into it.
#[cfg(target_os = "android")]
fn writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// WebView user-data folder of this app (keeps the webview cache out of the same directory the
/// system would pick from the executable name).
pub fn webview_dir() -> PathBuf {
    data_dir().join("webview")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell and the store must never compute two different folders again (that is what
    /// put the settings into `~/.local/share` on macOS while the tiles went to
    /// `~/Library/Application Support`).
    #[test]
    fn the_shell_and_the_store_share_one_data_directory() {
        assert_eq!(data_dir(), ride_route::store::data_dir());
    }

    #[test]
    fn every_file_of_the_app_sits_under_that_directory() {
        let root = data_dir();
        assert_eq!(tiles_dir(), root.join("tiles"));
        assert_eq!(ble_log_path(), root.join("ble.log"));
        assert_eq!(webview_dir(), root.join("webview"));
        assert!(!root.as_os_str().is_empty());
    }
}
