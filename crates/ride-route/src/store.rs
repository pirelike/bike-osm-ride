//! Local persistence: settings, the recent-routes index, the processed routes, the ride log and
//! the imported workouts.
//!
//! Layout under [`data_dir`] (`%LOCALAPPDATA%\bike-osm-ride` on Windows):
//!
//! ```text
//! settings.json             ride_model::Settings (pretty JSON)
//! recent.json               Vec<RecentRoute>, newest first (at most MAX_RECENT)
//! routes/<id>.json          the processed ride_model::Route
//! rides/<id>/<stem>.json    one finished ride (see [`crate::rides`])
//! rides/<id>/index.json     Vec<RideMeta>, newest first
//! workouts/<id>.json        one workout imported from a MyWhoosh / Zwift file
//! ```
//!
//! The directory can be overridden with the `RIDE_DATA_DIR` environment variable (tests,
//! portable installs). Writes are atomic (temp file + rename), reads never panic: a
//! missing or corrupt file simply yields defaults / `None`. The free functions use
//! [`data_dir`]; [`Store`] works on any directory (that is what the tests use).
//!
//! The ride records are **not** touched by the recent-route housekeeping: deleting a route from
//! the recent list keeps its history.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use ride_model::{RecentRoute, Route, Settings, Workout};

const APP_DIR: &str = "bike-osm-ride";
/// Imported workouts, one file per workout.
const WORKOUTS_DIR: &str = "workouts";
/// Number of routes kept in the recent list (older ones are deleted from disk).
pub const MAX_RECENT: usize = 20;
/// Refuse to read route/settings files larger than this (corrupt or foreign files).
pub(crate) const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Operating-system families with their own convention for per-user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Unix,
}

impl Platform {
    const CURRENT: Platform = if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Unix
    };
}

/// Absolute, non-empty path from an environment-style lookup.
fn abs_path(v: Option<OsString>) -> Option<PathBuf> {
    v.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// Per-user local data directory of the platform (what the `directories` crate calls
/// `data_local_dir`): `%LOCALAPPDATA%`, `~/Library/Application Support`,
/// `$XDG_DATA_HOME` or `~/.local/share`.
///
/// Written against `std` only: the `directories` crate drags in `option-ext`
/// (MPL-2.0), which would break the project's "permissive licences only" rule for a
/// three-line lookup.
fn local_data_base(platform: Platform, get: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    match platform {
        Platform::Windows => abs_path(get("LOCALAPPDATA"))
            .or_else(|| abs_path(get("USERPROFILE")).map(|p| p.join("AppData").join("Local"))),
        Platform::MacOs => {
            abs_path(get("HOME")).map(|p| p.join("Library").join("Application Support"))
        }
        Platform::Unix => abs_path(get("XDG_DATA_HOME"))
            .or_else(|| abs_path(get("HOME")).map(|p| p.join(".local").join("share"))),
    }
}

/// Application data directory: `$RIDE_DATA_DIR` if set and non-empty, else
/// `<local app data>/bike-osm-ride` (`%LOCALAPPDATA%/bike-osm-ride` on Windows,
/// `~/Library/Application Support/bike-osm-ride` on macOS, `$XDG_DATA_HOME/bike-osm-ride` or
/// `~/.local/share/bike-osm-ride` on Linux).
pub fn data_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("RIDE_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    local_data_base(Platform::CURRENT, &|k| std::env::var_os(k))
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub(crate) fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub(crate) fn invalid_id() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Érvénytelen útvonal-azonosító.",
    )
}

pub(crate) fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map_or_else(|| "data".to_string(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Replace values a hand-edited or corrupt settings file could use to break the physics
/// (zero/negative masses, NaN, …) with the defaults.
fn sanitize(mut s: Settings) -> Settings {
    let d = Settings::default();
    let fix = |v: &mut f64, default: f64| {
        if !v.is_finite() || *v <= 0.0 {
            *v = default;
        }
    };
    fix(&mut s.rider.rider_kg, d.rider.rider_kg);
    fix(&mut s.rider.bike_kg, d.rider.bike_kg);
    fix(&mut s.rider.ftp_w, d.rider.ftp_w);
    fix(&mut s.rider.max_hr, d.rider.max_hr);
    fix(&mut s.rider.cda, d.rider.cda);
    fix(&mut s.rider.crr, d.rider.crr);
    fix(&mut s.rider.drivetrain_eff, d.rider.drivetrain_eff);
    fix(&mut s.rider.air_density, d.rider.air_density);
    s.rider.drivetrain_eff = s.rider.drivetrain_eff.min(1.0);
    s.rider.trainer_grade_scale = if s.rider.trainer_grade_scale.is_finite() {
        s.rider.trainer_grade_scale.clamp(0.0, 1.5)
    } else {
        1.0
    };
    fix(&mut s.ui.lookahead_m, d.ui.lookahead_m);
    s
}

/// A persistence root. Use [`Store::at`] in tests, the free functions in the app.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn default_location() -> Self {
        Self::at(data_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("recent.json")
    }

    fn route_path(&self, id: &str) -> Option<PathBuf> {
        valid_id(id).then(|| self.dir.join("routes").join(format!("{id}.json")))
    }

    /// Stored settings, or the defaults when missing/unreadable (never fails).
    pub fn load_settings(&self) -> Settings {
        let _g = lock();
        read_limited(&self.settings_path())
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .map_or_else(Settings::default, sanitize)
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        let _g = lock();
        let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
        write_atomic(&self.settings_path(), &json)
    }

    fn read_index(&self) -> Vec<RecentRoute> {
        read_limited(&self.index_path())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &[RecentRoute]) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(index).map_err(io::Error::other)?;
        write_atomic(&self.index_path(), &json)
    }

    /// Recent routes, newest first. Entries whose route file vanished are left out.
    pub fn list_recent(&self) -> Vec<RecentRoute> {
        let _g = lock();
        let mut list: Vec<RecentRoute> = self
            .read_index()
            .into_iter()
            .filter(|r| self.route_path(&r.id).is_some_and(|p| p.is_file()))
            .collect();
        list.sort_by(|a, b| b.last_opened_unix.total_cmp(&a.last_opened_unix));
        list
    }

    /// Store the processed route and make it the most recent one.
    pub fn save_recent(&self, route: &Route) -> io::Result<()> {
        self.save_recent_at(route, now_unix())
    }

    /// Like [`Store::save_recent`] with an explicit "opened" timestamp.
    pub fn save_recent_at(&self, route: &Route, opened_unix: f64) -> io::Result<()> {
        let path = self.route_path(&route.id).ok_or_else(invalid_id)?;
        let _g = lock();
        let json = serde_json::to_vec(route).map_err(io::Error::other)?;
        write_atomic(&path, &json)?;

        let mut index = self.read_index();
        index.retain(|r| r.id != route.id);
        index.insert(
            0,
            RecentRoute {
                id: route.id.clone(),
                name: route.name.clone(),
                total_dist: route.total_dist,
                ascent: route.ascent,
                source: route.source,
                last_opened_unix: opened_unix,
            },
        );
        index.sort_by(|a, b| b.last_opened_unix.total_cmp(&a.last_opened_unix));
        for old in index.split_off(index.len().min(MAX_RECENT)) {
            if let Some(p) = self.route_path(&old.id) {
                let _ = std::fs::remove_file(p);
            }
        }
        self.write_index(&index)
    }

    /// The stored route, if present and readable.
    pub fn load_recent(&self, id: &str) -> Option<Route> {
        let path = self.route_path(id)?;
        let _g = lock();
        let bytes = read_limited(&path)?;
        let route: Route = serde_json::from_slice(&bytes).ok()?;
        // A hand-edited / damaged file must not hand the engine an unusable route.
        (route.points.len() >= 2 && route.total_dist.is_finite() && route.total_dist > 0.0)
            .then_some(route)
    }

    /// Remove a route and its index entry. Missing entries are not an error.
    pub fn delete_recent(&self, id: &str) -> io::Result<()> {
        let path = self.route_path(id).ok_or_else(invalid_id)?;
        let _g = lock();
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut index = self.read_index();
        let before = index.len();
        index.retain(|r| r.id != id);
        if index.len() != before {
            self.write_index(&index)?;
        }
        Ok(())
    }

    // ───── imported workouts ─────

    fn workout_path(&self, id: &str) -> Option<PathBuf> {
        valid_id(id).then(|| self.dir.join(WORKOUTS_DIR).join(format!("{id}.json")))
    }

    /// Workouts imported from files, by name. A missing directory is empty, a corrupt file is
    /// skipped; an entry that is not an import (its id does not carry the import prefix) is
    /// ignored, so a hand-dropped file cannot shadow a built-in workout.
    pub fn list_workouts(&self) -> Vec<Workout> {
        let dir = self.dir.join(WORKOUTS_DIR);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let _g = lock();
        let mut out: Vec<Workout> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(bytes) = read_limited(&path) else {
                continue;
            };
            let Ok(workout) = serde_json::from_slice::<Workout>(&bytes) else {
                continue;
            };
            if workout.is_imported() && !workout.steps.is_empty() {
                out.push(workout);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Store one imported workout as `<id>.json`; re-importing the same file replaces it.
    pub fn save_workout(&self, workout: &Workout) -> io::Result<()> {
        if !workout.is_imported() {
            return Err(invalid_id());
        }
        let path = self.workout_path(&workout.id).ok_or_else(invalid_id)?;
        let json = serde_json::to_vec_pretty(workout).map_err(io::Error::other)?;
        let _g = lock();
        write_atomic(&path, &json)
    }

    /// Remove an imported workout. A missing file is not an error.
    pub fn delete_workout(&self, id: &str) -> io::Result<()> {
        let path = self.workout_path(id).ok_or_else(invalid_id)?;
        let _g = lock();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

pub fn load_settings() -> Settings {
    Store::default_location().load_settings()
}

pub fn save_settings(settings: &Settings) -> io::Result<()> {
    Store::default_location().save_settings(settings)
}

/// Newest first.
pub fn list_recent() -> Vec<RecentRoute> {
    Store::default_location().list_recent()
}

/// Store the processed route (JSON) and update the recent index / `last_opened_unix`.
pub fn save_recent(route: &Route) -> io::Result<()> {
    Store::default_location().save_recent(route)
}

pub fn load_recent(id: &str) -> Option<Route> {
    Store::default_location().load_recent(id)
}

pub fn delete_recent(id: &str) -> io::Result<()> {
    Store::default_location().delete_recent(id)
}

/// Workouts imported from files, by name.
pub fn list_workouts() -> Vec<Workout> {
    Store::default_location().list_workouts()
}

pub fn save_workout(workout: &Workout) -> io::Result<()> {
    Store::default_location().save_workout(workout)
}

pub fn delete_workout(id: &str) -> io::Result<()> {
    Store::default_location().delete_workout(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{MapStyle, RouteSource, Theme};

    fn route(id: &str, name: &str) -> Route {
        let mut r = ride_model::testing::straight_route(&[(500.0, 4.0), (300.0, -2.0)], 120.0);
        r.id = id.to_string();
        r.name = name.to_string();
        r.source = RouteSource::GpxFile;
        r
    }

    #[test]
    fn settings_roundtrip_and_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        assert_eq!(
            store.load_settings(),
            Settings::default(),
            "missing file -> defaults"
        );
        let mut s = Settings::default();
        s.rider.rider_kg = 82.5;
        s.rider.ftp_w = 260.0;
        s.ui.theme = Theme::Light;
        s.ui.map_style = MapStyle::Osm;
        s.ui.lookahead_m = 5000.0;
        store.save_settings(&s).unwrap();
        assert_eq!(store.load_settings(), s);
        // overwriting works (atomic rename over an existing file) and leaves no temp files
        s.rider.rider_kg = 70.0;
        store.save_settings(&s).unwrap();
        assert_eq!(store.load_settings().rider.rider_kg, 70.0);
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn corrupt_or_partial_settings_do_not_break_loading() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        std::fs::write(tmp.path().join("settings.json"), b"{ not json").unwrap();
        assert_eq!(store.load_settings(), Settings::default());
        // partial file: unknown/missing fields fall back to defaults
        std::fs::write(
            tmp.path().join("settings.json"),
            br#"{"rider":{"ftp_w":300.0},"extra":1}"#,
        )
        .unwrap();
        let s = store.load_settings();
        assert_eq!(s.rider.ftp_w, 300.0);
        assert_eq!(s.rider.rider_kg, Settings::default().rider.rider_kg);
        // nonsense values that would break the physics are replaced
        std::fs::write(tmp.path().join("settings.json"), br#"{"rider":{"rider_kg":0.0,"cda":-1.0,"trainer_grade_scale":9.0,"drivetrain_eff":4.0}}"#).unwrap();
        let s = store.load_settings();
        assert_eq!(s.rider.rider_kg, 75.0);
        assert_eq!(s.rider.cda, 0.32);
        assert_eq!(s.rider.trainer_grade_scale, 1.5);
        assert_eq!(s.rider.drivetrain_eff, 1.0);
    }

    #[test]
    fn recent_roundtrip_order_and_replace() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        assert!(store.list_recent().is_empty());
        store.save_recent_at(&route("aaaa", "Első"), 100.0).unwrap();
        store
            .save_recent_at(&route("bbbb", "Második – ékezetes név"), 200.0)
            .unwrap();
        let list = store.list_recent();
        assert_eq!(
            list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["bbbb", "aaaa"]
        );
        assert_eq!(list[0].name, "Második – ékezetes név");
        assert!((list[0].total_dist - 800.0).abs() < 1e-6);

        // re-opening the first one moves it to the front, no duplicate
        store
            .save_recent_at(&route("aaaa", "Első (újra)"), 300.0)
            .unwrap();
        let list = store.list_recent();
        assert_eq!(
            list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["aaaa", "bbbb"]
        );
        assert_eq!(list[0].name, "Első (újra)");
        assert_eq!(list[0].last_opened_unix, 300.0);

        // full route comes back identical
        let loaded = store.load_recent("bbbb").unwrap();
        assert_eq!(loaded, route("bbbb", "Második – ékezetes név"));
        assert!(store.load_recent("missing").is_none());
    }

    #[test]
    fn recent_list_is_capped_and_old_files_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        for i in 0..(MAX_RECENT + 3) {
            store
                .save_recent_at(&route(&format!("r{i:03}"), "x"), i as f64)
                .unwrap();
        }
        let list = store.list_recent();
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].id, format!("r{:03}", MAX_RECENT + 2));
        assert!(store.load_recent("r000").is_none(), "oldest were evicted");
        assert!(!tmp.path().join("routes").join("r000.json").exists());
        assert_eq!(
            std::fs::read_dir(tmp.path().join("routes"))
                .unwrap()
                .count(),
            MAX_RECENT
        );
    }

    #[test]
    fn delete_is_idempotent_and_updates_the_index() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        store.save_recent_at(&route("aaaa", "A"), 1.0).unwrap();
        store.save_recent_at(&route("bbbb", "B"), 2.0).unwrap();
        store.delete_recent("aaaa").unwrap();
        store.delete_recent("aaaa").unwrap();
        store.delete_recent("never-existed").unwrap();
        assert_eq!(store.list_recent().len(), 1);
        assert!(store.load_recent("aaaa").is_none());
        assert!(store.load_recent("bbbb").is_some());
    }

    #[test]
    fn damaged_files_are_ignored_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        store.save_recent_at(&route("aaaa", "A"), 1.0).unwrap();
        store.save_recent_at(&route("bbbb", "B"), 2.0).unwrap();
        // truncated route file
        std::fs::write(
            tmp.path().join("routes").join("aaaa.json"),
            b"{\"id\":\"aaaa\",",
        )
        .unwrap();
        assert!(store.load_recent("aaaa").is_none());
        // a route with no points is not usable either
        let mut empty = route("cccc", "C");
        empty.points.clear();
        std::fs::create_dir_all(tmp.path().join("routes")).unwrap();
        std::fs::write(
            tmp.path().join("routes").join("cccc.json"),
            serde_json::to_vec(&empty).unwrap(),
        )
        .unwrap();
        assert!(store.load_recent("cccc").is_none());
        // corrupt index: empty list, and saving afterwards repairs it
        std::fs::write(tmp.path().join("recent.json"), b"garbage").unwrap();
        assert!(store.list_recent().is_empty());
        store.save_recent_at(&route("dddd", "D"), 5.0).unwrap();
        assert_eq!(store.list_recent().len(), 1);
        // index entry whose file is gone is hidden
        std::fs::remove_file(tmp.path().join("routes").join("dddd.json")).unwrap();
        assert!(store.list_recent().is_empty());
    }

    #[test]
    fn path_traversal_ids_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path().join("data"));
        for bad in ["../x", "a/b", "a\\b", "", "..", "x.json", &"a".repeat(65)] {
            assert!(store.load_recent(bad).is_none(), "{bad}");
            assert!(store.delete_recent(bad).is_err(), "{bad}");
            let mut r = route("ok", "x");
            r.id = bad.to_string();
            assert!(store.save_recent(&r).is_err(), "{bad}");
        }
        assert!(!tmp.path().join("x").exists());
    }

    /// Serialises the tests that read or modify `RIDE_DATA_DIR`.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn env_override_selects_the_data_dir() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("RIDE_DATA_DIR");
        std::env::set_var("RIDE_DATA_DIR", tmp.path());
        assert_eq!(data_dir(), tmp.path());

        let mut s = Settings::default();
        s.rider.rider_kg = 91.0;
        save_settings(&s).unwrap();
        assert!(tmp.path().join("settings.json").is_file());
        assert_eq!(load_settings().rider.rider_kg, 91.0);
        save_recent(&route("envroute", "Env")).unwrap();
        assert_eq!(list_recent().len(), 1);
        assert!(load_recent("envroute").is_some());
        delete_recent("envroute").unwrap();
        assert!(list_recent().is_empty());

        match previous {
            Some(v) => std::env::set_var("RIDE_DATA_DIR", v),
            None => std::env::remove_var("RIDE_DATA_DIR"),
        }
    }

    #[test]
    fn platform_data_dirs() {
        // Absolute paths look different on Windows and elsewhere.
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        let abs = |rest: &str| format!("{root}{rest}");
        let env = |pairs: Vec<(&'static str, String)>| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| OsString::from(v))
            }
        };

        let local = abs("Users/Huba/AppData/Local");
        let e = env(vec![("LOCALAPPDATA", local.clone())]);
        assert_eq!(
            local_data_base(Platform::Windows, &e),
            Some(PathBuf::from(&local))
        );
        let e = env(vec![("USERPROFILE", abs("Users/Huba"))]);
        assert_eq!(
            local_data_base(Platform::Windows, &e),
            Some(
                PathBuf::from(abs("Users/Huba"))
                    .join("AppData")
                    .join("Local")
            )
        );
        assert_eq!(local_data_base(Platform::Windows, &env(vec![])), None);

        let e = env(vec![
            ("XDG_DATA_HOME", abs("data")),
            ("HOME", abs("home/u")),
        ]);
        assert_eq!(
            local_data_base(Platform::Unix, &e),
            Some(PathBuf::from(abs("data")))
        );
        let e = env(vec![("HOME", abs("home/u"))]);
        assert_eq!(
            local_data_base(Platform::Unix, &e),
            Some(PathBuf::from(abs("home/u")).join(".local").join("share"))
        );
        assert_eq!(
            local_data_base(Platform::MacOs, &e),
            Some(
                PathBuf::from(abs("home/u"))
                    .join("Library")
                    .join("Application Support")
            )
        );

        // relative or empty values are ignored
        let e = env(vec![("XDG_DATA_HOME", "rel/path".to_string())]);
        assert_eq!(local_data_base(Platform::Unix, &e), None);
        let e = env(vec![
            ("XDG_DATA_HOME", String::new()),
            ("HOME", String::new()),
        ]);
        assert_eq!(local_data_base(Platform::Unix, &e), None);
    }

    #[test]
    fn default_location_is_under_local_app_data() {
        // Only meaningful when the override is not active in this process.
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        if std::env::var_os("RIDE_DATA_DIR").is_none() {
            let d = data_dir();
            assert!(d.ends_with(APP_DIR), "{}", d.display());
        }
    }

    fn imported(id: &str, name: &str) -> Workout {
        Workout {
            id: id.into(),
            name: name.into(),
            category: ride_model::WorkoutCategory::Endurance,
            goal: "cél".into(),
            description: "leírás".into(),
            ftp_pct: 70,
            duration_min: 10,
            tags: vec![],
            sources: vec!["x.xml".into()],
            steps: vec![ride_model::WorkoutStep {
                kind: ride_model::StepKind::Steady,
                name: "Blokk".into(),
                duration_s: 600,
                target: ride_model::StepTarget::PctFtp { value: 70.0 },
                cadence: None,
                note: None,
                messages: Vec::new(),
            }],
        }
    }

    #[test]
    fn imported_workouts_round_trip_and_survive_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        assert!(store.list_workouts().is_empty(), "no directory yet");

        let workout = imported("import-00000000000000aa", "Pacing #2");
        store.save_workout(&workout).unwrap();
        let again = imported("import-00000000000000bb", "Alpha");
        store.save_workout(&again).unwrap();
        let list = store.list_workouts();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha", "sorted by name");
        assert_eq!(list[1], workout, "the whole workout round-trips");

        // saving the same id replaces instead of duplicating
        let mut renamed = workout.clone();
        renamed.name = "Pacing #2 (javított)".into();
        store.save_workout(&renamed).unwrap();
        let list = store.list_workouts();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|w| w.name == "Pacing #2 (javított)"));

        // a corrupt file and a file that is not an import are ignored
        std::fs::write(tmp.path().join(WORKOUTS_DIR).join("broken.json"), b"{oops").unwrap();
        let builtin = imported("a-endurance-beginner-45", "Beépített");
        std::fs::write(
            tmp.path().join(WORKOUTS_DIR).join("builtin.json"),
            serde_json::to_vec(&builtin).unwrap(),
        )
        .unwrap();
        let list = store.list_workouts();
        assert_eq!(list.len(), 2, "only the two real imports");
        assert!(list.iter().all(|w| w.is_imported()));

        // a built-in id cannot be written through the import path
        assert!(store.save_workout(&builtin).is_err());

        // deleting is idempotent
        store.delete_workout(&workout.id).unwrap();
        store.delete_workout(&workout.id).unwrap();
        assert_eq!(store.list_workouts().len(), 1);
    }
}
