//! Small helpers that belong to the model: recent-route entries, the empty
//! snapshot, pure route geometry lookups and a test fixture builder.

use crate::*;
use serde::{Deserialize, Serialize};

/// Entry of the "recent routes" list on the welcome screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentRoute {
    pub id: String,
    pub name: String,
    pub total_dist: f64,
    pub ascent: f64,
    pub source: RouteSource,
    pub last_opened_unix: f64,
}

impl RideSnapshot {
    /// Snapshot used before any route is loaded (state `Empty`).
    pub fn empty() -> Self {
        Self {
            state: RideState::Empty,
            pos: RoutePosition::default(),
            speed_ms: 0.0,
            power_w: None,
            power_3s: None,
            cadence_rpm: None,
            hr_bpm: None,
            resistance: None,
            elapsed_s: 0.0,
            dist: 0.0,
            remaining_dist: 0.0,
            eta_s: None,
            ascent_done: 0.0,
            descent_done: 0.0,
            ascent_left: 0.0,
            avg_speed_ms: 0.0,
            max_speed_ms: 0.0,
            avg_power_w: None,
            max_power_w: None,
            normalized_power_w: None,
            work_kj: 0.0,
            kcal: 0.0,
            avg_hr: None,
            max_hr: None,
            power_zone: None,
            hr_zone: None,
            next_climb: None,
            next_climb_in: None,
            trainer_grade: 0.0,
            workout: None,
            ghost: None,
            segment: None,
        }
    }
}

// ───────────────────── Route geometry lookups (pure) ─────────────────────

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Initial great-circle bearing from point 1 to point 2, degrees clockwise from north.
pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

impl Route {
    /// Index of the last point whose `dist` is `<= d` (d is clamped to the route).
    pub fn index_at(&self, d: f64) -> usize {
        if self.points.is_empty() {
            return 0;
        }
        let d = d.clamp(0.0, self.total_dist);
        match self
            .points
            .binary_search_by(|p| p.dist.partial_cmp(&d).unwrap_or(std::cmp::Ordering::Less))
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    /// Interpolated position / grade / heading at distance `d` metres (clamped).
    pub fn locate(&self, d: f64) -> RoutePosition {
        let n = self.points.len();
        if n == 0 {
            return RoutePosition::default();
        }
        if n == 1 {
            let p = self.points[0];
            return RoutePosition {
                dist: 0.0,
                lat: p.lat,
                lon: p.lon,
                ele: p.ele,
                grade: p.grade,
                heading_deg: 0.0,
            };
        }
        let d = d.clamp(0.0, self.total_dist);
        let i = self.index_at(d).min(n - 2);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let span = b.dist - a.dist;
        let t = if span > 1e-9 {
            ((d - a.dist) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        RoutePosition {
            dist: d,
            lat: lerp(a.lat, b.lat, t),
            lon: lerp(a.lon, b.lon, t),
            ele: lerp(a.ele, b.ele, t),
            grade: lerp(a.grade, b.grade, t),
            heading_deg: bearing_deg(a.lat, a.lon, b.lat, b.lon),
        }
    }

    /// Points with `from <= dist <= to` (plain slice, no interpolation at the ends).
    pub fn slice(&self, from: f64, to: f64) -> &[RoutePoint] {
        if self.points.is_empty() || to < from {
            return &[];
        }
        let start = self.points.partition_point(|p| p.dist < from);
        let end = self.points.partition_point(|p| p.dist <= to);
        &self.points[start..end.max(start)]
    }

    /// Index of the climb the rider is inside at `d`, or else the next one ahead.
    pub fn climb_ahead(&self, d: f64) -> Option<usize> {
        self.climbs.iter().position(|c| c.end_dist > d)
    }
}

/// Fixtures for tests in other crates (not gated by `cfg(test)` on purpose).
pub mod testing {
    use super::*;

    /// A straight, eastbound route made of `(length_m, grade_percent)` segments,
    /// sampled every 10 m, starting at 47.5 N / 19.0 E and `start_ele` metres.
    pub fn straight_route(segments: &[(f64, f64)], start_ele: f64) -> Route {
        let step = 10.0;
        let m_per_deg_lon = 111_320.0 * 47.5f64.to_radians().cos();
        let mut points = vec![RoutePoint {
            lat: 47.5,
            lon: 19.0,
            ele: start_ele,
            dist: 0.0,
            grade: segments.first().map_or(0.0, |s| s.1),
        }];
        let (mut dist, mut ele) = (0.0, start_ele);
        let (mut ascent, mut descent) = (0.0, 0.0);
        for &(len, grade) in segments {
            let n = (len / step).round().max(1.0) as usize;
            for _ in 0..n {
                dist += step;
                let de = step * grade / 100.0;
                ele += de;
                if de > 0.0 {
                    ascent += de
                } else {
                    descent -= de
                }
                points.push(RoutePoint {
                    lat: 47.5,
                    lon: 19.0 + dist / m_per_deg_lon,
                    ele,
                    dist,
                    grade,
                });
            }
        }
        let min_ele = points.iter().map(|p| p.ele).fold(f64::INFINITY, f64::min);
        let max_ele = points
            .iter()
            .map(|p| p.ele)
            .fold(f64::NEG_INFINITY, f64::max);
        let last = *points.last().unwrap();
        Route {
            id: "test".into(),
            name: "Tesztútvonal".into(),
            source: RouteSource::Demo,
            total_dist: last.dist,
            ascent,
            descent,
            min_ele,
            max_ele,
            bounds: Bounds {
                south: 47.5,
                west: 19.0,
                north: 47.5,
                east: last.lon,
            },
            climbs: vec![],
            waypoints: vec![],
            has_elevation: true,
            segments: Vec::new(),
            timeline: Vec::new(),
            points,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_interpolates_and_clamps() {
        let r = testing::straight_route(&[(1000.0, 5.0), (500.0, -2.0)], 100.0);
        assert!((r.total_dist - 1500.0).abs() < 1e-6);
        let p = r.locate(505.0);
        assert!((p.ele - (100.0 + 505.0 * 0.05)).abs() < 1e-6);
        assert!(
            (p.heading_deg - 90.0).abs() < 0.5,
            "eastbound, got {}",
            p.heading_deg
        );
        assert_eq!(r.locate(-10.0).dist, 0.0);
        assert!((r.locate(1e9).dist - r.total_dist).abs() < 1e-9);
    }

    #[test]
    fn slice_returns_window() {
        let r = testing::straight_route(&[(1000.0, 0.0)], 0.0);
        let s = r.slice(100.0, 200.0);
        assert_eq!(s.first().unwrap().dist, 100.0);
        assert_eq!(s.last().unwrap().dist, 200.0);
        assert!(r.slice(50.0, 10.0).is_empty());
    }
}
