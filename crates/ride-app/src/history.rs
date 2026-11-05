//! The ride log on the app side: turning the saved rides of a route into what the ride screen
//! needs (a ghost to replay, the PR time of every climb) and comparing a finished ride with the
//! history that existed before it.
//!
//! Everything here is pure: the controller reads the store on a blocking thread, hands the index
//! over and gets a [`HistoryPlan`] back. That keeps the file I/O and the track building off the
//! UI thread and makes the whole decision testable without a data directory.

use std::collections::HashMap;

use ride_model::{
    GhostMode, GhostStatus, RideComparison, RideMeta, RideRecord, Route, SegmentResult,
};
use ride_sim::ghost::{GhostSet, Track};
use ride_sim::segments;

/// What the ride screen needs from the history of one route.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryPlan {
    /// The ghost to replay and the per-climb PR rides. `None` = nothing to replay at all.
    pub ghost: Option<GhostSet>,
    /// PR time of every climb, indexed like `Route::climbs` (`None` = no time on that climb yet).
    pub prs: Vec<Option<f64>>,
}

/// The plan plus the counts the settings dialog shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub plan: HistoryPlan,
    pub status: GhostStatus,
}

/// Reads the saved rides of `route` from the default data directory and builds the plan.
pub fn plan(route: &Route, mode: GhostMode) -> History {
    let route_id = route.id.clone();
    let metas = ride_route::rides::list_rides(&route_id);
    let plan = build_plan(route, mode, &metas, |stem| {
        ride_route::rides::load_ride(&route_id, stem)
    });
    History {
        plan,
        status: status_of(&metas, mode, route_time(route)),
    }
}

/// Total time of the route's own timeline, seconds: the `<time>` stamps of the GPX it came from.
///
/// This is the "ride" the route itself carries, so a freshly downloaded track has a ghost before
/// the first ride is stored. `None` when the file had no complete timeline.
pub fn route_time(route: &Route) -> Option<f64> {
    let last = *route.timeline.last()?;
    (route.timeline.len() == route.points.len() && last.is_finite() && last > 0.0).then_some(last)
}

/// What the log of a loaded route holds, and whether `mode` finds a ride to replay.
pub fn status_of(metas: &[RideMeta], mode: GhostMode, route_time_s: Option<f64>) -> GhostStatus {
    let chosen = match mode {
        GhostMode::Route => None,
        _ => segments::choose_ghost(metas, mode),
    };
    let replayable = match mode {
        GhostMode::Route => route_time_s.is_some(),
        _ => chosen.is_some(),
    };
    GhostStatus {
        loaded: true,
        rides: metas.len(),
        simulated: metas.iter().filter(|meta| meta.simulated).count(),
        replayable,
        partial: chosen.is_some_and(|meta| !meta.complete),
        route_time_s,
    }
}

/// Builds the plan from an already listed index; `load` fetches one record by its stem.
///
/// The PR rides are independent of the ghost mode: turning the ghost off keeps the live segment
/// comparison working (only the ghost itself disappears).
pub fn build_plan(
    route: &Route,
    mode: GhostMode,
    metas: &[RideMeta],
    mut load: impl FnMut(&str) -> Option<RideRecord>,
) -> HistoryPlan {
    let mut prs = vec![None; route.climbs.len()];
    let mut segment_tracks: Vec<Option<(f64, Track)>> = vec![None; route.climbs.len()];
    let best = segments::best_times(metas);
    let mut loaded: HashMap<String, Option<Track>> = HashMap::new();
    for (index, climb) in route.climbs.iter().enumerate() {
        let Some((time_s, stem)) = best.get(&segments::segment_key(climb)) else {
            continue;
        };
        prs[index] = Some(*time_s);
        let track = loaded
            .entry(stem.clone())
            .or_insert_with(|| load(stem).and_then(|record| Track::from_points(&record.points)))
            .clone();
        if let Some(track) = track {
            segment_tracks[index] = Some((*time_s, track));
        }
    }
    // "Route" replays the GPX's own recording instead of a stored ride.
    let main = match mode {
        GhostMode::Route => Track::from_route(route),
        _ => segments::choose_ghost(metas, mode)
            .and_then(|meta| load(&meta.stem))
            .and_then(|record| Track::from_points(&record.points)),
    };
    let ghost = GhostSet {
        main,
        segments: segment_tracks,
    };
    HistoryPlan {
        ghost: (!ghost.is_empty()).then_some(ghost),
        prs,
    }
}

/// Comparison of `record` against the PRs that existed *before* it was saved.
///
/// A segment is a "new PR" only when an earlier time was beaten: a first ride is the reference,
/// not a record. A simulated ride keeps its times (they are shown) but never becomes a PR.
pub fn compare(
    record: &RideRecord,
    route: &Route,
    previous: &HashMap<String, (f64, String)>,
    ghost_delta_s: Option<f64>,
) -> RideComparison {
    let times = segments::segment_times(route, &record.points);
    let mut results = Vec::with_capacity(times.len());
    for segment in times {
        let previous_pr_s = previous.get(&segment.key).map(|(time, _)| *time);
        let is_pr = !record.simulated
            && previous_pr_s.is_some_and(|best| segment.time_s < best - PR_MARGIN_S);
        results.push(SegmentResult {
            climb: segment.climb,
            time_s: segment.time_s,
            previous_pr_s,
            is_pr,
        });
    }
    RideComparison {
        simulated: record.simulated,
        ghost_delta_s,
        segments: results,
    }
}

/// A new time must beat the old one by at least this much to count as a PR (seconds): the 1 Hz
/// recording cannot resolve less, and a tie must not flicker between rides.
const PR_MARGIN_S: f64 = 0.05;

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::{RecordPoint, RideSummary};
    use std::sync::Arc;

    fn route() -> Route {
        let mut route = straight_route(&[(3_000.0, 2.0)], 100.0);
        route.id = "route-1".into();
        route.climbs = vec![
            ride_model::Climb {
                start_dist: 200.0,
                end_dist: 1_200.0,
                length: 1_000.0,
                gain: 50.0,
                avg_grade: 5.0,
                max_grade: 5.0,
                start_ele: 104.0,
                end_ele: 154.0,
                category: None,
            },
            ride_model::Climb {
                start_dist: 2_000.0,
                end_dist: 2_800.0,
                length: 800.0,
                gain: 30.0,
                avg_grade: 3.75,
                max_grade: 4.0,
                start_ele: 160.0,
                end_ele: 190.0,
                category: None,
            },
        ];
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
            speed_ms: 10.0,
            power_w: Some(200.0),
            cadence_rpm: Some(85.0),
            hr_bpm: Some(140.0),
            grade: 1.0,
        }
    }

    fn record(stem_started: f64, len_m: f64, speed: f64, simulated: bool) -> RideRecord {
        let n = (len_m / speed).ceil() as usize;
        let points: Vec<RecordPoint> = (0..=n)
            .map(|i| point(i as f64, (i as f64 * speed).min(len_m)))
            .collect();
        RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: "route-1".into(),
            route_name: "Teszt".into(),
            simulated,
            summary: RideSummary {
                route_name: "Teszt".into(),
                route_id: "route-1".into(),
                started_unix_s: stem_started,
                elapsed_s: len_m / speed,
                dist: len_m,
                ascent: 0.0,
                avg_speed_ms: speed,
                max_speed_ms: speed,
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
            points,
            source: ride_model::RideSource::Recorded,
            power_estimated: false,
            estimate: None,
        }
    }

    fn meta(record: &RideRecord, simulated: bool) -> RideMeta {
        let route = route();
        RideMeta {
            stem: format!("{:.0}", record.summary.started_unix_s),
            started_unix_s: record.summary.started_unix_s,
            elapsed_s: record.summary.elapsed_s,
            start_dist: 0.0,
            end_dist: record.summary.dist,
            complete: record.summary.dist + 5.0 >= route.total_dist,
            simulated,
            segments: segments::segment_times(&route, &record.points),
            ..Default::default()
        }
    }

    #[test]
    fn the_plan_carries_the_pr_times_and_the_best_ghost() {
        let route = route();
        // two complete rides: 10 m/s (300 s) and 8 m/s (375 s)
        let quick = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let slow = record(1_700_001_000.0, 3_000.0, 8.0, false);
        let simulated = record(1_700_002_000.0, 3_000.0, 12.0, true);
        let metas = vec![
            meta(&simulated, true),
            meta(&slow, false),
            meta(&quick, false),
        ];
        let by_stem: HashMap<String, RideRecord> = [quick.clone(), slow.clone(), simulated.clone()]
            .into_iter()
            .map(|r| (format!("{:.0}", r.summary.started_unix_s), r))
            .collect();

        let plan = build_plan(&route, GhostMode::Best, &metas, |stem| {
            by_stem.get(stem).cloned()
        });
        // the PR times ignore the simulated ride: 100 s and 80 s at 10 m/s
        assert_eq!(plan.prs.len(), 2);
        assert!((plan.prs[0].unwrap() - 100.0).abs() < 1.5);
        assert!((plan.prs[1].unwrap() - 80.0).abs() < 1.5);
        let ghost = plan.ghost.expect("a ghost");
        let main = ghost.main.expect("the best ride is replayed");
        assert!((main.end_dist() - 3_000.0).abs() < 1e-6);
        assert_eq!(ghost.segments.len(), 2);
        assert!(ghost.segments[0].is_some() && ghost.segments[1].is_some());
        // the "best" ghost is the quick one: 300 s for the route
        assert!((main.end_time() - 300.0).abs() < 1.5);
    }

    #[test]
    fn the_status_counts_the_rides_and_says_whether_the_mode_can_replay_one() {
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let simulated = record(1_700_001_000.0, 3_000.0, 12.0, true);
        let only_simulated = vec![meta(&simulated, true)];
        let both = vec![meta(&simulated, true), meta(&ride, false)];

        // nothing stored yet: the settings dialog must not claim a ghost
        let empty = status_of(&[], GhostMode::Best, None);
        assert!(empty.loaded);
        assert_eq!((empty.rides, empty.simulated), (0, 0));
        assert!(!empty.replayable);
        assert_eq!(empty.route_time_s, None);

        // a simulated ride is not a "Best" ghost, but it is the "Last" one
        let status = status_of(&only_simulated, GhostMode::Best, None);
        assert_eq!((status.rides, status.simulated), (1, 1));
        assert!(!status.replayable);
        assert!(status_of(&only_simulated, GhostMode::Last, None).replayable);

        // a real ride is replayable in both modes
        let status = status_of(&both, GhostMode::Best, None);
        assert_eq!((status.rides, status.simulated), (2, 1));
        assert!(status.replayable);

        // "Off" replays nothing, however much history there is
        assert!(!status_of(&both, GhostMode::Off, None).replayable);

        // "Route" replays the GPX timeline instead of the log: the rides do not matter
        assert!(!status_of(&both, GhostMode::Route, None).replayable);
        let with_timeline = status_of(&[], GhostMode::Route, Some(5_025.0));
        assert!(with_timeline.replayable);
        assert_eq!(with_timeline.route_time_s, Some(5_025.0));

        // a ride that stopped early is still replayed, and the hint says so
        let mut partial = meta(&record(1_700_000_000.0, 1_000.0, 10.0, false), false);
        partial.end_dist = 1_000.0;
        partial.complete = false;
        let status = status_of(&[partial], GhostMode::Best, None);
        assert!(status.replayable, "a partial ride is a ghost too");
        assert!(status.partial);
        // ... and a complete ride is not "partial"
        assert!(!status_of(&both, GhostMode::Best, None).partial);
    }

    #[test]
    fn the_route_timeline_is_its_own_ghost() {
        let mut timed = route();
        // 3 km at 10 m/s: the GPX's own 300 s
        timed.timeline = timed.points.iter().map(|point| point.dist / 10.0).collect();
        assert_eq!(route_time(&timed), Some(300.0));

        // the log is empty, yet the route mode has a ghost
        let plan = build_plan(&timed, GhostMode::Route, &[], |_| None);
        let ghost = plan.ghost.expect("the GPX is replayed");
        let track = ghost.main.as_ref().expect("its timeline");
        assert!((track.end_dist() - 3_000.0).abs() < 1e-6);
        assert!((track.end_time() - 300.0).abs() < 1e-6);
        // no PR rides: the segment PRs still come from the log only
        assert!(plan.prs.iter().all(Option::is_none));

        // a route without a timeline (a BRouter route) has no such ghost
        let plain = route();
        assert_eq!(route_time(&plain), None);
        assert_eq!(
            build_plan(&plain, GhostMode::Route, &[], |_| None).ghost,
            None
        );

        // a rider slower than the GPX falls behind it
        let sample = ride_model::SensorSample {
            power_w: Some(150.0),
            ..ride_model::SensorSample::default()
        };
        let mut engine = ride_sim::RideEngine::new(
            std::sync::Arc::new(timed),
            ride_model::RiderSettings::default(),
        );
        engine.set_ghost(Some(ghost));
        let waiting = engine.snapshot().ghost.expect("waits on the start line");
        assert_eq!(waiting.dist, 0.0);
        engine.start(1_700_500_000.0);
        let mut last = None;
        for _ in 0..120 {
            last = engine.step(0.5, &sample, 1_700_500_000.0).ghost;
        }
        let ghost = last.expect("the ghost is replayed");
        assert!(
            ghost.delta_s.is_some_and(|delta| delta > 0.0),
            "a slower rider is behind the GPX: {:?}",
            ghost.delta_s
        );
    }

    #[test]
    fn turning_the_ghost_off_keeps_the_segment_prs() {
        let route = route();
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let metas = vec![meta(&ride, false)];
        let plan = build_plan(&route, GhostMode::Off, &metas, |_| Some(ride.clone()));
        let ghost = plan.ghost.expect("the PR rides are still there");
        assert!(ghost.main.is_none(), "no ghost in Off mode");
        assert!(ghost.segments[0].is_some(), "but the climb PR is");
        assert!(plan.prs[0].is_some());
    }

    #[test]
    fn no_history_means_no_plan() {
        let route = route();
        let plan = build_plan(&route, GhostMode::Best, &[], |_| None);
        assert_eq!(plan.ghost, None);
        assert_eq!(plan.prs, vec![None, None]);
        // an index entry whose record cannot be read is ignored, not fatal: the PR *time* comes
        // from the index, only the live comparison needs the recording
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let plan = build_plan(&route, GhostMode::Best, &[meta(&ride, false)], |_| None);
        assert_eq!(plan.ghost, None);
        assert!(plan.prs[0].is_some() && plan.prs[1].is_some());
    }

    #[test]
    fn the_comparison_finds_new_prs_and_keeps_first_times() {
        let route = route();
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let previous = HashMap::from([
            ("20-120".to_string(), (120.0, "older".to_string())),
            ("200-280".to_string(), (70.0, "older".to_string())),
        ]);
        let comparison = compare(&ride, &route, &previous, Some(12.0));
        assert_eq!(comparison.ghost_delta_s, Some(12.0));
        assert!(!comparison.simulated);
        let first = &comparison.segments[0];
        assert_eq!(first.climb, 0);
        assert_eq!(first.previous_pr_s, Some(120.0));
        assert!(first.is_pr, "100 s beats 120 s");
        let second = &comparison.segments[1];
        assert!(!second.is_pr, "80 s does not beat 70 s");
        assert_eq!(second.previous_pr_s, Some(70.0));

        // no history at all: neither is a "new PR", but both times are reported
        let comparison = compare(&ride, &route, &HashMap::new(), None);
        assert!(comparison.segments.iter().all(|s| !s.is_pr));
        assert!(comparison
            .segments
            .iter()
            .all(|s| s.previous_pr_s.is_none()));
    }

    #[test]
    fn a_simulated_ride_never_becomes_a_pr() {
        let route = route();
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, true);
        let previous = HashMap::from([("20-120".to_string(), (500.0, "older".to_string()))]);
        let comparison = compare(&ride, &route, &previous, Some(-3.0));
        assert!(comparison.simulated);
        assert_eq!(comparison.segments.len(), 2);
        assert!(
            comparison.segments.iter().all(|s| !s.is_pr),
            "simulated times are shown but not ranked"
        );
        assert_eq!(comparison.ghost_delta_s, Some(-3.0));
    }

    #[test]
    fn a_margin_keeps_ties_out_of_the_pr_list() {
        let route = route();
        let ride = record(1_700_000_000.0, 3_000.0, 10.0, false);
        let exact = segments::segment_times(&route, &ride.points)[0].time_s;
        let previous = HashMap::from([("20-120".to_string(), (exact, "older".to_string()))]);
        let comparison = compare(&ride, &route, &previous, None);
        assert!(!comparison.segments[0].is_pr, "a tie is not a new PR");
    }

    // ── end to end, without the UI ─────────────────────────────────────────────
    //
    // The whole ride-log pipeline as the controller drives it: engine → record → store → index →
    // plan → engine (ghost + live segment) → comparison. Only the Dioxus wiring is left out.

    /// Rides `route` at a constant power until the end (or `max_s` of ride time).
    fn ride_engine(route: &Arc<Route>, watts: f64, max_s: f64) -> (RideSummary, Vec<RecordPoint>) {
        use ride_model::{RideState, RiderSettings, SensorSample};
        use ride_sim::RideEngine;

        let mut engine = RideEngine::new(Arc::clone(route), RiderSettings::default());
        engine.start(1_700_000_000.0);
        let sample = SensorSample {
            power_w: Some(watts),
            cadence_rpm: Some(88.0),
            hr_bpm: Some(140.0),
            ..SensorSample::default()
        };
        let mut t = 0.0;
        loop {
            let snapshot = engine.step(0.5, &sample, 1_700_000_000.0 + t);
            t += 0.5;
            if snapshot.state == RideState::Finished || t >= max_s {
                break;
            }
        }
        (engine.summary(), engine.history().to_vec())
    }

    /// One ride exactly as the controller would store it.
    fn stored(
        route: &Route,
        started: f64,
        watts: f64,
        max_s: f64,
    ) -> (RideRecord, Arc<Route>, RideSummary) {
        let arc = Arc::new(route.clone());
        let (mut summary, points) = ride_engine(&arc, watts, max_s);
        summary.started_unix_s = started;
        let record = RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: route.id.clone(),
            route_name: route.name.clone(),
            simulated: false,
            summary: summary.clone(),
            points,
            source: ride_model::RideSource::Recorded,
            power_estimated: false,
            estimate: None,
        };
        (record, arc, summary)
    }

    #[test]
    fn two_stored_rides_give_the_next_one_a_ghost_a_pr_and_a_comparison() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let store = ride_route::store::Store::at(tmp.path());
        let route = route();

        // Two rides: 250 W and 300 W over the same course, stored exactly like `on_finished` does.
        let (slow, _, slow_summary) = stored(&route, 1_700_000_000.0, 250.0, 900.0);
        let (fast, arc, fast_summary) = stored(&route, 1_700_100_000.0, 300.0, 900.0);
        assert!(slow_summary.dist > 2_500.0, "the slow ride finished");
        assert!(
            fast_summary.elapsed_s < slow_summary.elapsed_s,
            "300 W is quicker: {} vs {}",
            fast_summary.elapsed_s,
            slow_summary.elapsed_s
        );
        let slow_meta = store.save_ride(&slow, &arc).expect("save").expect("kept");
        let fast_meta = store.save_ride(&fast, &arc).expect("save").expect("kept");
        assert!(slow_meta.complete && fast_meta.complete);
        assert_eq!(store.list_rides(&route.id).len(), 2);

        // The index drives the plan: with only the slower ride stored, that is the ghost and the
        // PR, exactly what the second ride would have seen.
        let only_slow = build_plan(
            &route,
            GhostMode::Best,
            std::slice::from_ref(&slow_meta),
            |stem| store.load_ride(&route.id, stem),
        );
        let prs = only_slow.prs.clone();
        assert!(prs.iter().all(Option::is_some), "both climbs have a PR");
        let ghost_set = only_slow.ghost.clone().expect("a ghost");

        // Ride the course again with that ghost: the 250 W trace is beaten by the 300 W rider.
        let sample = ride_model::SensorSample {
            power_w: Some(300.0),
            ..ride_model::SensorSample::default()
        };
        let metas = store.list_rides(&route.id);
        let mut engine =
            ride_sim::RideEngine::new(Arc::clone(&arc), ride_model::RiderSettings::default());
        engine.set_ghost(Some(ghost_set));
        // Loading the route is enough to see the ghost: it waits on the start line.
        let waiting = engine
            .snapshot()
            .ghost
            .expect("the ghost waits on the start line");
        assert_eq!(waiting.dist, 0.0);
        assert_eq!(waiting.delta_s, Some(0.0));
        engine.start(1_700_200_000.0);
        let mut last = engine.snapshot();
        for _ in 0..200 {
            last = engine.step(0.5, &sample, 1_700_200_000.0);
        }
        let ghost = last.ghost.expect("the ghost is replayed");
        assert!(
            ghost.delta_s.is_some_and(|delta| delta < 0.0),
            "the quicker rider is ahead of the 250 W ghost: {:?}",
            ghost.delta_s
        );
        assert!(
            engine.ghost().is_some(),
            "the engine keeps the aligned ghost"
        );

        // ... and the live segment compares against the PR at the same distance.
        let mut inside = None;
        let mut engine2 =
            ride_sim::RideEngine::new(Arc::clone(&arc), ride_model::RiderSettings::default());
        let plan = build_plan(
            &route,
            GhostMode::Best,
            std::slice::from_ref(&slow_meta),
            |stem| store.load_ride(&route.id, stem),
        );
        engine2.set_ghost(plan.ghost);
        engine2.start(1_700_300_000.0);
        for _ in 0..200 {
            let snapshot = engine2.step(0.5, &sample, 1_700_300_000.0);
            if let Some(segment) = snapshot.segment {
                inside = Some(segment);
            }
        }
        let segment = inside.expect("inside the first climb");
        assert_eq!(segment.climb, 0);
        assert_eq!(
            segment.pr_s, prs[0],
            "the PR time comes from the stored ride"
        );
        assert!(segment.delta_s.is_some_and(|delta| delta < 0.0));

        // The finished faster ride beats the slower one on both segments.
        let previous = segments::best_times(&[slow_meta]);
        let comparison = compare(&fast, &route, &previous, Some(-12.0));
        assert_eq!(comparison.segments.len(), 2);
        assert!(comparison.segments.iter().all(|s| s.is_pr));
        assert_eq!(comparison.ghost_delta_s, Some(-12.0));
        assert!(!comparison.simulated);
        // the best ghost of the two-rider index is the quicker ride
        let both = build_plan(&route, GhostMode::Best, &metas, |stem| {
            store.load_ride(&route.id, stem)
        });
        assert!(status_of(&metas, GhostMode::Best, None).replayable);
        let best = both.ghost.expect("a ghost").main.expect("the best trace");
        assert!(
            (best.end_time() - fast_summary.elapsed_s).abs() < 2.0,
            "the best ghost is the quicker ride: {} vs {}",
            best.end_time(),
            fast_summary.elapsed_s
        );
    }
}
