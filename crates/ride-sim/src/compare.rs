//! Rides of one route side by side.
//!
//! Every stored ride is a 1 Hz list of samples along the route ([`RecordPoint`], `dist` in metres
//! along the route), so two rides of the same route share one axis: the distance. [`DistSeries`]
//! puts a ride on a fixed distance grid (10 m by default) so charts can overlay them,
//! [`delta_time`] says where one ride gained or lost seconds on another, and [`section_stats`]
//! sums up one stretch of the route (a climb, a zoomed range).
//!
//! Pure functions, no I/O. Everything is finite-safe: a ride that does not cover a distance gives
//! `None` there instead of a made-up number.

use ride_model::RecordPoint;

use crate::analysis::normalized_power;
use crate::ghost::{Track, COVER_EPS_M};

/// Default grid step, metres.
pub const GRID_M: f64 = 10.0;
/// Normalised power needs at least this many 1 Hz samples to mean anything, seconds.
const MIN_NP_SAMPLES: usize = 30;
/// A stretch needs at least this much climbing for a VAM figure, metres.
const MIN_VAM_ASCENT_M: f64 = 10.0;

/// One ride sampled at `start_dist + i · step_m`.
///
/// The grid is anchored at multiples of `step_m`, so two series with the same step line up index
/// for index (after an offset): see [`delta_time`].
#[derive(Debug, Clone, PartialEq)]
pub struct DistSeries {
    pub step_m: f64,
    pub start_dist: f64,
    /// Timer time at each grid distance, seconds (first arrival there).
    pub time_s: Vec<f64>,
    pub speed_ms: Vec<f64>,
    pub power_w: Vec<Option<f64>>,
    pub cadence_rpm: Vec<Option<f64>>,
    pub hr_bpm: Vec<Option<f64>>,
    pub ele: Vec<f64>,
}

impl DistSeries {
    /// The recording on the grid; `None` when it covers less than one step of distance.
    pub fn from_points(points: &[RecordPoint], step_m: f64) -> Option<Self> {
        if !(step_m.is_finite() && step_m > 0.0) {
            return None;
        }
        let track = Track::from_points(points)?;
        // monotone distance of every usable sample, like the track keeps it
        let usable: Vec<(f64, &RecordPoint)> = {
            let mut top = 0.0f64;
            points
                .iter()
                .filter(|p| p.dist.is_finite() && p.t.is_finite())
                .map(|p| {
                    top = top.max(p.dist.max(0.0));
                    (top, p)
                })
                .collect()
        };
        let first = (track.start_dist() / step_m).ceil();
        let last = (track.end_dist() / step_m).floor();
        if last < first {
            return None;
        }
        let n = (last - first) as usize + 1;
        let mut out = Self {
            step_m,
            start_dist: first * step_m,
            time_s: Vec::with_capacity(n),
            speed_ms: Vec::with_capacity(n),
            power_w: Vec::with_capacity(n),
            cadence_rpm: Vec::with_capacity(n),
            hr_bpm: Vec::with_capacity(n),
            ele: Vec::with_capacity(n),
        };
        for k in 0..n {
            let d = out.start_dist + k as f64 * step_m;
            out.time_s.push(track.time_at(d));
            let i = usable.partition_point(|(top, _)| *top < d);
            let (a, b) = (i.saturating_sub(1), i.min(usable.len() - 1));
            let (da, pa) = usable[a];
            let (db, pb) = usable[b];
            let frac = if db - da > 1e-9 {
                ((d - da) / (db - da)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            out.speed_ms.push(lerp(pa.speed_ms, pb.speed_ms, frac));
            out.ele.push(lerp(pa.ele, pb.ele, frac));
            out.power_w.push(lerp_opt(pa.power_w, pb.power_w, frac));
            out.cadence_rpm
                .push(lerp_opt(pa.cadence_rpm, pb.cadence_rpm, frac));
            out.hr_bpm.push(lerp_opt(pa.hr_bpm, pb.hr_bpm, frac));
        }
        Some(out)
    }

    pub fn len(&self) -> usize {
        self.time_s.len()
    }

    pub fn is_empty(&self) -> bool {
        self.time_s.is_empty()
    }

    /// Distance of grid point `i`, metres.
    pub fn dist_at(&self, i: usize) -> f64 {
        self.start_dist + i as f64 * self.step_m
    }

    pub fn end_dist(&self) -> f64 {
        if self.is_empty() {
            self.start_dist
        } else {
            self.dist_at(self.len() - 1)
        }
    }

    /// Grid index of `dist`, if the series covers it (the nearest grid point).
    pub fn index_of(&self, dist: f64) -> Option<usize> {
        if self.is_empty() || !dist.is_finite() {
            return None;
        }
        let i = ((dist - self.start_dist) / self.step_m).round();
        (i >= 0.0 && (i as usize) < self.len()).then_some(i as usize)
    }

    /// The same ride with power, cadence, heart rate and speed averaged over `window_m` metres
    /// (centred, missing values skipped). Time and elevation are untouched.
    pub fn smoothed(&self, window_m: f64) -> Self {
        let half = if window_m.is_finite() && window_m > 0.0 {
            (window_m / self.step_m / 2.0).round() as usize
        } else {
            0
        };
        if half == 0 {
            return self.clone();
        }
        let n = self.len();
        let average = |values: &dyn Fn(usize) -> Option<f64>| -> Vec<Option<f64>> {
            (0..n)
                .map(|i| {
                    let (lo, hi) = (i.saturating_sub(half), (i + half).min(n - 1));
                    let (sum, count) = (lo..=hi)
                        .filter_map(values)
                        .fold((0.0, 0usize), |(s, c), v| (s + v, c + 1));
                    (count > 0).then(|| sum / count as f64)
                })
                .collect()
        };
        let mut out = self.clone();
        out.power_w = average(&|i| self.power_w[i]);
        out.cadence_rpm = average(&|i| self.cadence_rpm[i]);
        out.hr_bpm = average(&|i| self.hr_bpm[i]);
        out.speed_ms = average(&|i| Some(self.speed_ms[i]))
            .into_iter()
            .zip(&self.speed_ms)
            .map(|(smooth, raw)| smooth.unwrap_or(*raw))
            .collect();
        out
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let (a, b) = (
        if a.is_finite() { a } else { 0.0 },
        if b.is_finite() { b } else { 0.0 },
    );
    a + (b - a) * t
}

/// Both ends present: interpolate. One end missing: the nearer sample decides.
fn lerp_opt(a: Option<f64>, b: Option<f64>, t: f64) -> Option<f64> {
    let a = a.filter(|v| v.is_finite());
    let b = b.filter(|v| v.is_finite());
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (Some(a), None) if t < 0.5 => Some(a),
        (None, Some(b)) if t >= 0.5 => Some(b),
        _ => None,
    }
}

/// Time difference of `ride` against `reference` along the distance, in seconds, on `ride`'s
/// grid: **positive = `ride` is behind**, like every delta in the app. Both rides are zeroed at
/// the first distance they share, so a ride that started further along compares fairly.
/// `None` where the reference does not reach, and everywhere when the grids do not line up.
pub fn delta_time(ride: &DistSeries, reference: &DistSeries) -> Vec<Option<f64>> {
    let none = || vec![None; ride.len()];
    if (ride.step_m - reference.step_m).abs() > 1e-9 || ride.is_empty() || reference.is_empty() {
        return none();
    }
    let common = ride.start_dist.max(reference.start_dist);
    let (Some(i0), Some(j0)) = (ride.index_of(common), reference.index_of(common)) else {
        return none();
    };
    (0..ride.len())
        .map(|i| {
            if i < i0 {
                return None;
            }
            let j = reference.index_of(ride.dist_at(i))?;
            if reference.end_dist() + COVER_EPS_M < ride.dist_at(i) {
                return None;
            }
            Some((ride.time_s[i] - ride.time_s[i0]) - (reference.time_s[j] - reference.time_s[j0]))
        })
        .collect()
}

/// What one ride did on a stretch of the route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionStats {
    pub start_dist: f64,
    pub end_dist: f64,
    pub time_s: f64,
    pub avg_speed_ms: f64,
    pub avg_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub ascent_m: f64,
    /// Climbing speed in metres per hour, for a stretch with at least 10 m of ascent.
    pub vam: Option<f64>,
}

/// Statistics of the recording between `d0` and `d1` (metres along the route).
///
/// `None` when the ride did not cover the whole stretch, or when it has fewer than two samples
/// in it. The time is interpolated at the two ends; averages are over the 1 Hz samples inside.
pub fn section_stats(points: &[RecordPoint], d0: f64, d1: f64) -> Option<SectionStats> {
    if !(d0.is_finite() && d1.is_finite()) || d1 <= d0 {
        return None;
    }
    let track = Track::from_points(points)?;
    if !(track.covers(d0) && track.covers(d1)) {
        return None;
    }
    let time_s = track.time_at(d1) - track.time_at(d0);
    if !(time_s.is_finite() && time_s > 0.0) {
        return None;
    }
    let inside: Vec<&RecordPoint> = points
        .iter()
        .filter(|p| p.dist.is_finite() && p.dist >= d0 && p.dist <= d1)
        .collect();
    if inside.len() < 2 {
        return None;
    }
    let mean = |values: Vec<f64>| -> Option<f64> {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let power: Vec<f64> = inside
        .iter()
        .filter_map(|p| p.power_w)
        .filter(|v| v.is_finite())
        .map(|v| v.max(0.0))
        .collect();
    let hr: Vec<f64> = inside
        .iter()
        .filter_map(|p| p.hr_bpm)
        .filter(|v| v.is_finite())
        .collect();
    let cadence: Vec<f64> = inside
        .iter()
        .filter_map(|p| p.cadence_rpm)
        .filter(|v| v.is_finite())
        .collect();
    let mut ascent_m = 0.0;
    for pair in inside.windows(2) {
        let up = pair[1].ele - pair[0].ele;
        if up.is_finite() && up > 0.0 {
            ascent_m += up;
        }
    }
    let normalized_power_w = if power.len() >= MIN_NP_SAMPLES {
        normalized_power(power.iter().copied())
    } else {
        None
    };
    Some(SectionStats {
        start_dist: d0,
        end_dist: d1,
        time_s,
        avg_speed_ms: (d1 - d0) / time_s,
        avg_power_w: mean(power),
        normalized_power_w,
        max_hr: hr.iter().copied().reduce(f64::max),
        avg_hr: mean(hr),
        avg_cadence: mean(cadence),
        ascent_m,
        vam: (ascent_m >= MIN_VAM_ASCENT_M).then(|| ascent_m / time_s * 3600.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ride(speed: f64, len_m: f64, stop_at_m: Option<(f64, f64)>) -> Vec<RecordPoint> {
        let mut out = Vec::new();
        let (mut t, mut dist) = (0.0, 0.0);
        let mut stop_left = stop_at_m.map_or(0.0, |(_, s)| s);
        while dist < len_m {
            out.push(RecordPoint {
                t,
                unix_s: 1_700_000_000.0 + t,
                dist,
                lat: 47.5,
                lon: 19.0,
                ele: 100.0 + dist * 0.05,
                speed_ms: speed,
                power_w: Some(200.0),
                cadence_rpm: Some(85.0),
                hr_bpm: Some(140.0 + dist / 100.0),
                grade: 5.0,
            });
            t += 1.0;
            match stop_at_m {
                Some((at, _)) if dist >= at && stop_left > 0.0 => stop_left -= 1.0,
                _ => dist += speed,
            }
        }
        out
    }

    #[test]
    fn a_ride_lands_on_the_grid_with_its_values() {
        let s = DistSeries::from_points(&ride(10.0, 3_000.0, None), GRID_M).unwrap();
        assert_eq!(s.start_dist, 0.0);
        assert_eq!(s.len(), 300); // the last sample is at 2 990 m
        assert!((s.time_s[100] - 100.0).abs() < 1e-6, "1 000 m at 100 s");
        assert!((s.speed_ms[50] - 10.0).abs() < 1e-9);
        assert_eq!(s.power_w[10], Some(200.0));
        assert!((s.ele[20] - (100.0 + 200.0 * 0.05)).abs() < 1e-6);
        assert_eq!(s.index_of(500.0), Some(50));
        assert_eq!(s.index_of(99_999.0), None);
        assert!(DistSeries::from_points(&[], GRID_M).is_none());
        assert!(DistSeries::from_points(&ride(10.0, 3_000.0, None), 0.0).is_none());
    }

    #[test]
    fn a_missing_sensor_stays_missing() {
        let mut points = ride(10.0, 1_000.0, None);
        for p in &mut points {
            p.power_w = None;
        }
        let s = DistSeries::from_points(&points, GRID_M).unwrap();
        assert!(s.power_w.iter().all(Option::is_none));
        assert!(s.hr_bpm.iter().all(Option::is_some));
        assert!(s.smoothed(100.0).power_w.iter().all(Option::is_none));
    }

    #[test]
    fn the_time_difference_is_positive_for_the_slower_ride() {
        let fast = DistSeries::from_points(&ride(10.0, 3_000.0, None), GRID_M).unwrap();
        let slow = DistSeries::from_points(&ride(8.0, 3_000.0, None), GRID_M).unwrap();
        let behind = delta_time(&slow, &fast);
        let ahead = delta_time(&fast, &slow);
        assert_eq!(behind[0], Some(0.0));
        // at 2 000 m: 250 s against 200 s
        let i = slow.index_of(2_000.0).unwrap();
        assert!((behind[i].unwrap() - 50.0).abs() < 0.5, "{:?}", behind[i]);
        assert!((ahead[i].unwrap() + 50.0).abs() < 0.5, "{:?}", ahead[i]);
    }

    #[test]
    fn a_stop_shows_up_as_lost_seconds_and_a_short_reference_leaves_a_gap() {
        let plain = DistSeries::from_points(&ride(10.0, 2_000.0, None), GRID_M).unwrap();
        let stopped =
            DistSeries::from_points(&ride(10.0, 2_000.0, Some((1_000.0, 30.0))), GRID_M).unwrap();
        let delta = delta_time(&stopped, &plain);
        let before = delta[stopped.index_of(900.0).unwrap()].unwrap();
        let after = delta[stopped.index_of(1_500.0).unwrap()].unwrap();
        assert!(before.abs() < 0.5, "{before}");
        assert!((after - 30.0).abs() < 1.5, "{after}");

        let short = DistSeries::from_points(&ride(10.0, 1_000.0, None), GRID_M).unwrap();
        let delta = delta_time(&plain, &short);
        assert!(delta[plain.index_of(500.0).unwrap()].is_some());
        assert_eq!(delta[plain.index_of(1_500.0).unwrap()], None);
    }

    #[test]
    fn a_ride_that_started_further_along_is_zeroed_where_both_exist() {
        let full = DistSeries::from_points(&ride(10.0, 3_000.0, None), GRID_M).unwrap();
        let later: Vec<RecordPoint> = ride(10.0, 3_000.0, None)
            .into_iter()
            .filter(|p| p.dist >= 1_000.0)
            .map(|p| RecordPoint {
                t: p.t - 100.0,
                ..p
            })
            .collect();
        let later = DistSeries::from_points(&later, GRID_M).unwrap();
        let delta = delta_time(&later, &full);
        assert_eq!(delta[0], Some(0.0));
        assert!(delta[later.len() - 1].unwrap().abs() < 0.5);
        // the other way round the reference starts later: nothing to compare before that
        let reverse = delta_time(&full, &later);
        assert_eq!(reverse[0], None);
        assert_eq!(reverse[full.index_of(1_000.0).unwrap()], Some(0.0));
    }

    #[test]
    fn grids_with_different_steps_do_not_compare() {
        let a = DistSeries::from_points(&ride(10.0, 1_000.0, None), 10.0).unwrap();
        let b = DistSeries::from_points(&ride(10.0, 1_000.0, None), 20.0).unwrap();
        assert!(delta_time(&a, &b).iter().all(Option::is_none));
    }

    #[test]
    fn smoothing_averages_but_keeps_the_length_and_the_time() {
        let mut points = ride(10.0, 1_000.0, None);
        for (i, p) in points.iter_mut().enumerate() {
            p.power_w = Some(if i % 2 == 0 { 100.0 } else { 300.0 });
        }
        let raw = DistSeries::from_points(&points, GRID_M).unwrap();
        let smooth = raw.smoothed(100.0);
        assert_eq!(smooth.len(), raw.len());
        assert_eq!(smooth.time_s, raw.time_s);
        let w = smooth.power_w[50].unwrap();
        assert!((w - 200.0).abs() < 25.0, "{w}");
        assert_eq!(raw.smoothed(0.0), raw);
    }

    #[test]
    fn section_stats_sum_up_a_stretch() {
        let points = ride(5.0, 3_000.0, None);
        let s = section_stats(&points, 500.0, 1_500.0).unwrap();
        assert!((s.time_s - 200.0).abs() < 1e-6);
        assert!((s.avg_speed_ms - 5.0).abs() < 1e-6);
        assert_eq!(s.avg_power_w, Some(200.0));
        assert!((s.normalized_power_w.unwrap() - 200.0).abs() < 1e-6);
        assert_eq!(s.avg_cadence, Some(85.0));
        assert!((s.ascent_m - 50.0).abs() < 1.0, "{}", s.ascent_m);
        // 50 m in 200 s = 900 m/h
        assert!((s.vam.unwrap() - 900.0).abs() < 25.0, "{:?}", s.vam);
        assert!(s.max_hr.unwrap() > s.avg_hr.unwrap());
    }

    #[test]
    fn a_stretch_the_ride_did_not_cover_has_no_stats() {
        let points = ride(5.0, 1_000.0, None);
        assert!(section_stats(&points, 500.0, 2_000.0).is_none());
        assert!(section_stats(&points, 600.0, 500.0).is_none());
        assert!(section_stats(&points, f64::NAN, 500.0).is_none());
        assert!(section_stats(&[], 0.0, 100.0).is_none());
        // short stretch: no NP, no VAM
        let s = section_stats(&points, 100.0, 200.0).unwrap();
        assert_eq!(s.normalized_power_w, None);
        assert_eq!(s.vam, None);
    }
}
