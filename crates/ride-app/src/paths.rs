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

/// Where an exported ride goes when there is no save dialog (Android): the shared Download
/// folder if it is writable, otherwise the app's data folder.
#[cfg(target_os = "android")]
pub fn export_dir() -> PathBuf {
    let shared = PathBuf::from("/storage/emulated/0/Download");
    let probe = shared.join(".bike-osm-ride-probe");
    if std::fs::write(&probe, b"").is_ok() {
        let _ = std::fs::remove_file(&probe);
        return shared;
    }
    let own = data_dir().join("exports");
    let _ = std::fs::create_dir_all(&own);
    own
}

/// Android gives every app a private files folder (`/data/data/<package>/files`) but no
/// `LOCALAPPDATA`/`HOME`; pointing `RIDE_DATA_DIR` at it makes every crate use it.
/// Called first thing in `main`, before any thread exists.
#[cfg(target_os = "android")]
pub fn init_android_data_dir() {
    if std::env::var_os("RIDE_DATA_DIR").is_some_and(|v| !v.is_empty()) {
        return;
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
        let dir = PathBuf::from("/data/data").join(package).join("files");
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("RIDE_DATA_DIR", &dir);
        std::env::set_var("HOME", &dir);
    }
}

/// The TCX copies handed to the Strava upload view.
#[cfg(not(target_os = "android"))]
pub fn strava_dir() -> PathBuf {
    data_dir().join("strava")
}

/// Profile of the Strava upload WebView, so the sign-in lasts between rides.
#[cfg(not(target_os = "android"))]
pub fn strava_webview_dir() -> PathBuf {
    data_dir().join("strava-webview")
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
