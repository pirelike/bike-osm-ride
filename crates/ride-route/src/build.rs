//! The route pipeline: raw GPX points → processed [`Route`].
//!
//! 1. **Clean**: drop invalid coordinates and points closer than [`MIN_STEP_M`] to the
//!    previous kept one (duplicates, GPS jitter); cumulative haversine distance. Position
//!    "outlier" heuristics are deliberately absent: a point that is far from both
//!    neighbours is indistinguishable from the tip of a legitimate out-and-back.
//! 2. **Elevation check**: `has_elevation` is `false` when fewer than 80 % of the points
//!    carry a non-zero elevation or all values are (almost) identical. Exact `0.0`
//!    values are treated as "unknown" when most elevations are non-zero (GPS dropouts).
//!    Isolated physically impossible elevation spikes (> 3 m + 35 % grade up and
//!    straight back down) are repaired, remaining gaps are interpolated over distance.
//! 3. **Resample** to a uniform spacing of at most [`RESAMPLE_M`] (the total length is
//!    split into equal parts, so the last point sits exactly at `total_dist`).
//! 4. **Smooth elevation**: 5-sample median (removes single-point spikes), then a
//!    ~50 m moving average with a window that shrinks symmetrically at the ends (so the
//!    first/last elevation stay exact).
//! 5. **Grade** over a ~80 m window, clamped to ±25 %.
//! 6. **Ascent/descent** with a 3 m hysteresis (zig-zag filter on the smoothed profile).
//! 7. **Climbs** ([`crate::climbs`]), bounds, waypoints and a stable id (FNV-1a over the
//!    cleaned geometry quantised to 1e-5°, so the same track gives the same id whether
//!    it comes from a file or from BRouter).

use ride_model::{Bounds, Route, RoutePoint, RouteSegment, RouteSource, Waypoint};

use crate::geo;
use crate::gpx::{GpxData, RawPoint};
use crate::{climbs, err, Result};

/// Target spacing of the processed route, metres.
pub const RESAMPLE_M: f64 = 10.0;
/// Points closer than this to the previous kept point are dropped (GPS jitter).
pub const MIN_STEP_M: f64 = 1.0;
/// Length of the elevation smoothing window, metres.
pub const SMOOTH_WINDOW_M: f64 = 50.0;
/// Length of the window the grade is measured over, metres.
pub const GRADE_WINDOW_M: f64 = 80.0;
/// Grades are clamped to ±this value (percent).
pub const MAX_GRADE_PCT: f64 = 25.0;
/// Hysteresis of the ascent/descent accumulators, metres.
pub const ASCENT_HYSTERESIS_M: f64 = 3.0;
/// Shorter routes are rejected.
pub const MIN_ROUTE_M: f64 = 20.0;
/// Longer routes are rejected (a 5000 km track would mean ~500 000 route points and is
/// almost certainly a broken file).
pub const MAX_ROUTE_M: f64 = 5_000_000.0;
/// Share of points that must have a usable elevation for `has_elevation`.
pub const MIN_ELEVATION_COVERAGE: f64 = 0.8;
/// A GPX timeline shorter than this is not a ride (a broken or synthetic file).
pub const MIN_TIMELINE_S: f64 = 10.0;
/// A timeline longer than this means the file spans several days (a broken recording, or a
/// route with a timestamp per waypoint): it is not replayed as a ghost.
pub const MAX_TIMELINE_S: f64 = 24.0 * 3600.0;

/// Waypoints farther than this from the route are not shown (their position on the
/// route would be meaningless).
const WAYPOINT_MAX_OFFSET_M: f64 = 1000.0;
const MAX_WAYPOINTS: usize = 500;

/// A cleaned input point: valid, de-jittered, with its cumulative distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CleanPoint {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    pub dist: f64,
    /// Absolute time from the file's `<time>`, seconds since the Unix epoch.
    pub time: Option<f64>,
}

/// Step 1: validity filter, duplicate/jitter removal, cumulative distance.
pub fn clean(raw: &[RawPoint]) -> Vec<CleanPoint> {
    let valid: Vec<RawPoint> = raw
        .iter()
        .copied()
        .filter(|p| geo::is_valid_coord(p.lat, p.lon) && !(p.lat == 0.0 && p.lon == 0.0))
        .collect();
    let mut out: Vec<CleanPoint> = Vec::with_capacity(valid.len());
    for p in &valid {
        let ele = p.ele.filter(|e| e.is_finite());
        let time = p.time.filter(|t| t.is_finite() && *t > 0.0);
        let Some(last) = out.last().copied() else {
            out.push(CleanPoint {
                lat: p.lat,
                lon: p.lon,
                ele,
                dist: 0.0,
                time,
            });
            continue;
        };
        let d = geo::haversine_m(last.lat, last.lon, p.lat, p.lon);
        if d < MIN_STEP_M {
            if let Some(l) = out.last_mut() {
                if l.ele.is_none() {
                    l.ele = ele;
                }
            }
            continue;
        }
        out.push(CleanPoint {
            lat: p.lat,
            lon: p.lon,
            ele,
            dist: last.dist + d,
            time,
        });
    }
    out
}

/// The file's own timestamps as elapsed seconds at every resampled point.
///
/// Only a complete timeline is used: every cleaned point must carry a `<time>`, the total must be
/// a plausible ride ([`MIN_TIMELINE_S`]..[`MAX_TIMELINE_S`]), and the values are clamped to be
/// non-decreasing (a GPS glitch must not rewind the ghost). The result is indexed like
/// [`Route::points`], so `timeline[0] == 0.0` and `timeline.last()` is the route's own time.
/// Empty when the file has no usable timeline.
pub fn timeline(cleaned: &[CleanPoint], samples: &[Sample]) -> Vec<f64> {
    if cleaned.len() < 2 || samples.len() < 2 {
        return Vec::new();
    }
    let Some(first) = cleaned[0].time else {
        return Vec::new();
    };
    let mut times: Vec<f64> = Vec::with_capacity(cleaned.len());
    for point in cleaned {
        let Some(time) = point.time else {
            return Vec::new();
        };
        let time = time.max(times.last().copied().unwrap_or(time));
        times.push(time);
    }
    let total = times[times.len() - 1] - first;
    if !(MIN_TIMELINE_S..=MAX_TIMELINE_S).contains(&total) {
        return Vec::new();
    }
    // The same walk `resample` does, but on time instead of elevation.
    let mut out = Vec::with_capacity(samples.len());
    let mut j = 0usize;
    for sample in samples {
        let d = sample.dist;
        while j + 2 < cleaned.len() && cleaned[j + 1].dist < d {
            j += 1;
        }
        let (a, b) = (&cleaned[j], &cleaned[j + 1]);
        let span = b.dist - a.dist;
        let frac = if span > 0.0 {
            ((d - a.dist) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let elapsed = (times[j] + (times[j + 1] - times[j]) * frac) - first;
        out.push(elapsed.max(0.0));
    }
    if out.last().is_some_and(|end| end.is_finite() && *end > 0.0) {
        out
    } else {
        Vec::new()
    }
}

/// Step 2: decide whether the cleaned points carry elevation; repair/interpolate it.
/// Returns `has_elevation`. When `false`, all elevations are set to `Some(0.0)`.
pub fn prepare_elevation(points: &mut [CleanPoint]) -> bool {
    let total = points.len();
    if total == 0 {
        return false;
    }
    let nonzero = points
        .iter()
        .filter(|p| p.ele.is_some_and(|e| e != 0.0))
        .count();
    let zero = points.iter().filter(|p| p.ele == Some(0.0)).count();
    if nonzero > 0 && nonzero >= zero {
        for p in points.iter_mut() {
            if p.ele == Some(0.0) {
                p.ele = None;
            }
        }
    }
    let known: Vec<f64> = points.iter().filter_map(|p| p.ele).collect();
    let coverage = known.len() as f64 / total as f64;
    let (mn, mx) = known
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &e| {
            (a.min(e), b.max(e))
        });
    let has = coverage >= MIN_ELEVATION_COVERAGE && known.len() >= 2 && (mx - mn) >= 0.01;
    if !has {
        for p in points.iter_mut() {
            p.ele = Some(0.0);
        }
        return false;
    }
    despike_elevation(points);
    fill_missing_elevation(points);
    true
}

/// Repair isolated impossible elevation spikes (up and straight back down).
fn despike_elevation(points: &mut [CleanPoint]) {
    for i in 1..points.len().saturating_sub(1) {
        let (Some(a), Some(b), Some(c)) = (points[i - 1].ele, points[i].ele, points[i + 1].ele)
        else {
            continue;
        };
        let d1 = points[i].dist - points[i - 1].dist;
        let d2 = points[i + 1].dist - points[i].dist;
        let (up, down) = (b - a, c - b);
        if up * down < 0.0 && up.abs() > 3.0 + 0.35 * d1 && down.abs() > 3.0 + 0.35 * d2 {
            let span = d1 + d2;
            let t = if span > 0.0 { d1 / span } else { 0.5 };
            points[i].ele = Some(a + (c - a) * t);
        }
    }
}

/// Fill `None` elevations by linear interpolation over distance (ends are held).
fn fill_missing_elevation(points: &mut [CleanPoint]) {
    let n = points.len();
    let mut i = 0;
    while i < n {
        if points[i].ele.is_some() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && points[i].ele.is_none() {
            i += 1;
        }
        let before = start
            .checked_sub(1)
            .and_then(|k| points[k].ele.map(|e| (points[k].dist, e)));
        let after = (i < n)
            .then(|| points[i].ele.map(|e| (points[i].dist, e)))
            .flatten();
        for p in &mut points[start..i] {
            let d = p.dist;
            p.ele = Some(match (before, after) {
                (Some((d0, e0)), Some((d1, e1))) if d1 > d0 => {
                    e0 + (e1 - e0) * (d - d0) / (d1 - d0)
                }
                (Some((_, e0)), _) => e0,
                (None, Some((_, e1))) => e1,
                (None, None) => 0.0,
            });
        }
    }
}

/// A resampled point before smoothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub lat: f64,
    pub lon: f64,
    pub ele: f64,
    pub dist: f64,
}

/// Step 3: resample to uniform spacing (`<= RESAMPLE_M`), first/last points preserved.
/// Requires at least two points with strictly increasing `dist`.
pub fn resample(points: &[CleanPoint]) -> Vec<Sample> {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    let total = last.dist;
    if points.len() < 2 || total <= 0.0 {
        return vec![Sample {
            lat: first.lat,
            lon: first.lon,
            ele: first.ele.unwrap_or(0.0),
            dist: 0.0,
        }];
    }
    let parts = (total / RESAMPLE_M).ceil().max(1.0) as usize;
    let step = total / parts as f64;
    let mut out = Vec::with_capacity(parts + 1);
    let mut j = 0usize;
    for k in 0..=parts {
        let d = if k == parts { total } else { k as f64 * step };
        while j + 2 < points.len() && points[j + 1].dist < d {
            j += 1;
        }
        let (a, b) = (&points[j], &points[j + 1]);
        let span = b.dist - a.dist;
        let t = if span > 0.0 {
            ((d - a.dist) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (ea, eb) = (a.ele.unwrap_or(0.0), b.ele.unwrap_or(0.0));
        out.push(Sample {
            lat: a.lat + (b.lat - a.lat) * t,
            lon: geo::lerp_lon(a.lon, b.lon, t),
            ele: ea + (eb - ea) * t,
            dist: d,
        });
    }
    out
}

/// Step 4: remove spikes (median of 5) and smooth over ~[`SMOOTH_WINDOW_M`].
/// `step_m` is the sample spacing. The first and last samples keep their value.
pub fn smooth_elevation(ele: &[f64], step_m: f64) -> Vec<f64> {
    let n = ele.len();
    if n < 3 {
        return ele.to_vec();
    }
    let mut med = ele.to_vec();
    if n >= 5 {
        for i in 2..n - 2 {
            let mut w = [ele[i - 2], ele[i - 1], ele[i], ele[i + 1], ele[i + 2]];
            w.sort_by(|a, b| a.total_cmp(b));
            med[i] = w[2];
        }
    } else {
        for i in 1..n - 1 {
            let mut w = [ele[i - 1], ele[i], ele[i + 1]];
            w.sort_by(|a, b| a.total_cmp(b));
            med[i] = w[1];
        }
    }
    let k = ((SMOOTH_WINDOW_M / 2.0) / step_m.max(0.1)).floor().max(1.0) as usize;
    let mut prefix = Vec::with_capacity(n + 1);
    prefix.push(0.0);
    let mut acc = 0.0;
    for &v in &med {
        acc += v;
        prefix.push(acc);
    }
    (0..n)
        .map(|i| {
            let r = k.min(i).min(n - 1 - i);
            (prefix[i + r + 1] - prefix[i - r]) / (2 * r + 1) as f64
        })
        .collect()
}

/// Step 5: grade in percent over ~[`GRADE_WINDOW_M`], clamped to ±[`MAX_GRADE_PCT`].
pub fn compute_grades(dist: &[f64], ele: &[f64], step_m: f64) -> Vec<f64> {
    let n = dist.len().min(ele.len());
    let k = ((GRADE_WINDOW_M / 2.0) / step_m.max(0.1)).round().max(1.0) as usize;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(k);
            let hi = (i + k).min(n - 1);
            let dd = dist[hi] - dist[lo];
            if dd > 1e-6 {
                (100.0 * (ele[hi] - ele[lo]) / dd).clamp(-MAX_GRADE_PCT, MAX_GRADE_PCT)
            } else {
                0.0
            }
        })
        .collect()
}

/// Step 6: total ascent and descent (both positive) with a zig-zag hysteresis of
/// `hysteresis` metres: a rise/fall only counts once it exceeds the threshold, and
/// then counts in full. `ascent - descent` equals the net change up to the hysteresis.
pub fn ascent_descent(ele: &[f64], hysteresis: f64) -> (f64, f64) {
    let Some((&first, rest)) = ele.split_first() else {
        return (0.0, 0.0);
    };
    let (mut ascent, mut descent) = (0.0, 0.0);
    let mut anchor = first;
    let mut ext = first;
    let mut dir = 0i8;
    for &e in rest {
        match dir {
            0 => {
                if e - anchor >= hysteresis {
                    dir = 1;
                    ext = e;
                } else if anchor - e >= hysteresis {
                    dir = -1;
                    ext = e;
                }
            }
            1 => {
                if e > ext {
                    ext = e;
                } else if ext - e >= hysteresis {
                    ascent += ext - anchor;
                    anchor = ext;
                    dir = -1;
                    ext = e;
                }
            }
            _ => {
                if e < ext {
                    ext = e;
                } else if e - ext >= hysteresis {
                    descent += anchor - ext;
                    anchor = ext;
                    dir = 1;
                    ext = e;
                }
            }
        }
    }
    match dir {
        1 => ascent += ext - anchor,
        -1 => descent += anchor - ext,
        _ => {}
    }
    (ascent, descent)
}

/// Stable route id: FNV-1a (64 bit) over the cleaned geometry quantised to 1e-5° (~1 m).
pub fn route_id(points: &[CleanPoint]) -> String {
    let mut h = geo::Fnv1a::new();
    for p in points {
        h.write(&((p.lat * 1e5).round() as i32).to_le_bytes());
        h.write(&((p.lon * 1e5).round() as i32).to_le_bytes());
    }
    format!("{:016x}", h.finish())
}

/// Name policy: the GPX name unless it is BRouter's generic `brouter_<profile>_<n>`,
/// else the hint (file name / link description), else a placeholder.
pub fn pick_name(gpx_name: Option<&str>, hint: &str) -> String {
    let limit = |s: &str| -> String { s.trim().chars().take(120).collect() };
    if let Some(n) = gpx_name.map(str::trim).filter(|n| !n.is_empty()) {
        if !n.to_ascii_lowercase().starts_with("brouter_") {
            return limit(n);
        }
    }
    let h = limit(hint);
    if h.is_empty() {
        "Névtelen útvonal".to_string()
    } else {
        h
    }
}

fn map_waypoints(data: &GpxData, points: &[RoutePoint]) -> Vec<Waypoint> {
    let mut out = Vec::new();
    for (i, w) in data.waypoints.iter().take(MAX_WAYPOINTS).enumerate() {
        if !geo::is_valid_coord(w.lat, w.lon) {
            continue;
        }
        let nearest = points
            .iter()
            .map(|p| (p.dist, geo::approx_dist2_m(w.lat, w.lon, p.lat, p.lon)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((dist, d2)) = nearest else { continue };
        if d2.sqrt() > WAYPOINT_MAX_OFFSET_M {
            continue;
        }
        let name = if w.name.trim().is_empty() {
            format!("Útpont {}", i + 1)
        } else {
            w.name.trim().to_string()
        };
        out.push(Waypoint {
            name,
            lat: w.lat,
            lon: w.lon,
            ele: w.ele.filter(|e| e.is_finite()),
            dist_on_route: dist,
        });
    }
    out.sort_by(|a, b| a.dist_on_route.total_cmp(&b.dist_on_route));
    out
}

/// A segment's ends must lie this close to the route, metres.
const SEGMENT_MATCH_M: f64 = 40.0;

/// The named segments of the file (tracks lying on the route) as distances along the route. A
/// segment whose ends cannot be found on the route in riding order, or whose length along the
/// route does not fit its own length, is dropped.
fn map_segments(data: &GpxData, points: &[RoutePoint]) -> Vec<RouteSegment> {
    let nearest = |lat: f64, lon: f64, from: usize| -> Option<usize> {
        let (i, d2) = points
            .iter()
            .enumerate()
            .skip(from)
            .map(|(i, p)| (i, geo::approx_dist2_m(lat, lon, p.lat, p.lon)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        (d2.sqrt() <= SEGMENT_MATCH_M).then_some(i)
    };
    let mut out = Vec::new();
    for track in &data.segment_tracks {
        let (Some(first), Some(last)) = (track.points.first(), track.points.last()) else {
            continue;
        };
        let own: f64 = track
            .points
            .windows(2)
            .map(|w| geo::haversine_m(w[0].lat, w[0].lon, w[1].lat, w[1].lon))
            .sum();
        let Some(a) = nearest(first.lat, first.lon, 0) else {
            continue;
        };
        let Some(b) = nearest(last.lat, last.lon, a + 1) else {
            continue;
        };
        let (start, end) = (points[a].dist, points[b].dist);
        let along = end - start;
        if along <= 0.0 || (own > 0.0 && !(own * 0.5..=own * 1.5).contains(&along)) {
            continue;
        }
        out.push(RouteSegment {
            name: track.name.clone(),
            start_dist: start,
            end_dist: end,
        });
    }
    out.sort_by(|x, y| x.start_dist.total_cmp(&y.start_dist));
    out
}

/// Run the whole pipeline on parsed GPX data.
pub fn build_route(data: &GpxData, name_hint: &str, source: RouteSource) -> Result<Route> {
    if data.points.len() < 2 {
        return err("Az útvonalnak legalább két pontot kell tartalmaznia.");
    }
    let mut cleaned = clean(&data.points);
    if cleaned.len() < 2 {
        return err("Az útvonal nem tartalmaz két különböző pontot.");
    }
    let total = cleaned.last().map_or(0.0, |p| p.dist);
    if total < MIN_ROUTE_M {
        return err(format!(
            "Az útvonal túl rövid ({total:.0} m); legalább {MIN_ROUTE_M:.0} m kell."
        ));
    }
    if total > MAX_ROUTE_M {
        return err(format!(
            "Az útvonal túl hosszú ({:.0} km); legfeljebb {:.0} km engedélyezett.",
            total / 1000.0,
            MAX_ROUTE_M / 1000.0
        ));
    }
    let id = route_id(&cleaned);
    let has_elevation = prepare_elevation(&mut cleaned);
    let samples = resample(&cleaned);
    let step = total / (samples.len().saturating_sub(1)).max(1) as f64;

    let raw_ele: Vec<f64> = samples.iter().map(|s| s.ele).collect();
    let dist: Vec<f64> = samples.iter().map(|s| s.dist).collect();
    let (ele, grade) = if has_elevation {
        let ele = smooth_elevation(&raw_ele, step);
        let grade = compute_grades(&dist, &ele, step);
        (ele, grade)
    } else {
        (vec![0.0; samples.len()], vec![0.0; samples.len()])
    };

    let points: Vec<RoutePoint> = samples
        .iter()
        .zip(ele.iter().zip(&grade))
        .map(|(s, (&e, &g))| RoutePoint {
            lat: s.lat,
            lon: s.lon,
            ele: e,
            dist: s.dist,
            grade: g,
        })
        .collect();

    let (ascent, descent) = if has_elevation {
        ascent_descent(&ele, ASCENT_HYSTERESIS_M)
    } else {
        (0.0, 0.0)
    };
    let (min_ele, max_ele) = ele
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &e| {
            (a.min(e), b.max(e))
        });
    let bounds = points.iter().fold(
        Bounds {
            south: f64::INFINITY,
            west: f64::INFINITY,
            north: f64::NEG_INFINITY,
            east: f64::NEG_INFINITY,
        },
        |b, p| Bounds {
            south: b.south.min(p.lat),
            west: b.west.min(p.lon),
            north: b.north.max(p.lat),
            east: b.east.max(p.lon),
        },
    );
    let climbs = if has_elevation {
        climbs::detect(&points)
    } else {
        Vec::new()
    };
    let waypoints = map_waypoints(data, &points);
    let segments = map_segments(data, &points);
    // The GPX's own `<time>` stamps become the route's timeline: its own ghost, available before
    // the first ride of this route.
    let timeline = timeline(&cleaned, &samples);

    Ok(Route {
        id,
        name: pick_name(data.name.as_deref(), name_hint),
        source,
        total_dist: points.last().map_or(0.0, |p| p.dist),
        points,
        timeline,
        ascent,
        descent,
        min_ele,
        max_ele,
        bounds,
        climbs,
        waypoints,
        has_elevation,
        segments,
    })
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// Build raw points for an eastbound route from `(length_m, grade_pct)` segments with
    /// the given raw spacing; used to feed the pipeline with known profiles.
    pub fn raw_from_segments(
        segments: &[(f64, f64)],
        start_ele: f64,
        spacing_m: f64,
    ) -> Vec<RawPoint> {
        let m_per_deg_lon = 111_320.0 * 47.5f64.to_radians().cos();
        let mut pts = vec![RawPoint {
            lat: 47.5,
            lon: 19.0,
            ele: Some(start_ele),
            time: None,
        }];
        let (mut d, mut e) = (0.0, start_ele);
        for &(len, g) in segments {
            let n = (len / spacing_m).round().max(1.0) as usize;
            let step = len / n as f64;
            for _ in 0..n {
                d += step;
                e += step * g / 100.0;
                pts.push(RawPoint {
                    lat: 47.5,
                    lon: 19.0 + d / m_per_deg_lon,
                    ele: Some(e),
                    time: None,
                });
            }
        }
        pts
    }

    pub fn gpx_data(points: Vec<RawPoint>) -> GpxData {
        GpxData {
            points,
            ..GpxData::default()
        }
    }

    pub fn route_from_segments(segments: &[(f64, f64)], start_ele: f64) -> Route {
        build_route(
            &gpx_data(raw_from_segments(segments, start_ele, 7.0)),
            "teszt",
            RouteSource::GpxFile,
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn rp(lat: f64, lon: f64, ele: Option<f64>) -> RawPoint {
        RawPoint {
            lat,
            lon,
            ele,
            time: None,
        }
    }

    #[test]
    fn clean_drops_invalid_duplicates_and_jitter() {
        let raw = vec![
            rp(47.5, 19.0, Some(100.0)),
            rp(47.5, 19.0, Some(100.0)),      // exact duplicate
            rp(47.500_002, 19.000_001, None), // ~0.3 m jitter
            rp(f64::NAN, 19.0, None),         // invalid
            rp(0.0, 0.0, None),               // null island
            rp(47.5005, 19.0, Some(101.0)),   // 55 m
        ];
        let c = clean(&raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].dist, 0.0);
        assert!((c[1].dist - 55.6).abs() < 1.0, "got {}", c[1].dist);
    }

    #[test]
    fn clean_keeps_the_tip_of_an_out_and_back() {
        // Up the road to a viewpoint and back along the very same road: the tip must stay.
        let out_and_back = vec![
            rp(47.5, 19.0, None),
            rp(47.51, 19.0, None),
            rp(47.52, 19.0, None),
            rp(47.51, 19.000_01, None),
            rp(47.5, 19.0, None),
        ];
        let c = clean(&out_and_back);
        assert_eq!(c.len(), 5);
        assert!((c[4].dist - 4.0 * 1111.9).abs() < 20.0, "{}", c[4].dist);
    }

    #[test]
    fn elevation_coverage_rules() {
        let mk = |eles: &[Option<f64>]| -> Vec<CleanPoint> {
            eles.iter()
                .enumerate()
                .map(|(i, &e)| CleanPoint {
                    lat: 47.5,
                    lon: 19.0 + i as f64 * 1e-3,
                    ele: e,
                    dist: i as f64 * 75.0,
                    time: None,
                })
                .collect()
        };
        // plenty of elevation
        let mut p = mk(&[
            Some(100.0),
            Some(101.0),
            Some(103.0),
            Some(102.0),
            Some(104.0),
        ]);
        assert!(prepare_elevation(&mut p));
        // all missing
        let mut p = mk(&[None; 5]);
        assert!(!prepare_elevation(&mut p));
        assert!(p.iter().all(|x| x.ele == Some(0.0)));
        // all zero
        let mut p = mk(&[Some(0.0); 5]);
        assert!(!prepare_elevation(&mut p));
        // constant
        let mut p = mk(&[Some(120.0); 5]);
        assert!(!prepare_elevation(&mut p));
        // 60 % coverage is not enough, 80 % is
        let mut p = mk(&[Some(100.0), None, Some(103.0), None, Some(104.0)]);
        assert!(!prepare_elevation(&mut p));
        let mut p = mk(&[Some(100.0), None, Some(103.0), Some(105.0), Some(104.0)]);
        assert!(prepare_elevation(&mut p));
        assert_eq!(p[1].ele, Some(101.5), "gap is interpolated over distance");
    }

    #[test]
    fn zero_elevation_dropouts_are_treated_as_missing() {
        let mk = |eles: &[f64]| -> Vec<CleanPoint> {
            eles.iter()
                .enumerate()
                .map(|(i, &e)| CleanPoint {
                    lat: 47.5,
                    lon: 19.0 + i as f64 * 1e-3,
                    ele: Some(e),
                    dist: i as f64 * 75.0,
                    time: None,
                })
                .collect()
        };
        let mut p = mk(&[
            200.0, 201.0, 0.0, 203.0, 204.0, 205.0, 206.0, 207.0, 208.0, 209.0,
        ]);
        assert!(prepare_elevation(&mut p));
        assert_eq!(p[2].ele, Some(202.0));
        // A route that really is at sea level (mostly zero) keeps its zeros.
        let mut p = mk(&[0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0]);
        assert!(prepare_elevation(&mut p));
        assert_eq!(p[0].ele, Some(0.0));
    }

    #[test]
    fn isolated_elevation_spikes_are_repaired_but_steep_ramps_are_not() {
        let mk = |eles: &[f64], spacing: f64| -> Vec<CleanPoint> {
            eles.iter()
                .enumerate()
                .map(|(i, &e)| CleanPoint {
                    lat: 47.5,
                    lon: 19.0,
                    ele: Some(e),
                    dist: i as f64 * spacing,
                    time: None,
                })
                .collect()
        };
        let mut p = mk(&[100.0, 100.5, 180.0, 101.0, 101.5], 10.0);
        despike_elevation(&mut p);
        assert!((p[2].ele.unwrap() - 100.75).abs() < 0.3);
        // 20 % ramp, monotone: untouched.
        let mut p = mk(&[100.0, 102.0, 104.0, 106.0, 108.0], 10.0);
        let before = p.clone();
        despike_elevation(&mut p);
        assert_eq!(p, before);
        // A real summit sampled sparsely (200 m spacing, 8 m peak) is kept.
        let mut p = mk(&[100.0, 108.0, 100.0], 200.0);
        let before = p.clone();
        despike_elevation(&mut p);
        assert_eq!(p, before);
    }

    #[test]
    fn resampling_invariants() {
        let raw = raw_from_segments(&[(1234.0, 4.0), (800.0, -3.0)], 150.0, 37.0);
        let mut c = clean(&raw);
        assert!(prepare_elevation(&mut c));
        let s = resample(&c);
        let total = c.last().unwrap().dist;
        assert_eq!(s[0].dist, 0.0);
        assert!((s.last().unwrap().dist - total).abs() < 1e-9);
        let mut max_gap: f64 = 0.0;
        let mut min_gap = f64::MAX;
        for w in s.windows(2) {
            let g = w[1].dist - w[0].dist;
            assert!(g > 0.0, "monotonic distance");
            max_gap = max_gap.max(g);
            min_gap = min_gap.min(g);
        }
        assert!(max_gap <= RESAMPLE_M + 1e-9, "max spacing {max_gap}");
        assert!(min_gap > RESAMPLE_M * 0.5, "min spacing {min_gap}");
        // Geometry stays on the original polyline (eastbound, constant latitude).
        assert!(s.iter().all(|p| (p.lat - 47.5).abs() < 1e-9));
        // Resampled points are really ~spacing apart on the ground.
        let ground = geo::haversine_m(s[10].lat, s[10].lon, s[11].lat, s[11].lon);
        assert!(
            (ground - (s[11].dist - s[10].dist)).abs() < 0.05,
            "{ground}"
        );
    }

    #[test]
    fn smoothing_removes_spikes_and_preserves_slopes_and_ends() {
        let n = 200;
        let mut e: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.5).collect(); // 5 % at 10 m steps
        e[100] += 40.0;
        e[50] -= 25.0;
        let s = smooth_elevation(&e, 10.0);
        for (i, v) in s.iter().enumerate() {
            let truth = 100.0 + i as f64 * 0.5;
            assert!((v - truth).abs() < 0.6, "i={i} got {v} want {truth}");
        }
        assert_eq!(s[0], e[0]);
        assert_eq!(s[n - 1], e[n - 1]);
        // Staircase noise (BRouter quantisation) is flattened.
        let stair: Vec<f64> = (0..100)
            .map(|i| 100.0 + ((i as f64 * 0.9).sin() * 2.0).round() * 0.25)
            .collect();
        let s = smooth_elevation(&stair, 10.0);
        let rough = |v: &[f64]| v.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>();
        assert!(rough(&s) < rough(&stair));
    }

    #[test]
    fn grades_follow_the_profile_and_are_clamped() {
        let dist: Vec<f64> = (0..100).map(|i| i as f64 * 10.0).collect();
        let ele: Vec<f64> = dist.iter().map(|d| 100.0 + d * 0.07).collect();
        let g = compute_grades(&dist, &ele, 10.0);
        assert!(g.iter().all(|x| (x - 7.0).abs() < 1e-9));
        let wall: Vec<f64> = dist.iter().map(|d| 100.0 + d * 0.6).collect();
        assert!(compute_grades(&dist, &wall, 10.0)
            .iter()
            .all(|&x| x == MAX_GRADE_PCT));
        let down: Vec<f64> = dist.iter().map(|d| 100.0 - d * 0.6).collect();
        assert!(compute_grades(&dist, &down, 10.0)
            .iter()
            .all(|&x| x == -MAX_GRADE_PCT));
    }

    #[test]
    fn ascent_hysteresis() {
        // 2 m noise around a flat line never counts.
        let noisy: Vec<f64> = (0..200)
            .map(|i| 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert_eq!(ascent_descent(&noisy, 3.0), (0.0, 0.0));
        // Clean up/down/up: 20 up, 10 down, 15 up.
        let prof = [100.0, 110.0, 120.0, 115.0, 110.0, 118.0, 125.0];
        let (a, d) = ascent_descent(&prof, 3.0);
        assert_eq!((a, d), (20.0 + 15.0, 10.0));
        // Net change is preserved up to the hysteresis.
        assert!((a - d - (125.0 - 100.0)).abs() <= 3.0);
        // A 2 m dip inside a climb does not split or reduce it.
        let dip = [100.0, 105.0, 103.0, 108.0, 112.0];
        assert_eq!(ascent_descent(&dip, 3.0), (12.0, 0.0));
        // Pure descent.
        assert_eq!(ascent_descent(&[50.0, 40.0, 30.0], 3.0), (0.0, 20.0));
        assert_eq!(ascent_descent(&[], 3.0), (0.0, 0.0));
        assert_eq!(ascent_descent(&[5.0], 3.0), (0.0, 0.0));
    }

    #[test]
    fn route_from_known_profile() {
        let r = route_from_segments(
            &[(2000.0, 0.0), (4000.0, 7.0), (1000.0, 0.0), (2000.0, -5.0)],
            150.0,
        );
        assert!((r.total_dist - 9000.0).abs() < 30.0, "{}", r.total_dist); // generator uses a slightly different metres-per-degree
        assert!(r.has_elevation);
        assert_eq!(r.points[0].dist, 0.0);
        assert!((r.points.last().unwrap().dist - r.total_dist).abs() < 1e-9);
        // 4 km at 7 % = 280 m up, 2 km at -5 % = 100 m down (smoothing may shave a metre or two).
        assert!((r.ascent - 280.0).abs() < 6.0, "ascent {}", r.ascent);
        assert!((r.descent - 100.0).abs() < 6.0, "descent {}", r.descent);
        assert!((r.max_ele - 430.0).abs() < 2.0, "max {}", r.max_ele);
        assert!((r.min_ele - 150.0).abs() < 2.0);
        let mid = r.locate(4000.0);
        assert!((mid.grade - 7.0).abs() < 0.3, "grade {}", mid.grade);
        assert!(r.bounds.west < r.bounds.east && r.bounds.south <= r.bounds.north);
        assert_eq!(r.climbs.len(), 1);
    }

    #[test]
    fn route_without_elevation_is_flat() {
        let raw: Vec<RawPoint> = raw_from_segments(&[(1000.0, 5.0)], 100.0, 25.0)
            .into_iter()
            .map(|p| RawPoint { ele: None, ..p })
            .collect();
        let r = build_route(&gpx_data(raw), "x", RouteSource::GpxFile).unwrap();
        assert!(!r.has_elevation);
        assert_eq!((r.ascent, r.descent), (0.0, 0.0));
        assert!(r.points.iter().all(|p| p.ele == 0.0 && p.grade == 0.0));
        assert!(r.climbs.is_empty());
        assert!((r.total_dist - 1000.0).abs() < 3.0, "{}", r.total_dist);
    }

    #[test]
    fn too_short_or_degenerate_routes_are_errors() {
        let one = gpx_data(vec![rp(47.5, 19.0, None)]);
        assert!(build_route(&one, "x", RouteSource::GpxFile).is_err());
        let same = gpx_data(vec![
            rp(47.5, 19.0, None),
            rp(47.5, 19.0, None),
            rp(47.500_001, 19.0, None),
        ]);
        assert!(build_route(&same, "x", RouteSource::GpxFile).is_err());
        let tiny = gpx_data(vec![rp(47.5, 19.0, None), rp(47.5001, 19.0, None)]);
        let e = build_route(&tiny, "x", RouteSource::GpxFile)
            .unwrap_err()
            .to_string();
        assert!(e.contains("rövid"), "{e}");
    }

    #[test]
    fn absurdly_long_routes_are_rejected() {
        let data = gpx_data(vec![rp(47.5, 19.0, None), rp(47.5, 100.0, None)]);
        let e = build_route(&data, "x", RouteSource::GpxFile)
            .unwrap_err()
            .to_string();
        assert!(e.contains("túl hosszú"), "{e}");
    }

    #[test]
    fn id_is_stable_geometry_only_and_filename_safe() {
        let raw = raw_from_segments(&[(1500.0, 3.0)], 100.0, 20.0);
        let a = build_route(&gpx_data(raw.clone()), "A", RouteSource::GpxFile).unwrap();
        let b = build_route(&gpx_data(raw.clone()), "B", RouteSource::Bikerouter).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.len(), 16);
        assert!(a.id.chars().all(|c| c.is_ascii_hexdigit()));
        let c = build_route(
            &gpx_data(raw_from_segments(&[(1600.0, 3.0)], 100.0, 20.0)),
            "A",
            RouteSource::GpxFile,
        )
        .unwrap();
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn name_policy() {
        assert_eq!(pick_name(Some("Balaton kör"), "fájl"), "Balaton kör");
        assert_eq!(
            pick_name(Some("brouter_trekking_0"), "Bikerouter útvonal"),
            "Bikerouter útvonal"
        );
        assert_eq!(pick_name(None, "  fájl  "), "fájl");
        assert_eq!(pick_name(Some("  "), ""), "Névtelen útvonal");
    }

    #[test]
    fn segment_tracks_become_distances_along_the_route() {
        let mut data = gpx_data(raw_from_segments(&[(3000.0, 2.0)], 100.0, 20.0));
        // the stretch from 1000 m to 1600 m of the route, as its own track
        data.segment_tracks.push(crate::gpx::RawTrack {
            name: "Emelkedő sprint".into(),
            points: data.points[50..=80].to_vec(),
        });
        // reversed: ridden the other way, so it is not a segment of this route
        let mut back = data.points[50..=80].to_vec();
        back.reverse();
        data.segment_tracks.push(crate::gpx::RawTrack {
            name: "Visszafelé".into(),
            points: back,
        });
        let r = build_route(&data, "x", RouteSource::GpxFile).unwrap();
        assert_eq!(r.segments.len(), 1, "{:?}", r.segments);
        let seg = &r.segments[0];
        assert_eq!(seg.name, "Emelkedő sprint");
        assert!((seg.start_dist - 1000.0).abs() < 15.0, "{seg:?}");
        assert!((seg.end_dist - 1600.0).abs() < 15.0, "{seg:?}");
    }

    #[test]
    fn waypoints_are_attached_to_the_nearest_route_point() {
        let mut data = gpx_data(raw_from_segments(&[(3000.0, 0.0)], 100.0, 20.0));
        let m_per_deg_lon = 111_320.0 * 47.5f64.to_radians().cos();
        let at = |d: f64, dlat: f64| (47.5 + dlat, 19.0 + d / m_per_deg_lon);
        for (name, d, dlat) in [
            ("Vége", 2900.0, 0.0),
            ("Fele", 1500.0, 0.0002),
            ("", 500.0, 0.0),
            ("Messze", 1000.0, 0.05),
        ] {
            let (lat, lon) = at(d, dlat);
            data.waypoints.push(crate::gpx::RawWaypoint {
                name: name.into(),
                lat,
                lon,
                ele: None,
            });
        }
        let r = build_route(&data, "t", RouteSource::GpxFile).unwrap();
        assert_eq!(r.waypoints.len(), 3, "the far-away one is dropped");
        assert_eq!(r.waypoints[0].name, "Útpont 3");
        assert!((r.waypoints[0].dist_on_route - 500.0).abs() < 6.0);
        assert!((r.waypoints[1].dist_on_route - 1500.0).abs() < 6.0);
        assert_eq!(r.waypoints[2].name, "Vége");
    }

    /// Raw points 10 m apart, heading east, one timestamp per second (10 m/s).
    fn timed_line(points: usize, times: Option<f64>) -> Vec<RawPoint> {
        let m_per_deg_lon = 111_320.0 * 47.5f64.to_radians().cos();
        (0..points)
            .map(|i| RawPoint {
                lat: 47.5,
                lon: 19.0 + i as f64 * 10.0 / m_per_deg_lon,
                ele: Some(100.0),
                time: times.map(|start| start + i as f64),
            })
            .collect()
    }

    #[test]
    fn the_gpx_timeline_becomes_the_route_timeline() {
        // 1 km, one point per second at 10 m/s: the file's own 100 s
        let route = build_route(
            &gpx_data(timed_line(101, Some(1_700_000_000.0))),
            "t",
            RouteSource::GpxFile,
        )
        .unwrap();
        assert_eq!(route.timeline.len(), route.points.len());
        assert_eq!(route.timeline[0], 0.0);
        assert!(
            route.timeline.windows(2).all(|pair| pair[0] <= pair[1]),
            "the timeline never goes backwards"
        );
        let end = *route.timeline.last().unwrap();
        assert!((end - 100.0).abs() < 1.0, "the file says 100 s: {end}");
        // the timeline follows the resampled geometry
        assert!((end - route.total_dist / 10.0).abs() < 2.0);
    }

    #[test]
    fn a_missing_or_absurd_timeline_is_dropped() {
        // no `<time>` at all (a BRouter route): no timeline
        let plain =
            build_route(&gpx_data(timed_line(101, None)), "t", RouteSource::GpxFile).unwrap();
        assert!(plain.timeline.is_empty());

        // one point without a time: the whole timeline is unusable
        let mut partial = timed_line(101, Some(1_700_000_000.0));
        partial[50].time = None;
        let route = build_route(&gpx_data(partial), "t", RouteSource::GpxFile).unwrap();
        assert!(route.timeline.is_empty());

        // timestamps spanning days: a broken recording, not a ride
        let mut days = timed_line(101, Some(1_700_000_000.0));
        for (i, point) in days.iter_mut().enumerate() {
            point.time = Some(1_700_000_000.0 + i as f64 * 3600.0);
        }
        let route = build_route(&gpx_data(days), "t", RouteSource::GpxFile).unwrap();
        assert!(route.timeline.is_empty());

        // a timestamp that rewinds is clamped instead of breaking the ghost
        let mut rewound = timed_line(101, Some(1_700_000_000.0));
        rewound[50].time = Some(1_699_999_990.0);
        let route = build_route(&gpx_data(rewound), "t", RouteSource::GpxFile).unwrap();
        assert_eq!(route.timeline.len(), route.points.len());
        assert!(route.timeline.windows(2).all(|pair| pair[0] <= pair[1]));
    }
}
