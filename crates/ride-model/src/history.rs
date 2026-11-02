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
}

/// One line of the per-route index: everything needed to pick a ghost or a PR without loading
/// the records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
