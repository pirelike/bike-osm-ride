//! The built-in demo route: ≈ 24 km north-west of Budapest (Hűvösvölgy towards the
//! Pilis), rolling hills and one 4 km climb at ≈ 7 %, followed by a long descent.
//!
//! Fully deterministic (closed-form functions, no random numbers) and sent through the
//! same pipeline as every imported file, so it exercises the real code path. The
//! geometry is synthetic: it is *near* real roads, not on them.

use std::f64::consts::PI;
use std::sync::OnceLock;

use ride_model::{Route, RouteSource};

use crate::build;
use crate::geo::EARTH_RADIUS_M;
use crate::gpx::{GpxData, RawPoint};

const START_LAT: f64 = 47.5390;
const START_LON: f64 = 18.9805;
const START_ELE: f64 = 215.0;
const LENGTH_M: f64 = 24_000.0;
const STEP_M: f64 = 20.0;
const CLIMB_START_M: f64 = 10_500.0;
const CLIMB_END_M: f64 = 14_500.0;

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Heading in degrees at distance `d`: roughly north-north-west with gentle bends.
fn bearing_deg(d: f64) -> f64 {
    340.0 + 35.0 * (d / 1900.0).sin() + 12.0 * (d / 610.0 + 1.3).sin()
}

/// Road grade in percent at distance `d`.
fn grade_pct(d: f64) -> f64 {
    let rolling = 2.0 * (2.0 * PI * d / 2800.0).sin() + 1.2 * (2.0 * PI * d / 1150.0 + 0.8).sin();
    let climb = 7.0 + 1.0 * (2.0 * PI * (d - CLIMB_START_M) / 900.0).sin();
    let w_climb = smoothstep(CLIMB_START_M - 100.0, CLIMB_START_M + 100.0, d)
        * (1.0 - smoothstep(CLIMB_END_M - 100.0, CLIMB_END_M + 100.0, d));
    let w_descent = smoothstep(14_800.0, 15_400.0, d) * (1.0 - smoothstep(18_600.0, 19_200.0, d));
    (1.0 - w_climb) * rolling + w_climb * climb - 4.0 * w_descent
}

fn raw_points() -> Vec<RawPoint> {
    let n = (LENGTH_M / STEP_M) as usize;
    let mut pts = Vec::with_capacity(n + 1);
    let (mut lat, mut lon, mut ele) = (START_LAT, START_LON, START_ELE);
    pts.push(RawPoint {
        lat,
        lon,
        ele: Some(ele),
        time: None,
    });
    for i in 0..n {
        // Midpoint rule: evaluate heading and grade in the middle of the step.
        let mid = (i as f64 + 0.5) * STEP_M;
        let b = bearing_deg(mid).to_radians();
        let dlat = STEP_M * b.cos() / EARTH_RADIUS_M;
        let dlon = STEP_M * b.sin() / (EARTH_RADIUS_M * lat.to_radians().cos());
        lat += dlat.to_degrees();
        lon += dlon.to_degrees();
        ele += STEP_M * grade_pct(mid) / 100.0;
        pts.push(RawPoint {
            lat,
            lon,
            ele: Some((ele * 4.0).round() / 4.0),
            time: None,
        });
    }
    pts
}

fn build_demo() -> Route {
    let data = GpxData {
        name: Some("Demó útvonal (Budai-hegység)".to_string()),
        points: raw_points(),
        ..GpxData::default()
    };
    match build::build_route(&data, "Demó útvonal", RouteSource::Demo) {
        Ok(r) => r,
        // Cannot happen (fixed, valid input); keep the API infallible without panicking.
        Err(_) => Route {
            id: "demo".into(),
            name: "Demó útvonal".into(),
            source: RouteSource::Demo,
            points: Vec::new(),
            total_dist: 0.0,
            ascent: 0.0,
            descent: 0.0,
            min_ele: 0.0,
            max_ele: 0.0,
            bounds: ride_model::Bounds {
                south: START_LAT,
                west: START_LON,
                north: START_LAT,
                east: START_LON,
            },
            climbs: Vec::new(),
            waypoints: Vec::new(),
            has_elevation: false,
            segments: Vec::new(),
            timeline: Vec::new(),
        },
    }
}

/// Built-in synthetic route (rolling hills + a proper climb). Deterministic.
pub fn demo_route() -> Route {
    static DEMO: OnceLock<Route> = OnceLock::new();
    DEMO.get_or_init(build_demo).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_deterministic_and_well_formed() {
        let a = demo_route();
        let b = build_demo();
        assert_eq!(a, b);
        assert_eq!(a.source, RouteSource::Demo);
        assert!(a.has_elevation);
        assert!(
            (a.total_dist - LENGTH_M).abs() < 100.0,
            "length {}",
            a.total_dist
        );
        assert!(
            a.points.len() > 2000 && a.points.len() < 2600,
            "{}",
            a.points.len()
        );
        assert!(a
            .points
            .windows(2)
            .all(|w| w[1].dist > w[0].dist && w[1].dist - w[0].dist <= 10.0 + 1e-9));
        assert!(a.points.iter().all(|p| p.grade.abs() <= 25.0));
    }

    #[test]
    fn demo_lies_in_the_budapest_area() {
        let r = demo_route();
        assert!(
            r.bounds.south > 47.4 && r.bounds.north < 47.9,
            "{:?}",
            r.bounds
        );
        assert!(
            r.bounds.west > 18.6 && r.bounds.east < 19.2,
            "{:?}",
            r.bounds
        );
        let p0 = r.points[0];
        assert!((p0.lat - START_LAT).abs() < 1e-6 && (p0.lon - START_LON).abs() < 1e-6);
        assert!(
            r.min_ele > 150.0 && r.max_ele < 700.0,
            "{} {}",
            r.min_ele,
            r.max_ele
        );
    }

    #[test]
    fn demo_has_exactly_one_four_km_seven_percent_climb() {
        let r = demo_route();
        assert_eq!(r.climbs.len(), 1, "{:?}", r.climbs);
        let c = &r.climbs[0];
        assert!((c.length - 4000.0).abs() < 350.0, "length {}", c.length);
        assert!((c.avg_grade - 7.0).abs() < 0.8, "avg {}", c.avg_grade);
        assert!(
            c.start_dist > 9_500.0 && c.start_dist < 11_000.0,
            "start {}",
            c.start_dist
        );
        assert!(c.max_grade > c.avg_grade && c.max_grade < 12.0);
        assert!(c.category.is_some());
        // Rolling hills are real but not climbs: some ascent outside the climb.
        assert!(
            r.ascent > c.gain + 40.0,
            "ascent {} climb {}",
            r.ascent,
            c.gain
        );
        assert!(r.descent > 100.0);
    }
}
