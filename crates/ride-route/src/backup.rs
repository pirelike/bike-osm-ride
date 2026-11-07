//! Backup and restore: one `.zip` with everything the rider cannot get back from the internet.
//!
//! ```text
//! backup.json                    format version, app version, creation time
//! settings.json                  ride_model::Settings, WITHOUT the Google key
//! recent.json                    the recent-routes index
//! routes/<id>.json               the processed routes
//! rides/<route id>/<stem>.json   the ride records
//! rides/<route id>/index.json    the per-route ride index
//! workouts/<id>.json             imported workouts
//! ```
//!
//! It is a **whitelist**: caches (map tiles, Street View imagery), logs, browser profiles and
//! anything else in the data folder never enter the archive, and the Google key is blanked on the
//! way out and ignored on the way back in (a secret must not travel in a file the rider mails
//! to themselves).
//!
//! Restoring only **merges**: nothing that exists is overwritten or deleted. A route is a content
//! hash, so the same id is the same route; a ride is keyed by its start time. The restore has two
//! phases. The first reads every entry, bounded in size, and checks it (names, JSON, versions);
//! nothing is written. Only when the whole archive passed does the second write, under the store
//! lock, with the recent list last. An entry's name is never used as a path: it is mapped onto the
//! whitelist, and the path is rebuilt from the validated ids. That is what keeps a crafted archive
//! (`../../x`, absolute paths, drive letters) from reaching anywhere outside the data folder.
//!
//! The settings are not written here. They are returned (sanitised, key blanked) so the
//! controller applies them through its normal path; a file written behind its back would be
//! overwritten by the pending debounced save.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use ride_model::{RecentRoute, RideMeta, RideRecord, Route, Settings, Workout};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::rides::{valid_stem, INDEX_FILE, RIDES_DIR};
use crate::store::{self, valid_id, Store, MAX_FILE_BYTES};
use ride_i18n::{t, tf};

/// The archive layout version this code writes and reads.
pub const BACKUP_FORMAT: u32 = 1;
const MANIFEST: &str = "backup.json";
const SETTINGS: &str = "settings.json";
const RECENT: &str = "recent.json";
const ROUTES_DIR: &str = "routes";
const WORKOUTS_DIR: &str = "workouts";

/// Upper bounds for restoring an archive that came from anywhere. The sizes in a zip header are
/// not trusted: every entry is read through a counter.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    /// One entry, uncompressed. The store itself refuses to read a larger file.
    pub max_entry_bytes: u64,
    /// All entries together, uncompressed.
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 50_000,
            max_entry_bytes: MAX_FILE_BYTES,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    created_unix_s: u64,
}

/// What a backup contains.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub routes: usize,
    pub rides: usize,
    pub workouts: usize,
}

/// What a restore added. Everything that was already there is not counted.
#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub routes: usize,
    pub rides: usize,
    pub workouts: usize,
    /// Entries left out: unknown names, unreadable JSON, rides of a newer version, ...
    pub skipped: usize,
    /// The settings of the backup, sanitised, with the Google key empty. `None` when the archive
    /// had none (or they were unreadable).
    pub settings: Option<Settings>,
}

/// `bike-osm-ride-mentes-2026-11-05.zip`
pub fn suggested_filename(unix_s: f64) -> String {
    tf(
        "route.backup.suggested_filename",
        &[("date", &ride_sim::tcx::iso_date(unix_s))],
    )
}

fn zip_err(err: zip::result::ZipError) -> String {
    tf(
        "route.backup.a_zip_fajl_nem_olvashato_err",
        &[("err", &err.to_string())],
    )
}

fn io_err(what: &str, err: &io::Error) -> String {
    format!("{what}: {err}")
}

// ───── export ─────

/// Writes the backup of the data folder into `out`. `settings` is the live configuration (the
/// file on disk may lag behind the debounced save); the Google key is blanked here.
pub fn export<W: Write + Seek>(
    store: &Store,
    settings: &Settings,
    app_version: &str,
    created_unix_s: u64,
    out: W,
) -> Result<ExportReport, String> {
    let mut settings = settings.clone();
    settings.ui.google_maps_key.clear();
    let settings_json = serde_json::to_vec_pretty(&settings).map_err(|err| {
        tf(
            "route.backup.a_beallitasok_nem_menthetok_err",
            &[("err", &err.to_string())],
        )
    })?;
    let manifest_json = serde_json::to_vec_pretty(&Manifest {
        format: BACKUP_FORMAT,
        app_version: app_version.to_string(),
        created_unix_s,
    })
    .map_err(|err| {
        tf(
            "route.backup.a_mentes_fejlece_nem_irhato_err",
            &[("err", &err.to_string())],
        )
    })?;

    let mut zip = ZipWriter::new(out);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut report = ExportReport::default();

    add(&mut zip, options, MANIFEST, &manifest_json)?;
    add(&mut zip, options, SETTINGS, &settings_json)?;

    // One consistent snapshot: a ride saved in the middle of the copy waits for the lock.
    let _guard = store::lock();
    if let Some(bytes) = store::read_limited(&store.dir().join(RECENT)) {
        add(&mut zip, options, RECENT, &bytes)?;
    }
    for (id, path) in json_files(&store.dir().join(ROUTES_DIR)) {
        if !valid_id(&id) {
            continue;
        }
        if let Some(bytes) = store::read_limited(&path) {
            add(
                &mut zip,
                options,
                &format!("{ROUTES_DIR}/{id}.json"),
                &bytes,
            )?;
            report.routes += 1;
        }
    }
    for route_dir in sub_dirs(&store.dir().join(RIDES_DIR)) {
        let Some(route_id) = route_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !valid_id(route_id) {
            continue;
        }
        for (stem, path) in json_files(&route_dir) {
            let name = format!("{RIDES_DIR}/{route_id}/{stem}.json");
            if stem == "index" {
                if let Some(bytes) = store::read_limited(&path) {
                    add(&mut zip, options, &name, &bytes)?;
                }
            } else if valid_stem(&stem) {
                if let Some(bytes) = store::read_limited(&path) {
                    add(&mut zip, options, &name, &bytes)?;
                    report.rides += 1;
                }
            }
        }
    }
    for (id, path) in json_files(&store.dir().join(WORKOUTS_DIR)) {
        if !valid_id(&id) {
            continue;
        }
        if let Some(bytes) = store::read_limited(&path) {
            add(
                &mut zip,
                options,
                &format!("{WORKOUTS_DIR}/{id}.json"),
                &bytes,
            )?;
            report.workouts += 1;
        }
    }
    zip.finish().map_err(|err| {
        tf(
            "route.backup.a_mentes_lezarasa_nem_sikerult_err",
            &[("err", &err.to_string())],
        )
    })?;
    Ok(report)
}

fn add<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    options: SimpleFileOptions,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    zip.start_file(name, options).map_err(|err| {
        tf(
            "route.backup.a_mentesbe_nem_irhato_be_a_z_name_err",
            &[("name", name), ("err", &err.to_string())],
        )
    })?;
    zip.write_all(bytes).map_err(|err| {
        io_err(
            &tf(
                "route.backup.a_mentesbe_nem_irhato_be_a_z_name",
                &[("name", name)],
            ),
            &err,
        )
    })
}

/// `(file stem, path)` of the `*.json` files directly in `dir`, by name. A missing folder is empty.
fn json_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            Some((stem, p))
        })
        .collect();
    out.sort();
    out
}

fn sub_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

// ───── restore ─────

/// What an entry of the archive is, by its name. Names are matched, never used as paths.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Manifest,
    Settings,
    Recent,
    Route(String),
    RideIndex(String),
    Ride(String, String),
    Workout(String),
}

/// `Err(())`: the name is unsafe (the whole archive is refused). `Ok(None)`: a harmless name this
/// version does not know.
fn classify(name: &str) -> Result<Option<Kind>, ()> {
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    let unsafe_name = trimmed.is_empty()
        || trimmed.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if unsafe_name {
        return Err(());
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    let json_stem = |file: &str| file.strip_suffix(".json").map(str::to_string);
    Ok(match parts.as_slice() {
        [MANIFEST] => Some(Kind::Manifest),
        [SETTINGS] => Some(Kind::Settings),
        [RECENT] => Some(Kind::Recent),
        [ROUTES_DIR, file] => json_stem(file).filter(|id| valid_id(id)).map(Kind::Route),
        [WORKOUTS_DIR, file] => json_stem(file).filter(|id| valid_id(id)).map(Kind::Workout),
        [RIDES_DIR, route_id, file] if valid_id(route_id) => {
            let stem = json_stem(file);
            match stem {
                Some(s) if s == "index" => Some(Kind::RideIndex((*route_id).to_string())),
                Some(s) if valid_stem(&s) => Some(Kind::Ride((*route_id).to_string(), s)),
                _ => None,
            }
        }
        _ => None,
    })
}

/// What the validation pass found worth writing. Entries are kept as archive indices, so the
/// second pass reads them again instead of holding the whole archive in memory.
#[derive(Default)]
struct Plan {
    settings: Option<Settings>,
    recent: Vec<RecentRoute>,
    routes: BTreeMap<String, usize>,
    workouts: BTreeMap<String, usize>,
    rides: BTreeMap<(String, String), usize>,
    indexes: BTreeMap<String, Vec<RideMeta>>,
}

/// Merges the backup in `input` into the store (see the module docs).
pub fn restore<R: Read + Seek>(store: &Store, input: R) -> Result<RestoreReport, String> {
    restore_with(store, input, &Limits::default())
}

pub fn restore_with<R: Read + Seek>(
    store: &Store,
    input: R,
    limits: &Limits,
) -> Result<RestoreReport, String> {
    let mut archive = ZipArchive::new(input)
        .map_err(|_| t("route.backup.ez_a_fajl_nem_ervenyes_zip_vagy_serult").to_string())?;
    if archive.len() > limits.max_entries {
        return Err(
            t("route.backup.a_mentes_gyanusan_sok_fajlt_tartalmaz_nem_allitom_vi").to_string(),
        );
    }

    let mut report = RestoreReport::default();
    let plan = validate(&mut archive, limits, &mut report)?;
    write_plan(store, &mut archive, limits, plan, &mut report)?;
    Ok(report)
}

fn validate<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: &Limits,
    report: &mut RestoreReport,
) -> Result<Plan, String> {
    let mut plan = Plan::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut manifest_seen = false;
    let mut total = 0_u64;

    for index in 0..archive.len() {
        let (name, is_dir, is_symlink) = {
            let entry = archive.by_index(index).map_err(zip_err)?;
            (entry.name().to_string(), entry.is_dir(), entry.is_symlink())
        };
        let kind = classify(&name).map_err(|()| {
            t("route.backup.a_mentes_ervenytelen_fajlnevet_tartalmaz_nem_allitom").to_string()
        })?;
        if !seen.insert(name.clone()) {
            return Err(
                t("route.backup.a_mentesben_egy_fajl_ketszer_szerepel_nem_allitom_vi").to_string(),
            );
        }
        if is_dir {
            continue;
        }
        let Some(kind) = kind.filter(|_| !is_symlink) else {
            report.skipped += 1;
            continue;
        };
        let bytes = read_entry(archive, index, limits, &mut total)?;
        match kind {
            Kind::Manifest => {
                let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| {
                    t("route.backup.a_mentes_fejlece_backup_json_olvashatatlan").to_string()
                })?;
                if manifest.format > BACKUP_FORMAT {
                    return Err(t(
                        "route.backup.ez_a_mentes_az_alkalmazas_ujabb_verziojabol_valo_fri",
                    )
                    .to_string());
                }
                manifest_seen = true;
            }
            Kind::Settings => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(settings) => {
                    let mut settings = store::sanitize(settings);
                    // Never from a file: the local key stays what it is.
                    settings.ui.google_maps_key.clear();
                    plan.settings = Some(settings);
                }
                Err(_) => report.skipped += 1,
            },
            Kind::Recent => match serde_json::from_slice::<Vec<RecentRoute>>(&bytes) {
                Ok(list) => plan.recent = list,
                Err(_) => report.skipped += 1,
            },
            Kind::Route(id) => match serde_json::from_slice::<Route>(&bytes) {
                Ok(route) if route.id == id && usable_route(&route) => {
                    plan.routes.insert(id, index);
                }
                _ => report.skipped += 1,
            },
            Kind::Workout(id) => match serde_json::from_slice::<Workout>(&bytes) {
                Ok(w) if w.id == id && w.is_imported() && !w.steps.is_empty() => {
                    plan.workouts.insert(id, index);
                }
                _ => report.skipped += 1,
            },
            Kind::Ride(route_id, stem) => match serde_json::from_slice::<RideRecord>(&bytes) {
                // a record of a newer version may mean something else: leave it out
                Ok(r) if r.version <= ride_model::RIDE_RECORD_VERSION && !r.points.is_empty() => {
                    plan.rides.insert((route_id, stem), index);
                }
                _ => report.skipped += 1,
            },
            Kind::RideIndex(route_id) => match serde_json::from_slice::<Vec<RideMeta>>(&bytes) {
                Ok(metas) => {
                    plan.indexes.insert(route_id, metas);
                }
                Err(_) => report.skipped += 1,
            },
        }
    }
    if !manifest_seen {
        return Err(
            t("route.backup.ez_nem_bike_osm_ride_mentes_hianyzik_belole_a_backup").to_string(),
        );
    }
    report.settings = plan.settings.take();
    Ok(plan)
}

/// The same test `Store::load_recent` applies to a stored route.
fn usable_route(route: &Route) -> bool {
    route.points.len() >= 2 && route.total_dist.is_finite() && route.total_dist > 0.0
}

/// One entry, read through a counter: the sizes in the header are only a claim.
fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    limits: &Limits,
    total: &mut u64,
) -> Result<Vec<u8>, String> {
    let entry = archive.by_index(index).map_err(zip_err)?;
    let mut bytes = Vec::new();
    entry
        .take(limits.max_entry_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| io_err(t("route.backup.a_mentes_egyik_fajlja_serult"), &err))?;
    let len = bytes.len() as u64;
    if len > limits.max_entry_bytes {
        return Err(
            t("route.backup.a_mentes_egyik_fajlja_tul_nagy_nem_allitom_vissza").to_string(),
        );
    }
    *total = total.saturating_add(len);
    if *total > limits.max_total_bytes {
        return Err(
            t("route.backup.a_mentes_kicsomagolva_tul_nagy_nem_allitom_vissza").to_string(),
        );
    }
    Ok(bytes)
}

fn write_plan<R: Read + Seek>(
    store: &Store,
    archive: &mut ZipArchive<R>,
    limits: &Limits,
    plan: Plan,
    report: &mut RestoreReport,
) -> Result<(), String> {
    let write_err = |err: io::Error| {
        io_err(
            t("route.backup.a_visszaallitas_kozben_hiba_tortent_amit_addig_vissz"),
            &err,
        )
    };
    let mut total = 0_u64;
    let _guard = store::lock();

    for (id, index) in &plan.routes {
        let Some(path) = store.route_path(id) else {
            continue;
        };
        if path.is_file() {
            continue;
        }
        let bytes = read_entry(archive, *index, limits, &mut total)?;
        store::write_atomic(&path, &bytes).map_err(write_err)?;
        report.routes += 1;
    }

    let route_ids: BTreeSet<&String> = plan
        .indexes
        .keys()
        .chain(plan.rides.keys().map(|(route_id, _)| route_id))
        .collect();
    let mut used: HashSet<(&String, &String)> = HashSet::new();
    for route_id in route_ids {
        let dir = store.dir().join(RIDES_DIR).join(route_id);
        let mut index = store.read_ride_index(&dir);
        let mut known: HashSet<String> = index.iter().map(|m| m.stem.clone()).collect();
        let before = index.len();
        for meta in plan.indexes.get(route_id).map(Vec::as_slice).unwrap_or(&[]) {
            if !valid_stem(&meta.stem) || known.contains(&meta.stem) {
                continue;
            }
            let Some((key, entry)) = plan
                .rides
                .get_key_value(&(route_id.clone(), meta.stem.clone()))
            else {
                continue;
            };
            used.insert((&key.0, &key.1));
            let file = dir.join(format!("{}.json", meta.stem));
            if !file.is_file() {
                let bytes = read_entry(archive, *entry, limits, &mut total)?;
                store::write_atomic(&file, &bytes).map_err(write_err)?;
                report.rides += 1;
            }
            known.insert(meta.stem.clone());
            index.push(meta.clone());
        }
        if index.len() != before {
            index.sort_by(|a, b| b.started_unix_s.total_cmp(&a.started_unix_s));
            let json = serde_json::to_vec_pretty(&index)
                .map_err(|err| write_err(io::Error::other(err)))?;
            store::write_atomic(&dir.join(INDEX_FILE), &json).map_err(write_err)?;
        }
    }
    // a record the index of the archive does not list could not be shown anywhere
    report.skipped += plan
        .rides
        .keys()
        .filter(|(route_id, stem)| !used.contains(&(route_id, stem)))
        .count();

    for (id, index) in &plan.workouts {
        let Some(path) = store.workout_path(id) else {
            continue;
        };
        if path.is_file() {
            continue;
        }
        let bytes = read_entry(archive, *index, limits, &mut total)?;
        store::write_atomic(&path, &bytes).map_err(write_err)?;
        report.workouts += 1;
    }

    // Last: an entry of the recent list needs its route file. No cap here: the normal housekeeping
    // trims the list the next time a route is opened, and a restore never deletes anything.
    let mut recent = store.read_index();
    let have: HashSet<String> = recent.iter().map(|r| r.id.clone()).collect();
    let before = recent.len();
    for entry in plan.recent {
        let present = store.route_path(&entry.id).is_some_and(|p| p.is_file());
        if present && !have.contains(&entry.id) && entry.last_opened_unix.is_finite() {
            recent.push(entry);
        }
    }
    if recent.len() != before {
        recent.sort_by(|a, b| b.last_opened_unix.total_cmp(&a.last_opened_unix));
        store.write_index(&recent).map_err(write_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ride_model::{RecordPoint, RideSource, RideSummary};

    use super::*;

    fn route(id: &str) -> Route {
        let mut route = ride_model::testing::straight_route(&[(1_000.0, 2.0)], 100.0);
        route.id = id.to_string();
        route.name = format!("Út {id}");
        route
    }

    fn point(t: f64, dist: f64) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0,
            speed_ms: 8.0,
            power_w: Some(200.0),
            cadence_rpm: Some(85.0),
            hr_bpm: Some(140.0),
            grade: 1.0,
        }
    }

    fn record(route_id: &str, started: f64) -> RideRecord {
        RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: route_id.to_string(),
            route_name: format!("Út {route_id}"),
            simulated: false,
            summary: RideSummary {
                route_name: format!("Út {route_id}"),
                route_id: route_id.to_string(),
                started_unix_s: started,
                elapsed_s: 600.0,
                dist: 800.0,
                ascent: 10.0,
                avg_speed_ms: 1.3,
                max_speed_ms: 9.0,
                avg_power_w: Some(200.0),
                max_power_w: Some(320.0),
                normalized_power_w: Some(210.0),
                intensity_factor: Some(0.8),
                tss: Some(40.0),
                work_kj: 120.0,
                kcal: 120.0,
                avg_hr: Some(140.0),
                max_hr: Some(170.0),
                avg_cadence: Some(85.0),
                power_zone_secs: [60.0; 7],
                hr_zone_secs: [30.0; 5],
            },
            points: (0..600)
                .map(|t| point(f64::from(t), f64::from(t) * 1.3))
                .collect(),
            source: RideSource::Recorded,
            power_estimated: false,
            estimate: None,
        }
    }

    /// A store with two routes, two rides on one of them and one imported workout.
    fn filled(dir: &Path) -> Store {
        let store = Store::at(dir);
        for id in ["aaaa", "bbbb"] {
            store
                .save_recent_at(&route(id), 1_000.0)
                .expect("save route");
        }
        for started in [1_700_000_000.0, 1_700_100_000.0] {
            store
                .save_ride(&record("aaaa", started), &route("aaaa"))
                .expect("save ride")
                .expect("long enough");
        }
        let mut settings = Settings::default();
        settings.ui.google_maps_key = "AIzaSECRETKEY123".to_string();
        settings.rider.ftp_w = 250.0;
        store.save_settings(&settings).expect("save settings");
        store
    }

    fn settings() -> Settings {
        let mut s = Settings::default();
        s.ui.google_maps_key = "AIzaSECRETKEY123".to_string();
        s.rider.ftp_w = 250.0;
        s
    }

    fn make_backup(store: &Store) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        export(store, &settings(), "9.9.9", 1_700_000_000, &mut out).expect("export");
        out.into_inner()
    }

    fn names(bytes: &[u8]) -> Vec<String> {
        let archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
        archive.file_names().map(str::to_string).collect()
    }

    /// A hand-made archive, to feed the restore things the exporter would never write.
    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, options).expect("start");
            zip.write_all(bytes).expect("write");
        }
        zip.finish().expect("finish").into_inner()
    }

    fn manifest() -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            format: BACKUP_FORMAT,
            app_version: "t".into(),
            created_unix_s: 1,
        })
        .expect("manifest")
    }

    #[test]
    fn a_backup_restores_into_an_empty_folder() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        let bytes = make_backup(&store);

        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let report = restore(&target, Cursor::new(&bytes)).expect("restore");

        assert_eq!((report.routes, report.rides, report.skipped), (2, 2, 0));
        assert_eq!(target.list_recent().len(), 2);
        assert_eq!(target.list_rides("aaaa").len(), 2);
        let original = store.list_rides("aaaa");
        let restored = target.list_rides("aaaa");
        assert_eq!(restored.len(), original.len());
        assert_eq!(restored[0].stem, original[0].stem);
        let ride = target.load_ride("aaaa", &original[0].stem).expect("ride");
        assert_eq!(ride.points.len(), 600);
        assert!(target.load_recent("bbbb").is_some());
        let restored_settings = report.settings.expect("settings travel");
        assert!((restored_settings.rider.ftp_w - 250.0).abs() < 1e-9);
    }

    #[test]
    fn the_google_key_never_enters_the_archive() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        let bytes = make_backup(&store);

        let mut zip = ZipArchive::new(Cursor::new(&bytes)).expect("zip");
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).expect("entry");
            let mut text = Vec::new();
            entry.read_to_end(&mut text).expect("read");
            let text = String::from_utf8_lossy(&text);
            assert!(
                !text.contains("AIzaSECRETKEY123"),
                "{} leaks the key",
                entry.name()
            );
        }
        // and a hand-edited archive cannot smuggle one in either
        let mut forged = Settings::default();
        forged.ui.google_maps_key = "AIzaFROMTHEFILE".to_string();
        let forged = serde_json::to_vec(&forged).expect("json");
        let bytes = archive(&[(MANIFEST, &manifest()), (SETTINGS, &forged)]);
        let dst = tempfile::tempdir().expect("tmp");
        let report = restore(&Store::at(dst.path()), Cursor::new(bytes)).expect("restore");
        assert!(report
            .settings
            .expect("settings")
            .ui
            .google_maps_key
            .is_empty());
    }

    #[test]
    fn caches_logs_and_profiles_stay_out() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        for junk in [
            "tiles/12/1/1.png",
            "streetview/img/a.jpg",
            "streetview/aaaa.json",
            "ble.log",
            "crash.log.1",
            "webview/Cache/x",
            "strava/token.json",
            "exports/a.tcx",
            "routes/leftover.json.123.tmp",
        ] {
            let path = src.path().join(junk);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(&path, b"junk").expect("write");
        }
        let names = names(&make_backup(&store));
        for name in &names {
            let allowed = name == MANIFEST
                || name == SETTINGS
                || name == RECENT
                || name.starts_with("routes/")
                || name.starts_with("rides/")
                || name.starts_with("workouts/");
            assert!(allowed, "{name} is not on the whitelist");
            assert!(
                !name.ends_with(".tmp") && !name.contains("leftover"),
                "{name}"
            );
        }
        assert!(names.iter().any(|n| n == "routes/aaaa.json"));
        assert!(names.iter().any(|n| n == "rides/aaaa/index.json"));
    }

    #[test]
    fn unsafe_names_are_refused_before_anything_is_written() {
        for bad in [
            "../evil.json",
            "routes/../../evil.json",
            "/etc/passwd",
            "C:/Windows/x.json",
            "routes\\a.json",
            "rides//x.json",
        ] {
            let bytes = archive(&[(MANIFEST, &manifest()), (bad, b"{}")]);
            let dst = tempfile::tempdir().expect("tmp");
            let err = restore(&Store::at(dst.path()), Cursor::new(bytes))
                .expect_err(&format!("{bad} must be refused"));
            assert!(err.contains("érvénytelen fájlnevet"), "{bad}: {err}");
            assert!(
                std::fs::read_dir(dst.path()).expect("dir").next().is_none(),
                "{bad} left something behind"
            );
        }
    }

    #[test]
    fn a_name_that_is_only_unknown_is_skipped_not_fatal() {
        let bytes = archive(&[
            (MANIFEST, &manifest()),
            ("notes.txt", b"hello"),
            ("routes/not valid.json", b"{}"),
            ("rides/aaaa/readme.json", b"{}"),
        ]);
        let dst = tempfile::tempdir().expect("tmp");
        let report = restore(&Store::at(dst.path()), Cursor::new(bytes)).expect("restore");
        assert_eq!(report.skipped, 3);
        assert_eq!((report.routes, report.rides, report.workouts), (0, 0, 0));
    }

    /// The zip reader keeps one entry per name, so a doubled name cannot apply twice (the check in
    /// `validate` is a second line of defence). Either way nothing may be written for it.
    #[test]
    fn a_doubled_name_is_never_applied_twice() {
        // the writer refuses a duplicate itself, so rename one entry in the finished bytes (same
        // length, so the offsets and the CRC stay valid)
        let bytes = archive(&[
            (MANIFEST, &manifest()),
            ("routes/aaa.json", b"{}"),
            ("routes/aab.json", b"{}"),
        ]);
        let from = b"routes/aab.json";
        let to = b"routes/aaa.json";
        let mut forged = bytes.clone();
        let mut at = 0;
        while let Some(pos) = forged[at..].windows(from.len()).position(|w| w == from) {
            forged[at + pos..at + pos + from.len()].copy_from_slice(to);
            at += pos + from.len();
        }
        assert_ne!(forged, bytes);
        let dst = tempfile::tempdir().expect("tmp");
        match restore(&Store::at(dst.path()), Cursor::new(forged)) {
            Ok(report) => assert_eq!((report.routes, report.rides), (0, 0)),
            Err(err) => assert!(err.contains("kétszer") || err.contains("zip"), "{err}"),
        }
        assert!(!dst.path().join("routes").exists());
    }

    #[test]
    fn a_zip_bomb_and_a_huge_total_are_refused() {
        let big = vec![b' '; 2 * 1024 * 1024];
        let bytes = archive(&[(MANIFEST, &manifest()), ("recent.json", &big)]);
        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());

        let small_entry = Limits {
            max_entry_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let err = restore_with(&target, Cursor::new(&bytes), &small_entry).expect_err("bomb");
        assert!(err.contains("túl nagy"), "{err}");

        let small_total = Limits {
            max_total_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let err = restore_with(&target, Cursor::new(&bytes), &small_total).expect_err("total");
        assert!(err.contains("túl nagy"), "{err}");

        let few = Limits {
            max_entries: 1,
            ..Limits::default()
        };
        assert!(restore_with(&target, Cursor::new(&bytes), &few).is_err());
        assert!(std::fs::read_dir(dst.path()).expect("dir").next().is_none());
    }

    #[test]
    fn something_that_is_not_a_backup_is_refused_politely() {
        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let err = restore(&target, Cursor::new(b"not a zip at all".to_vec())).expect_err("text");
        assert!(err.contains("nem érvényes zip"), "{err}");
        let no_manifest = archive(&[("recent.json", b"[]")]);
        let err = restore(&target, Cursor::new(no_manifest)).expect_err("no manifest");
        assert!(err.contains("backup.json"), "{err}");
        let newer = serde_json::to_vec(&Manifest {
            format: BACKUP_FORMAT + 1,
            app_version: "99".into(),
            created_unix_s: 0,
        })
        .expect("json");
        let err = restore(&target, Cursor::new(archive(&[(MANIFEST, &newer)]))).expect_err("newer");
        assert!(err.contains("újabb verzió"), "{err}");
    }

    #[test]
    fn a_ride_of_a_newer_version_is_skipped_not_an_error() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        let stem = store.list_rides("aaaa")[0].stem.clone();
        let index = std::fs::read(src.path().join("rides/aaaa/index.json")).expect("index");
        let mut record = record("aaaa", 1_700_000_000.0);
        record.version = ride_model::RIDE_RECORD_VERSION + 1;
        let record = serde_json::to_vec(&record).expect("json");
        let name = format!("rides/aaaa/{stem}.json");
        let bytes = archive(&[
            (MANIFEST, &manifest()),
            ("rides/aaaa/index.json", &index),
            (&name, &record),
        ]);
        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let report = restore(&target, Cursor::new(bytes)).expect("restore");
        assert_eq!(report.rides, 0);
        assert!(report.skipped >= 1);
        assert!(target.list_rides("aaaa").is_empty());
    }

    #[test]
    fn merging_never_overwrites_what_is_there() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        let bytes = make_backup(&store);

        // the target already has one of the two rides, changed, plus a ride of its own
        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let mut local = record("aaaa", 1_700_000_000.0);
        local.points.truncate(400);
        local.summary.elapsed_s = 400.0;
        local.summary.dist = 500.0;
        target
            .save_ride(&local, &route("aaaa"))
            .expect("save")
            .expect("kept");
        target
            .save_ride(&record("aaaa", 1_600_000_000.0), &route("aaaa"))
            .expect("save")
            .expect("kept");
        target
            .save_recent_at(&route("cccc"), 5_000.0)
            .expect("route");
        target
            .save_settings(&settings())
            .expect("local settings stay untouched");

        let report = restore(&target, Cursor::new(&bytes)).expect("restore");
        assert_eq!(report.rides, 1, "only the ride that was missing");
        assert_eq!(report.routes, 2);
        let rides = target.list_rides("aaaa");
        assert_eq!(rides.len(), 3);
        let kept = target.load_ride("aaaa", "1700000000").expect("local ride");
        assert_eq!(kept.points.len(), 400, "the local record wins");
        let recent: Vec<String> = target.list_recent().into_iter().map(|r| r.id).collect();
        assert_eq!(recent.first().map(String::as_str), Some("cccc"));
        assert_eq!(recent.len(), 3);
        // restoring the same file again adds nothing
        let again = restore(&target, Cursor::new(&bytes)).expect("restore again");
        assert_eq!((again.routes, again.rides, again.workouts), (0, 0, 0));
        assert_eq!(target.list_rides("aaaa").len(), 3);
    }

    #[test]
    fn rides_of_a_route_that_is_not_in_the_recent_list_survive() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        // the route behind the rides drops out of the recent list, the rides stay
        store.delete_recent("aaaa").expect("delete route");
        let bytes = make_backup(&store);
        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let report = restore(&target, Cursor::new(bytes)).expect("restore");
        assert_eq!(report.rides, 2);
        assert_eq!(target.list_rides("aaaa").len(), 2);
        assert!(target.load_recent("aaaa").is_none());
    }

    #[test]
    fn broken_settings_are_sanitised_and_a_broken_entry_is_skipped() {
        let bad_settings = br#"{"rider": {"rider_kg": 0.0, "ftp_w": -5.0}}"#;
        let broken_route = b"{ not json";
        let bytes = archive(&[
            (MANIFEST, &manifest()),
            (SETTINGS, bad_settings),
            ("routes/zzzz.json", broken_route),
        ]);
        let dst = tempfile::tempdir().expect("tmp");
        let report = restore(&Store::at(dst.path()), Cursor::new(bytes)).expect("restore");
        let defaults = Settings::default();
        let settings = report.settings.expect("settings");
        assert!((settings.rider.rider_kg - defaults.rider.rider_kg).abs() < 1e-9);
        assert!((settings.rider.ftp_w - defaults.rider.ftp_w).abs() < 1e-9);
        assert_eq!(report.skipped, 1);
        assert!(!dst.path().join("routes").join("zzzz.json").exists());
    }

    #[test]
    fn workouts_roundtrip_and_a_foreign_one_is_not_restored() {
        let src = tempfile::tempdir().expect("tmp");
        let store = filled(src.path());
        let lib = ride_model::WorkoutLibrary::embedded();
        let mut workout = lib.all().first().expect("a built-in workout").clone();
        workout.id = format!("{}abc", ride_model::IMPORT_PREFIX);
        store.save_workout(&workout).expect("save workout");
        let bytes = make_backup(&store);

        let dst = tempfile::tempdir().expect("tmp");
        let target = Store::at(dst.path());
        let report = restore(&target, Cursor::new(&bytes)).expect("restore");
        assert_eq!(report.workouts, 1);
        assert_eq!(target.list_workouts().len(), 1);

        // an entry that is not an import would shadow a built-in workout: left out
        let mut builtin = workout.clone();
        builtin.id = "c-test-ftp-20min".to_string();
        let json = serde_json::to_vec(&builtin).expect("json");
        let bytes = archive(&[
            (MANIFEST, &manifest()),
            ("workouts/c-test-ftp-20min.json", &json),
        ]);
        let other = tempfile::tempdir().expect("tmp");
        let report = restore(&Store::at(other.path()), Cursor::new(bytes)).expect("restore");
        assert_eq!((report.workouts, report.skipped), (0, 1));
    }

    #[test]
    fn the_file_name_carries_the_date() {
        assert_eq!(
            suggested_filename(1_762_300_800.0),
            "bike-osm-ride-mentes-2025-11-05.zip"
        );
    }

    #[test]
    fn classify_maps_names_onto_the_whitelist() {
        assert_eq!(classify("backup.json"), Ok(Some(Kind::Manifest)));
        assert_eq!(
            classify("routes/abc-1_2.json"),
            Ok(Some(Kind::Route("abc-1_2".into())))
        );
        assert_eq!(
            classify("rides/abc/1700000000.json"),
            Ok(Some(Kind::Ride("abc".into(), "1700000000".into())))
        );
        assert_eq!(
            classify("rides/abc/index.json"),
            Ok(Some(Kind::RideIndex("abc".into())))
        );
        assert_eq!(classify("rides/abc/0123.json"), Ok(None));
        assert_eq!(classify("tiles/1/2/3.png"), Ok(None));
        assert_eq!(classify("routes/"), Ok(None));
        assert_eq!(classify("../x"), Err(()));
    }
}
