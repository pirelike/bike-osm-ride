//! Replay of an earlier ride on the same route: the "ghost".
//!
//! A finished ride is a 1 Hz list of `(t, dist)` samples ([`RecordPoint`]): `t` is the timer
//! time (pauses excluded) and `dist` the distance along the route. [`Track`] turns that into a
//! pair of monotone lookups — time at a distance and distance at a time — by binary search and
//! linear interpolation, so a ghost costs nothing per frame and needs no per-second replay.
//!
//! [`Ghost::align`] shifts the ghost's clock to the rider's start: the difference is zero at the
//! start line and grows by real seconds thereafter. A ghost whose ride does not cover the start
//! distance (it began further along the route) cannot be aligned and stays inactive.
//!
//! Everything is finite-safe: NaN samples are dropped, a track with less than two usable
//! samples is rejected, and every lookup is clamped to the ends instead of panicking.

use ride_model::{RecordPoint, Route};

/// Distances this close to the ends of a track still count as covered (metres). A ride ends a
/// few centimetres short of the route end because of the integration, not because of the rider.
pub const COVER_EPS_M: f64 = 1.0;

/// The `(time, distance)` trace of a recorded ride.
///
/// Both axes are non-decreasing: a sample that goes back in time is dropped, a sample that goes
/// back in distance (a rider rolling backwards, GPS noise) is flattened to the previous
/// distance. That keeps both binary searches well defined.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    t: Vec<f64>,
    d: Vec<f64>,
}

impl Track {
    /// Builds the trace of a recording; `None` when it has fewer than two samples or covers no
    /// distance at all.
    pub fn from_points(points: &[RecordPoint]) -> Option<Self> {
        Self::from_samples(points.iter().map(|point| (point.t, point.dist)))
    }

    /// Builds a trace from parallel `(time, distance)` samples: a recording, or the route's own
    /// timeline.
    pub fn from_samples(samples: impl IntoIterator<Item = (f64, f64)>) -> Option<Self> {
        let mut t: Vec<f64> = Vec::new();
        let mut d: Vec<f64> = Vec::new();
        for (time, dist) in samples {
            if !(time.is_finite() && dist.is_finite()) {
                continue;
            }
            let time = time.max(0.0);
            let dist = dist.max(0.0).max(d.last().copied().unwrap_or(0.0));
            if t.last().is_some_and(|last| time < *last) {
                continue;
            }
            t.push(time);
            d.push(dist);
        }
        let span = d.last()? - d.first()?;
        if t.len() < 2 || !(span.is_finite() && span >= 1e-6) {
            return None;
        }
        Some(Self { t, d })
    }

    /// The route's own recorded time: the `<time>` stamps of the GPX it was loaded from, one
    /// elapsed second per [`RoutePoint`]. `None` when the route has no such timeline (a BRouter
    /// route, or a file without times), so the caller can fall back to the ride log.
    pub fn from_route(route: &Route) -> Option<Self> {
        if route.timeline.len() != route.points.len() || route.timeline.len() < 2 {
            return None;
        }
        Self::from_samples(
            route
                .points
                .iter()
                .zip(&route.timeline)
                .map(|(point, &time)| (time, point.dist)),
        )
    }

    /// Number of samples kept.
    pub fn len(&self) -> usize {
        self.t.len()
    }

    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }

    pub fn start_dist(&self) -> f64 {
        self.d[0]
    }

    pub fn end_dist(&self) -> f64 {
        self.d[self.d.len() - 1]
    }

    pub fn start_time(&self) -> f64 {
        self.t[0]
    }

    pub fn end_time(&self) -> f64 {
        self.t[self.t.len() - 1]
    }

    /// Whether the ride reached `dist` (with [`COVER_EPS_M`] of tolerance at the end).
    pub fn covers(&self, dist: f64) -> bool {
        dist.is_finite()
            && dist >= self.start_dist() - COVER_EPS_M
            && dist <= self.end_dist() + COVER_EPS_M
    }

    /// Timer time at `dist`, interpolated between samples and clamped to the ends.
    pub fn time_at(&self, dist: f64) -> f64 {
        let last = self.t.len() - 1;
        let d = if dist.is_finite() { dist } else { self.d[0] };
        if d <= self.d[0] {
            return self.t[0];
        }
        if d >= self.d[last] {
            return self.t[last];
        }
        // first sample at or past `d`; duplicates resolve to the first arrival at that distance
        let i = self.d.partition_point(|x| *x < d);
        let (a, b) = (i - 1, i);
        let span = self.d[b] - self.d[a];
        if span <= 1e-9 {
            return self.t[b];
        }
        let frac = ((d - self.d[a]) / span).clamp(0.0, 1.0);
        self.t[a] + (self.t[b] - self.t[a]) * frac
    }

    /// Distance at timer time `t`, interpolated between samples and clamped to the ends.
    pub fn dist_at(&self, t: f64) -> f64 {
        let last = self.d.len() - 1;
        let time = if t.is_finite() { t } else { self.t[0] };
        if time <= self.t[0] {
            return self.d[0];
        }
        if time >= self.t[last] {
            return self.d[last];
        }
        // first sample past `time`
        let i = self.t.partition_point(|x| *x <= time);
        let (a, b) = (i - 1, i);
        let span = self.t[b] - self.t[a];
        if span <= 1e-9 {
            return self.d[b];
        }
        let frac = ((time - self.t[a]) / span).clamp(0.0, 1.0);
        self.d[a] + (self.d[b] - self.d[a]) * frac
    }
}

/// A track aligned to the rider's start: its clock is shifted by the time it took to reach the
/// start distance.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    track: Track,
    offset_t: f64,
}

impl Ghost {
    /// Aligns `track` so both rides are at zero difference on `start_dist`.
    /// `None` when the recorded ride does not cover that distance.
    pub fn align(track: Track, start_dist: f64) -> Option<Self> {
        if !track.covers(start_dist) {
            return None;
        }
        Some(Self {
            offset_t: track.time_at(start_dist),
            track,
        })
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    /// Where the ghost is after `elapsed_s` of the rider's timer time (it stops at its end).
    pub fn dist_at(&self, elapsed_s: f64) -> f64 {
        self.track.dist_at(elapsed_s + self.offset_t)
    }

    /// Time difference at the rider's distance: positive = the rider is behind the ghost.
    /// `None` while the ghost's ride does not reach that distance.
    pub fn delta_at(&self, elapsed_s: f64, dist: f64) -> Option<f64> {
        if !self.track.covers(dist) {
            return None;
        }
        let delta = elapsed_s - (self.track.time_at(dist) - self.offset_t);
        delta.is_finite().then_some(delta)
    }
}

/// What the engine replays: the ghost ride plus, for every climb of the route, the PR ride.
///
/// `segments` is indexed like `Route::climbs` and holds `(PR time for the whole climb, the PR
/// ride's trace)`. It is independent of the ghost mode: the live segment comparison keeps working
/// with the ghost turned off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GhostSet {
    /// The ghost ride's trace (`None` = no ghost, or none selected).
    pub main: Option<Track>,
    pub segments: Vec<Option<(f64, Track)>>,
}

impl GhostSet {
    /// `true` when there is neither a ghost nor a segment PR: nothing to replay.
    pub fn is_empty(&self) -> bool {
        self.main.is_none() && self.segments.iter().all(Option::is_none)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(t: f64, dist: f64) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0,
            speed_ms: 5.0,
            power_w: Some(200.0),
            cadence_rpm: Some(85.0),
            hr_bpm: Some(140.0),
            grade: 1.0,
        }
    }

    /// A 1 Hz ride covering `len_m` metres at a constant `speed` m/s.
    fn track(len_m: f64, speed: f64) -> Track {
        let n = (len_m / speed).ceil() as usize;
        let points: Vec<RecordPoint> = (0..=n)
            .map(|i| point(i as f64, (i as f64 * speed).min(len_m)))
            .collect();
        Track::from_points(&points).expect("a usable track")
    }

    #[test]
    fn a_track_interpolates_both_ways() {
        let track = track(1_000.0, 10.0);
        assert_eq!(track.start_time(), 0.0);
        assert!((track.end_dist() - 1_000.0).abs() < 1e-9);
        // 10 m/s: 500 m at 50 s
        assert!((track.time_at(500.0) - 50.0).abs() < 1e-6);
        assert!((track.dist_at(50.0) - 500.0).abs() < 1e-6);
        assert!((track.time_at(505.0) - 50.5).abs() < 1e-6);
        // clamped ends, never NaN
        assert_eq!(track.time_at(-100.0), 0.0);
        assert_eq!(track.time_at(1e9), track.end_time());
        assert_eq!(track.dist_at(-5.0), 0.0);
        assert_eq!(track.dist_at(1e9), track.end_dist());
        assert_eq!(track.time_at(f64::NAN), 0.0);
        assert_eq!(track.dist_at(f64::NAN), 0.0);
    }

    #[test]
    fn a_dist_that_goes_backwards_is_flattened_and_nan_samples_are_dropped() {
        let points = vec![
            point(0.0, 0.0),
            point(1.0, 10.0),
            point(2.0, 8.0),      // rolled back
            point(3.0, f64::NAN), // dropped
            point(4.0, 30.0),
            point(f64::NAN, 40.0), // dropped
            point(5.0, 40.0),
        ];
        let track = Track::from_points(&points).expect("still usable");
        assert_eq!(track.len(), 5);
        assert!((track.end_dist() - 40.0).abs() < 1e-9);
        // 8 m is reached at t = 0.8 s on the way up; the flattened sample keeps that answer
        assert!((track.time_at(8.0) - 0.8).abs() < 1e-6);
        // the plateau at 10 m resolves to the first arrival, not to the second
        assert!((track.time_at(10.0) - 1.0).abs() < 1e-6);
        // and the distance at 2 s is still 10 m (never the rolled-back 8 m)
        assert!((track.dist_at(2.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_recordings_give_no_track() {
        assert_eq!(Track::from_points(&[]), None);
        assert_eq!(Track::from_points(&[point(0.0, 0.0)]), None);
        // standing still the whole time
        let still: Vec<RecordPoint> = (0..10).map(|i| point(i as f64, 0.0)).collect();
        assert_eq!(Track::from_points(&still), None);
    }

    #[test]
    fn alignment_at_the_start_line_makes_the_difference_zero() {
        let ghost = Ghost::align(track(1_000.0, 10.0), 0.0).expect("aligned at 0");
        assert_eq!(ghost.delta_at(0.0, 0.0), Some(0.0));
        assert!((ghost.dist_at(0.0)).abs() < 1e-9);
        // after 50 s the rider is at 400 m, the ghost at 500 m: 10 s behind
        assert!((ghost.delta_at(50.0, 400.0).unwrap() - 10.0).abs() < 1e-6);
        assert!((ghost.dist_at(50.0) - 500.0).abs() < 1e-6);
        // ahead of the ghost
        assert!(ghost.delta_at(60.0, 700.0).unwrap() < 0.0);
    }

    #[test]
    fn alignment_from_the_middle_of_the_route_shifts_the_clock() {
        let ghost = Ghost::align(track(1_000.0, 10.0), 500.0).expect("aligned at 500");
        // the ghost needs 50 s to get to 500 m; at the rider's t = 0 the ghost is there
        assert!((ghost.dist_at(0.0) - 500.0).abs() < 1e-9);
        assert_eq!(ghost.delta_at(0.0, 500.0), Some(0.0));
        // the rider needs 100 s for the second half, the ghost 50 s: 50 s behind at the end
        assert!((ghost.delta_at(100.0, 1_000.0).unwrap() - 50.0).abs() < 1e-6);
        // faster than the ghost: negative
        assert!((ghost.delta_at(25.0, 750.0).unwrap() + 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_ride_that_starts_after_the_ghost_cannot_be_aligned() {
        // the ghost ride began at 200 m (a seek), so it does not cover a start at 0
        let points: Vec<RecordPoint> = (0..=10)
            .map(|i| point(i as f64, 200.0 + i as f64 * 10.0))
            .collect();
        let track = Track::from_points(&points).unwrap();
        assert!(Ghost::align(track.clone(), 0.0).is_none());
        assert!(Ghost::align(track, 200.0).is_some());
    }

    #[test]
    fn past_the_end_of_the_ghost_ride_there_is_no_difference() {
        let ghost = Ghost::align(track(500.0, 10.0), 0.0).unwrap();
        // the ghost ride ended at 500 m
        assert!(ghost.delta_at(80.0, 700.0).is_none());
        // the last centimetres are still covered (integration ends a hair short)
        assert!(ghost.delta_at(50.0, 500.0).is_some());
        assert!(ghost.delta_at(50.0, 500.9).is_some(), "epsilon at the end");
        // and the ghost stops at its own end
        assert!((ghost.dist_at(1_000.0) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn an_empty_ghost_set_is_recognised() {
        assert!(GhostSet::default().is_empty());
        let one = GhostSet {
            main: Some(track(100.0, 5.0)),
            segments: vec![None],
        };
        assert!(!one.is_empty());
        let segment_only = GhostSet {
            main: None,
            segments: vec![None, Some((120.0, track(100.0, 5.0)))],
        };
        assert!(!segment_only.is_empty());
    }
}
