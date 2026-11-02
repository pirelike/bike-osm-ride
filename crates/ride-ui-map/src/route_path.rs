//! Route geometry → one SVG path per grade colour bucket.
//!
//! The paths are built in *world pixel* coordinates at a fixed integer tile zoom, relative to
//! the route's centre so the numbers stay small and precise. The camera is applied by one
//! `<g transform>` attribute, so panning and zooming never rebuild the path data: the map
//! memoises this module per route and per integer tile zoom (see `MapView`).
//!
//! A path is built for every grade bucket of `ride_ui_kit::palette` that the route touches
//! (plus one dark casing underneath, drawn by the caller from [`RoutePaths::casing`]).

use std::fmt::Write as _;

use ride_model::Route;
use ride_ui_kit::palette::{grade_bucket, GRADE_BUCKETS};

use crate::camera::GeoPoint;
use crate::mercator::{lat_to_y, lon_to_x};

/// Two kept route points must be at least this far apart (world pixels) — everything closer
/// is invisible at the current zoom and only bloats the path data.
pub const MIN_SEGMENT_PX: f64 = 0.7;

/// Upper bound on the points written into one set of paths. Routes longer than this are
/// thinned by taking every n-th point; the shape stays recognisable and a zoom change stays
/// cheap even for a 100 km route.
pub const MAX_PATH_POINTS: usize = 12_000;

/// BigDecimal-ish formatting of one coordinate; two decimals are 1/100 of a world pixel.
fn write_coord(out: &mut String, value: f64) {
    if value.is_finite() {
        let _ = write!(out, "{value:.2}");
    } else {
        // cannot happen (points are filtered) but never writes "NaN" into SVG data
        let _ = write!(out, "0.00");
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Projected {
    x: f64,
    y: f64,
    grade: f64,
}

/// The complete route overlay for one route at one integer tile zoom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoutePaths {
    /// Integer zoom the coordinates are measured at.
    pub zoom: u8,
    /// World-pixel offset subtracted from every coordinate (the route centre at `zoom`).
    pub shift_x: f64,
    pub shift_y: f64,
    /// One continuous polyline for the casing (under all coloured segments).
    pub casing: String,
    /// `(grade bucket, path data)` for the buckets the route actually touches, ascending.
    pub buckets: Vec<(usize, String)>,
    /// Number of points that made it into the paths (after thinning, before bucketing).
    pub points: usize,
}

impl RoutePaths {
    /// True when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.casing.is_empty() && self.buckets.is_empty()
    }
}

/// Centre of the route in geographic coordinates (bounds centre when usable, else the first
/// valid point). Used as the origin of the local pixel system.
fn reference_point(route: &Route) -> GeoPoint {
    let bounds = route.bounds;
    if bounds.south.is_finite()
        && bounds.north.is_finite()
        && bounds.west.is_finite()
        && bounds.east.is_finite()
    {
        let lat = (bounds.south + bounds.north) / 2.0;
        let lon = (bounds.west + bounds.east) / 2.0;
        if lat.is_finite() && lon.is_finite() {
            return GeoPoint::new(lat, lon);
        }
    }
    for point in &route.points {
        if point.lat.is_finite() && point.lon.is_finite() {
            return GeoPoint::new(point.lat, point.lon);
        }
    }
    GeoPoint::new(0.0, 0.0)
}

/// Drops duplicate points that are closer than [`MIN_SEGMENT_PX`] and thins very long routes.
fn thin(points: &[Projected]) -> Vec<Projected> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let limit = MIN_SEGMENT_PX * MIN_SEGMENT_PX;
    let mut kept: Vec<Projected> = Vec::with_capacity(points.len().min(MAX_PATH_POINTS));
    if let Some(first) = points.first() {
        kept.push(*first);
    }
    let mut last = match points.first() {
        Some(first) => *first,
        None => return kept,
    };
    for point in points.iter().skip(1) {
        let dx = point.x - last.x;
        let dy = point.y - last.y;
        if dx * dx + dy * dy >= limit {
            kept.push(*point);
            last = *point;
        }
    }
    // the finish marker must sit exactly at the end of the drawn line
    if let Some(end) = points.last() {
        let same = kept
            .last()
            .is_some_and(|p| (p.x - end.x).abs() < 1e-9 && (p.y - end.y).abs() < 1e-9);
        if !same {
            kept.push(*end);
        }
    }
    if kept.len() > MAX_PATH_POINTS {
        let stride = kept.len().div_ceil(MAX_PATH_POINTS).max(1);
        let mut thinned: Vec<Projected> = kept.iter().step_by(stride).copied().collect();
        if let Some(end) = kept.last() {
            let same = thinned
                .last()
                .is_some_and(|p| (p.x - end.x).abs() < 1e-9 && (p.y - end.y).abs() < 1e-9);
            if !same {
                thinned.push(*end);
            }
        }
        return thinned;
    }
    kept
}

/// Builds the route overlay for `route` at integer tile zoom `zoom` (clamped into the
/// supported range). Missing, empty and broken routes yield an empty [`RoutePaths`].
pub fn build_route_paths(route: Option<&Route>, zoom: u8) -> RoutePaths {
    let Some(route) = route else {
        return RoutePaths::default();
    };
    let zoom = zoom.clamp(
        crate::mercator::MIN_ZOOM as u8,
        crate::mercator::MAX_ZOOM as u8,
    );
    let zoom_f = f64::from(zoom);
    let reference = reference_point(route);
    let shift_x = lon_to_x(reference.lon, zoom_f);
    let shift_y = lat_to_y(reference.lat, zoom_f);
    if !shift_x.is_finite() || !shift_y.is_finite() {
        return RoutePaths {
            zoom,
            ..RoutePaths::default()
        };
    }

    let mut projected: Vec<Projected> = Vec::with_capacity(route.points.len());
    for point in &route.points {
        if !(point.lat.is_finite() && point.lon.is_finite()) {
            continue;
        }
        projected.push(Projected {
            x: lon_to_x(point.lon, zoom_f) - shift_x,
            y: lat_to_y(point.lat, zoom_f) - shift_y,
            grade: point.grade,
        });
    }
    let points = thin(&projected);

    // casing: the full line, drawn once underneath all coloured segments
    let mut casing = String::new();
    if let Some(first) = points.first() {
        let _ = write!(casing, "M");
        write_coord(&mut casing, first.x);
        let _ = write!(casing, " ");
        write_coord(&mut casing, first.y);
        for point in points.iter().skip(1) {
            let _ = write!(casing, "L");
            write_coord(&mut casing, point.x);
            let _ = write!(casing, " ");
            write_coord(&mut casing, point.y);
        }
    }

    let mut buckets: Vec<String> = vec![String::new(); GRADE_BUCKETS];
    let mut last_index: Vec<Option<usize>> = vec![None; GRADE_BUCKETS];
    for index in 0..points.len().saturating_sub(1) {
        let (Some(a), Some(b)) = (points.get(index), points.get(index + 1)) else {
            continue;
        };
        let bucket = grade_bucket((a.grade + b.grade) / 2.0);
        let Some(data) = buckets.get_mut(bucket) else {
            continue;
        };
        let Some(previous) = last_index.get_mut(bucket) else {
            continue;
        };
        if *previous != Some(index) {
            let _ = write!(data, "M");
            write_coord(data, a.x);
            let _ = write!(data, " ");
            write_coord(data, a.y);
        }
        let _ = write!(data, "L");
        write_coord(data, b.x);
        let _ = write!(data, " ");
        write_coord(data, b.y);
        *previous = Some(index + 1);
    }

    RoutePaths {
        zoom,
        shift_x,
        shift_y,
        casing,
        buckets: buckets
            .into_iter()
            .enumerate()
            .filter(|(_, data)| !data.is_empty())
            .collect(),
        points: points.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{Bounds, RoutePoint, RouteSource};

    fn route_from(points: &[(f64, f64, f64)]) -> Route {
        let points: Vec<RoutePoint> = points
            .iter()
            .enumerate()
            .map(|(index, (lat, lon, grade))| RoutePoint {
                lat: *lat,
                lon: *lon,
                ele: 100.0,
                dist: index as f64 * 10.0,
                grade: *grade,
            })
            .collect();
        let (mut south, mut north) = (f64::MAX, f64::MIN);
        let (mut west, mut east) = (f64::MAX, f64::MIN);
        for point in &points {
            south = south.min(point.lat);
            north = north.max(point.lat);
            west = west.min(point.lon);
            east = east.max(point.lon);
        }
        Route {
            id: "test".into(),
            name: "Teszt".into(),
            source: RouteSource::Demo,
            total_dist: points.last().map_or(0.0, |p| p.dist),
            points,
            ascent: 0.0,
            descent: 0.0,
            min_ele: 0.0,
            max_ele: 0.0,
            bounds: Bounds {
                south: if south.is_finite() { south } else { 0.0 },
                west: if west.is_finite() { west } else { 0.0 },
                north: if north.is_finite() { north } else { 0.0 },
                east: if east.is_finite() { east } else { 0.0 },
            },
            climbs: vec![],
            waypoints: vec![],
            has_elevation: true,
            segments: Vec::new(),
            timeline: Vec::new(),
        }
    }

    fn count(data: &str, command: char) -> usize {
        data.chars().filter(|c| *c == command).count()
    }

    #[test]
    fn three_points_produce_one_polyline() {
        let route = route_from(&[
            (47.50, 19.00, 0.0),
            (47.51, 19.01, 0.0),
            (47.52, 19.02, 0.0),
        ]);
        let paths = build_route_paths(Some(&route), 14);
        assert_eq!(paths.zoom, 14);
        assert_eq!(paths.points, 3);
        assert!(paths.casing.starts_with('M'), "{}", paths.casing);
        assert_eq!(count(&paths.casing, 'M'), 1);
        assert_eq!(count(&paths.casing, 'L'), 2);
        assert_eq!(paths.buckets.len(), 1);
        assert_eq!(paths.buckets[0].0, grade_bucket(0.0));
        assert_eq!(count(&paths.buckets[0].1, 'M'), 1);
        assert_eq!(count(&paths.buckets[0].1, 'L'), 2);
        assert!(!paths.is_empty());
        // coordinates are local to the route centre: small and finite
        for token in paths
            .casing
            .split(['M', 'L', ' '])
            .filter(|t| !t.is_empty())
        {
            let parsed = token.parse::<f64>();
            assert!(parsed.is_ok(), "unparsable coordinate {token:?}");
            if let Ok(value) = parsed {
                assert!(value.is_finite(), "bad coordinate {token:?}");
                assert!(value.abs() < 100_000.0, "coordinate not local: {value}");
            }
        }
    }

    #[test]
    fn grades_split_the_route_into_colour_buckets() {
        // flat → 40 % → flat: the rising segment lands in a different bucket than the flats
        let route = route_from(&[
            (47.50, 19.00, 0.0),
            (47.51, 19.01, 0.0),
            (47.52, 19.02, 40.0),
        ]);
        let paths = build_route_paths(Some(&route), 13);
        assert_eq!(paths.buckets.len(), 2, "{:?}", paths.buckets);
        assert_eq!(paths.buckets[0].0, grade_bucket(0.0));
        assert_eq!(paths.buckets[1].0, grade_bucket(20.0));
        // each bucket draws only its own segment
        assert_eq!(count(&paths.buckets[0].1, 'L'), 1);
        assert_eq!(count(&paths.buckets[1].1, 'L'), 1);
        // the casing still covers both segments
        assert_eq!(count(&paths.casing, 'L'), 2);
        // buckets are ascending so the drawing order is deterministic
        assert!(paths.buckets[0].0 < paths.buckets[1].0);

        // a run of equal grades is one subpath, not one per segment
        let flat = route_from(&[
            (47.50, 19.00, 0.0),
            (47.51, 19.01, 0.0),
            (47.52, 19.02, 0.0),
            (47.53, 19.03, 0.0),
        ]);
        let paths = build_route_paths(Some(&flat), 13);
        assert_eq!(paths.buckets.len(), 1);
        assert_eq!(count(&paths.buckets[0].1, 'M'), 1);
        assert_eq!(count(&paths.buckets[0].1, 'L'), 3);
    }

    #[test]
    fn missing_and_empty_routes_are_harmless() {
        assert_eq!(build_route_paths(None, 12), RoutePaths::default());
        let empty = route_from(&[]);
        let paths = build_route_paths(Some(&empty), 12);
        assert_eq!(paths.zoom, 12);
        assert!(paths.is_empty());
        assert_eq!(paths.points, 0);
        assert!(paths.buckets.is_empty());

        let single = route_from(&[(47.5, 19.0, 3.0)]);
        let paths = build_route_paths(Some(&single), 12);
        assert_eq!(paths.points, 1);
        assert_eq!(paths.casing, "M0.00 0.00");
        assert!(paths.buckets.is_empty());
    }

    #[test]
    fn broken_coordinates_are_skipped() {
        let mut route = route_from(&[
            (47.50, 19.00, 0.0),
            (47.51, 19.01, 5.0),
            (47.52, 19.02, 0.0),
        ]);
        route.points[1].lat = f64::NAN;
        let paths = build_route_paths(Some(&route), 12);
        assert_eq!(paths.points, 2);
        // NaN grades are treated as flat instead of poisoning the path data
        route.points[1] = route.points[2];
        route.points[1].grade = f64::NAN;
        let paths = build_route_paths(Some(&route), 12);
        assert!(!paths.is_empty());
        assert!(!paths.casing.contains("NaN"));
    }

    #[test]
    fn zoom_is_clamped_and_thinning_keeps_the_shape() {
        let route = route_from(&[(47.50, 19.00, 1.0), (47.51, 19.01, 1.0)]);
        assert_eq!(build_route_paths(Some(&route), 0).zoom, 3);
        assert_eq!(build_route_paths(Some(&route), 40).zoom, 19);

        // a thousand points inside one pixel collapse to very few path points at zoom 3
        let dense: Vec<(f64, f64, f64)> = (0..1000)
            .map(|i| (47.5 + f64::from(i) * 1e-5, 19.0 + f64::from(i) * 1e-5, 2.0))
            .collect();
        let dense_route = route_from(&dense);
        let far = build_route_paths(Some(&dense_route), 3);
        assert!(far.points <= 4, "{} points at zoom 3", far.points);
        // at street zoom every point is far enough apart to be kept
        let near = build_route_paths(Some(&dense_route), 19);
        assert!(near.points > 900, "{} points at zoom 19", near.points);
    }

    #[test]
    fn the_casing_covers_every_segment_of_every_bucket() {
        let route = route_from(&[
            (47.50, 19.00, -6.0),
            (47.51, 19.01, 2.0),
            (47.52, 19.02, 7.0),
            (47.53, 19.03, -2.0),
        ]);
        let paths = build_route_paths(Some(&route), 14);
        let segments: usize = paths.buckets.iter().map(|(_, d)| count(d, 'L')).sum();
        assert_eq!(segments, count(&paths.casing, 'L'));
        assert!(paths.buckets.len() >= 3);
    }
}
