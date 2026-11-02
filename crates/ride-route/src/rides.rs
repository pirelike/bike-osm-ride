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

use ride_model::{RideMeta, RideRecord, RideSummary, Route};
use ride_sim::segments;

use crate::store::{self, Store};

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

fn valid_stem(stem: &str) -> bool {
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
        let Some(summary) = usable_summary(&record.summary) else {
            return Ok(None);
        };
        let route_id = if record.route_id.is_empty() {
            route.id.as_str()
        } else {
            record.route_id.as_str()
        };
        let dir = route_dir(self, route_id).ok_or_else(store::invalid_id)?;
        let stem = stem_of(summary.started_unix_s).ok_or_else(invalid_time)?;

        let started_unix_s = summary.started_unix_s.round();
        let start_dist = record.points.first().map_or(0.0, |p| p.dist).max(0.0);
        let end_dist = summary.dist.max(0.0);
        let total = route.total_dist.max(0.0);
        let meta = RideMeta {
            stem: stem.clone(),
            started_unix_s,
            elapsed_s: summary.elapsed_s,
            start_dist,
            end_dist,
            complete: total > 0.0 && end_dist + COMPLETE_MARGIN_M >= total,
            simulated: record.simulated,
            segments: segments::segment_times(route, &record.points),
        };

        let record = sanitized(record);
        let json = serde_json::to_vec(&record).map_err(io::Error::other)?;

        let _guard = store::lock();
        store::write_atomic(&dir.join(format!("{stem}.json")), &json)?;
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
        // a record from a newer version may mean something different: ignore it rather than
        // misread it (a record without a recording is no use as a ghost either)
        if record.version > ride_model::RIDE_RECORD_VERSION || record.points.is_empty() {
            return None;
        }
        Some(record)
    }

    fn read_ride_index(&self, dir: &std::path::Path) -> Vec<RideMeta> {
        store::read_limited(&dir.join(INDEX_FILE))
            .and_then(|b| serde_json::from_slice::<Vec<RideMeta>>(&b).ok())
            .unwrap_or_default()
    }
}

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
        "Érvénytelen menet-kezdés (időbélyeg).",
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

/// Newest first.
pub fn list_rides(route_id: &str) -> Vec<RideMeta> {
    Store::default_location().list_rides(route_id)
}

pub fn load_ride(route_id: &str, stem: &str) -> Option<RideRecord> {
    Store::default_location().load_ride(route_id, stem)
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
