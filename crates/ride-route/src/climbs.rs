//! Climb detection on the processed (smoothed, resampled) profile.
//!
//! Rules (from the project brief):
//! * a climb is at least **500 m** long, averages at least **3 %** and gains at least
//!   **25 m**;
//! * dips (stretches that are not climbing) shorter than **150 m** do not split a
//!   climb;
//! * category by the Tour-style score `length [m] × average grade [%]`:
//!   ≥ 8 000 → Cat 4, ≥ 16 000 → Cat 3, ≥ 32 000 → Cat 2, ≥ 64 000 → Cat 1,
//!   ≥ 80 000 → HC; smaller climbs that still pass the three limits get `None`.
//!
//! Method: every sample whose (80 m) grade is at least [`SEED_GRADE_PCT`] is "climbing";
//! consecutive climbing samples form runs, runs separated by less than
//! [`MAX_DIP_M`] are merged, and the ends of every run are then snapped to the real
//! foot (lowest point) and summit (highest point) within [`SNAP_M`], never crossing a
//! neighbouring climb. `gain` is the net elevation difference between foot and summit
//! (so `avg_grade = gain / length`); `max_grade` is the steepest 80 m grade inside.

use ride_model::{Climb, ClimbCategory, RoutePoint};

pub const MIN_LENGTH_M: f64 = 500.0;
pub const MIN_AVG_GRADE_PCT: f64 = 3.0;
pub const MIN_GAIN_M: f64 = 25.0;
pub const MAX_DIP_M: f64 = 150.0;
/// Grade above which a sample counts as climbing when seeding runs.
pub const SEED_GRADE_PCT: f64 = 2.0;
/// How far the ends of a run may move to reach the foot / summit.
pub const SNAP_M: f64 = 80.0;

/// Category for a climb of `length_m` metres at `avg_grade_pct` percent.
pub fn category(length_m: f64, avg_grade_pct: f64) -> Option<ClimbCategory> {
    let score = length_m * avg_grade_pct;
    match score {
        s if s >= 80_000.0 => Some(ClimbCategory::Hc),
        s if s >= 64_000.0 => Some(ClimbCategory::Cat1),
        s if s >= 32_000.0 => Some(ClimbCategory::Cat2),
        s if s >= 16_000.0 => Some(ClimbCategory::Cat3),
        s if s >= 8_000.0 => Some(ClimbCategory::Cat4),
        _ => None,
    }
}

/// Detect the climbs of a processed route (`points` sorted by `dist`).
pub fn detect(points: &[RoutePoint]) -> Vec<Climb> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }

    // 1. runs of climbing samples
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if points[i].grade >= SEED_GRADE_PCT {
            let start = i;
            while i + 1 < n && points[i + 1].grade >= SEED_GRADE_PCT {
                i += 1;
            }
            runs.push((start, i));
        }
        i += 1;
    }

    // 2. merge across short dips
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for run in runs {
        match merged.last_mut() {
            Some(prev) if points[run.0].dist - points[prev.1].dist < MAX_DIP_M => prev.1 = run.1,
            _ => merged.push(run),
        }
    }

    // 3. snap ends, measure, validate
    let mut climbs: Vec<Climb> = Vec::new();
    let mut floor = 0usize; // first index the next climb may start at
    for (k, &(s0, e0)) in merged.iter().enumerate() {
        let ceil = merged.get(k + 1).map_or(n - 1, |next| next.0);
        let start = snap_foot(points, s0, e0, floor);
        let end = snap_summit(points, e0, start, ceil);
        if end <= start {
            continue;
        }
        let (a, b) = (&points[start], &points[end]);
        let length = b.dist - a.dist;
        let gain = b.ele - a.ele;
        if length < MIN_LENGTH_M || gain < MIN_GAIN_M {
            continue;
        }
        let avg_grade = 100.0 * gain / length;
        if avg_grade < MIN_AVG_GRADE_PCT {
            continue;
        }
        let max_grade = points[start..=end]
            .iter()
            .map(|p| p.grade)
            .fold(avg_grade, f64::max);
        climbs.push(Climb {
            start_dist: a.dist,
            end_dist: b.dist,
            length,
            gain,
            avg_grade,
            max_grade,
            start_ele: a.ele,
            end_ele: b.ele,
            category: category(length, avg_grade),
        });
        floor = end;
    }
    climbs
}

/// Lowest point within `SNAP_M` of the run start (not before `floor`, not past the run end).
fn snap_foot(points: &[RoutePoint], run_start: usize, run_end: usize, floor: usize) -> usize {
    let d0 = points[run_start].dist;
    let lo = first_at_or_after(points, floor, run_start, d0 - SNAP_M);
    let hi = last_at_or_before(points, run_start, run_end, d0 + SNAP_M);
    pick_extreme(points, lo, hi, run_start, false)
}

/// Highest point within `SNAP_M` of the run end (not before `start`, not past `ceil`).
fn snap_summit(points: &[RoutePoint], run_end: usize, start: usize, ceil: usize) -> usize {
    let d1 = points[run_end].dist;
    let lo = first_at_or_after(points, start, run_end, d1 - SNAP_M);
    let hi = last_at_or_before(points, run_end, ceil, d1 + SNAP_M);
    pick_extreme(points, lo, hi, run_end, true)
}

/// First index in `from..=to` whose distance is at least `dist` (`to` when there is none).
/// Binary search: a scan from `from` would walk the whole stretch of rejected runs in front of
/// every new run.
fn first_at_or_after(points: &[RoutePoint], from: usize, to: usize, dist: f64) -> usize {
    let window = &points[from..=to];
    from + window
        .partition_point(|p| p.dist < dist)
        .min(window.len() - 1)
}

/// Last index in `from..=to` whose distance is at most `dist` (`from` when there is none).
fn last_at_or_before(points: &[RoutePoint], from: usize, to: usize, dist: f64) -> usize {
    let window = &points[from..=to];
    from + window.partition_point(|p| p.dist <= dist).saturating_sub(1)
}

/// Index in `lo..=hi` with the lowest (or highest) elevation; among near-ties
/// (within 0.3 m) the one closest to `anchor`, so flat tops/feet do not drift.
fn pick_extreme(
    points: &[RoutePoint],
    lo: usize,
    hi: usize,
    anchor: usize,
    highest: bool,
) -> usize {
    let (lo, hi) = (lo.min(hi), hi.max(lo));
    let key = |i: usize| {
        if highest {
            points[i].ele
        } else {
            -points[i].ele
        }
    };
    let best = (lo..=hi).map(key).fold(f64::NEG_INFINITY, f64::max);
    (lo..=hi)
        .filter(|&i| key(i) >= best - 0.3)
        .min_by_key(|&i| i.abs_diff(anchor))
        .unwrap_or(anchor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::testutil::route_from_segments;

    fn climbs_of(segments: &[(f64, f64)]) -> Vec<Climb> {
        route_from_segments(segments, 200.0).climbs
    }

    #[test]
    fn categories_by_length_times_grade() {
        assert_eq!(category(1000.0, 5.0), None); // 5 000
        assert_eq!(category(2000.0, 4.0), Some(ClimbCategory::Cat4)); // 8 000
        assert_eq!(category(4000.0, 7.0), Some(ClimbCategory::Cat3)); // 28 000
        assert_eq!(category(4000.0, 4.0), Some(ClimbCategory::Cat3)); // 16 000
        assert_eq!(category(8000.0, 4.0), Some(ClimbCategory::Cat2)); // 32 000
        assert_eq!(category(8000.0, 8.0), Some(ClimbCategory::Cat1)); // 64 000
        assert_eq!(category(10_000.0, 8.0), Some(ClimbCategory::Hc)); // 80 000
        assert_eq!(category(10_000.0, 7.99), Some(ClimbCategory::Cat1));
    }

    #[test]
    fn single_clean_climb_is_found_with_correct_numbers() {
        let c = climbs_of(&[(2000.0, 0.0), (4000.0, 7.0), (1500.0, -4.0)]);
        assert_eq!(c.len(), 1, "{c:?}");
        let c = &c[0];
        assert!(
            (c.start_dist - 2000.0).abs() < 40.0,
            "start {}",
            c.start_dist
        );
        assert!((c.length - 4000.0).abs() < 60.0, "length {}", c.length);
        assert!((c.avg_grade - 7.0).abs() < 0.3, "avg {}", c.avg_grade);
        assert!((c.gain - 280.0).abs() < 8.0, "gain {}", c.gain);
        assert!((c.end_ele - c.start_ele - c.gain).abs() < 1e-9);
        assert!(c.max_grade >= c.avg_grade && c.max_grade < 8.0);
        assert!((c.end_dist - c.start_dist - c.length).abs() < 1e-9);
        assert_eq!(c.category, Some(ClimbCategory::Cat3));
    }

    #[test]
    fn short_dip_merges_long_dip_splits() {
        // 1.5 km @ 6 %, 100 m @ -2 %, 1.5 km @ 6 %  -> one climb
        let c = climbs_of(&[
            (1000.0, 0.0),
            (1500.0, 6.0),
            (100.0, -2.0),
            (1500.0, 6.0),
            (1000.0, 0.0),
        ]);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!((c[0].length - 3100.0).abs() < 80.0, "len {}", c[0].length);
        // 1.5 km @ 6 %, 400 m @ -2 %, 1.5 km @ 6 % -> two climbs
        let c = climbs_of(&[
            (1000.0, 0.0),
            (1500.0, 6.0),
            (400.0, -2.0),
            (1500.0, 6.0),
            (1000.0, 0.0),
        ]);
        assert_eq!(c.len(), 2, "{c:?}");
        assert!(c[0].end_dist < c[1].start_dist);
        // A flat 100 m rest inside a climb is a dip too.
        let c = climbs_of(&[
            (1000.0, 0.0),
            (1500.0, 6.0),
            (100.0, 0.0),
            (1500.0, 6.0),
            (1000.0, 0.0),
        ]);
        assert_eq!(c.len(), 1, "{c:?}");
    }

    #[test]
    fn climbs_below_the_limits_are_ignored() {
        // too short (400 m @ 10 %)
        assert!(climbs_of(&[(1000.0, 0.0), (400.0, 10.0), (1000.0, 0.0)]).is_empty());
        // too gentle (3 km @ 2 %)
        assert!(climbs_of(&[(1000.0, 0.0), (3000.0, 2.0), (1000.0, 0.0)]).is_empty());
        // avg ok (3.5 %) but gain only 21 m
        assert!(climbs_of(&[(1000.0, 0.0), (600.0, 3.5), (1000.0, 0.0)]).is_empty());
        // pure descent and flat
        assert!(climbs_of(&[(3000.0, -6.0), (2000.0, 0.0)]).is_empty());
    }

    #[test]
    fn small_but_valid_climb_has_no_category() {
        let c = climbs_of(&[(1000.0, 0.0), (800.0, 5.0), (1000.0, 0.0)]); // 4 000 points
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].category, None);
        assert!((c[0].gain - 40.0).abs() < 3.0);
    }

    #[test]
    fn climb_at_the_very_start_and_end_of_the_route() {
        let c = climbs_of(&[(1500.0, 6.0), (1000.0, 0.0)]);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(c[0].start_dist < 20.0);
        let c = climbs_of(&[(1000.0, 0.0), (1500.0, 6.0)]);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(
            (c[0].end_dist - 2500.0).abs() < 20.0,
            "end {}",
            c[0].end_dist
        );
    }

    #[test]
    fn rolling_terrain_without_a_sustained_climb_has_none() {
        // +-3 % rollers of ~400 m: never a 500 m stretch at >= 3 % net.
        let seg: Vec<(f64, f64)> = (0..30)
            .map(|i| (400.0, if i % 2 == 0 { 3.0 } else { -3.0 }))
            .collect();
        assert!(climbs_of(&seg).is_empty());
    }

    #[test]
    fn climbs_are_ordered_and_disjoint() {
        let c = climbs_of(&[
            (500.0, 0.0),
            (1200.0, 5.0),
            (800.0, -5.0),
            (2000.0, 8.0),
            (1000.0, -8.0),
            (3000.0, 4.0),
        ]);
        assert_eq!(c.len(), 3, "{c:?}");
        for w in c.windows(2) {
            assert!(w[0].end_dist <= w[1].start_dist);
        }
    }

    #[test]
    fn degenerate_inputs() {
        assert!(detect(&[]).is_empty());
        let r = route_from_segments(&[(100.0, 0.0)], 100.0);
        assert!(detect(&r.points[..2]).is_empty());
    }
}
