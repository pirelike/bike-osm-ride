//! The numbers behind the review page, without any UI: which metric is drawn, the lines of the
//! rides on the distance axis, the SVG markup of the chart, and the rows of the section table.
//!
//! Every ride is a [`DistSeries`] (10 m grid along the route), so overlaying rides is just
//! drawing their values over the same x axis. Nothing here panics on missing sensors: a ride
//! without heart rate simply has no line in the heart-rate chart.

use std::fmt::Write as _;
use std::sync::Arc;

use ride_i18n::t;
use ride_model::{RideRecord, Route};
use ride_sim::compare::{delta_time, section_stats, DistSeries, SectionStats};
use ride_ui_kit::format::{fmt_delta_s, fmt_opt0, fmt_speed_kmh};
use ride_ui_profile::geom;

/// Rides compared at once: the number of series colours.
pub const SLOTS: usize = 4;

/// A line has at most this many points on screen; more is averaged into buckets.
pub const MAX_LINE_POINTS: usize = 500;

/// Dash pattern of the `slot`-th ride. The first (the reference) is solid; the others differ in
/// the pattern as well as in the colour.
pub fn dash(slot: usize) -> &'static str {
    ["", "7 4", "2 4", "10 4 2 4"][slot % SLOTS]
}

/// What the chart shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Power,
    HeartRate,
    Cadence,
    Speed,
    /// Seconds gained or lost against the first ride along the way.
    Delta,
}

impl Metric {
    pub const ALL: [Metric; 5] = [
        Metric::Power,
        Metric::HeartRate,
        Metric::Cadence,
        Metric::Speed,
        Metric::Delta,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Metric::Power => "power",
            Metric::HeartRate => "hr",
            Metric::Cadence => "cadence",
            Metric::Speed => "speed",
            Metric::Delta => "delta",
        }
    }

    pub fn from_key(key: &str) -> Option<Metric> {
        Metric::ALL.into_iter().find(|m| m.key() == key)
    }

    pub fn title(self) -> &'static str {
        match self {
            Metric::Power => t("ui_review.chart.teljesitmeny"),
            Metric::HeartRate => t("ui_review.chart.pulzus"),
            Metric::Cadence => t("ui_review.chart.kadencia"),
            Metric::Speed => t("ui_review.chart.sebesseg"),
            Metric::Delta => t("ui_review.chart.idokulonbseg"),
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Metric::Power => "W",
            Metric::HeartRate => "bpm",
            Metric::Cadence => "rpm",
            Metric::Speed => "km/h",
            Metric::Delta => "s",
        }
    }

    /// The metric's button on a phone: its unit, which reads the same in every language and fits
    /// five buttons into one row (the time difference is `±s`, a difference in seconds).
    pub fn short_label(self) -> &'static str {
        match self {
            Metric::Delta => "±s",
            other => other.unit(),
        }
    }

    /// The smallest span the y axis may have (a flat heart-rate line must not fill the plot).
    fn min_span(self) -> f64 {
        match self {
            Metric::Power => 100.0,
            Metric::HeartRate => 30.0,
            Metric::Cadence => 30.0,
            Metric::Speed => 10.0,
            Metric::Delta => 10.0,
        }
    }

    /// Text of one value on the axis or in the read-out.
    pub fn value_text(self, value: f64) -> String {
        match self {
            Metric::Delta => fmt_delta_s(value),
            Metric::Speed => fmt_speed_kmh(value / 3.6),
            _ => fmt_opt0(Some(value)),
        }
    }

    /// The value of ride `series` at grid point `i`.
    fn raw(self, series: &DistSeries, i: usize) -> Option<f64> {
        match self {
            Metric::Power => series.power_w.get(i).copied().flatten(),
            Metric::HeartRate => series.hr_bpm.get(i).copied().flatten(),
            Metric::Cadence => series.cadence_rpm.get(i).copied().flatten(),
            Metric::Speed => series
                .speed_ms
                .get(i)
                .copied()
                .filter(|v| v.is_finite())
                .map(|v| v * 3.6),
            Metric::Delta => None,
        }
    }
}

/// One ride's line: `(distance, value)` with `None` where the sensor has nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub slot: usize,
    pub points: Vec<(f64, Option<f64>)>,
}

/// The lines of `rides` for `metric` between `d0` and `d1` (metres), averaged down to
/// [`MAX_LINE_POINTS`]. `Delta` is measured against ride 0, whose own line is the zero rule and
/// is left out. Rides without any value in the window give no line.
pub fn lines(rides: &[Arc<DistSeries>], metric: Metric, d0: f64, d1: f64) -> Vec<Line> {
    let mut out = Vec::new();
    for (slot, series) in rides.iter().enumerate().take(SLOTS) {
        if series.is_empty() {
            continue;
        }
        let values: Vec<Option<f64>> = if metric == Metric::Delta {
            if slot == 0 {
                continue;
            }
            match rides.first() {
                Some(reference) => delta_time(series, reference),
                None => continue,
            }
        } else {
            (0..series.len()).map(|i| metric.raw(series, i)).collect()
        };
        let lo = series.index_of(d0.max(series.start_dist)).unwrap_or(0);
        let hi = series
            .index_of(d1.min(series.end_dist()))
            .unwrap_or(series.len() - 1);
        if hi < lo {
            continue;
        }
        let window: Vec<(f64, Option<f64>)> = (lo..=hi)
            .map(|i| (series.dist_at(i), values.get(i).copied().flatten()))
            .collect();
        if window.iter().all(|(_, v)| v.is_none()) {
            continue;
        }
        out.push(Line {
            slot,
            points: bucket(&window, MAX_LINE_POINTS),
        });
    }
    out
}

/// Averages `points` into at most `max` buckets (a bucket with no value stays a gap).
fn bucket(points: &[(f64, Option<f64>)], max: usize) -> Vec<(f64, Option<f64>)> {
    if points.len() <= max || max == 0 {
        return points.to_vec();
    }
    let stride = points.len().div_ceil(max);
    points
        .chunks(stride)
        .map(|chunk| {
            let dist = chunk.iter().map(|(d, _)| d).sum::<f64>() / chunk.len() as f64;
            let (sum, n) = chunk
                .iter()
                .filter_map(|(_, v)| *v)
                .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
            (dist, (n > 0).then(|| sum / n as f64))
        })
        .collect()
}

/// The y axis for the drawn lines: a rounded range that holds every value.
pub fn y_bounds(metric: Metric, lines: &[Line]) -> (f64, f64) {
    let values = lines
        .iter()
        .flat_map(|l| l.points.iter().filter_map(|(_, v)| *v))
        .filter(|v| v.is_finite());
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in values {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        return match metric {
            Metric::Delta => (-10.0, 10.0),
            _ => (0.0, metric.min_span()),
        };
    }
    let (mut lo, mut hi) = match metric {
        Metric::Power | Metric::Cadence | Metric::Speed => (0.0, hi * 1.05),
        Metric::HeartRate => (lo - 5.0, hi + 5.0),
        Metric::Delta => {
            let m = lo.abs().max(hi.abs()).max(2.0) * 1.1;
            (-m, m)
        }
    };
    if hi - lo < metric.min_span() {
        let mid = 0.5 * (lo + hi);
        lo = if matches!(metric, Metric::HeartRate | Metric::Delta) {
            mid - metric.min_span() / 2.0
        } else {
            0.0
        };
        hi = lo + metric.min_span();
    }
    let step = geom::nice_step(hi - lo, 4);
    ((lo / step).floor() * step, (hi / step).ceil() * step)
}

/// What the chart is drawn from, ready for the component.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartGeometry {
    /// SVG body in (distance, value) space; draw it inside the group transform of
    /// [`ride_ui_profile::geom::view_transform`] for `[x0, x1] × [y0, y1]`.
    pub markup: String,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    /// Horizontal rules, from the bottom.
    pub ticks: Vec<f64>,
    /// Number of lines drawn.
    pub drawn: usize,
}

pub fn chart_geometry(metric: Metric, lines: &[Line], x0: f64, x1: f64) -> ChartGeometry {
    let (y0, y1) = y_bounds(metric, lines);
    let mut markup = String::new();
    if metric == Metric::Delta {
        // the reference ride is the zero rule
        let _ = write!(
            markup,
            "<line x1=\"{x0:.2}\" x2=\"{x1:.2}\" y1=\"0\" y2=\"0\" stroke=\"var(--series-1)\" stroke-width=\"2\" vector-effect=\"non-scaling-stroke\"></line>"
        );
    }
    let mut drawn = 0;
    // later rides first, so the reference stays on top
    for line in lines.iter().rev() {
        let mut run: Vec<(f64, f64)> = Vec::new();
        let flush = |markup: &mut String, run: &mut Vec<(f64, f64)>| {
            if run.len() >= 2 {
                let _ = write!(markup, "<polyline points=\"");
                for (x, y) in run.iter() {
                    let _ = write!(markup, "{x:.1},{y:.2} ");
                }
                let _ = write!(
                    markup,
                    "\" fill=\"none\" stroke=\"var(--series-{})\" stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\" vector-effect=\"non-scaling-stroke\"",
                    line.slot % SLOTS + 1
                );
                let pattern = dash(line.slot);
                if !pattern.is_empty() {
                    let _ = write!(markup, " stroke-dasharray=\"{pattern}\"");
                }
                markup.push_str("></polyline>");
            }
            run.clear();
        };
        for (dist, value) in &line.points {
            match value.filter(|v| v.is_finite()) {
                Some(v) if *dist >= x0 - 1.0 && *dist <= x1 + 1.0 => run.push((*dist, v)),
                _ => flush(&mut markup, &mut run),
            }
        }
        flush(&mut markup, &mut run);
        drawn += 1;
    }
    ChartGeometry {
        markup,
        x0,
        x1,
        y0,
        y1,
        ticks: geom::grid_values_n(y0, y1, 4),
        drawn,
    }
}

/// The value of ride `series` at `dist` for the read-out under the cursor.
pub fn value_at(rides: &[Arc<DistSeries>], slot: usize, metric: Metric, dist: f64) -> Option<f64> {
    let series = rides.get(slot)?;
    let i = series.index_of(dist)?;
    if metric == Metric::Delta {
        if slot == 0 {
            return Some(0.0);
        }
        return delta_time(series, rides.first()?).get(i).copied().flatten();
    }
    metric.raw(series, i)
}

/// What the legend shows for ride `slot` while no cursor is down: the mean of the metric over
/// `[d0, d1]`, or for [`Metric::Delta`] the difference at the end of the window (what the ride
/// gained or lost over the section). `None` when the ride has no value there.
pub fn window_value(
    rides: &[Arc<DistSeries>],
    slot: usize,
    metric: Metric,
    d0: f64,
    d1: f64,
) -> Option<f64> {
    let series = rides.get(slot)?;
    if series.is_empty() || !(d0.is_finite() && d1.is_finite()) || d1 <= d0 {
        return None;
    }
    let lo = series.index_of(d0.max(series.start_dist))?;
    let hi = series.index_of(d1.min(series.end_dist()))?;
    if hi < lo {
        return None;
    }
    if metric == Metric::Delta {
        if slot == 0 {
            return Some(0.0);
        }
        let delta = delta_time(series, rides.first()?);
        return (lo..=hi)
            .rev()
            .find_map(|i| delta.get(i).copied().flatten());
    }
    let (sum, n) = (lo..=hi)
        .filter_map(|i| metric.raw(series, i))
        .filter(|v| v.is_finite())
        .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

/// The stretch every selected ride covers, `None` when they share less than 100 m.
pub fn common_span(rides: &[Arc<DistSeries>]) -> Option<(f64, f64)> {
    let start = rides
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.start_dist)
        .reduce(f64::max)?;
    let end = rides
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.end_dist())
        .reduce(f64::min)?;
    (end - start >= 100.0).then_some((start, end))
}

/// One line of the section table.
#[derive(Clone, Debug, PartialEq)]
pub struct StatsRow {
    pub slot: usize,
    pub stats: Option<SectionStats>,
    /// Seconds slower (+) or faster (−) than the first ride on this section.
    pub time_delta_s: Option<f64>,
}

/// The table: every ride's statistics on `[d0, d1]`, the time difference against the first ride
/// that has any.
pub fn stats_rows(records: &[Arc<RideRecord>], d0: f64, d1: f64) -> Vec<StatsRow> {
    let mut rows: Vec<StatsRow> = records
        .iter()
        .take(SLOTS)
        .enumerate()
        .map(|(slot, record)| StatsRow {
            slot,
            stats: section_stats(&record.points, d0, d1),
            time_delta_s: None,
        })
        .collect();
    let reference = rows.iter().find_map(|r| r.stats.map(|s| s.time_s));
    if let Some(reference) = reference {
        for row in &mut rows {
            row.time_delta_s = row.stats.map(|s| s.time_s - reference);
        }
    }
    rows
}

/// Index range of the climbs worth a quick-pick chip: the route's climbs, longest first, at most
/// `max`, returned in route order.
pub fn climb_ranges(route: &Route, max: usize) -> Vec<(usize, f64, f64)> {
    let mut all: Vec<(usize, f64, f64)> = route
        .climbs
        .iter()
        .enumerate()
        .filter(|(_, c)| c.end_dist > c.start_dist && c.start_dist.is_finite())
        .map(|(i, c)| (i, c.start_dist, c.end_dist))
        .collect();
    all.sort_by(|a, b| (b.2 - b.1).total_cmp(&(a.2 - a.1)));
    all.truncate(max);
    all.sort_by(|a, b| a.1.total_cmp(&b.1));
    all
}

/// A number with the language's decimal separator and no trailing zeros of the fraction:
/// `75`, `9,5`, `0,004` (Hungarian).
pub fn hu(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    ride_i18n::decimal_trimmed(value, decimals)
}

/// `2026-10-04 07:30` (UTC, like the summary dialog's date).
pub fn fmt_stamp(unix_s: f64) -> String {
    if !unix_s.is_finite() || unix_s.abs() > 4.0e9 {
        return "—".into();
    }
    let secs = unix_s.floor() as i64;
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RecordPoint, RideSource, RideSummary};
    use ride_sim::compare::GRID_M;

    fn points(speed: f64, len: f64, watts: Option<f64>) -> Vec<RecordPoint> {
        (0..)
            .map(|i| i as f64)
            .take_while(|t| t * speed <= len)
            .map(|t| RecordPoint {
                t,
                unix_s: 1_700_000_000.0 + t,
                dist: t * speed,
                lat: 47.5,
                lon: 19.0 + t * speed / 75_000.0,
                ele: 100.0 + t * speed * 0.04,
                speed_ms: speed,
                power_w: watts.map(|w| w + (t % 10.0)),
                cadence_rpm: Some(85.0),
                hr_bpm: Some(140.0 + t / 20.0),
                grade: 4.0,
            })
            .collect()
    }

    fn record(speed: f64, len: f64, watts: Option<f64>) -> Arc<RideRecord> {
        Arc::new(RideRecord {
            version: 1,
            route_id: "r".into(),
            route_name: "Kör".into(),
            simulated: false,
            summary: RideSummary {
                route_name: "Kör".into(),
                route_id: "r".into(),
                started_unix_s: 1_700_000_000.0,
                elapsed_s: len / speed,
                dist: len,
                ascent: 0.0,
                avg_speed_ms: speed,
                max_speed_ms: speed,
                avg_power_w: watts,
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
            points: points(speed, len, watts),
            source: RideSource::Recorded,
            power_estimated: false,
            estimate: None,
        })
    }

    fn series(records: &[Arc<RideRecord>]) -> Vec<Arc<DistSeries>> {
        records
            .iter()
            .map(|r| Arc::new(DistSeries::from_points(&r.points, GRID_M).unwrap()))
            .collect()
    }

    #[test]
    fn metrics_round_trip_through_their_keys() {
        for m in Metric::ALL {
            assert_eq!(Metric::from_key(m.key()), Some(m));
            assert!(!m.title().is_empty() && !m.unit().is_empty());
        }
        assert_eq!(Metric::from_key("nincs"), None);
        assert_eq!(dash(0), "");
        assert_ne!(dash(1), dash(2));
    }

    #[test]
    fn lines_follow_the_window_and_skip_rides_without_the_sensor() {
        let rides = series(&[
            record(8.0, 4_000.0, Some(200.0)),
            record(10.0, 4_000.0, None),
        ]);
        let power = lines(&rides, Metric::Power, 1_000.0, 2_000.0);
        assert_eq!(power.len(), 1, "the second ride has no power");
        assert_eq!(power[0].slot, 0);
        let (first, last) = (power[0].points[0].0, power[0].points.last().unwrap().0);
        assert!((first - 1_000.0).abs() <= GRID_M && (last - 2_000.0).abs() <= GRID_M);
        // speed has both, in km/h
        let speed = lines(&rides, Metric::Speed, 0.0, 4_000.0);
        assert_eq!(speed.len(), 2);
        assert!((speed[1].points[10].1.unwrap() - 36.0).abs() < 0.5);
        // the delta never draws the reference
        let delta = lines(&rides, Metric::Delta, 0.0, 4_000.0);
        assert_eq!(delta.iter().map(|l| l.slot).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn a_long_window_is_averaged_down() {
        let rides = series(&[record(10.0, 30_000.0, Some(200.0))]);
        let l = lines(&rides, Metric::Power, 0.0, 30_000.0);
        assert!(
            l[0].points.len() <= MAX_LINE_POINTS,
            "{}",
            l[0].points.len()
        );
        assert!(l[0].points.len() > 200);
    }

    #[test]
    fn the_axis_holds_the_data_and_the_markup_is_clean() {
        let rides = series(&[
            record(8.0, 4_000.0, Some(200.0)),
            record(10.0, 4_000.0, Some(260.0)),
        ]);
        for metric in Metric::ALL {
            let l = lines(&rides, metric, 0.0, 4_000.0);
            let g = chart_geometry(metric, &l, 0.0, 4_000.0);
            assert!(g.y1 > g.y0, "{metric:?}");
            for line in &l {
                for (_, v) in &line.points {
                    if let Some(v) = v {
                        assert!(*v >= g.y0 - 1e-6 && *v <= g.y1 + 1e-6, "{metric:?} {v}");
                    }
                }
            }
            assert!(
                !g.markup.contains("NaN") && !g.markup.contains("inf"),
                "{metric:?}"
            );
            assert!(!g.markup.contains("opacity"));
            assert!(g.markup.contains("vector-effect"));
        }
        let g = chart_geometry(
            Metric::Power,
            &lines(&rides, Metric::Power, 0.0, 4_000.0),
            0.0,
            4_000.0,
        );
        assert_eq!(g.drawn, 2);
        assert!(g.markup.contains("var(--series-1)") && g.markup.contains("var(--series-2)"));
        assert!(
            g.markup.contains("stroke-dasharray"),
            "the second ride is dashed"
        );
        // power starts at zero, delta is symmetric around it
        assert_eq!(g.y0, 0.0);
        let d = lines(&rides, Metric::Delta, 0.0, 4_000.0);
        let dg = chart_geometry(Metric::Delta, &d, 0.0, 4_000.0);
        assert!((dg.y0 + dg.y1).abs() < 1e-6 && dg.markup.contains("<line"));
    }

    #[test]
    fn an_empty_chart_still_has_an_axis() {
        for metric in Metric::ALL {
            let (lo, hi) = y_bounds(metric, &[]);
            assert!(hi > lo);
            let g = chart_geometry(metric, &[], 0.0, 1_000.0);
            assert_eq!(g.drawn, 0);
        }
    }

    #[test]
    fn the_read_out_under_the_cursor() {
        let rides = series(&[
            record(8.0, 4_000.0, Some(200.0)),
            record(10.0, 4_000.0, Some(260.0)),
        ]);
        assert!(value_at(&rides, 0, Metric::Power, 1_000.0).unwrap() >= 200.0);
        assert_eq!(value_at(&rides, 0, Metric::Delta, 1_000.0), Some(0.0));
        let behind = value_at(&rides, 1, Metric::Delta, 2_000.0).unwrap();
        assert!(
            (behind + 50.0).abs() < 1.0,
            "the faster ride is 50 s ahead: {behind}"
        );
        assert_eq!(value_at(&rides, 0, Metric::Power, 99_000.0), None);
        assert_eq!(value_at(&rides, 3, Metric::Power, 1_000.0), None);
    }

    #[test]
    fn the_common_span_and_the_table() {
        let records = [
            record(8.0, 4_000.0, Some(200.0)),
            record(10.0, 3_000.0, Some(260.0)),
        ];
        let rides = series(&records);
        let (a, b) = common_span(&rides).unwrap();
        assert!(a < 10.0 && (b - 3_000.0).abs() < 20.0, "{a} {b}");
        assert_eq!(common_span(&[]), None);

        let rows = stats_rows(&records, 500.0, 2_500.0);
        assert_eq!(rows.len(), 2);
        let first = rows[0].stats.unwrap();
        assert!((first.time_s - 250.0).abs() < 1.0);
        assert_eq!(rows[0].time_delta_s, Some(0.0));
        assert!(
            (rows[1].time_delta_s.unwrap() + 50.0).abs() < 1.5,
            "{:?}",
            rows[1].time_delta_s
        );
        // a section one ride did not cover has no row data, the other still compares to itself
        let rows = stats_rows(&records, 500.0, 3_800.0);
        assert!(rows[1].stats.is_none() && rows[1].time_delta_s.is_none());
        assert_eq!(rows[0].time_delta_s, Some(0.0));
    }

    #[test]
    fn numbers_use_the_decimal_comma() {
        assert_eq!(hu(75.0, 1), "75");
        assert_eq!(hu(9.5, 1), "9,5");
        assert_eq!(hu(0.004, 4), "0,004");
        assert_eq!(hu(0.0095, 4), "0,0095");
        assert_eq!(hu(f64::NAN, 1), "—");
    }

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(fmt_stamp(0.0), "1970-01-01 00:00");
        assert_eq!(fmt_stamp(1_700_000_000.0), "2023-11-14 22:13");
        assert_eq!(fmt_stamp(f64::NAN), "—");
        assert_eq!(fmt_stamp(1.0e15), "—");
    }

    #[test]
    fn climbs_are_picked_longest_first_and_listed_in_route_order() {
        let mut route = ride_model::testing::straight_route(&[(5_000.0, 0.0)], 100.0);
        let climb = |s: f64, e: f64| ride_model::Climb {
            start_dist: s,
            end_dist: e,
            length: e - s,
            gain: 50.0,
            avg_grade: 5.0,
            max_grade: 8.0,
            start_ele: 100.0,
            end_ele: 150.0,
            category: None,
        };
        route.climbs = vec![
            climb(100.0, 400.0),
            climb(1_000.0, 2_500.0),
            climb(3_000.0, 3_800.0),
        ];
        let picked = climb_ranges(&route, 2);
        assert_eq!(picked.iter().map(|c| c.0).collect::<Vec<_>>(), [1, 2]);
    }
}
