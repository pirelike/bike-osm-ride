//! The ride log ("tekerés-napló"): saved rides, their index entries, the ghost replay and the
//! comparison a finished ride produced.
//!
//! One ride is one JSON file below `<data>/rides/<route id>/<started unix>.json`; the folder
//! also holds an `index.json` with one [`RideMeta`] per saved ride, so listing the rides of a
//! route never reads the (large) records themselves. The route id is the stable hash of the
//! geometry ([`Route::id`](crate::Route::id)), so the same GPX always finds its own history.
//!
//! The ghost is a *replay* of an earlier ride on the same route: its 1 Hz `(t, dist)` trace is
//! played back next to the live rider, and the difference of the two clock times is the
//! advantage / disadvantage shown in seconds. Segments are the detected climbs
//! ([`Route::climbs`](crate::Route::climbs)); their PR times come from the same records.
//!
//! Units: seconds, metres. Every `delta` is **positive when the rider is behind** the ghost.

use serde::{Deserialize, Serialize};

use crate::{RecordPoint, RideSummary, RoutePosition};

/// On-disk version of [`RideRecord`]. A reader must ignore a record it does not know.
pub const RIDE_RECORD_VERSION: u32 = 1;

/// One finished ride, exactly as it is stored.
///
/// `route_id` / `route_name` are repeated from `summary` so that a record identifies its route
/// without the summary being read (and so a renamed route keeps the name of the ride).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RideRecord {
    pub version: u32,
    pub route_id: String,
    pub route_name: String,
    /// Ridden with the simulated rider: kept in the history but never a PR and never the
    /// "Best" ghost.
    pub simulated: bool,
    pub summary: RideSummary,
    /// The 1 Hz recording (timer time, no pauses).
    pub points: Vec<RecordPoint>,
    /// Where the ride came from. Records written before the review page have no such field.
    #[serde(default)]
    pub source: RideSource,
    /// The watt values in `points` were estimated from speed and grade, not measured.
    #[serde(default)]
    pub power_estimated: bool,
    /// The parameters of that estimate (`None` when the power was measured).
    #[serde(default)]
    pub estimate: Option<EstimateParams>,
}

/// Where a stored ride came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RideSource {
    /// Ridden in this app (trainer or simulated rider).
    #[default]
    Recorded,
    /// Loaded from a GPX / TCX file of another app and projected onto the route.
    Imported,
}

/// The rider and bike values a power estimate was made with.
///
/// Either the rider's own settings (`from_profile`) or values typed in for that one analysis
/// (someone else's ride). Surface is only the rolling-resistance coefficient: there is no gravel
/// model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EstimateParams {
    pub rider_kg: f64,
    pub bike_kg: f64,
    pub crr: f64,
    pub cda: f64,
    pub drivetrain_eff: f64,
    pub air_density: f64,
    /// Taken from the rider's settings (rather than typed in for this analysis).
    pub from_profile: bool,
}

/// One line of the per-route index: everything needed to pick a ghost or a PR without loading
/// the records.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RideMeta {
    /// File stem of the record: the start time in whole seconds since the Unix epoch.
    pub stem: String,
    pub started_unix_s: f64,
    pub elapsed_s: f64,
    /// Distance of the first recorded point (metres). `> 0` when the ride started with a seek.
    pub start_dist: f64,
    /// Distance of the last recorded point (metres).
    pub end_dist: f64,
    /// The ride reached the end of the route (`end_dist` is the route length).
    pub complete: bool,
    pub simulated: bool,
    /// The climbs that were ridden from foot to summit, with the time spent on them.
    #[serde(default)]
    pub segments: Vec<SegmentTime>,
    #[serde(default)]
    pub source: RideSource,
    /// The record's watt values are an estimate (see [`RideRecord::power_estimated`]).
    #[serde(default)]
    pub power_estimated: bool,
    /// Average power and heart rate, so the review list needs not open every record.
    #[serde(default)]
    pub avg_power_w: Option<f64>,
    #[serde(default)]
    pub avg_hr: Option<f64>,
    /// Name of the route at the time of the ride (the review page lists routes that are no longer
    /// among the recent ones). Empty in older index files.
    #[serde(default)]
    pub route_name: String,
    /// Training stress score of the ride, so the weekly trend needs not open every record.
    #[serde(default)]
    pub tss: Option<f64>,
    /// The best average power over the windows of `ride_sim::power_curve::DURATIONS` (watts).
    /// `None`: not computed yet (an entry written before the power page existed; it is filled in
    /// from the record on first use). `Some` with only `None` values: computed, and there is
    /// nothing to show (no power, or not the rider's own watts).
    #[serde(default)]
    pub mmp: Option<Vec<Option<f32>>>,
}

/// All stored rides of one route, for the review page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouteRides {
    pub route_id: String,
    pub route_name: String,
    /// Newest first.
    pub rides: Vec<RideMeta>,
}

/// The time a ride needed on one segment (a detected climb).
///
/// `key` is the climb's start/end distance in 10 m steps (`"430-1130"`), computed by the ride
/// engine: a metre of jitter in the detection must not turn one climb into two segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentTime {
    pub key: String,
    /// Index into `Route::climbs`.
    pub climb: usize,
    pub time_s: f64,
}

/// What a finished ride did against the history that existed before it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RideComparison {
    /// The ride was simulated: the numbers are shown, but they are not PRs.
    pub simulated: bool,
    /// Time difference against the ghost at the finish line (positive = behind it).
    /// `None` when no ghost was active or the ghost did not reach the finish.
    pub ghost_delta_s: Option<f64>,
    pub segments: Vec<SegmentResult>,
}

/// One segment of a finished ride, measured against the PR before the ride.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentResult {
    /// Index into `Route::climbs`.
    pub climb: usize,
    pub time_s: f64,
    /// The best time of an earlier ride on this segment (`None` = first time it was ridden).
    pub previous_pr_s: Option<f64>,
    /// The ride beat an existing PR. A first ride is not a "new PR" (there was nothing to beat).
    pub is_pr: bool,
}

/// Which earlier ride is replayed as the ghost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GhostMode {
    /// No ghost.
    Off,
    /// The route's own recorded time: the `<time>` stamps of the GPX it was loaded from. Available
    /// before the first ride of this route, so a downloaded (or exported) track can be raced
    /// against straight away.
    Route,
    /// The most recent ride on this route (simulated rides included).
    Last,
    /// The fastest complete, non-simulated ride of this route.
    #[default]
    Best,
}

/// The ghost's live position and the time difference, as the HUD shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GhostLive {
    /// Distance the ghost is at right now, metres (it stops at the end of its own ride).
    pub dist: f64,
    pub pos: RoutePosition,
    /// `elapsed - (ghost's time at the rider's distance)`: positive = the rider is behind.
    /// `None` when the ghost's ride does not reach the rider's distance (yet/at all).
    pub delta_s: Option<f64>,
}

/// The segment (climb) the rider is inside right now, against its PR ride.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SegmentLive {
    /// Index into `Route::climbs`.
    pub climb: usize,
    /// Time spent inside the climb so far, seconds.
    pub elapsed_s: f64,
    /// The PR time of the whole climb (`None` = no PR ride for it).
    pub pr_s: Option<f64>,
    /// Difference to the PR ride at the same distance: positive = the rider is behind.
    pub delta_s: Option<f64>,
}

/// What the ride log of the loaded route can offer, for the settings hint.
///
/// The ghost is invisible until there is a ride to replay, so the settings dialog says why:
/// no route, no stored ride, only simulated rides, or nothing that qualifies as a ghost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GhostStatus {
    /// A route is loaded, so the counts below are about it.
    pub loaded: bool,
    /// Stored rides of the route (simulated ones included).
    pub rides: usize,
    /// How many of those are simulated.
    pub simulated: usize,
    /// The selected mode found a ride to replay.
    pub replayable: bool,
    /// That ride never reached the end of the route, so the ghost stops at its own end.
    pub partial: bool,
    /// Total time of the route's own timeline (the GPX's `<time>` stamps), when it has one.
    pub route_time_s: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_mode_defaults_to_best_and_round_trips() {
        assert_eq!(GhostMode::default(), GhostMode::Best);
        for mode in [
            GhostMode::Off,
            GhostMode::Route,
            GhostMode::Last,
            GhostMode::Best,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(serde_json::from_str::<GhostMode>(&json).unwrap(), mode);
        }
        assert_eq!(serde_json::to_string(&GhostMode::Last).unwrap(), "\"Last\"");
    }

    #[test]
    fn an_index_line_and_a_record_from_before_the_review_page_still_load() {
        let meta: RideMeta = serde_json::from_str(
            r#"{"stem":"1700000000","started_unix_s":1700000000.0,"elapsed_s":900.0,
                "start_dist":0.0,"end_dist":5000.0,"complete":true,"simulated":false}"#,
        )
        .unwrap();
        assert_eq!(meta.source, RideSource::Recorded);
        assert!(!meta.power_estimated);
        assert_eq!(meta.avg_power_w, None);
        assert_eq!(meta.avg_hr, None);
        assert_eq!(meta.route_name, "");
        // and from before the power page: no TSS, and "not computed yet" (not "nothing to show")
        assert_eq!(meta.tss, None);
        assert_eq!(meta.mmp, None);

        let with_curve: RideMeta = serde_json::from_str(
            r#"{"stem":"1","started_unix_s":1.0,"elapsed_s":900.0,"start_dist":0.0,
                "end_dist":5000.0,"complete":true,"simulated":false,
                "tss":61.5,"mmp":[500.0,300.0,null,null,null]}"#,
        )
        .unwrap();
        assert_eq!(with_curve.tss, Some(61.5));
        assert_eq!(
            with_curve.mmp,
            Some(vec![Some(500.0), Some(300.0), None, None, None])
        );

        let record = RideRecord {
            version: RIDE_RECORD_VERSION,
            route_id: "abc".into(),
            route_name: "Teszt".into(),
            simulated: false,
            summary: RideSummary {
                route_name: "Teszt".into(),
                route_id: "abc".into(),
                started_unix_s: 1_700_000_000.0,
                elapsed_s: 900.0,
                dist: 5_000.0,
                ascent: 40.0,
                avg_speed_ms: 5.5,
                max_speed_ms: 9.0,
                avg_power_w: None,
                max_power_w: None,
                normalized_power_w: None,
                intensity_factor: None,
                tss: None,
                work_kj: 0.0,
                kcal: 0.0,
                avg_hr: None,
                max_hr: None,
                avg_cadence: None,
                power_zone_secs: [0.0; 7],
                hr_zone_secs: [0.0; 5],
            },
            points: Vec::new(),
            source: RideSource::Imported,
            power_estimated: true,
            estimate: None,
        };
        let mut value = serde_json::to_value(&record).unwrap();
        let object = value.as_object_mut().unwrap();
        for key in ["source", "power_estimated", "estimate"] {
            object.remove(key);
        }
        let old: RideRecord = serde_json::from_value(value).unwrap();
        assert_eq!(old.source, RideSource::Recorded);
        assert!(!old.power_estimated);
        assert_eq!(old.estimate, None);
    }

    #[test]
    fn a_comparison_survives_a_round_trip_and_old_json_stays_readable() {
        let comparison = RideComparison {
            simulated: true,
            ghost_delta_s: Some(12.5),
            segments: vec![SegmentResult {
                climb: 1,
                time_s: 272.0,
                previous_pr_s: Some(280.0),
                is_pr: true,
            }],
        };
        let json = serde_json::to_string(&comparison).unwrap();
        assert_eq!(
            serde_json::from_str::<RideComparison>(&json).unwrap(),
            comparison
        );
        // a comparison written by an older version (no fields at all) still loads
        assert_eq!(
            serde_json::from_str::<RideComparison>("{}").unwrap(),
            RideComparison::default()
        );
    }
}
