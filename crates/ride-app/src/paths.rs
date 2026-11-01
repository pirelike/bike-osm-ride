//! Where the app keeps its files (tile cache, BLE hex log, WebView2 profile).
//!
//! The root is `%LOCALAPPDATA%/bike-osm-ride/`; `RIDE_DATA_DIR` overrides it (tests).

use std::ffi::OsString;
use std::path::PathBuf;

pub const APP_DIR: &str = "bike-osm-ride";

/// Data root computed from an environment lookup (injectable for tests).
pub fn data_dir_from(lookup: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    let non_empty = |key: &str| lookup(key).filter(|v| !v.is_empty());
    if let Some(dir) = non_empty("RIDE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(base) = non_empty("LOCALAPPDATA") {
        return PathBuf::from(base).join(APP_DIR);
    }
    if let Some(home) = non_empty("HOME").or_else(|| non_empty("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_DIR);
    }
    std::env::temp_dir().join(APP_DIR)
}

pub fn data_dir() -> PathBuf {
    data_dir_from(|key| std::env::var_os(key))
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

/// WebView2 user-data folder (keeps its cache out of `%LOCALAPPDATA%/<exe name>`).
pub fn webview_dir() -> PathBuf {
    data_dir().join("webview")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn override_wins() {
        let dir = data_dir_from(lookup(&[
            ("RIDE_DATA_DIR", "D:/t"),
            ("LOCALAPPDATA", "C:/L"),
        ]));
        assert_eq!(dir, PathBuf::from("D:/t"));
    }

    #[test]
    fn local_app_data_is_the_default_root() {
        let dir = data_dir_from(lookup(&[("LOCALAPPDATA", "C:/L")]));
        assert_eq!(dir, PathBuf::from("C:/L").join("bike-osm-ride"));
    }

    #[test]
    fn empty_values_are_ignored() {
        let dir = data_dir_from(lookup(&[("RIDE_DATA_DIR", ""), ("LOCALAPPDATA", "C:/L")]));
        assert_eq!(dir, PathBuf::from("C:/L").join("bike-osm-ride"));
    }

    #[test]
    fn falls_back_to_temp() {
        let dir = data_dir_from(lookup(&[]));
        assert!(dir.ends_with("bike-osm-ride"));
    }
}
