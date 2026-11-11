//! Puts a recorded track on a stored route.
//!
//! Two GPS recordings of the same road are never identical, so an imported activity is not found
//! by its geometry hash. Instead every sample is projected onto the route polyline and gets the
//! distance along the route of its nearest point. From then on the ride lives on the route's own
//! distance axis, like a ride made in the app.
//!
//! The search is *monotone*: after the first sample the projection only looks a little behind
//! ([`MAX_BACKTRACK_M`]) and as far ahead as the time since the last match allows. That is what
//! keeps an out-and-back or a lollipop route straight: the way back lies on the way out, but the
//! rider is already past the turn. A sample farther than [`TOLERANCE_M`] from the route is
//! *off-route* (a detour, a GPS jump) and gets no distance.

use ride_model::Route;

use crate::activity::ActivitySample;
use crate::geo;
use crate::{err, Result};
use ride_i18n::{t, tf};

/// A sample this close to the route counts as on it, metres.
pub const TOLERANCE_M: f64 = 30.0;
/// How far back along the route the next sample may land (GPS jitter, a rider rolling back).
pub const MAX_BACKTRACK_M: f64 = 50.0;
/// The matched part of the track must span at least this much of the route, metres.
pub const MIN_SPAN_M: f64 = 500.0;
/// Below this share of the route the match is reported as partial (the caller shows a warning).
pub const FULL_COVERAGE: f64 = 0.9;
/// Fastest plausible bike over a short gap, m/s: bounds how far ahead the search looks.
const MAX_SPEED_MS: f64 = 30.0;
/// Extra look-ahead on top of the speed bound, metres.
const AHEAD_MARGIN_M: f64 = 300.0;
/// On the first sample, among nearly equally close candidates the earliest along the route wins
/// (a loop starts where it ends), metres.
const FIRST_TIE_M: f64 = 10.0;

/// The outcome of projecting an activity onto a route.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    /// Per sample of the activity: the distance along the route, `None` when it was off-route.
    /// The `Some` values never decrease.
    pub dist: Vec<Option<f64>>,
    /// Samples farther than [`TOLERANCE_M`] from the route.
    pub off_route: usize,
    pub start_dist: f64,
    pub end_dist: f64,
    /// Matched span as a share of the route length, `0..=1`.
    pub coverage: f64,
    /// Median distance of the on-route samples from the route line, metres.
    pub median_offset_m: f64,
}

impl Matched {
    /// The track covers less than [`FULL_COVERAGE`] of the route.
    pub fn is_partial(&self) -> bool {
        self.coverage < FULL_COVERAGE
    }

    pub fn matched(&self) -> usize {
        self.dist.iter().flatten().count()
    }
}

/// Nearest point of the polyline segments `from..to` (point indices) to `(lat, lon)`:
/// `(distance along the route, offset in metres)`.
fn nearest(route: &Route, lat: f64, lon: f64, from: usize, to: usize) -> Option<(f64, f64)> {
    let points = &route.points;
    let kx = 111_320.0 * lat.to_radians().cos();
    let ky = 110_540.0;
    let xy = |i: usize| ((points[i].lon - lon) * kx, (points[i].lat - lat) * ky);
    let last = to.min(points.len().saturating_sub(1));
    let mut best: Option<(f64, f64)> = None;
    for i in from..last {
        let (ax, ay) = xy(i);
        let (bx, by) = xy(i + 1);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 1e-12 {
            (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let off = (ax + t * dx).hypot(ay + t * dy);
        if best.is_none_or(|(_, o)| off < o) {
            let d = points[i].dist + (points[i + 1].dist - points[i].dist) * t;
            best = Some((d, off));
        }
    }
    best
}

/// First sample: search the whole route; among the candidates that are about as close as the
/// closest, take the one earliest along the route.
fn first_match(route: &Route, lat: f64, lon: f64) -> Option<(f64, f64)> {
    let n = route.points.len();
    let best = nearest(route, lat, lon, 0, n.saturating_sub(1))?;
    // walk again for the earliest near-tie (cheap: once per activity)
    let mut chunk_start = 0;
    while chunk_start + 1 < n {
        let chunk_end = (chunk_start + 64).min(n - 1);
        if let Some((d, off)) = nearest(route, lat, lon, chunk_start, chunk_end) {
            if off <= best.1 + FIRST_TIE_M && off <= TOLERANCE_M {
                return Some((d, off));
            }
        }
        chunk_start = chunk_end;
    }
    Some(best)
}

/// Projects every sample onto `route`.
///
/// An error (Hungarian, shown to the user) when the track hardly touches the route: fewer than
/// half of the samples on it, or a matched span under [`MIN_SPAN_M`].
pub fn match_to_route(samples: &[ActivitySample], route: &Route) -> Result<Matched> {
    if route.points.len() < 2 || !(route.total_dist.is_finite() && route.total_dist > 0.0) {
        return err(t(
            "route.matching.a_kivalasztott_utvonalnak_nincs_hossza_erre_nem_lehe",
        ));
    }
    let mut dist: Vec<Option<f64>> = Vec::with_capacity(samples.len());
    let mut offsets: Vec<f64> = Vec::new();
    let mut last: Option<(f64, f64)> = None; // (distance, sample time) of the last match
    let mut off_route = 0usize;
    // Before the first match every sample needs a scan of the whole route. A failed scan says
    // how far the route is (`off`): a sample that moved less than `off - TOLERANCE_M` from
    // there cannot be on it, so no new scan is needed until the rider got that far. Without
    // this a file of another route costs samples × route points.
    let mut missed: Option<(f64, f64, f64)> = None; // (lat, lon, offset) of the last failed scan
    let most_off = samples.len() - samples.len() / 2;

    for sample in samples {
        if off_route > most_off {
            // fewer than half of the samples can match now: the verdict is already known
            dist.push(None);
            off_route += 1;
            continue;
        }
        let found = match last {
            None => {
                let pointless = missed.is_some_and(|(lat, lon, off)| {
                    // 1 % and a metre of slack for the flat projection of `nearest`
                    geo::haversine_m(lat, lon, sample.lat, sample.lon)
                        < off * 0.99 - TOLERANCE_M - 1.0
                });
                if pointless {
                    None
                } else {
                    let found = first_match(route, sample.lat, sample.lon);
                    if let Some((_, off)) = found.filter(|&(_, off)| off > TOLERANCE_M) {
                        missed = Some((sample.lat, sample.lon, off));
                    }
                    found
                }
            }
            Some((last_dist, last_time)) => {
                let gap = (sample.time - last_time).max(0.0);
                let lo_d = (last_dist - MAX_BACKTRACK_M).max(0.0);
                let hi_d = last_dist + MAX_SPEED_MS * gap + AHEAD_MARGIN_M;
                let from = route.index_at(lo_d);
                let to = (route.index_at(hi_d) + 1).min(route.points.len() - 1);
                nearest(route, sample.lat, sample.lon, from, to)
            }
        };
        match found.filter(|&(_, off)| off <= TOLERANCE_M) {
            Some((d, off)) => {
                // jitter backwards is flattened: the distance never decreases
                let d = last.map_or(d, |(prev, _)| d.max(prev));
                last = Some((d, sample.time));
                dist.push(Some(d));
                offsets.push(off);
            }
            None => {
                off_route += 1;
                dist.push(None);
            }
        }
    }

    let matched = dist.iter().flatten().count();
    if matched == 0 || (matched as f64) < samples.len() as f64 * 0.5 {
        let share = if samples.is_empty() {
            0
        } else {
            (100.0 * matched as f64 / samples.len() as f64).round() as u32
        };
        return err(tf(
            "route.matching.az_edzes_nyoma_nem_illik_az_utvonalra_a_pontoknak_cs",
            &[
                ("share", &share.to_string()),
                ("tolerance", &(TOLERANCE_M as u32).to_string()),
            ],
        ));
    }
    let start_dist = dist.iter().flatten().copied().next().unwrap_or(0.0);
    let end_dist = dist.iter().flatten().copied().last().unwrap_or(0.0);
    let span = end_dist - start_dist;
    if span < MIN_SPAN_M {
        return err(tf(
            "route.matching.az_edzes_csak_span_0_m_en_fut_az_utvonalon_ez_keves_",
            &[
                ("span", &ride_i18n::decimal(span, 0)),
                ("min", &ride_i18n::decimal(MIN_SPAN_M, 0)),
            ],
        ));
    }
    offsets.sort_by(f64::total_cmp);
    Ok(Matched {
        dist,
        off_route,
        start_dist,
        end_dist,
        coverage: (span / route.total_dist).clamp(0.0, 1.0),
        median_offset_m: offsets[offsets.len() / 2],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::RoutePoint;

    const M_PER_DEG_LON: f64 = 111_320.0 * 0.676_204_6; // cos 47.5°

    fn sample(t: f64, lat: f64, lon: f64) -> ActivitySample {
        ActivitySample {
            time: 1_700_000_000.0 + t,
            lat,
            lon,
            ele: None,
            hr: None,
            cadence: None,
            power: None,
        }
    }

    /// A rider at 8 m/s along `lon = 19 + x / M_PER_DEG_LON`, `dy_m` metres north of the line.
    fn eastbound(len_m: f64, dy_m: f64) -> Vec<ActivitySample> {
        (0..)
            .map(|i| i as f64)
            .take_while(|t| t * 8.0 <= len_m)
            .map(|t| sample(t, 47.5 + dy_m / 110_540.0, 19.0 + t * 8.0 / M_PER_DEG_LON))
            .collect()
    }

    #[test]
    fn a_track_beside_the_route_gets_its_distances() {
        let route = straight_route(&[(3_000.0, 1.0)], 100.0);
        let m = match_to_route(&eastbound(3_000.0, 12.0), &route).unwrap();
        assert_eq!(m.off_route, 0);
        assert!(m.start_dist < 5.0, "{}", m.start_dist);
        assert!((m.end_dist - 3_000.0).abs() < 10.0, "{}", m.end_dist);
        assert!(m.coverage > 0.95 && !m.is_partial(), "{}", m.coverage);
        assert!(
            (m.median_offset_m - 12.0).abs() < 1.0,
            "{}",
            m.median_offset_m
        );
        // 8 m/s: sample 100 is at 800 m
        assert!(
            (m.dist[100].unwrap() - 800.0).abs() < 3.0,
            "{:?}",
            m.dist[100]
        );
        let flat = m.dist.iter().flatten().copied().collect::<Vec<_>>();
        assert!(flat.windows(2).all(|w| w[1] >= w[0]), "monotone");
    }

    #[test]
    fn a_detour_is_off_route_and_the_search_finds_the_route_again() {
        let route = straight_route(&[(4_000.0, 0.0)], 100.0);
        let mut samples = eastbound(4_000.0, 0.0);
        // a 40 s excursion 200 m to the side, then back on the line further along
        for s in &mut samples[200..240] {
            s.lat += 200.0 / 110_540.0;
        }
        let m = match_to_route(&samples, &route).unwrap();
        assert_eq!(m.off_route, 40);
        assert!(m.dist[200].is_none() && m.dist[239].is_none());
        assert!(
            (m.dist[260].unwrap() - 2_080.0).abs() < 5.0,
            "{:?}",
            m.dist[260]
        );
        assert!((m.end_dist - 4_000.0).abs() < 10.0);
    }

    #[test]
    fn a_partial_ride_is_matched_and_flagged() {
        let route = straight_route(&[(5_000.0, 0.0)], 100.0);
        let samples: Vec<_> = eastbound(5_000.0, 5.0)
            .into_iter()
            .skip(125) // starts 1 000 m in
            .take(150) // and stops 1 200 m later
            .collect();
        let m = match_to_route(&samples, &route).unwrap();
        assert!((m.start_dist - 1_000.0).abs() < 10.0, "{}", m.start_dist);
        assert!(m.is_partial());
        assert!((m.coverage - 0.238).abs() < 0.02, "{}", m.coverage);
    }

    #[test]
    fn an_out_and_back_route_keeps_the_way_back_apart() {
        // 1 500 m out and the same 1 500 m back along the same line
        let mut route = straight_route(&[(1_500.0, 0.0)], 100.0);
        let out: Vec<RoutePoint> = route.points.clone();
        for p in out.iter().rev().skip(1) {
            route.points.push(RoutePoint {
                dist: 3_000.0 - p.dist,
                ..*p
            });
        }
        route.total_dist = 3_000.0;
        let mut samples = eastbound(1_500.0, 3.0);
        let turn = samples.len() as f64;
        for i in 0..188 {
            let x = 1_500.0 - (i as f64 + 1.0) * 8.0;
            samples.push(sample(
                turn + i as f64,
                47.5,
                19.0 + x.max(0.0) / M_PER_DEG_LON,
            ));
        }
        let m = match_to_route(&samples, &route).unwrap();
        let out_leg = m.dist[100].unwrap();
        let back_leg = m.dist[samples.len() - 20].unwrap();
        assert!((out_leg - 800.0).abs() < 5.0, "{out_leg}");
        assert!(
            back_leg > 2_500.0,
            "the way back is the second half: {back_leg}"
        );
        assert!(m.end_dist > 2_900.0, "{}", m.end_dist);
    }

    #[test]
    fn a_track_elsewhere_or_a_stub_is_refused_in_hungarian() {
        let route = straight_route(&[(3_000.0, 0.0)], 100.0);
        let far = eastbound(3_000.0, 500.0);
        let e = match_to_route(&far, &route).unwrap_err().to_string();
        assert!(e.contains("nem illik az útvonalra"), "{e}");
        let stub: Vec<_> = eastbound(3_000.0, 0.0).into_iter().take(40).collect(); // 312 m
        let e = match_to_route(&stub, &route).unwrap_err().to_string();
        assert!(e.contains("kevés"), "{e}");
        assert!(match_to_route(&[], &route).is_err());
    }
}
