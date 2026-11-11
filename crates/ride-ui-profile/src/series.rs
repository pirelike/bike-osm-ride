//! Pure helpers for the live charts: metric metadata, window slicing, moving average,
//! zone classification and the static SVG markup of one chart window.
//!
//! Same trick as the elevation profile: the per-run shapes are expressed in *data space*
//! (x = seconds, y = the metric's unit) and drawn through a single group transform, so the
//! markup is rebuilt at the recording rate (1 Hz) and only the transform changes in
//! between.

use std::fmt::Write as _;

use ride_model::{RecordPoint, RiderSettings, Theme};
use ride_ui_kit::format::{fmt_duration, fmt_opt0, fmt_speed_kmh};
use ride_ui_kit::palette;

use crate::geom;
use crate::ChartMetric;
use ride_i18n::{t, tf};

/// Two samples further apart than this belong to different runs (a sensor gap is not
/// bridged by a straight line), seconds. It is the limit for the engine's 1 Hz recording; a
/// recording that is sparser on the whole uses [`gap_limit`].
const GAP_S: f64 = 1.6;

/// The usual spacing of the samples in `t`: the median step, `1.0` when there is none.
fn typical_step(t: &[f64]) -> f64 {
    let mut steps: Vec<f64> = t
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    if steps.is_empty() {
        return 1.0;
    }
    steps.sort_by(f64::total_cmp);
    steps[steps.len() / 2]
}

/// Largest step that still joins two samples. A history recorded more sparsely than 1 Hz (a
/// slow device whose ride loop ticks less often, or the `RIDE_TIMESCALE` hook) would otherwise
/// fall apart into single points and draw no line at all; a gap stays a gap relative to the
/// recording's own rhythm.
fn gap_limit(t: &[f64]) -> f64 {
    GAP_S.max(typical_step(t) * GAP_S)
}

/// Radius of the moving average applied before drawing, in samples.
const SMOOTH_RADIUS: usize = 1;

// ───────────────────────────── metric metadata ─────────────────────────────

impl ChartMetric {
    /// Hungarian title of the chart.
    pub fn title(self) -> &'static str {
        match self {
            ChartMetric::Power => t("ui_profile.series.teljesitmeny"),
            ChartMetric::HeartRate => t("ui_profile.series.pulzus"),
            ChartMetric::Cadence => t("ui_profile.series.kadencia"),
            ChartMetric::Speed => t("ui_profile.series.sebesseg"),
        }
    }

    /// Unit shown next to the value.
    pub fn unit(self) -> &'static str {
        match self {
            ChartMetric::Power => "W",
            ChartMetric::HeartRate => "bpm",
            ChartMetric::Cadence => "rpm",
            ChartMetric::Speed => "km/h",
        }
    }

    /// Raw recorded value of the metric (speed stays in m/s, as recorded).
    pub fn value(self, p: &RecordPoint) -> Option<f64> {
        let v = match self {
            ChartMetric::Power => p.power_w,
            ChartMetric::HeartRate => p.hr_bpm,
            ChartMetric::Cadence => p.cadence_rpm,
            ChartMetric::Speed => Some(p.speed_ms),
        }?;
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }

    /// A value formatted for the read-out (speed is converted to km/h).
    pub fn value_text(self, value: Option<f64>) -> String {
        match (self, value) {
            (ChartMetric::Speed, Some(v)) => fmt_speed_kmh(v),
            (_, Some(v)) => fmt_opt0(Some(v)),
            (_, None) => "—".to_string(),
        }
    }

    /// Label of a y-axis grid line.
    pub fn axis_text(self, value: f64) -> String {
        match self {
            ChartMetric::Speed => fmt_speed_kmh(value),
            _ => fmt_opt0(Some(value)),
        }
    }

    /// Moving-average radius used before drawing.
    pub fn smoothing_radius(self) -> usize {
        SMOOTH_RADIUS
    }

    /// Power and heart rate carry a zone; cadence and speed do not.
    pub fn has_zone(self) -> bool {
        matches!(self, ChartMetric::Power | ChartMetric::HeartRate)
    }

    /// Prefix of the zone badge (`Z` for power, `H` for heart rate, empty otherwise).
    pub fn zone_prefix(self) -> &'static str {
        match self {
            ChartMetric::Power => "Z",
            ChartMetric::HeartRate => "H",
            ChartMetric::Cadence | ChartMetric::Speed => "",
        }
    }
}

// ───────────────────────────── zones ─────────────────────────────

/// Coggan power zone `1..=7` of an absolute power relative to FTP (`<55, 55-75, 76-90,
/// 91-105, 106-120, 121-150, >150 %`). `None` without power or with an unusable FTP.
pub fn power_zone(power_w: f64, ftp_w: f64) -> Option<u8> {
    if !power_w.is_finite() || !ftp_w.is_finite() || ftp_w <= 0.0 {
        return None;
    }
    let pct = power_w.max(0.0) / ftp_w * 100.0;
    Some(if pct < 55.0 {
        1
    } else if pct < 76.0 {
        2
    } else if pct < 91.0 {
        3
    } else if pct < 106.0 {
        4
    } else if pct < 121.0 {
        5
    } else if pct < 151.0 {
        6
    } else {
        7
    })
}

/// Heart-rate zone `1..=5` relative to the maximum (`<60, 60-70, 70-80, 80-90, >90 %`).
pub fn hr_zone(hr_bpm: f64, max_hr: f64) -> Option<u8> {
    if !hr_bpm.is_finite() || !max_hr.is_finite() || max_hr <= 0.0 {
        return None;
    }
    let pct = hr_bpm.max(0.0) / max_hr * 100.0;
    Some(if pct < 60.0 {
        1
    } else if pct < 70.0 {
        2
    } else if pct < 80.0 {
        3
    } else if pct < 90.0 {
        4
    } else {
        5
    })
}

/// Zone badge of a value: `(zone, Hungarian name, CSS variable)`.
pub fn zone_badge(
    metric: ChartMetric,
    value: Option<f64>,
    rider: &RiderSettings,
) -> Option<(u8, &'static str, &'static str)> {
    let value = value?;
    match metric {
        ChartMetric::Power => power_zone(value, rider.ftp_w)
            .map(|z| (z, palette::power_zone_name(z), palette::power_zone_var(z))),
        ChartMetric::HeartRate => hr_zone(value, rider.max_hr)
            .map(|z| (z, palette::hr_zone_name(z), palette::hr_zone_var(z))),
        ChartMetric::Cadence | ChartMetric::Speed => None,
    }
}

// ───────────────────────────── window & series ─────────────────────────────

/// The recorded samples of `[t_end - window_s, t_end]` (the history is append-only and
/// ordered by `t`). Empty for a non-finite window or an empty history.
pub fn window_slice(points: &[RecordPoint], t_end: f64, window_s: f64) -> &[RecordPoint] {
    if points.is_empty() || !t_end.is_finite() || !window_s.is_finite() || window_s <= 0.0 {
        return &[];
    }
    let end = points.partition_point(|p| p.t <= t_end);
    let start = points[..end].partition_point(|p| p.t < t_end - window_s);
    &points[start..end.max(start)]
}

/// Centred moving average over `radius` samples on each side, skipping missing values.
/// A sample without any neighbour stays `None` (a gap is never filled with a fake value).
pub fn moving_average(values: &[Option<f64>], radius: usize) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(values.len());
        let mut sum = 0.0;
        let mut count = 0usize;
        for value in values[lo..hi].iter().flatten() {
            if value.is_finite() {
                sum += value;
                count += 1;
            }
        }
        out.push(if count > 0 {
            Some(sum / count as f64)
        } else {
            None
        });
    }
    out
}

/// A chart window: sample times and (smoothed) values, both in recording order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    pub t: Vec<f64>,
    pub v: Vec<Option<f64>>,
}

/// How many of the newest (1 Hz) samples [`Series::last_value`] looks at: a value older than
/// that is a dropout, shown as "—".
pub const FRESH_SAMPLES: usize = 3;

impl Series {
    /// `true` when there is no sample to draw.
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }

    /// Number of samples that carry a value.
    pub fn value_count(&self) -> usize {
        self.v.iter().filter(|v| v.is_some()).count()
    }

    /// Last value of the window: the newest of the last [`FRESH_SAMPLES`] samples, `None` when
    /// they are all missing. A value from before a sensor dropout is not shown as live.
    pub fn last_value(&self) -> Option<f64> {
        self.v.iter().rev().take(FRESH_SAMPLES).find_map(|v| *v)
    }

    /// Value nearest to `t` (within half a sample interval), used by the hover read-out.
    pub fn value_at(&self, t: f64) -> Option<f64> {
        if self.t.is_empty() || !t.is_finite() {
            return None;
        }
        let idx = self.t.partition_point(|x| *x < t);
        let i = if idx >= self.t.len() {
            self.t.len() - 1
        } else if idx == 0 {
            0
        } else if (t - self.t[idx - 1]).abs() <= (self.t[idx] - t).abs() {
            idx - 1
        } else {
            idx
        };
        self.v.get(i).copied().flatten()
    }
}

/// Slices the window out of `history` and applies the metric's moving average.
pub fn series_for(history: &[RecordPoint], metric: ChartMetric, window_s: f64) -> Series {
    let t_end = match history.last() {
        Some(p) if p.t.is_finite() => p.t,
        _ => return Series::default(),
    };
    let slice = window_slice(history, t_end, window_s);
    if slice.is_empty() {
        return Series::default();
    }
    let t: Vec<f64> = slice.iter().map(|p| p.t).collect();
    let raw: Vec<Option<f64>> = slice.iter().map(|p| metric.value(p)).collect();
    let v = moving_average(&raw, metric.smoothing_radius());
    Series { t, v }
}

// ───────────────────────────── y range ─────────────────────────────

/// Padded value range of a chart window. Power, cadence and speed start at zero (honest
/// area charts); heart rate brackets the data so the shape stays readable. An empty window
/// falls back to a plausible default for the metric so the axis never collapses.
pub fn chart_y_range(metric: ChartMetric, values: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in values.iter().filter(|v| v.is_finite()) {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    if !lo.is_finite() || !hi.is_finite() || hi < lo {
        return match metric {
            ChartMetric::Power => (0.0, 300.0),
            ChartMetric::HeartRate => (80.0, 180.0),
            ChartMetric::Cadence => (0.0, 110.0),
            ChartMetric::Speed => (0.0, 12.0),
        };
    }
    let headroom = 1.12;
    match metric {
        ChartMetric::HeartRate => {
            let lo = (lo - 6.0).max(20.0);
            let hi = hi + 6.0;
            let step = geom::nice_step(hi - lo, 4);
            let lo = (lo / step).floor() * step;
            let hi = (hi / step).ceil() * step;
            (lo, (hi).max(lo + step))
        }
        _ => {
            let floor_value = match metric {
                ChartMetric::Power => 120.0,
                ChartMetric::Cadence => 70.0,
                ChartMetric::Speed => 6.0,
                ChartMetric::HeartRate => 100.0,
            };
            let hi = (hi.max(floor_value) * headroom).max(floor_value);
            let step = geom::nice_step(hi, 6);
            (0.0, (hi / step).ceil() * step)
        }
    }
}

// ───────────────────────────── chart geometry ─────────────────────────────

/// Serialised, static shapes of one chart window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartGeometry {
    /// Data space (seconds, metric units): one flat area (`--bg-3`), one 2 px ink line per
    /// unbroken run and, for power, the dashed FTP rule.
    pub markup: String,
    /// The zone strip along the bottom edge: `<rect>`s in *strip space* (x = seconds,
    /// y = `0..1`) for a `viewBox="t0 0 span 1"` SVG; empty for metrics without zones.
    pub strip: String,
    /// Left edge of the window, seconds (may be negative at the start of a ride).
    pub t0: f64,
    /// Right edge of the window, seconds.
    pub t1: f64,
    /// Lower bound of the value axis.
    pub y0: f64,
    /// Upper bound of the value axis.
    pub y1: f64,
    /// Reference value drawn as a dashed rule (FTP for power), inside `y0..=y1`.
    pub guide: Option<f64>,
    /// The window itself (for the hover read-out).
    pub series: Series,
}

impl ChartGeometry {
    /// `true` when there is nothing to draw yet.
    pub fn is_empty(&self) -> bool {
        self.series.value_count() < 2
    }

    /// Number of drawn runs (polyline pieces).
    pub fn runs(&self) -> usize {
        self.markup.matches("<polyline").count()
    }

    /// Number of zone cells in the strip.
    pub fn strip_cells(&self) -> usize {
        self.strip.matches("<rect").count()
    }
}

/// The reference value of a chart: the rider's FTP for power (`None` for the other metrics
/// and for an unusable FTP).
pub fn guide_value(metric: ChartMetric, rider: &RiderSettings) -> Option<f64> {
    match metric {
        ChartMetric::Power if rider.ftp_w.is_finite() && rider.ftp_w > 0.0 => Some(rider.ftp_w),
        _ => None,
    }
}

/// Zone `1..` of a value for the strip: power and heart-rate zones only.
fn strip_zone(metric: ChartMetric, value: f64, rider: &RiderSettings) -> Option<u8> {
    match metric {
        ChartMetric::Power => power_zone(value, rider.ftp_w),
        ChartMetric::HeartRate => hr_zone(value, rider.max_hr),
        ChartMetric::Cadence | ChartMetric::Speed => None,
    }
}

/// Zone colour of a strip cell (a hex colour of the theme).
fn strip_color(metric: ChartMetric, zone: u8, theme: Theme) -> String {
    match metric {
        ChartMetric::HeartRate => palette::hr_zone_color(zone, theme),
        _ => palette::power_zone_color(zone, theme),
    }
}

/// The zone strip of a window as `<rect>`s: consecutive samples of the same zone are merged
/// into one cell, a sensor gap leaves the strip empty. Each sample covers half a sample
/// interval on both sides.
pub fn zone_strip(
    series: &Series,
    metric: ChartMetric,
    rider: &RiderSettings,
    theme: Theme,
) -> String {
    if !metric.has_zone() {
        return String::new();
    }
    // (zone, start, end) in seconds
    let mut cells: Vec<(u8, f64, f64)> = Vec::new();
    let half = typical_step(&series.t) / 2.0;
    let gap = gap_limit(&series.t) - 2.0 * half;
    for (i, t) in series.t.iter().enumerate() {
        if !t.is_finite() {
            continue;
        }
        let Some(zone) = series
            .v
            .get(i)
            .copied()
            .flatten()
            .and_then(|v| strip_zone(metric, v, rider))
        else {
            continue;
        };
        let (start, end) = (t - half, t + half);
        match cells.last_mut() {
            Some(last) if last.0 == zone && start - last.2 <= gap => last.2 = end,
            _ => cells.push((zone, start, end)),
        }
    }
    let mut out = String::with_capacity(cells.len() * 64);
    for (zone, start, end) in cells {
        let _ = write!(
            out,
            "<rect x=\"{start:.2}\" y=\"0\" width=\"{:.2}\" height=\"1\" fill=\"{}\"></rect>",
            end - start,
            strip_color(metric, zone, theme)
        );
    }
    out
}

/// Builds the markup of one chart window: one flat area and one 2 px ink line per unbroken run
/// of samples (the colour lives in the zone strip, not in the line), plus the dashed FTP rule.
pub fn chart_geometry(
    series: &Series,
    metric: ChartMetric,
    rider: &RiderSettings,
    theme: Theme,
    window_s: f64,
) -> ChartGeometry {
    let window_s = if window_s.is_finite() && window_s > 0.0 {
        window_s
    } else {
        300.0
    };
    let t1 = series
        .t
        .last()
        .copied()
        .filter(|t| t.is_finite())
        .unwrap_or(0.0);
    let t0 = t1 - window_s;
    let mut values: Vec<f64> = series.v.iter().filter_map(|v| *v).collect();
    let guide_raw = guide_value(metric, rider);
    // the FTP rule always has a place on the axis
    if let (Some(guide), false) = (guide_raw, values.is_empty()) {
        values.push(guide);
    }
    let (y0, y1) = chart_y_range(metric, &values);
    let floor = y0 - (y1 - y0).max(1e-6);

    let mut markup = String::with_capacity(series.t.len() * 48 + 128);
    let gap = gap_limit(&series.t);
    let mut i = 0usize;
    while i < series.t.len() {
        if series.v.get(i).copied().flatten().is_none() {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        while end + 1 < series.t.len() {
            let next = end + 1;
            let contiguous = series
                .t
                .get(next)
                .zip(series.t.get(end))
                .is_some_and(|(a, b)| a - b <= gap);
            if !contiguous || series.v.get(next).copied().flatten().is_none() {
                break;
            }
            end = next;
        }
        push_run(
            &mut markup,
            &series.t[start..=end],
            &series.v[start..=end],
            floor,
        );
        i = end + 1;
    }
    let guide = guide_raw.filter(|g| *g >= y0 && *g <= y1 && !markup.is_empty());
    if let Some(g) = guide {
        let _ = write!(
            markup,
            "<line x1=\"{t0:.2}\" x2=\"{t1:.2}\" y1=\"{g:.2}\" y2=\"{g:.2}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\" stroke-dasharray=\"4 3\" vector-effect=\"non-scaling-stroke\"></line>"
        );
    }

    ChartGeometry {
        markup,
        strip: zone_strip(series, metric, rider, theme),
        t0,
        t1,
        y0,
        y1,
        guide,
        series: series.clone(),
    }
}

fn push_run(out: &mut String, t: &[f64], v: &[Option<f64>], floor: f64) {
    if t.len() < 2 {
        return;
    }
    // area first (so the line is drawn on top)
    out.push_str("<polygon points=\"");
    let mut first = true;
    for (t, _) in t.iter().zip(v.iter()) {
        if !t.is_finite() {
            continue;
        }
        if !first {
            out.push(' ');
        }
        let _ = write!(out, "{t:.2},{floor:.2}");
        first = false;
    }
    for (t, value) in t.iter().zip(v.iter()).rev() {
        if let (true, Some(value)) = (t.is_finite(), value) {
            let _ = write!(out, " {t:.2},{value:.2}");
        }
    }
    out.push_str("\" fill=\"var(--bg-3)\"></polygon><polyline points=\"");
    let mut first = true;
    for (t, value) in t.iter().zip(v.iter()) {
        let (true, Some(value)) = (t.is_finite(), value) else {
            continue;
        };
        if !first {
            out.push(' ');
        }
        let _ = write!(out, "{t:.2},{value:.2}");
        first = false;
    }
    out.push_str(
        "\" fill=\"none\" stroke=\"var(--chart-ink)\" stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\" vector-effect=\"non-scaling-stroke\"></polyline>",
    );
}

// ───────────────────────────── labels ─────────────────────────────

/// The three time labels under the chart: window start, middle, "now".
pub fn window_labels(window_s: f64) -> [String; 3] {
    let window_s = if window_s.is_finite() && window_s > 0.0 {
        window_s
    } else {
        300.0
    };
    [
        format!("−{}", fmt_duration(window_s)),
        format!("−{}", fmt_duration(window_s / 2.0)),
        t("ui_profile.most").to_string(),
    ]
}

/// Caption under the chart title: "utolsó 5 p" / "últimos 5 min".
pub fn window_caption(window_s: f64) -> String {
    let window_s = if window_s.is_finite() && window_s > 0.0 {
        window_s
    } else {
        300.0
    };
    tf(
        "ui_profile.series.utolso",
        &[("window", &ride_ui_kit::format::fmt_hm_text(window_s))],
    )
}

/// Value of the time axis after the hover: "−2:31 · 238 W".
pub fn hover_text(metric: ChartMetric, offset_s: f64, value: Option<f64>) -> String {
    let offset = if offset_s.is_finite() {
        offset_s.max(0.0)
    } else {
        0.0
    };
    format!(
        "−{} · {} {}",
        fmt_duration(offset),
        metric.value_text(value),
        metric.unit()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::RecordPoint;

    fn rider() -> RiderSettings {
        RiderSettings {
            ftp_w: 235.0,
            max_hr: 190.0,
            ..RiderSettings::default()
        }
    }

    fn point(t: f64, power: Option<f64>, hr: Option<f64>) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist: t * 7.0,
            lat: 47.5,
            lon: 19.0,
            ele: 120.0,
            speed_ms: 7.0,
            power_w: power,
            cadence_rpm: Some(84.0),
            hr_bpm: hr,
            grade: 1.0,
        }
    }

    /// 300 s of history with a wave in power and a lagged heart rate.
    fn history() -> Vec<RecordPoint> {
        (0..300)
            .map(|i| {
                let t = i as f64;
                let wave = (t / 9.0).sin();
                let power = Some(200.0 + wave * 70.0);
                let hr = Some(140.0 + (t / 30.0).sin() * 12.0);
                point(t, power, hr)
            })
            .collect()
    }

    // ── zones ──────────────────────────────────────────────────────────────────

    #[test]
    fn power_zones_follow_coggan() {
        let ftp = 200.0;
        for (watts, zone) in [
            (0.0, 1),
            (109.9, 1),
            (110.0, 2),
            (151.9, 2),
            (152.0, 3),
            (181.9, 3),
            (182.0, 4),
            (211.9, 4),
            (212.0, 5),
            (241.9, 5),
            (242.0, 6),
            (301.9, 6),
            (302.0, 7),
            (900.0, 7),
            (-20.0, 1),
        ] {
            assert_eq!(power_zone(watts, ftp), Some(zone), "{watts} W");
        }
        assert_eq!(power_zone(200.0, 0.0), None);
        assert_eq!(power_zone(200.0, f64::NAN), None);
        assert_eq!(power_zone(f64::NAN, 200.0), None);
    }

    #[test]
    fn hr_zones_follow_max_hr() {
        let max = 190.0;
        for (bpm, zone) in [
            (0.0, 1),
            (113.0, 1),
            (114.0, 2),
            (132.0, 2),
            (133.0, 3),
            (151.0, 3),
            (152.0, 4),
            (170.0, 4),
            (171.0, 5),
            (220.0, 5),
        ] {
            assert_eq!(hr_zone(bpm, max), Some(zone), "{bpm} bpm");
        }
        assert_eq!(hr_zone(150.0, 0.0), None);
        assert_eq!(hr_zone(f64::NAN, max), None);
    }

    #[test]
    fn zone_badges_are_named_and_coloured() {
        let rider = rider();
        let (zone, name, var) = zone_badge(ChartMetric::Power, Some(235.0), &rider).expect("zone");
        assert_eq!((zone, name, var), (4, "Küszöb", "var(--z4)"));
        let (zone, name, _) =
            zone_badge(ChartMetric::HeartRate, Some(150.0), &rider).expect("zone");
        assert_eq!((zone, name), (3, "Aerob"));
        assert_eq!(zone_badge(ChartMetric::Cadence, Some(90.0), &rider), None);
        assert_eq!(zone_badge(ChartMetric::Power, None, &rider), None);
    }

    // ── window slicing ─────────────────────────────────────────────────────────

    #[test]
    fn window_slice_takes_the_tail() {
        let history = history();
        let slice = window_slice(&history, 299.0, 60.0);
        assert_eq!(slice.len(), 61, "60 s window inclusive");
        assert_eq!(slice.first().map(|p| p.t), Some(239.0));
        assert_eq!(slice.last().map(|p| p.t), Some(299.0));
        // a window longer than the history returns everything
        assert_eq!(window_slice(&history, 299.0, 10_000.0).len(), 300);
        // degenerate input
        assert!(window_slice(&history, 299.0, 0.0).is_empty());
        assert!(window_slice(&history, f64::NAN, 60.0).is_empty());
        assert!(window_slice(&[], 0.0, 60.0).is_empty());
        // a t_end before the recording start is empty
        assert!(window_slice(&history, -5.0, 60.0).is_empty());
    }

    #[test]
    fn moving_average_smooths_and_keeps_gaps() {
        let values = vec![Some(10.0), Some(20.0), Some(30.0), None, None, Some(60.0)];
        let smoothed = moving_average(&values, 1);
        assert_eq!(smoothed.len(), values.len());
        assert_eq!(smoothed[0], Some(15.0));
        assert_eq!(smoothed[1], Some(20.0));
        assert_eq!(smoothed[2], Some(25.0));
        assert_eq!(smoothed[3], Some(30.0), "one neighbour left");
        assert_eq!(smoothed[4], Some(60.0));
        assert_eq!(smoothed[5], Some(60.0));
        // a longer gap is not invented
        let smoothed = moving_average(&[Some(1.0), None, None, None, Some(5.0)], 1);
        assert_eq!(smoothed[2], None);
        assert_eq!(smoothed[0], Some(1.0));
        assert!(moving_average(&[], 1).is_empty());
        // radius 0 is the identity (apart from non-finite cleanup)
        assert_eq!(
            moving_average(&[Some(1.0), Some(f64::NAN)], 0),
            vec![Some(1.0), None]
        );
    }

    #[test]
    fn series_extracts_the_metric_and_smooths_it() {
        let history = history();
        let series = series_for(&history, ChartMetric::Power, 60.0);
        assert_eq!(series.t.len(), 61);
        assert_eq!(series.value_count(), 61);
        assert!(series.last_value().is_some());
        // the series is smoothed: a single 3-sample window cannot exceed the raw peak
        let peak = series.v.iter().flatten().fold(f64::MIN, |a, b| a.max(*b));
        assert!(peak <= 270.0 + 1e-9, "smoothed peak {peak}");
        // a metric without data yields a series of `None`s but keeps the time axis
        let mut broken = history.clone();
        for p in &mut broken {
            p.hr_bpm = None;
        }
        let series = series_for(&broken, ChartMetric::HeartRate, 60.0);
        assert_eq!(series.value_count(), 0);
        assert!(series.last_value().is_none());
        assert!(!series.is_empty(), "the time axis still exists");
        // a dropout at the end: the value from before it is not live any more
        let mut dropout = history.clone();
        let n = dropout.len();
        for p in &mut dropout[n - 10..] {
            p.power_w = None;
        }
        let series = series_for(&dropout, ChartMetric::Power, 60.0);
        assert!(series.last_value().is_none(), "{:?}", series.last_value());
        assert!(series_for(&[], ChartMetric::Power, 60.0).is_empty());
    }

    #[test]
    fn value_at_picks_the_nearest_sample() {
        let series = Series {
            t: vec![0.0, 1.0, 2.0, 3.0],
            v: vec![Some(10.0), Some(20.0), None, Some(40.0)],
        };
        assert_eq!(series.value_at(0.0), Some(10.0));
        assert_eq!(series.value_at(1.4), Some(20.0));
        assert_eq!(series.value_at(2.4), None, "2.0 is nearer and has no value");
        assert_eq!(series.value_at(2.6), Some(40.0), "3.0 is nearer");
        assert_eq!(series.value_at(2.0), None);
        assert_eq!(series.value_at(99.0), Some(40.0));
        assert_eq!(series.value_at(f64::NAN), None);
        assert_eq!(Series::default().value_at(1.0), None);
    }

    // ── y range ────────────────────────────────────────────────────────────────

    #[test]
    fn chart_y_range_brackets_the_data() {
        let (lo, hi) = chart_y_range(ChartMetric::Power, &[100.0, 250.0, 180.0]);
        assert_eq!(lo, 0.0, "area charts start at zero");
        assert!(hi > 250.0, "{hi} must clear the peak");
        let (lo, hi) = chart_y_range(ChartMetric::HeartRate, &[140.0, 150.0, 145.0]);
        assert!(lo < 140.0 && hi > 150.0, "{lo}..{hi}");
        assert!(hi - lo >= 20.0);
        // empty and broken windows fall back to a usable axis
        for metric in [
            ChartMetric::Power,
            ChartMetric::HeartRate,
            ChartMetric::Cadence,
            ChartMetric::Speed,
        ] {
            let (lo, hi) = chart_y_range(metric, &[]);
            assert!(lo.is_finite() && hi.is_finite() && hi > lo);
            let (lo, hi) = chart_y_range(metric, &[f64::NAN, f64::INFINITY]);
            assert!(lo.is_finite() && hi.is_finite() && hi > lo);
        }
        // a cadence ride does not get a zero-height axis
        let (lo, hi) = chart_y_range(ChartMetric::Cadence, &[84.0]);
        assert_eq!(lo, 0.0);
        assert!(hi >= 70.0);
    }

    // ── geometry ───────────────────────────────────────────────────────────────

    #[test]
    fn a_sparser_recording_still_draws_a_line_and_keeps_its_gaps() {
        // Regression: a history recorded every 2 s (a slow ride loop, or RIDE_TIMESCALE) fell
        // apart into single points against the fixed 1.6 s limit, and the charts stayed empty.
        let rider = rider();
        let mut sparse: Vec<RecordPoint> = (0..150)
            .map(|i| point(i as f64 * 2.0, Some(180.0), Some(130.0)))
            .collect();
        let series = series_for(&sparse, ChartMetric::Power, 300.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 300.0);
        assert_eq!(
            geometry.markup.matches("<polyline").count(),
            1,
            "one unbroken line: {}",
            geometry.markup.len()
        );
        assert!(!geometry.strip.is_empty(), "the zone strip is drawn too");
        // a real gap (20 s without a sample) still breaks the line
        sparse.retain(|p| !(100.0..120.0).contains(&p.t));
        let series = series_for(&sparse, ChartMetric::Power, 300.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 300.0);
        assert_eq!(geometry.markup.matches("<polyline").count(), 2);
        assert_eq!(gap_limit(&[]), GAP_S);
        assert_eq!(
            gap_limit(&[0.0, 1.0, 2.0, 3.0]),
            GAP_S,
            "1 Hz keeps the old limit"
        );
    }

    #[test]
    fn chart_geometry_draws_runs_with_gaps_and_colours() {
        let history = history();
        let rider = rider();
        let theme = Theme::Dark;
        let series = series_for(&history, ChartMetric::Power, 120.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, theme, 120.0);
        assert!(!geometry.is_empty());
        assert!(geometry.markup.contains("<polygon"), "area fill");
        assert!(geometry.markup.contains("<polyline"), "line");
        assert!(geometry
            .markup
            .contains("vector-effect=\"non-scaling-stroke\""));
        assert!(!geometry.markup.contains("NaN"));
        // flat: the area is one surface colour, the line is ink, the zone colours live in the strip
        assert!(
            geometry.markup.contains("fill=\"var(--bg-3)\""),
            "flat area"
        );
        assert!(
            geometry.markup.contains("stroke=\"var(--chart-ink)\""),
            "ink line"
        );
        assert!(!geometry.markup.contains("opacity"), "no transparency");
        let color = palette::power_zone_color(4, theme);
        assert!(
            !geometry.markup.contains(&color),
            "no zone colour in the line"
        );
        assert!(
            geometry.strip.contains(&color),
            "zone colour {color} in the strip"
        );
        assert!((geometry.t1 - 299.0).abs() < 1e-9);
        assert!((geometry.t0 - 179.0).abs() < 1e-9);

        // a gap in the recording breaks the run instead of bridging it
        let mut gapped = history.clone();
        for p in gapped.iter_mut().skip(200).take(4) {
            p.power_w = None;
        }
        let series = series_for(&gapped, ChartMetric::Power, 120.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, theme, 120.0);
        assert!(geometry.runs() >= 2, "the hole splits the line");
    }

    #[test]
    fn chart_geometry_is_empty_without_enough_samples() {
        let rider = rider();
        let series = series_for(&[], ChartMetric::Power, 300.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 300.0);
        assert!(geometry.is_empty());
        assert!(geometry.markup.is_empty());
        assert!(geometry.y1 > geometry.y0);
        // a single sample is not a line
        let one = vec![point(0.0, Some(200.0), Some(140.0))];
        let series = series_for(&one, ChartMetric::Power, 300.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 300.0);
        assert!(geometry.is_empty());
        assert!(geometry.markup.is_empty());
    }

    #[test]
    fn chart_geometry_window_starts_before_the_ride() {
        let rider = rider();
        let history = history();
        let series = series_for(&history, ChartMetric::Cadence, 600.0);
        let geometry = chart_geometry(&series, ChartMetric::Cadence, &rider, Theme::Dark, 600.0);
        assert!((geometry.t0 - (-301.0)).abs() < 1e-9, "{}", geometry.t0);
        assert!((geometry.t1 - 299.0).abs() < 1e-9);
        // cadence has no zones: ink line, no strip, no FTP rule
        assert!(geometry.markup.contains("var(--chart-ink)"));
        assert!(geometry.strip.is_empty());
        assert_eq!(geometry.guide, None);
    }

    // ── metric metadata & labels ───────────────────────────────────────────────

    #[test]
    fn metric_metadata_is_hungarian() {
        assert_eq!(ChartMetric::Power.title(), "Teljesítmény");
        assert_eq!(ChartMetric::HeartRate.title(), "Pulzus");
        assert_eq!(ChartMetric::Cadence.title(), "Kadencia");
        assert_eq!(ChartMetric::Speed.title(), "Sebesség");
        assert_eq!(ChartMetric::Power.unit(), "W");
        assert_eq!(ChartMetric::HeartRate.unit(), "bpm");
        assert_eq!(ChartMetric::Cadence.unit(), "rpm");
        assert_eq!(ChartMetric::Speed.unit(), "km/h");
        assert!(ChartMetric::Power.has_zone());
        assert!(ChartMetric::HeartRate.has_zone());
        assert!(!ChartMetric::Cadence.has_zone());
        assert!(!ChartMetric::Speed.has_zone());
        assert_eq!(ChartMetric::Power.zone_prefix(), "Z");
        assert_eq!(ChartMetric::HeartRate.zone_prefix(), "H");
        assert_eq!(ChartMetric::Speed.zone_prefix(), "");
    }

    #[test]
    fn values_are_formatted_through_the_kit() {
        assert_eq!(ChartMetric::Power.value_text(Some(249.6)), "250");
        assert_eq!(ChartMetric::Power.value_text(None), "—");
        assert_eq!(ChartMetric::HeartRate.value_text(Some(147.7)), "148");
        assert_eq!(ChartMetric::Cadence.value_text(Some(84.2)), "84");
        assert_eq!(ChartMetric::Speed.value_text(Some(10.0)), "36.0");
        assert_eq!(ChartMetric::Speed.axis_text(10.0), "36.0");
        assert_eq!(ChartMetric::Power.axis_text(250.4), "250");
    }

    #[test]
    fn time_labels_and_captions() {
        assert_eq!(window_labels(300.0), ["−5:00", "−2:30", "most"]);
        assert_eq!(window_labels(90.0), ["−1:30", "−0:45", "most"]);
        assert_eq!(window_caption(300.0), "utolsó 5 p");
        assert_eq!(window_caption(600.0), "utolsó 10 p");
        // broken input falls back to the 5 minute default
        assert_eq!(window_labels(f64::NAN), ["−5:00", "−2:30", "most"]);
        assert_eq!(
            hover_text(ChartMetric::Power, 151.0, Some(238.4)),
            "−2:31 · 238 W"
        );
        assert_eq!(
            hover_text(ChartMetric::Speed, 0.0, Some(10.0)),
            "−0:00 · 36.0 km/h"
        );
        assert_eq!(
            hover_text(ChartMetric::Power, f64::NAN, None),
            "−0:00 · — W"
        );
    }

    #[test]
    fn metric_values_come_from_the_record() {
        let p = point(5.0, Some(200.0), Some(150.0));
        assert_eq!(ChartMetric::Power.value(&p), Some(200.0));
        assert_eq!(ChartMetric::HeartRate.value(&p), Some(150.0));
        assert_eq!(ChartMetric::Cadence.value(&p), Some(84.0));
        assert_eq!(ChartMetric::Speed.value(&p), Some(7.0));
        let mut broken = p;
        broken.power_w = Some(f64::NAN);
        assert_eq!(ChartMetric::Power.value(&broken), None);
    }

    // ── flat chart: strip and FTP rule ─────────────────────────────────────────

    #[test]
    fn zone_strip_merges_runs_of_equal_zone_and_leaves_gaps_empty() {
        let rider = rider(); // FTP 235
        let theme = Theme::Light;
        // 6 s: Z2 Z2 Z2 | Z5 Z5 | gap of 3 s | Z3
        let series = Series {
            t: vec![0.0, 1.0, 2.0, 3.0, 4.0, 8.0],
            v: vec![
                Some(150.0),
                Some(150.0),
                Some(150.0),
                Some(260.0),
                Some(260.0),
                Some(190.0),
            ],
        };
        let strip = zone_strip(&series, ChartMetric::Power, &rider, theme);
        assert_eq!(strip.matches("<rect").count(), 3, "{strip}");
        assert!(strip.contains(&palette::power_zone_color(2, theme)));
        assert!(strip.contains(&palette::power_zone_color(5, theme)));
        assert!(strip.contains(&palette::power_zone_color(3, theme)));
        // the first cell starts half a sample before the first sample and is 3 s long
        assert!(
            strip.starts_with("<rect x=\"-0.50\" y=\"0\" width=\"3.00\""),
            "{strip}"
        );
        assert!(!strip.contains("NaN"));
        // heart rate uses the HR zones
        let hr = Series {
            t: vec![0.0, 1.0],
            v: vec![Some(150.0), Some(150.0)],
        };
        let strip = zone_strip(&hr, ChartMetric::HeartRate, &rider, theme);
        assert!(strip.contains(&palette::hr_zone_color(3, theme)));
        // no zones for cadence / speed, no zones without an FTP
        assert!(zone_strip(&series, ChartMetric::Cadence, &rider, theme).is_empty());
        assert!(zone_strip(&series, ChartMetric::Speed, &rider, theme).is_empty());
        let mut no_ftp = rider.clone();
        no_ftp.ftp_w = 0.0;
        assert!(zone_strip(&series, ChartMetric::Power, &no_ftp, theme).is_empty());
        assert!(zone_strip(&Series::default(), ChartMetric::Power, &rider, theme).is_empty());
    }

    #[test]
    fn power_chart_has_a_dashed_ftp_rule_inside_the_axis() {
        let rider = rider();
        let series = series_for(&history(), ChartMetric::Power, 120.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 120.0);
        assert_eq!(geometry.guide, Some(235.0));
        assert!(geometry.markup.contains("stroke-dasharray"));
        assert!(geometry.markup.contains("<line"));
        assert!(geometry.y0 <= 235.0 && 235.0 <= geometry.y1);
        // even when the rider rides far below FTP the rule stays on the axis
        let mut easy = history();
        for p in &mut easy {
            p.power_w = Some(60.0);
        }
        let series = series_for(&easy, ChartMetric::Power, 120.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 120.0);
        assert!(geometry.y1 >= 235.0, "axis reaches FTP: {}", geometry.y1);
        assert_eq!(geometry.guide, Some(235.0));
        // no FTP, no rule; heart rate has no rule either
        let mut no_ftp = rider.clone();
        no_ftp.ftp_w = 0.0;
        let geometry = chart_geometry(&series, ChartMetric::Power, &no_ftp, Theme::Dark, 120.0);
        assert_eq!(geometry.guide, None);
        assert!(!geometry.markup.contains("<line"));
        let hr = series_for(&history(), ChartMetric::HeartRate, 120.0);
        let geometry = chart_geometry(&hr, ChartMetric::HeartRate, &rider, Theme::Dark, 120.0);
        assert_eq!(geometry.guide, None);
        assert_eq!(guide_value(ChartMetric::Speed, &rider), None);
    }

    #[test]
    fn chart_line_is_split_by_gaps_only_not_by_zone() {
        let rider = rider();
        // power sweeps through several zones without a gap: still one line, several strip cells
        let mut sweep = history();
        for (i, p) in sweep.iter_mut().enumerate() {
            p.power_w = Some(40.0 + i as f64);
        }
        let series = series_for(&sweep, ChartMetric::Power, 120.0);
        let geometry = chart_geometry(&series, ChartMetric::Power, &rider, Theme::Dark, 120.0);
        assert_eq!(geometry.runs(), 1);
        assert!(
            geometry.strip_cells() >= 3,
            "{} cells",
            geometry.strip_cells()
        );
    }
}
