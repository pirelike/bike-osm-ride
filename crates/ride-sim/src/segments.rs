//! Local segments: the detected climbs of a route, with the times the history rode them and the
//! PR (personal record) of each one.
//!
//! A segment *is* a climb ([`Route::climbs`]): the climb detection is deterministic for a given
//! route, so the index of the climb identifies the segment inside that route. The stored key
//! quantises the foot and the summit to 10 m (`"430-1130"`): a metre of jitter in a re-processed
//! route must not lose the history.
//!
//! Rules:
//! * a segment time is only recorded when the ride went through the *whole* climb (from before
//!   the foot to after the summit); entry and exit times are interpolated from the 1 Hz samples;
//! * `simulated` rides are kept in the history but never become a PR and never count as a "new
//!   PR" (their times are still shown in the summary);
//! * the "Best" ghost is the fastest *complete*, non-simulated ride that started at the route
//!   start; the "Last" ghost is simply the most recent ride of the route.

use std::collections::HashMap;

use ride_model::{Climb, GhostMode, RecordPoint, RideMeta, Route, SegmentTime};

use crate::ghost::Track;

/// Segment keys quantise the climb ends to this many metres.
pub const KEY_QUANTUM_M: f64 = 10.0;

/// How far a recorded ride may fall short of a climb end and still count as having ridden it
/// (metres): the integration stops a few centimetres before the end of the route.
const REACHED_EPS_M: f64 = 1.0;

/// Stable key of a climb inside its route: `start-end` in 10 m steps.
pub fn segment_key(climb: &Climb) -> String {
    let start = (climb.start_dist / KEY_QUANTUM_M).round();
    let end = (climb.end_dist / KEY_QUANTUM_M).round();
    format!("{start:.0}-{end:.0}")
}

/// Times of the climbs that `points` rode through completely.
pub fn segment_times(route: &Route, points: &[RecordPoint]) -> Vec<SegmentTime> {
    let Some(track) = Track::from_points(points) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, climb) in route.climbs.iter().enumerate() {
        if !(climb.start_dist.is_finite() && climb.end_dist.is_finite()) {
            continue;
        }
        if climb.end_dist <= climb.start_dist {
            continue;
        }
        let reached = track.start_dist() <= climb.start_dist + REACHED_EPS_M
            && track.end_dist() >= climb.end_dist - REACHED_EPS_M;
        if !reached {
            continue;
        }
        let time_s = track.time_at(climb.end_dist) - track.time_at(climb.start_dist);
        if !(time_s.is_finite() && time_s > 0.0) {
            continue;
        }
        out.push(SegmentTime {
            key: segment_key(climb),
            climb: index,
            time_s,
        });
    }
    out
}

/// Best time per segment key, with the ride (`stem`) that holds it. Simulated rides are skipped.
pub fn best_times(metas: &[RideMeta]) -> HashMap<String, (f64, String)> {
    let mut best: HashMap<String, (f64, String)> = HashMap::new();
    for meta in metas {
        if meta.simulated {
            continue;
        }
        for segment in &meta.segments {
            if !(segment.time_s.is_finite() && segment.time_s > 0.0) {
                continue;
            }
            let better = match best.get(&segment.key) {
                Some((time, _)) => segment.time_s < *time,
                None => true,
            };
            if better {
                best.insert(segment.key.clone(), (segment.time_s, meta.stem.clone()));
            }
        }
    }
    best
}

/// The ride to replay as the ghost, if the mode asks for one.
///
/// * `Route` — not a stored ride at all: the route's own timeline (`Route::timeline`, from the
///   GPX `<time>` stamps) is replayed instead, see `Track::from_route`;
/// * `Last` — the most recent ride of the route (simulated rides included: the rider asked for
///   the last thing they rode);
/// * `Best` — the smallest elapsed time among the complete, non-simulated rides that started at
///   the beginning of the route (a ride started by a seek cannot be compared fairly). When the
///   route has no complete ride yet, the non-simulated ride that got the furthest is replayed:
///   a partial ghost that stops at its own end is still better than no ghost at all (the rider
///   has just finished their first ride and expects to see it).
pub fn choose_ghost(metas: &[RideMeta], mode: GhostMode) -> Option<&RideMeta> {
    match mode {
        GhostMode::Off | GhostMode::Route => None,
        GhostMode::Last => metas
            .iter()
            .filter(|meta| meta.started_unix_s.is_finite())
            .max_by(|a, b| a.started_unix_s.total_cmp(&b.started_unix_s)),
        GhostMode::Best => {
            let usable = metas.iter().filter(|meta| {
                !meta.simulated
                    && meta.start_dist.is_finite()
                    && meta.start_dist < 50.0
                    && meta.elapsed_s.is_finite()
                    && meta.elapsed_s > 0.0
            });
            let complete = usable
                .clone()
                .filter(|meta| meta.complete)
                .min_by(|a, b| a.elapsed_s.total_cmp(&b.elapsed_s));
            complete.or_else(|| {
                usable
                    .filter(|meta| !meta.complete && meta.end_dist.is_finite())
                    .max_by(|a, b| {
                        a.end_dist
                            .total_cmp(&b.end_dist)
                            .then_with(|| a.started_unix_s.total_cmp(&b.started_unix_s))
                    })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;

    fn climb(start: f64, end: f64) -> Climb {
        Climb {
            start_dist: start,
            end_dist: end,
            length: end - start,
            gain: (end - start) * 0.06,
            avg_grade: 6.0,
            max_grade: 9.0,
            start_ele: 100.0,
            end_ele: 100.0 + (end - start) * 0.06,
            category: None,
        }
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

    /// A 1 Hz, constant 10 m/s ride up to `len_m`.
    fn ride(len_m: f64) -> Vec<RecordPoint> {
        let n = (len_m / 10.0).ceil() as usize;
        (0..=n)
            .map(|i| point(i as f64, (i as f64 * 10.0).min(len_m)))
            .collect()
    }

    fn meta(stem: &str, started: f64, elapsed: f64, complete: bool, simulated: bool) -> RideMeta {
        RideMeta {
            stem: stem.into(),
            started_unix_s: started,
            elapsed_s: elapsed,
            start_dist: 0.0,
            end_dist: if complete { 10_000.0 } else { 5_000.0 },
            complete,
            simulated,
            segments: vec![],
        }
    }

    #[test]
    fn keys_quantise_to_ten_metres() {
        assert_eq!(segment_key(&climb(4_296.0, 11_303.0)), "430-1130");
        assert_eq!(segment_key(&climb(4_296.0, 11_303.0)), "430-1130");
        assert_eq!(
            segment_key(&climb(4_296.0 + 2.0, 11_303.0 - 3.0)),
            "430-1130"
        );
        assert_eq!(segment_key(&climb(0.0, 500.0)), "0-50");
        // far enough apart is a different segment
        assert_ne!(
            segment_key(&climb(4_296.0, 11_303.0)),
            segment_key(&climb(4_306.0, 11_303.0))
        );
    }

    #[test]
    fn only_fully_ridden_climbs_get_a_time() {
        let mut route = straight_route(&[(20_000.0, 0.0)], 100.0);
        route.climbs = vec![climb(2_000.0, 5_000.0), climb(8_000.0, 12_000.0)];

        // a ride that ends inside the first climb: nothing
        assert!(segment_times(&route, &ride(3_000.0)).is_empty());

        // through the first, into the second: only the first
        let times = segment_times(&route, &ride(10_000.0));
        assert_eq!(times.len(), 1, "{times:?}");
        assert_eq!(times[0].climb, 0);
        assert_eq!(times[0].key, "200-500");
        assert!((times[0].time_s - 300.0).abs() < 1.0, "{}", times[0].time_s);

        // the whole route: both, in route order
        let times = segment_times(&route, &ride(20_000.0));
        assert_eq!(times.len(), 2);
        assert_eq!((times[0].climb, times[1].climb), (0, 1));
        assert!((times[1].time_s - 400.0).abs() < 1.0, "{}", times[1].time_s);
    }

    #[test]
    fn entry_and_exit_times_are_interpolated_between_samples() {
        let mut route = straight_route(&[(1_000.0, 0.0)], 100.0);
        // foot at 255 m, summit at 505 m: both fall between two 1 Hz samples (250/260, 500/510)
        route.climbs = vec![climb(255.0, 505.0)];
        let times = segment_times(&route, &ride(1_000.0));
        assert_eq!(times.len(), 1);
        assert!((times[0].time_s - 25.0).abs() < 1e-6, "{}", times[0].time_s);
    }

    #[test]
    fn a_ride_that_started_inside_the_climb_has_no_segment_time() {
        let mut route = straight_route(&[(20_000.0, 0.0)], 100.0);
        route.climbs = vec![climb(2_000.0, 5_000.0)];
        let partial: Vec<RecordPoint> = ride(20_000.0)
            .into_iter()
            .filter(|p| p.dist >= 3_000.0)
            .collect();
        assert!(segment_times(&route, &partial).is_empty());
        // a ride started before the climb (a seek backwards) does count
        let early: Vec<RecordPoint> = ride(20_000.0)
            .into_iter()
            .filter(|p| p.dist >= 1_000.0)
            .collect();
        assert_eq!(segment_times(&route, &early).len(), 1);
    }

    #[test]
    fn best_times_skip_simulated_rides() {
        let fast_simulated = RideMeta {
            segments: vec![SegmentTime {
                key: "200-500".into(),
                climb: 0,
                time_s: 100.0,
            }],
            simulated: true,
            ..meta("sim", 10.0, 50.0, true, true)
        };
        let slow = RideMeta {
            segments: vec![SegmentTime {
                key: "200-500".into(),
                climb: 0,
                time_s: 300.0,
            }],
            ..meta("slow", 20.0, 900.0, true, false)
        };
        let quick = RideMeta {
            segments: vec![
                SegmentTime {
                    key: "200-500".into(),
                    climb: 0,
                    time_s: 280.0,
                },
                SegmentTime {
                    key: "800-1200".into(),
                    climb: 1,
                    time_s: 250.0,
                },
            ],
            ..meta("quick", 30.0, 800.0, true, false)
        };
        let best = best_times(&[slow.clone(), fast_simulated, quick.clone()]);
        assert_eq!(best.get("200-500"), Some(&(280.0, "quick".to_string())));
        assert_eq!(best.get("800-1200"), Some(&(250.0, "quick".to_string())));
        assert_eq!(best.len(), 2, "the simulated 100 s is not a PR");
        // a tie keeps the earlier entry (the index is newest first)
        let mut tie = quick.clone();
        tie.stem = "tie".into();
        tie.segments[0].time_s = 280.0;
        let best = best_times(&[tie, quick]);
        assert_eq!(best.get("200-500").unwrap().1, "tie");
        // garbage times are ignored
        let broken = RideMeta {
            segments: vec![SegmentTime {
                key: "200-500".into(),
                climb: 0,
                time_s: f64::NAN,
            }],
            ..meta("broken", 40.0, 800.0, true, false)
        };
        assert_eq!(best_times(&[broken]).get("200-500"), None);
    }

    #[test]
    fn choose_ghost_follows_the_mode() {
        let older = meta("older", 100.0, 700.0, true, false);
        let newest = meta("newest", 300.0, 900.0, true, false);
        let fastest = meta("fastest", 200.0, 600.0, true, false);
        let simulated = meta("sim", 400.0, 300.0, true, true);
        let incomplete = RideMeta {
            elapsed_s: 200.0,
            ..meta("partial", 500.0, 200.0, false, false)
        };
        let seeked = RideMeta {
            start_dist: 1_000.0,
            ..meta("seeked", 600.0, 100.0, true, false)
        };
        let all = [
            older.clone(),
            newest,
            fastest,
            simulated,
            incomplete.clone(),
            seeked,
        ];

        assert_eq!(
            choose_ghost(&all, GhostMode::Off).map(|m| m.stem.as_str()),
            None
        );
        assert_eq!(
            choose_ghost(&all, GhostMode::Last).map(|m| m.stem.as_str()),
            Some("seeked"),
            "the most recent ride, whatever it is"
        );
        assert_eq!(
            choose_ghost(&all, GhostMode::Best).map(|m| m.stem.as_str()),
            Some("fastest"),
            "fastest complete, non-simulated ride from the start"
        );
        assert_eq!(choose_ghost(&[], GhostMode::Best), None);
        assert_eq!(choose_ghost(&[], GhostMode::Last), None);

        // Only a partial ride so far: it is replayed anyway, so the first ride of a route is
        // never "no ghost at all". A complete ride always wins over a partial one.
        assert_eq!(
            choose_ghost(std::slice::from_ref(&incomplete), GhostMode::Best)
                .map(|m| m.stem.as_str()),
            Some("partial")
        );
        let mut longer_partial = RideMeta {
            end_dist: 9_000.0,
            elapsed_s: 400.0,
            ..meta("partial2", 700.0, 400.0, false, false)
        };
        assert_eq!(
            choose_ghost(
                &[incomplete.clone(), longer_partial.clone()],
                GhostMode::Best
            )
            .map(|m| m.stem.as_str()),
            Some("partial2"),
            "the partial ride that got the furthest is the ghost"
        );
        assert_eq!(
            choose_ghost(&[older.clone(), incomplete.clone()], GhostMode::Best)
                .map(|m| m.stem.as_str()),
            Some("older"),
            "a complete ride beats a partial one, however short the partial is"
        );
        // a simulated partial ride is still not a "Best" ghost
        let simulated_partial = RideMeta {
            simulated: true,
            ..longer_partial.clone()
        };
        assert_eq!(
            choose_ghost(&[simulated_partial], GhostMode::Best).map(|m| m.stem.as_str()),
            None
        );
        // a ride started by a seek is not replayed in Best mode either
        longer_partial.start_dist = 2_000.0;
        assert_eq!(
            choose_ghost(&[longer_partial], GhostMode::Best).map(|m| m.stem.as_str()),
            None
        );
    }
}
