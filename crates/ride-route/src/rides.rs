//! The ride log: one JSON record per finished ride below `<data>/rides/<route id>/`.
//!
//! ```text
//! rides/<route id>/<stem>.json    a ride_model::RideRecord (compact JSON)
//! rides/<route id>/index.json     Vec<RideMeta>, newest first (pretty JSON)
//! ```
//!
//! `<stem>` is the ride's start time in whole seconds since the Unix epoch, so the file name
//! sorts the folder chronologically and needs no other identity. The index carries everything
//! needed to pick a ghost or a PR time ([`RideMeta`]) without opening the records; it is
//! rewritten on every save and entries whose file vanished are hidden when listing.
//!
//! Nothing here is touched by the recents housekeeping: deleting a route from the recent list
//! keeps its rides. A ride shorter than [`MIN_RIDE_S`] or [`MIN_RIDE_DIST_M`] is not stored at
//! all (it would only be a ghost nobody wants). The same helpers as [`crate::store`] are used,
//! so the writes are atomic and a corrupt file never breaks anything.

use std::io;
use std::path::PathBuf;

use ride_model::{RideMeta, RideRecord, RideSummary, Route, RouteRides};
use ride_sim::{power_curve, segments};

use crate::store::{self, Store};
use ride_i18n::t;

/// Folder (below the data directory) holding the ride log.
pub const RIDES_DIR: &str = "rides";
/// Name of the per-route index file.
pub const INDEX_FILE: &str = "index.json";
/// A ride shorter than this is not worth keeping (seconds).
pub const MIN_RIDE_S: f64 = 60.0;
/// ... and neither is one shorter than this (metres).
pub const MIN_RIDE_DIST_M: f64 = 200.0;
/// The route counts as completed when the ride got this close to its end, metres.
pub const COMPLETE_MARGIN_M: f64 = 5.0;

fn route_dir(store: &Store, route_id: &str) -> Option<PathBuf> {
    store::valid_id(route_id).then(|| store.dir().join(RIDES_DIR).join(route_id))
}

/// File name of a ride: whole seconds since the Unix epoch, no sign, no separator.
fn stem_of(started_unix_s: f64) -> Option<String> {
    let seconds = started_unix_s.round();
    if !(started_unix_s.is_finite() && seconds > 0.0 && seconds < 1e11) {
        return None;
    }
    Some(format!("{seconds:.0}"))
}

/// Whether a record can be used: of the one version this build knows (a record of another version
/// may mean something different: it is ignored rather than misread) and with a recording (a record
/// without one is no use as a ghost either). The ride log, the index repair and the restore of a
/// backup all ask this one question.
pub(crate) fn usable_record(record: &RideRecord) -> bool {
    record.version == ride_model::RIDE_RECORD_VERSION && !record.points.is_empty()
}

pub(crate) fn valid_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() <= 20
        && stem.chars().all(|c| c.is_ascii_digit())
        && !stem.starts_with('0')
}

impl Store {
    /// Stores one finished ride and refreshes the route's index.
    ///
    /// `Ok(None)` means the ride was too short to keep ([`MIN_RIDE_S`] / [`MIN_RIDE_DIST_M`]);
    /// it is not an error. The segment times are computed here from `route` and the recording,
    /// so the caller only hands over the record.
    pub fn save_ride(&self, record: &RideRecord, route: &Route) -> io::Result<Option<RideMeta>> {
        self.write_ride(record, route, None)
    }

    /// Rewrites the stored ride `stem` with an updated `record` (the watts estimated again): the
    /// ride keeps its file name and its place in the index. [`Store::save_ride`] cannot do that,
    /// it treats different bytes under the same start time as another ride and files them under
    /// the next free second.
    ///
    /// An error when no ride is stored under `stem`; `Ok(None)` when the record is too short to keep.
    pub fn replace_ride(
        &self,
        record: &RideRecord,
        route: &Route,
        stem: &str,
    ) -> io::Result<Option<RideMeta>> {
        if !valid_stem(stem) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                t("route.rides.ervenytelen_menet_kezdes_idobelyeg"),
            ));
        }
        self.write_ride(record, route, Some(stem))
    }

    fn write_ride(
        &self,
        record: &RideRecord,
        route: &Route,
        replace: Option<&str>,
    ) -> io::Result<Option<RideMeta>> {
        let Some(summary) = usable_summary(&record.summary) else {
            return Ok(None);
        };
        let route_id = if record.route_id.is_empty() {
            route.id.as_str()
        } else {
            record.route_id.as_str()
        };
        let dir = route_dir(self, route_id).ok_or_else(store::invalid_id)?;
        let base_stem = stem_of(summary.started_unix_s).ok_or_else(invalid_time)?;

        let record = sanitized(record);
        let json = serde_json::to_vec(&record).map_err(io::Error::other)?;

        let _guard = store::lock();
        // Two rides that started in the same second (an import next to a live ride, two
        // devices) must not overwrite each other: the second one takes the next free second.
        // The very same ride saved again (identical bytes) keeps its stem.
        let stem = match replace {
            Some(stem) => {
                if !dir.join(format!("{stem}.json")).is_file() {
                    return Err(io::Error::from(io::ErrorKind::NotFound));
                }
                stem.to_string()
            }
            None => free_stem(&dir, &base_stem, &json).ok_or_else(invalid_time)?,
        };
        let Some(meta) = meta_of(&record, Some(route), stem.clone()) else {
            return Ok(None);
        };
        store::write_atomic(&dir.join(format!("{stem}.json")), &json)?;
        // If the index write fails now, the record is not lost: `read_ride_index` adopts a
        // record the index does not know.
        let mut index = self.read_ride_index(&dir);
        index.retain(|m| m.stem != meta.stem);
        index.push(meta.clone());
        index.sort_by(|a, b| b.started_unix_s.total_cmp(&a.started_unix_s));
        let index_json = serde_json::to_vec_pretty(&index).map_err(io::Error::other)?;
        store::write_atomic(&dir.join(INDEX_FILE), &index_json)?;
        Ok(Some(meta))
    }

    /// Index of the rides of one route, newest first. Entries whose record vanished are left out.
    pub fn list_rides(&self, route_id: &str) -> Vec<RideMeta> {
        let Some(dir) = route_dir(self, route_id) else {
            return Vec::new();
        };
        let _guard = store::lock();
        let mut list = self.read_ride_index(&dir);
        list.retain(|meta| {
            valid_stem(&meta.stem) && dir.join(format!("{}.json", meta.stem)).is_file()
        });
        // A repaired index (records adopted after a damaged or failed index write) is written
        // back once, so the records are not parsed again on every listing.
        if self.index_needs_repair(&dir) {
            if let Ok(json) = serde_json::to_vec_pretty(&list) {
                let _ = store::write_atomic(&dir.join(INDEX_FILE), &json);
            }
        }
        list.sort_by(|a, b| b.started_unix_s.total_cmp(&a.started_unix_s));
        list
    }

    /// One stored ride, if it is present and readable.
    pub fn load_ride(&self, route_id: &str, stem: &str) -> Option<RideRecord> {
        let dir = route_dir(self, route_id)?;
        if !valid_stem(stem) {
            return None;
        }
        let _guard = store::lock();
        let bytes = store::read_limited(&dir.join(format!("{stem}.json")))?;
        let record: RideRecord = serde_json::from_slice(&bytes).ok()?;
        usable_record(&record).then_some(record)
    }

    /// Whether a ride with this start time is stored for the route (an import of the same file
    /// twice lands on the same stem).
    pub fn has_ride(&self, route_id: &str, started_unix_s: f64) -> bool {
        let (Some(dir), Some(stem)) = (route_dir(self, route_id), stem_of(started_unix_s)) else {
            return false;
        };
        let _guard = store::lock();
        dir.join(format!("{stem}.json")).is_file()
    }

    /// Every route that has stored rides, newest ride first, with its rides (newest first).
    ///
    /// Routes are found by their folder, not by the recent list: a route removed from the recent
    /// routes keeps its rides. The name comes from the index; an index written before the name
    /// was kept there is completed from the newest record.
    pub fn list_all_rides(&self) -> Vec<RouteRides> {
        let root = self.dir().join(RIDES_DIR);
        let Ok(entries) = std::fs::read_dir(&root) else {
            return Vec::new();
        };
        let ids: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|id| store::valid_id(id))
            .collect();
        let mut out = Vec::new();
        for route_id in ids {
            let rides = self.list_rides(&route_id);
            let Some(newest) = rides.first() else {
                continue;
            };
            let route_name = rides
                .iter()
                .map(|m| m.route_name.as_str())
                .find(|name| !name.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    self.load_ride(&route_id, &newest.stem)
                        .map(|record| record.route_name)
                })
                .unwrap_or_default();
            out.push(RouteRides {
                route_id,
                route_name,
                rides,
            });
        }
        out.sort_by(|a, b| {
            let newest = |r: &RouteRides| r.rides.first().map_or(0.0, |m| m.started_unix_s);
            newest(b)
                .total_cmp(&newest(a))
                .then_with(|| a.route_id.cmp(&b.route_id))
        });
        out
    }

    /// Deletes one stored ride and its index entry. `Ok(false)` when there was no such ride.
    pub fn delete_ride(&self, route_id: &str, stem: &str) -> io::Result<bool> {
        let dir = route_dir(self, route_id).ok_or_else(store::invalid_id)?;
        if !valid_stem(stem) {
            return Ok(false);
        }
        let _guard = store::lock();
        let file = dir.join(format!("{stem}.json"));
        let existed = file.is_file();
        if existed {
            std::fs::remove_file(&file)?;
        }
        let mut index = self.read_ride_index(&dir);
        let before = index.len();
        index.retain(|m| m.stem != stem);
        if index.len() != before {
            let json = serde_json::to_vec_pretty(&index).map_err(io::Error::other)?;
            store::write_atomic(&dir.join(INDEX_FILE), &json)?;
        }
        Ok(existed)
    }

    /// Fills in the power numbers of the index entries that have none (rides saved before the
    /// power page existed): their TSS and their power curve, computed from the record once and kept
    /// in the index from then on. Returns how many entries were updated.
    ///
    /// The records are read one at a time without the lock held; only the final patch of each
    /// index takes it, and it re-reads the index first, so a ride saved meanwhile is not lost.
    /// A record that cannot be read is left alone (and tried again next time).
    pub fn backfill_analysis(&self) -> usize {
        let root = self.dir().join(RIDES_DIR);
        let Ok(entries) = std::fs::read_dir(&root) else {
            return 0;
        };
        let ids: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|id| store::valid_id(id))
            .collect();
        let mut updated = 0;
        for route_id in ids {
            let todo: Vec<String> = self
                .list_rides(&route_id)
                .into_iter()
                .filter(|meta| power_curve::stored_curve(meta).is_none())
                .map(|meta| meta.stem)
                .collect();
            let mut computed: Vec<Computed> = Vec::new();
            for stem in todo {
                if let Some(record) = self.load_ride(&route_id, &stem) {
                    let (tss, mmp) = power_curve::index_numbers(&record);
                    computed.push((stem, tss, mmp));
                }
            }
            if computed.is_empty() {
                continue;
            }
            let Some(dir) = route_dir(self, &route_id) else {
                continue;
            };
            let _guard = store::lock();
            let mut index = self.read_ride_index(&dir);
            let mut changed = 0;
            for meta in &mut index {
                if power_curve::stored_curve(meta).is_some() {
                    continue;
                }
                if let Some((_, tss, mmp)) = computed.iter().find(|(stem, _, _)| *stem == meta.stem)
                {
                    meta.tss = *tss;
                    meta.mmp = mmp.clone();
                    changed += 1;
                }
            }
            if changed == 0 {
                continue;
            }
            let Ok(json) = serde_json::to_vec_pretty(&index) else {
                continue;
            };
            if store::write_atomic(&dir.join(INDEX_FILE), &json).is_ok() {
                updated += changed;
            }
        }
        updated
    }

    /// The index of one route folder, repaired: a record file the index does not list (the
    /// index was damaged, or its write failed after the record had been written) is adopted
    /// with a meta computed from the record, exactly as `save_ride` would have written it. A
    /// damaged index therefore never hides the history, and the next save does not overwrite
    /// it with a shorter list. The caller holds the store lock.
    pub(crate) fn read_ride_index(&self, dir: &std::path::Path) -> Vec<RideMeta> {
        let mut index = self.read_index_file(dir).unwrap_or_default();
        let orphans = orphan_stems(dir, &index);
        if orphans.is_empty() {
            return index;
        }
        let route = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|id| self.load_recent_unlocked(id));
        for stem in orphans {
            let Some(record) = store::read_limited(&dir.join(format!("{stem}.json")))
                .and_then(|bytes| serde_json::from_slice::<RideRecord>(&bytes).ok())
            else {
                continue;
            };
            if !usable_record(&record) {
                continue;
            }
            if let Some(meta) = meta_of(&record, route.as_ref(), stem) {
                index.push(meta);
            }
        }
        index.sort_by(|a, b| b.started_unix_s.total_cmp(&a.started_unix_s));
        index
    }

    /// The index file as written, `None` when it is missing or cannot be read.
    fn read_index_file(&self, dir: &std::path::Path) -> Option<Vec<RideMeta>> {
        store::read_limited(&dir.join(INDEX_FILE))
            .and_then(|b| serde_json::from_slice::<Vec<RideMeta>>(&b).ok())
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|m| power_curve::valid_timestamp(m.started_unix_s))
                    .collect()
            })
    }

    /// The index file is unreadable or misses a record of the folder.
    fn index_needs_repair(&self, dir: &std::path::Path) -> bool {
        match self.read_index_file(dir) {
            Some(index) => !orphan_stems(dir, &index).is_empty(),
            None => dir.join(INDEX_FILE).exists() || !orphan_stems(dir, &[]).is_empty(),
        }
    }
}

/// The index line of a stored record: everything `choose_ghost` and the PRs need. `route` gives
/// the segment times and whether the ride reached the end; without it (a repair whose route is
/// no longer cached) the ride is listed without segments and as not complete.
pub(crate) fn meta_of(
    record: &RideRecord,
    route: Option<&Route>,
    stem: String,
) -> Option<RideMeta> {
    let summary = usable_summary(&record.summary)?;
    let (tss, mmp) = power_curve::index_numbers(record);
    let start_dist = record.points.first().map_or(0.0, |p| p.dist).max(0.0);
    // `summary.dist` is the distance ridden; where the ride ended on the route is the last
    // recorded point (the engine always records the finish line).
    let end_dist = record
        .points
        .last()
        .map(|p| p.dist)
        .filter(|d| d.is_finite())
        .unwrap_or(start_dist + summary.dist)
        .max(0.0);
    let total = route.map_or(0.0, |r| r.total_dist.max(0.0));
    Some(RideMeta {
        stem,
        started_unix_s: summary.started_unix_s.round(),
        elapsed_s: summary.elapsed_s,
        start_dist,
        end_dist,
        complete: total > 0.0 && end_dist + COMPLETE_MARGIN_M >= total,
        simulated: record.simulated,
        segments: route.map_or_else(Vec::new, |r| segments::segment_times(r, &record.points)),
        source: record.source,
        power_estimated: record.power_estimated,
        avg_power_w: finite_opt(summary.avg_power_w),
        avg_hr: finite_opt(summary.avg_hr),
        route_name: record.route_name.clone(),
        tss,
        mmp,
    })
}

/// Record files of the folder that `index` does not list.
fn orphan_stems(dir: &std::path::Path, index: &[RideMeta]) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".json").map(str::to_string))
        .filter(|stem| valid_stem(stem) && !index.iter().any(|m| m.stem == *stem))
        .collect()
}

/// The stem a new record is written under: `base` if it is free or holds these very bytes,
/// else the next free second (a handful of tries; `None` past that).
fn free_stem(dir: &std::path::Path, base: &str, json: &[u8]) -> Option<String> {
    let first: u64 = base.parse().ok()?;
    (0..60).map(|k| first + k).find_map(|seconds| {
        let stem = seconds.to_string();
        let path = dir.join(format!("{stem}.json"));
        match std::fs::read(&path) {
            Err(_) => Some(stem),
            Ok(existing) if existing == json => Some(stem),
            Ok(_) => None,
        }
    })
}

/// A stem with the TSS and the power curve computed for it (see `Store::backfill_analysis`).
type Computed = (String, Option<f64>, Option<Vec<Option<f32>>>);

/// The summary if the ride is long enough to keep.
fn usable_summary(summary: &RideSummary) -> Option<&RideSummary> {
    let long_enough = summary.elapsed_s.is_finite() && summary.elapsed_s >= MIN_RIDE_S;
    let far_enough = summary.dist.is_finite() && summary.dist >= MIN_RIDE_DIST_M;
    let timed = summary.started_unix_s.is_finite() && summary.started_unix_s > 0.0;
    (long_enough && far_enough && timed).then_some(summary)
}

fn invalid_time() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        t("route.rides.ervenytelen_menet_kezdes_idobelyeg"),
    )
}

fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn finite_opt(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

/// `serde_json` writes a non-finite value as `null`, which the typed reader then rejects: a
/// single NaN from a broken sensor would make the whole record unreadable. Store zeros instead
/// (the history is a display aid, never a physics input).
fn sanitized(record: &RideRecord) -> RideRecord {
    let mut out = record.clone();
    let s = &mut out.summary;
    s.started_unix_s = finite(s.started_unix_s);
    s.elapsed_s = finite(s.elapsed_s);
    s.dist = finite(s.dist);
    s.ascent = finite(s.ascent);
    s.avg_speed_ms = finite(s.avg_speed_ms);
    s.max_speed_ms = finite(s.max_speed_ms);
    s.avg_power_w = finite_opt(s.avg_power_w);
    s.max_power_w = finite_opt(s.max_power_w);
    s.normalized_power_w = finite_opt(s.normalized_power_w);
    s.intensity_factor = finite_opt(s.intensity_factor);
    s.tss = finite_opt(s.tss);
    s.work_kj = finite(s.work_kj);
    s.kcal = finite(s.kcal);
    s.avg_hr = finite_opt(s.avg_hr);
    s.max_hr = finite_opt(s.max_hr);
    s.avg_cadence = finite_opt(s.avg_cadence);
    for v in s
        .power_zone_secs
        .iter_mut()
        .chain(s.hr_zone_secs.iter_mut())
    {
        *v = finite(*v);
    }
    for p in &mut out.points {
        p.t = finite(p.t);
        p.unix_s = finite(p.unix_s);
        p.dist = finite(p.dist);
        p.lat = finite(p.lat);
        p.lon = finite(p.lon);
        p.ele = finite(p.ele);
        p.speed_ms = finite(p.speed_ms);
        p.power_w = finite_opt(p.power_w);
        p.cadence_rpm = finite_opt(p.cadence_rpm);
        p.hr_bpm = finite_opt(p.hr_bpm);
        p.grade = finite(p.grade);
    }
    out
}

/// Stores one ride in the default location (see [`Store::save_ride`]).
pub fn save_ride(record: &RideRecord, route: &Route) -> io::Result<Option<RideMeta>> {
    Store::default_location().save_ride(record, route)
}

/// Rewrites a stored ride in the default location (see [`Store::replace_ride`]).
pub fn replace_ride(
    record: &RideRecord,
    route: &Route,
    stem: &str,
) -> io::Result<Option<RideMeta>> {
    Store::default_location().replace_ride(record, route, stem)
}

/// Newest first.
pub fn list_rides(route_id: &str) -> Vec<RideMeta> {
    Store::default_location().list_rides(route_id)
}

pub fn load_ride(route_id: &str, stem: &str) -> Option<RideRecord> {
    Store::default_location().load_ride(route_id, stem)
}

/// Every route with stored rides (see [`Store::list_all_rides`]).
pub fn list_all_rides() -> Vec<RouteRides> {
    Store::default_location().list_all_rides()
}

pub fn delete_ride(route_id: &str, stem: &str) -> io::Result<bool> {
    Store::default_location().delete_ride(route_id, stem)
}

pub fn has_ride(route_id: &str, started_unix_s: f64) -> bool {
    Store::default_location().has_ride(route_id, started_unix_s)
}

/// One recorded point, for the tests of other crates.
#[cfg(test)]
fn point(t: f64, dist: f64) -> ride_model::RecordPoint {
    ride_model::RecordPoint {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::{RideSummary, Route, RouteSource};

    fn route() -> Route {
        let mut route = straight_route(&[(2_000.0, 3.0)], 100.0);
        route.id = "route-1".into();
        route.name = "Teszt kör".into();
        route.source = RouteSource::GpxFile;
        route.climbs = vec![ride_model::Climb {
            start_dist: 200.0,
            end_dist: 900.0,
            length: 700.0,
            gain: 21.0,
            avg_grade: 3.0,
            max_grade: 3.0,
            start_ele: 106.0,
            end_ele: 127.0,
            category: None,
        }];
        route
    }

    fn summary(started: f64, elapsed: f64, dist: f64) -> RideSummary {
        RideSummary {
            route_name: "Teszt kör".into(),
            route_id: "route-1".into(),
            started_unix_s: started,
            elapsed_s: elapsed,
            dist,
            ascent: 21.0,
            avg_speed_ms: if elapsed > 0.0 { dist / elapsed } else { 0.0 },
            max_speed_ms: 9.0,
            avg_power_w: Some(210.0),
            max_power_w: Some(320.0),
            normalized_power_w: Some(225.0),
            intensity_factor: Some(1.1),
            tss: Some(95.0),
            work_kj: 400.0,
            kcal: 400.0,
            avg_hr: Some(142.0),
            max_hr: Some(170.0),
            avg_cadence: Some(86.0),
            power_zone_secs: [60.0; 7],
            hr_zone_secs: [30.0; 5],
        }
    }

    fn record(started: f64, elapsed: f64, dist: f64, simulated: bool) -> RideRecord {
        let n = elapsed.round() as usize;
        let points: Vec<_> = (0..=n)
            .map(|i| point(i as f64, (i as f64 * dist / elapsed).min(dist)))
            .collect();
        RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: "route-1".into(),
            route_name: "Teszt kör".into(),
            simulated,
            summary: summary(started, elapsed, dist),
            points,
            source: ride_model::RideSource::Recorded,
            power_estimated: false,
            estimate: None,
            measured_power: Vec::new(),
        }
    }

    #[test]
    fn a_ride_round_trips_and_lands_in_the_index() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        let ride = record(1_700_000_000.0, 300.0, 2_000.0, false);

        let meta = store
            .save_ride(&ride, &route)
            .unwrap()
            .expect("long enough to keep");
        assert_eq!(meta.stem, "1700000000");
        assert!(meta.complete, "the ride reached the end of the route");
        assert_eq!(meta.start_dist, 0.0);
        assert!((meta.end_dist - 2_000.0).abs() < 1e-9);
        assert_eq!(meta.segments.len(), 1, "the climb was ridden completely");
        assert!((meta.segments[0].time_s - 105.0).abs() < 1.5);

        // the record is readable and identical
        let loaded = store.load_ride("route-1", "1700000000").expect("stored");
        assert_eq!(loaded, ride);
        assert!(tmp.path().join("rides/route-1/1700000000.json").is_file());

        // and the index lists it
        let list = store.list_rides("route-1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], meta);
    }

    #[test]
    fn the_index_keeps_the_tss_and_the_power_curve() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let meta = store
            .save_ride(&record(1_700_000_000.0, 300.0, 2_000.0, false), &route())
            .unwrap()
            .expect("kept");
        assert_eq!(meta.tss, Some(95.0));
        let curve = meta.mmp.clone().expect("computed at save time");
        assert_eq!(curve.len(), ride_sim::power_curve::DURATIONS.len());
        assert!((curve[2].expect("5 min") - 200.0).abs() < 1e-3, "{curve:?}");
        assert_eq!(curve[3], None, "a 5 minute ride has no 20 minute value");

        // a simulated ride is computed too, and empty: nothing to back-fill later
        let simulated = store
            .save_ride(&record(1_700_001_000.0, 300.0, 2_000.0, true), &route())
            .unwrap()
            .expect("kept");
        assert_eq!(simulated.mmp, Some(vec![None; curve.len()]));
    }

    #[test]
    fn an_index_from_before_the_power_page_is_filled_in_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        for started in [1_700_000_000.0, 1_700_001_000.0] {
            store
                .save_ride(&record(started, 300.0, 2_000.0, false), &route)
                .unwrap();
        }
        // make the index look like one written by the previous version: no tss, no curve
        let index_path = tmp.path().join("rides/route-1/index.json");
        let mut index: Vec<RideMeta> =
            serde_json::from_slice(&std::fs::read(&index_path).unwrap()).unwrap();
        for meta in &mut index {
            meta.tss = None;
            meta.mmp = None;
        }
        std::fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        // one of the records is gone: it cannot be computed and must not break the others
        std::fs::remove_file(tmp.path().join("rides/route-1/1700001000.json")).unwrap();
        assert!(store.list_rides("route-1").iter().all(|m| m.mmp.is_none()));

        assert_eq!(store.backfill_analysis(), 1);
        let listed = store.list_rides("route-1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].tss, Some(95.0));
        assert!(listed[0].mmp.is_some());
        // the entry whose record vanished is still unfilled in the file, and a second run is a no-op
        let raw: Vec<RideMeta> =
            serde_json::from_slice(&std::fs::read(&index_path).unwrap()).unwrap();
        assert_eq!(raw.iter().filter(|m| m.mmp.is_none()).count(), 1);
        assert_eq!(store.backfill_analysis(), 0);
        // nothing to do in an empty folder either
        assert_eq!(Store::at(tmp.path().join("nowhere")).backfill_analysis(), 0);
    }

    #[test]
    fn every_route_with_rides_is_listed_and_a_ride_can_be_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_100.0, 200.0, 1_500.0, false), &route)
            .unwrap();
        store
            .save_ride(&record(1_700_000_200.0, 250.0, 1_800.0, false), &route)
            .unwrap();
        // a second route, ridden later
        let mut other = record(1_700_000_900.0, 300.0, 2_000.0, false);
        other.route_id = "route-2".into();
        other.route_name = "Másik kör".into();
        store.save_ride(&other, &route).unwrap();

        let all = store.list_all_rides();
        assert_eq!(
            all.iter().map(|r| r.route_id.as_str()).collect::<Vec<_>>(),
            ["route-2", "route-1"],
            "the route ridden last comes first"
        );
        assert_eq!(all[0].route_name, "Másik kör");
        assert_eq!(all[1].route_name, "Teszt kör");
        assert_eq!(all[1].rides.len(), 2);
        assert_eq!(all[1].rides[0].stem, "1700000200");

        assert!(store.has_ride("route-1", 1_700_000_100.0));
        assert!(!store.has_ride("route-1", 1_700_000_101.0));
        assert!(!store.has_ride("../x", 1_700_000_100.0));

        assert!(store.delete_ride("route-1", "1700000100").unwrap());
        assert!(
            !store.delete_ride("route-1", "1700000100").unwrap(),
            "already gone"
        );
        assert!(!store.delete_ride("route-1", "../../x").unwrap());
        assert!(store.delete_ride("../x", "1700000100").is_err());
        assert_eq!(store.list_rides("route-1").len(), 1);
        assert!(store.load_ride("route-1", "1700000100").is_none());
        assert!(!store.has_ride("route-1", 1_700_000_100.0));

        // the last ride of a route gone: the route drops out of the list
        assert!(store.delete_ride("route-1", "1700000200").unwrap());
        assert_eq!(store.list_all_rides().len(), 1);
        assert!(Store::at(tmp.path().join("nincs"))
            .list_all_rides()
            .is_empty());
    }

    #[test]
    fn an_index_without_route_names_is_completed_from_the_newest_record() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_100.0, 200.0, 1_500.0, false), &route)
            .unwrap();
        // strip the names from the index, like an index of an older version
        let index_path = tmp.path().join("rides/route-1/index.json");
        let mut index: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&index_path).unwrap()).unwrap();
        for entry in &mut index {
            entry.as_object_mut().unwrap().remove("route_name");
        }
        std::fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        let all = store.list_all_rides();
        assert_eq!(all[0].route_name, "Teszt kör");
    }

    #[test]
    fn the_index_is_newest_first_and_survives_a_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_100.0, 200.0, 1_500.0, false), &route)
            .unwrap();
        store
            .save_ride(&record(1_700_000_300.0, 350.0, 2_000.0, true), &route)
            .unwrap();
        store
            .save_ride(&record(1_700_000_200.0, 250.0, 1_800.0, false), &route)
            .unwrap();
        let list = store.list_rides("route-1");
        assert_eq!(
            list.iter().map(|m| m.stem.as_str()).collect::<Vec<_>>(),
            ["1700000300", "1700000200", "1700000100"]
        );
        assert!(list[0].simulated);
        // saving the same ride again does not duplicate the entry
        store
            .save_ride(&record(1_700_000_300.0, 350.0, 2_000.0, true), &route)
            .unwrap();
        assert_eq!(store.list_rides("route-1").len(), 3);
        // a second, independent store instance sees the same folder
        assert_eq!(Store::at(tmp.path()).list_rides("route-1").len(), 3);
        // an unknown route has no rides
        assert!(store.list_rides("nincs-ilyen").is_empty());
        assert!(store.load_ride("nincs-ilyen", "1700000100").is_none());
    }

    #[test]
    fn short_rides_are_not_saved() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        // 30 s
        assert_eq!(
            store
                .save_ride(&record(1_700_000_000.0, 30.0, 400.0, false), &route)
                .unwrap(),
            None
        );
        // long enough in time, but only 150 m
        assert_eq!(
            store
                .save_ride(&record(1_700_000_000.0, 120.0, 150.0, false), &route)
                .unwrap(),
            None
        );
        // no usable start time
        assert_eq!(
            store
                .save_ride(&record(0.0, 120.0, 900.0, false), &route)
                .unwrap(),
            None
        );
        assert!(store.list_rides("route-1").is_empty());
        assert!(!tmp.path().join("rides/route-1/index.json").exists());
    }

    #[test]
    fn the_completion_flag_follows_the_route_length() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        let meta = store
            .save_ride(&record(1_700_000_000.0, 200.0, 1_200.0, false), &route)
            .unwrap()
            .unwrap();
        assert!(!meta.complete);
        // within the margin counts as complete
        let meta = store
            .save_ride(&record(1_700_000_500.0, 200.0, 1_997.0, false), &route)
            .unwrap()
            .unwrap();
        assert!(meta.complete);
    }

    #[test]
    fn damaged_and_foreign_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_000.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        // the record vanished: the index entry is hidden
        std::fs::remove_file(tmp.path().join("rides/route-1/1700000000.json")).unwrap();
        assert!(store.list_rides("route-1").is_empty());
        assert!(store.load_ride("route-1", "1700000000").is_none());
        // corrupt index: empty, and a new save repairs it
        std::fs::write(tmp.path().join("rides/route-1/index.json"), b"garbage").unwrap();
        assert!(store.list_rides("route-1").is_empty());
        store
            .save_ride(&record(1_700_000_500.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        assert_eq!(store.list_rides("route-1").len(), 1);
        // a truncated record loads as nothing instead of panicking
        std::fs::write(
            tmp.path().join("rides/route-1/1700000500.json"),
            b"{\"version\":1,",
        )
        .unwrap();
        assert!(store.load_ride("route-1", "1700000500").is_none());
        // the index still lists the file (listing never reads whole records); it simply yields
        // no record, which is exactly what "no ghost" needs
        assert_eq!(store.list_rides("route-1").len(), 1);
    }

    #[test]
    fn a_damaged_index_does_not_hide_the_history() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store.save_recent(&route).unwrap();
        store
            .save_ride(&record(1_700_000_000.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        std::fs::write(tmp.path().join("rides/route-1/index.json"), b"garbage").unwrap();
        // listing adopts the record the index lost, with its segments and completion
        let listed = store.list_rides("route-1");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].complete);
        // and the next save does not overwrite the history with a shorter list
        store
            .save_ride(&record(1_700_000_500.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        assert_eq!(store.list_rides("route-1").len(), 2);
        // the repaired index was written back
        let index = store
            .read_index_file(&tmp.path().join("rides/route-1"))
            .unwrap();
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn a_record_without_an_index_entry_is_adopted() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_000.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        // what a failed index write after the record write leaves behind
        std::fs::remove_file(tmp.path().join("rides/route-1/index.json")).unwrap();
        assert_eq!(store.list_rides("route-1").len(), 1);
        assert!(store.has_ride("route-1", 1_700_000_000.0));
    }

    #[test]
    fn two_rides_in_the_same_second_are_both_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        let first = record(1_700_000_000.0, 200.0, 2_000.0, false);
        let second = record(1_700_000_000.0, 300.0, 2_000.0, false);
        let a = store.save_ride(&first, &route).unwrap().unwrap();
        let b = store.save_ride(&second, &route).unwrap().unwrap();
        assert_ne!(a.stem, b.stem);
        assert_eq!(store.list_rides("route-1").len(), 2);
        // the very same ride saved again keeps its file
        let again = store.save_ride(&first, &route).unwrap().unwrap();
        assert_eq!(again.stem, a.stem);
        assert_eq!(store.list_rides("route-1").len(), 2);
    }

    #[test]
    fn path_traversal_and_bad_stems_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path().join("data"));
        for bad in ["../x", "a/b", "a\\b", "", "..", "x.json", &"a".repeat(65)] {
            assert!(store.list_rides(bad).is_empty(), "{bad}");
            assert!(store.load_ride(bad, "1700000000").is_none(), "{bad}");
            // the record's own id is used when it has one; otherwise the route's
            let mut ride = record(1_700_000_000.0, 200.0, 900.0, false);
            ride.route_id = bad.to_string();
            let mut route = route();
            route.id = bad.to_string();
            assert!(store.save_ride(&ride, &route).is_err(), "{bad}");
        }
        for bad in ["../x", "abc", "-1", "", "0012", &"1".repeat(21)] {
            assert!(store.load_ride("route-1", bad).is_none(), "{bad:?}");
        }
        assert!(!tmp.path().join("x").exists());
    }

    #[test]
    fn the_recents_housekeeping_does_not_touch_the_rides() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        store
            .save_ride(&record(1_700_000_000.0, 200.0, 2_000.0, false), &route)
            .unwrap();
        store.save_recent_at(&route, 1.0).unwrap();
        store.delete_recent(&route.id).unwrap();
        assert_eq!(store.list_rides(&route.id).len(), 1, "history kept");
    }

    #[test]
    fn nan_values_do_not_make_a_record_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        let mut ride = record(1_700_000_000.0, 200.0, 2_000.0, false);
        ride.summary.avg_power_w = Some(f64::NAN);
        ride.points[3].dist = f64::NAN;
        ride.points[4].hr_bpm = Some(f64::INFINITY);
        store.save_ride(&ride, &route).unwrap();
        let loaded = store.load_ride("route-1", "1700000000").expect("readable");
        assert_eq!(loaded.summary.avg_power_w, None);
        assert_eq!(loaded.points[3].dist, 0.0);
        assert_eq!(loaded.points[4].hr_bpm, None);
    }

    #[test]
    fn a_record_from_a_newer_version_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let route = route();
        let mut ride = record(1_700_000_000.0, 200.0, 2_000.0, false);
        ride.version = ride_model::RIDE_RECORD_VERSION + 1;
        std::fs::create_dir_all(tmp.path().join("rides/route-1")).unwrap();
        std::fs::write(
            tmp.path().join("rides/route-1/1700000000.json"),
            serde_json::to_vec(&ride).unwrap(),
        )
        .unwrap();
        assert!(store.load_ride("route-1", "1700000000").is_none());
        // ... but the current version still loads
        ride.version = ride_model::RIDE_RECORD_VERSION;
        std::fs::write(
            tmp.path().join("rides/route-1/1700000000.json"),
            serde_json::to_vec(&ride).unwrap(),
        )
        .unwrap();
        assert!(store.load_ride("route-1", "1700000000").is_some());
        let _ = route;
    }

    #[test]
    fn the_stem_is_the_start_time_in_whole_seconds() {
        assert_eq!(stem_of(1_700_000_000.4).as_deref(), Some("1700000000"));
        assert_eq!(stem_of(1_700_000_000.6).as_deref(), Some("1700000001"));
        assert_eq!(stem_of(f64::NAN), None);
        assert_eq!(stem_of(0.0), None);
        assert_eq!(stem_of(-5.0), None);
        assert!(valid_stem("1700000000"));
        assert!(!valid_stem("01700000000"), "no leading zero");
        assert!(!valid_stem("17000.0"));
        assert!(!valid_stem(""));
    }

    /// The test fixture helper is only used above; keep it compiling when the module grows.
    #[test]
    fn the_summary_takes_the_route_id_from_the_record() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let mut ride = record(1_700_000_000.0, 200.0, 2_000.0, false);
        let mut other = route();
        other.id = "route-2".into();
        // the record names the route it was ridden on
        ride.route_id = "route-2".into();
        store.save_ride(&ride, &other).unwrap();
        assert_eq!(store.list_rides("route-2").len(), 1);
        assert!(store.list_rides("route-1").is_empty());
    }
}
