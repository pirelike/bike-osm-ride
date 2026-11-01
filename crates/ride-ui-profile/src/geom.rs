//! Pure geometry for the elevation profile: axis scaling, tick generation, the padded
//! elevation range, down-sampling and the static SVG markup.
//!
//! The expensive part of the profile — up to 400 grade-coloured segments — is expressed in
//! *route space* (x = distance in metres, y = elevation in metres) and built once per
//! route / theme / zoom bucket. While riding, only the group transform (the visible window)
//! and the small HTML overlays move, which is what keeps the component smooth at 10 Hz.
//!
//! Everything in this module is deterministic, allocation-light and unit-tested; the
//! component itself only turns these numbers into CSS percentages.

use std::fmt::Write as _;

use ride_model::{ProfileMode, Route, RoutePoint, RoutePosition, Theme};
use ride_ui_kit::palette;

/// Logical width of the SVG `viewBox`. The SVG is stretched to its element
/// (`preserveAspectRatio="none"`), so this is the unit of every x in [`view_transform`].
pub const VIEW_W: f64 = 1000.0;
/// Logical height of the SVG `viewBox`.
pub const VIEW_H: f64 = 1000.0;

/// Hard cap from the workstream brief: at most 400 drawn segments, so at most 401 points.
pub const MAX_POINTS: usize = 401;
/// Same cap as a segment count (for assertions and tests).
pub const MAX_SEGMENTS: usize = MAX_POINTS - 1;

/// While the rider advances, the static geometry is rebuilt after this many metres. The
/// sub-bucket movement is absorbed by the transform, so the profile scrolls smoothly.
pub const GEOMETRY_BUCKET_M: f64 = 250.0;

/// Smallest elevation span the y axis is allowed to collapse to, metres.
pub const MIN_Y_SPAN_M: f64 = 20.0;

/// Distance steps offered to the x axis, metres (100 m … 50 km).
const TICK_STEPS_M: [f64; 12] = [
    100.0, 200.0, 250.0, 500.0, 1_000.0, 2_000.0, 2_500.0, 5_000.0, 10_000.0, 20_000.0, 25_000.0,
    50_000.0,
];

/// Candidate axis steps used to round the elevation axis to readable values.
const NICE_STEPS: [f64; 13] = [
    0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0,
];

// ───────────────────────────── small numeric helpers ─────────────────────────────

/// A finite value or `fallback` (NaN and infinities never reach a CSS percentage).
fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Clamps `v` into `0..=1`, mapping a non-finite value to `0`.
pub fn clamp01(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Rounds `v` up to the next multiple of `step` (both finite, `step > 0`).
fn ceil_to(v: f64, step: f64) -> f64 {
    if !(v.is_finite() && step.is_finite()) || step <= 0.0 {
        return finite_or(v, 0.0);
    }
    (v / step).ceil() * step
}

/// Rounds `v` down to the previous multiple of `step`.
fn floor_to(v: f64, step: f64) -> f64 {
    if !(v.is_finite() && step.is_finite()) || step <= 0.0 {
        return finite_or(v, 0.0);
    }
    (v / step).floor() * step
}

/// The smallest "nice" step (`0.5, 1, 2, 2.5, 5, 10, …`) that fits `span` into at most
/// `max_steps` intervals. Non-finite input yields `1.0`.
pub fn nice_step(span: f64, max_steps: usize) -> f64 {
    let span = if span.is_finite() && span > 0.0 {
        span
    } else {
        1.0
    };
    let max = max_steps.max(1) as f64;
    for step in NICE_STEPS {
        if span / step <= max {
            return step;
        }
    }
    // wider than every candidate: fall back to the largest step scaled up by a power of ten
    let mut step = NICE_STEPS[NICE_STEPS.len() - 1];
    while span / step > max {
        step *= 10.0;
    }
    step
}

/// Guarantees `hi > lo` over a finite interval (used by the transforms).
fn sane_span(lo: f64, hi: f64) -> (f64, f64) {
    let lo = finite_or(lo, 0.0);
    let hi = finite_or(hi, lo + 1.0);
    if hi > lo {
        (lo, hi)
    } else {
        (lo, lo + 1.0)
    }
}

// ───────────────────────────── x scale ─────────────────────────────

/// Linear mapping between route distance (metres) and the horizontal axis of the plot.
///
/// The plot uses a fixed 0..[`VIEW_W`] coordinate system; the SVG stretches it to the
/// element, so [`XScale::pct`] is what the HTML overlays position themselves with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XScale {
    d0: f64,
    d1: f64,
}

impl XScale {
    /// A scale for the window `[d0, d1]`. Non-finite or inverted input is repaired (a zero
    /// length window becomes 1 m wide) so nothing downstream can divide by zero.
    pub fn new(d0: f64, d1: f64) -> Self {
        let d0 = finite_or(d0, 0.0).max(0.0);
        let (d0, d1) = sane_span(d0, finite_or(d1, d0 + 1.0));
        Self { d0, d1 }
    }

    /// Window start (metres).
    pub fn start(self) -> f64 {
        self.d0
    }

    /// Window end (metres).
    pub fn end(self) -> f64 {
        self.d1
    }

    /// Window length in metres, always `> 0`.
    pub fn span(self) -> f64 {
        (self.d1 - self.d0).max(1e-9)
    }

    /// Position of `dist` inside the window, clamped to `0..=1` (NaN → `0`).
    pub fn frac(self, dist: f64) -> f64 {
        if !dist.is_finite() {
            return 0.0;
        }
        ((dist - self.d0) / self.span()).clamp(0.0, 1.0)
    }

    /// Position of `dist` in `viewBox` units (`0..=VIEW_W`).
    pub fn x(self, dist: f64) -> f64 {
        self.frac(dist) * VIEW_W
    }

    /// Position of `dist` in percent of the plot width (`0..=100`).
    pub fn pct(self, dist: f64) -> f64 {
        self.frac(dist) * 100.0
    }

    /// Inverse of [`XScale::frac`] for a `0..=1` pointer fraction.
    pub fn dist_at_frac(self, frac: f64) -> f64 {
        self.d0 + clamp01(frac) * self.span()
    }

    /// Inverse of [`XScale::x`]: the distance under a pointer at `x` viewBox units.
    pub fn dist_at_x(self, x: f64) -> f64 {
        let frac = if x.is_finite() { x / VIEW_W } else { 0.0 };
        self.dist_at_frac(frac)
    }
}

// ───────────────────────────── y range ─────────────────────────────

/// Padded, readable elevation range for the y axis. The input is never cut: the result
/// contains `[min_ele, max_ele]`, keeps 18 % headroom above the highest point (room for the
/// climb labels) and 10 % below the lowest. The data bounds are first quantised to a "nice"
/// step, so the axis does not breathe while the rider scrolls through a look-ahead window.
/// Flat ranges get [`MIN_Y_SPAN_M`]; NaN yields a symmetric default around zero.
pub fn y_range(min_ele: f64, max_ele: f64) -> (f64, f64) {
    let a = finite_or(min_ele, 0.0);
    let b = finite_or(max_ele, a);
    let (lo, hi) = if b >= a { (a, b) } else { (b, a) };
    let span = hi - lo;
    if span < MIN_Y_SPAN_M {
        let mid = (lo + hi) * 0.5;
        let half = MIN_Y_SPAN_M * 0.5;
        let q = MIN_Y_SPAN_M / 4.0;
        return (floor_to(mid - half, q), ceil_to(mid + half, q));
    }
    let q = nice_step(span, 20);
    let lo = floor_to(lo, q);
    let hi = ceil_to(hi, q);
    let span = (hi - lo).max(MIN_Y_SPAN_M);
    (lo - span * 0.10, hi + span * 0.18)
}

/// Height of `ele` inside `[lo, hi]` as `0..=1` measured from the bottom (`0` at `lo`).
/// Non-finite elevations become `0.5` so a broken sample cannot place a marker off-canvas.
pub fn elevation_frac(ele: f64, lo: f64, hi: f64) -> f64 {
    let (lo, hi) = sane_span(lo, hi);
    if !ele.is_finite() {
        return 0.5;
    }
    ((ele - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Interior horizontal grid values of the elevation axis (endpoints excluded, so the
/// topmost label never sits under the transport controls docked over the profile).
pub fn grid_values(lo: f64, hi: f64) -> Vec<f64> {
    grid_values_n(lo, hi, 7)
}

/// [`grid_values`] with at most `max_steps` intervals between the axis ends (a small chart gets
/// two or three hairlines, not seven).
pub fn grid_values_n(lo: f64, hi: f64, max_steps: usize) -> Vec<f64> {
    let (lo, hi) = sane_span(lo, hi);
    let span = hi - lo;
    let step = nice_step(span, max_steps);
    let mut out = Vec::new();
    let mut v = floor_to(lo, step) + step;
    // guard against a pathological step: never emit more than 12 lines
    while v < hi - 1e-9 && out.len() < 12 {
        if v > lo + 1e-9 {
            out.push(v);
        }
        v += step;
    }
    out
}

// ───────────────────────────── x ticks ─────────────────────────────

/// Distance step (metres) for the x ticks so that at most `max_ticks` fit into `span_m`.
pub fn km_tick_step(span_m: f64, max_ticks: usize) -> f64 {
    let span = if span_m.is_finite() && span_m > 0.0 {
        span_m
    } else {
        1_000.0
    };
    let max = max_ticks.max(1) as f64;
    for step in TICK_STEPS_M {
        if span / step <= max {
            return step;
        }
    }
    let mut step = TICK_STEPS_M[TICK_STEPS_M.len() - 1];
    while span / step > max {
        step *= 2.0;
    }
    step
}

/// Tick distances (metres) inside `[d0, d1]`, aligned to a step that fits `max_ticks`.
pub fn km_ticks(d0: f64, d1: f64, max_ticks: usize) -> Vec<f64> {
    let d0 = finite_or(d0, 0.0).max(0.0);
    let d1 = finite_or(d1, d0);
    let (lo, hi) = if d1 >= d0 { (d0, d1) } else { (d1, d0) };
    if hi - lo < 1e-9 {
        return Vec::new();
    }
    let step = km_tick_step(hi - lo, max_ticks);
    let mut out = Vec::new();
    let mut v = ceil_to(lo, step);
    while v <= hi + 1e-6 && out.len() < 64 {
        out.push(v);
        v += step;
    }
    out
}

/// Label of an x tick: kilometres with exactly the precision its step implies
/// (`2500 m` → `"2.5"`, `1000 m` → `"1"`).
pub fn tick_label(dist_m: f64, step_m: f64) -> String {
    let step_km = if step_m.is_finite() && step_m > 0.0 {
        step_m / 1000.0
    } else {
        1.0
    };
    ride_ui_kit::format::fmt_step_value(finite_or(dist_m, 0.0) / 1000.0, step_km)
}

// ───────────────────────────── windows ─────────────────────────────

/// Share of the look-ahead window kept behind the rider, clamped to 20..=200 m so the
/// marker is never glued to the left edge and never wastes half the width.
pub fn lookahead_behind_m(lookahead_m: f64) -> f64 {
    let look = finite_or(lookahead_m, 3_000.0);
    (look * 0.08).clamp(20.0, 200.0)
}

/// Visible distance window for a mode: the whole route, or `lookahead_m` metres starting
/// slightly behind the rider. Near the end the window slides back so it keeps its length;
/// at the start it starts at zero. Both bounds stay inside `0..=total`.
pub fn window_bounds(total_m: f64, mode: ProfileMode, dist: f64, lookahead_m: f64) -> (f64, f64) {
    let total = if total_m.is_finite() && total_m > 0.0 {
        total_m
    } else {
        0.0
    };
    if total <= 0.0 {
        return (0.0, 1.0);
    }
    match mode {
        ProfileMode::Full => (0.0, total),
        ProfileMode::Lookahead => {
            let look = finite_or(lookahead_m, 3_000.0).clamp(200.0, total.max(200.0));
            let behind = lookahead_behind_m(look);
            let dist = finite_or(dist, 0.0).clamp(0.0, total);
            let mut d0 = (dist - behind).max(0.0);
            let mut d1 = (d0 + look).min(total);
            if d1 - d0 < look {
                // the window hit the end of the route: slide it back, keeping its length
                d0 = (total - look).max(0.0);
                d1 = total;
            }
            if d1 <= d0 {
                d0 = (total - 1.0).max(0.0);
                d1 = total;
            }
            (d0, d1)
        }
    }
}

/// Quantised position of the rider used as the key of the cached static geometry.
pub fn bucket_index(dist: f64) -> u64 {
    let d = if dist.is_finite() { dist.max(0.0) } else { 0.0 };
    (d / GEOMETRY_BUCKET_M).floor().min(1e12) as u64
}

/// Route-space bounds the cached geometry has to cover for `bucket`. It always contains the
/// live window for that bucket (plus one bucket of margin, so the transform only ever
/// reveals geometry that exists).
pub fn geometry_bounds(
    total_m: f64,
    mode: ProfileMode,
    lookahead_m: f64,
    bucket: u64,
    live: (f64, f64),
) -> (f64, f64) {
    let total = if total_m.is_finite() && total_m > 0.0 {
        total_m
    } else {
        0.0
    };
    if total <= 0.0 {
        return (0.0, 1.0);
    }
    if mode == ProfileMode::Full {
        return (0.0, total);
    }
    let look = finite_or(lookahead_m, 3_000.0).clamp(200.0, total.max(200.0));
    let behind = lookahead_behind_m(look);
    let bucket_start = bucket.min(1_000_000_000_000) as f64 * GEOMETRY_BUCKET_M;
    let live_lo = finite_or(live.0, bucket_start);
    let live_hi = finite_or(live.1, bucket_start + look);
    let start = (live_lo.min(bucket_start - behind) - GEOMETRY_BUCKET_M).max(0.0);
    let end = (live_hi.max(bucket_start + look) + GEOMETRY_BUCKET_M).min(total);
    if end - start < 1.0 {
        let end = (start + 1.0).min(total).max(start + 1.0);
        (start, end)
    } else {
        (start, end)
    }
}

// ───────────────────────────── down-sampling ─────────────────────────────

/// Peak-preserving down-sampling into at most `max_points` points (so at most
/// `max_points - 1` segments). Each interior bucket contributes its lowest and its highest
/// point in distance order, so spikes and summits survive; the first and last point are
/// always kept.
pub fn down_sample(points: &[RoutePoint], max_points: usize) -> Vec<RoutePoint> {
    let max = max_points.max(2);
    if points.len() <= max {
        return points.to_vec();
    }
    let n = points.len();
    let first = points[0];
    let last = points[n - 1];
    if max < 5 {
        // tiny budget: first, last and (when asked) the highest interior point
        let mut mid = first;
        for p in &points[1..n - 1] {
            if p.ele.is_finite() && (!mid.ele.is_finite() || p.ele > mid.ele) {
                mid = *p;
            }
        }
        let mut out = vec![first];
        if max >= 3 && mid.dist > first.dist && mid.dist < last.dist {
            out.push(mid);
        }
        if last.dist > out.last().map_or(-1.0, |p| p.dist) {
            out.push(last);
        }
        return out;
    }

    let interior = &points[1..n - 1];
    let buckets = (max - 2) / 2;
    let per = interior.len().div_ceil(buckets.max(1)).max(1);
    let mut out: Vec<RoutePoint> = Vec::with_capacity(max);
    out.push(first);
    let mut i = 0;
    while i < interior.len() {
        let end = (i + per).min(interior.len());
        let chunk = &interior[i..end];
        let mut lo = 0usize;
        let mut hi = 0usize;
        for (k, p) in chunk.iter().enumerate() {
            if p.ele.is_finite() {
                if !chunk[lo].ele.is_finite() || p.ele < chunk[lo].ele {
                    lo = k;
                }
                if !chunk[hi].ele.is_finite() || p.ele > chunk[hi].ele {
                    hi = k;
                }
            }
        }
        let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        for k in [a, b] {
            let p = chunk[k];
            if out.last().is_none_or(|prev| p.dist > prev.dist) && p.dist < last.dist {
                out.push(p);
            }
        }
        i = end;
    }
    if last.dist > out.last().map_or(-1.0, |p| p.dist) {
        out.push(last);
    }
    if out.len() > max {
        // cannot happen with the bucket arithmetic above, but never return more than asked
        out.truncate(max.saturating_sub(1));
        out.push(last);
    }
    out
}

// ───────────────────────────── route space helpers ─────────────────────────────

/// `[from, to]` clamped to `0..=total` and ordered, always with a positive length.
fn sane_bounds(total: f64, from: f64, to: f64) -> (f64, f64) {
    let total = if total.is_finite() && total > 0.0 {
        total
    } else {
        1.0
    };
    let a = finite_or(from, 0.0);
    let b = finite_or(to, total);
    let (a, b) = if b >= a { (a, b) } else { (b, a) };
    let a = a.clamp(0.0, total);
    let b = b.clamp(0.0, total);
    if b - a < 1e-9 {
        (a, (a + 1.0).min(total).max(a))
    } else {
        (a, b)
    }
}

/// Interpolated route point at `dist` (the model's [`Route::locate`] as a `RoutePoint`).
fn point_at(route: &Route, dist: f64) -> RoutePoint {
    let p: RoutePosition = route.locate(dist);
    RoutePoint {
        lat: p.lat,
        lon: p.lon,
        ele: p.ele,
        dist: p.dist,
        grade: p.grade,
    }
}

/// Lowest and highest elevation inside `[from, to]`, including the interpolated ends.
/// `None` when the route has no point with a usable elevation.
pub fn elevation_range(route: &Route, from: f64, to: f64) -> Option<(f64, f64)> {
    if route.points.is_empty() {
        return None;
    }
    let (from, to) = sane_bounds(route.total_dist, from, to);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut take = |ele: f64| {
        if ele.is_finite() {
            lo = lo.min(ele);
            hi = hi.max(ele);
        }
    };
    take(point_at(route, from).ele);
    for p in route.slice(from, to) {
        take(p.ele);
    }
    take(point_at(route, to).ele);
    if lo.is_finite() && hi.is_finite() && hi >= lo {
        Some((lo, hi))
    } else {
        None
    }
}

// ───────────────────────────── static geometry ─────────────────────────────

/// The cached static shapes of one profile window, already serialised as SVG markup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileGeometry {
    /// `<polygon>` fills and one `<polyline>` outline in route space (metres).
    pub markup: String,
    /// First distance covered by the markup, metres.
    pub from: f64,
    /// Last distance covered by the markup, metres.
    pub to: f64,
    /// Number of drawn segments (≤ [`MAX_SEGMENTS`]).
    pub segments: usize,
    /// Number of flat fill polygons (runs of equal grade bucket).
    pub runs: usize,
}

impl ProfileGeometry {
    /// `true` when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.segments == 0
    }
}

/// Builds the static markup for the route window `[from, to]`: one flat fill polygon per run of
/// the same grade bucket plus one crisp 2 px outline over the whole ridge.
pub fn build_geometry(route: &Route, from: f64, to: f64, theme: Theme) -> ProfileGeometry {
    if route.points.len() < 2 {
        return ProfileGeometry::default();
    }
    let (from, to) = sane_bounds(route.total_dist, from, to);
    let mut pts: Vec<RoutePoint> = Vec::new();
    let start = point_at(route, from);
    if start.ele.is_finite() {
        pts.push(start);
    }
    for p in route.slice(from, to) {
        if pts.last().is_none_or(|last| p.dist - last.dist > 1e-6) {
            pts.push(*p);
        }
    }
    let end = point_at(route, to);
    if pts.last().is_none_or(|last| end.dist - last.dist > 1e-6) {
        pts.push(end);
    }
    let pts = down_sample(&pts, MAX_POINTS);
    let markup = markup_for(&pts, theme);
    let runs = markup.matches("<polygon").count();
    ProfileGeometry {
        markup,
        from,
        to,
        segments: pts.len().saturating_sub(1),
        runs,
    }
}

/// Grade of the segment between two points (the average of the two sampled grades; a
/// non-finite grade is treated as flat).
fn mid_grade(a: &RoutePoint, b: &RoutePoint) -> f64 {
    match (a.grade.is_finite(), b.grade.is_finite()) {
        (true, true) => (a.grade + b.grade) * 0.5,
        (true, false) => a.grade,
        (false, true) => b.grade,
        (false, false) => 0.0,
    }
}

/// Index ranges (`start..=end`, `end > start`) of consecutive segments that share a grade
/// bucket — one ridge polyline each.
fn bucket_runs(points: &[RoutePoint]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i + 1 < points.len() {
        let bucket = palette::grade_bucket(mid_grade(&points[i], &points[i + 1]));
        let mut j = i + 1;
        while j + 1 < points.len()
            && palette::grade_bucket(mid_grade(&points[j], &points[j + 1])) == bucket
        {
            j += 1;
        }
        runs.push((i, j));
        i = j;
    }
    runs
}

/// The flat printed-sheet profile in route space: one opaque polygon per run of equal grade
/// bucket (hard colour steps, no gradient and no transparency) and a single crisp 2 px outline
/// over the whole ridge. The fill runs far below the visible range; the SVG viewport (the plot
/// ends on the km ruler) clips it.
fn markup_for(points: &[RoutePoint], theme: Theme) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut min_ele = f64::INFINITY;
    for p in points {
        if p.ele.is_finite() {
            min_ele = min_ele.min(p.ele);
        }
    }
    let min_ele = if min_ele.is_finite() { min_ele } else { 0.0 };
    let base = min_ele - 5_000.0;

    let mut out = String::with_capacity(points.len() * 90 + 256);
    for (start, end) in bucket_runs(points) {
        let slice: Vec<&RoutePoint> = points[start..=end]
            .iter()
            .filter(|p| p.dist.is_finite() && p.ele.is_finite())
            .collect();
        let (Some(first), Some(last)) = (slice.first(), slice.last()) else {
            continue;
        };
        if slice.len() < 2 || last.dist <= first.dist {
            continue;
        }
        let color = palette::grade_bucket_color(
            palette::grade_bucket(mid_grade(&points[start], &points[start + 1])),
            theme,
        );
        let _ = write!(out, "<polygon points=\"{:.1},{base:.1}", first.dist);
        for p in &slice {
            let _ = write!(out, " {:.1},{:.1}", p.dist, p.ele);
        }
        let _ = write!(
            out,
            " {:.1},{base:.1}\" fill=\"{color}\" stroke=\"{color}\" stroke-width=\"1\" stroke-linejoin=\"round\" vector-effect=\"non-scaling-stroke\"></polygon>",
            last.dist
        );
    }
    out.push_str("<polyline points=\"");
    let mut first = true;
    for p in points {
        if !(p.dist.is_finite() && p.ele.is_finite()) {
            continue;
        }
        if !first {
            out.push(' ');
        }
        let _ = write!(out, "{:.1},{:.1}", p.dist, p.ele);
        first = false;
    }
    out.push_str(
        "\" fill=\"none\" stroke=\"var(--chart-ink)\" stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\" vector-effect=\"non-scaling-stroke\"></polyline>",
    );
    out
}

// ───────────────────────────── km ruler ─────────────────────────────

/// Major ruler steps in metres: the label step of the km ruler (a numeral every major).
const RULER_MAJOR_M: [f64; 12] = [
    100.0, 200.0, 250.0, 500.0, 1_000.0, 2_000.0, 2_500.0, 5_000.0, 10_000.0, 20_000.0, 25_000.0,
    50_000.0,
];

/// At most this many numerals on the ruler.
pub const RULER_MAX_MAJORS: usize = 8;

/// Upper bound of ticks drawn on the ruler (minor and major together).
const RULER_MAX_TICKS: usize = 96;

/// `(major, minor)` ruler steps in metres for a window of `span_m`: the numeral step is the
/// smallest of `0.1 0.2 0.25 0.5 1 2 2.5 5 10 20 25 50 km` that fits [`RULER_MAX_MAJORS`]
/// numerals, and a tick follows every fifth (every fourth for 2 / 20 km steps) of it. A 24 km
/// route therefore gets a tick per kilometre and a numeral per 5 km, like a map scale.
pub fn ruler_steps(span_m: f64) -> (f64, f64) {
    let span = if span_m.is_finite() && span_m > 0.0 {
        span_m
    } else {
        1_000.0
    };
    let mut major = RULER_MAJOR_M[RULER_MAJOR_M.len() - 1];
    let mut found = false;
    for step in RULER_MAJOR_M {
        if span / step <= RULER_MAX_MAJORS as f64 {
            major = step;
            found = true;
            break;
        }
    }
    if !found {
        while span / major > RULER_MAX_MAJORS as f64 {
            major *= 2.0;
        }
    }
    let minor = if matches!(major as u64, 200 | 2_000 | 20_000) {
        // 2, 20, 200 ...: four sub-steps (0.5, 5, 50 ...)
        major / 4.0
    } else {
        major / 5.0
    };
    (major, minor)
}

/// One tick of the km ruler.
#[derive(Clone, Debug, PartialEq)]
pub struct RulerTick {
    /// Route distance, metres.
    pub dist: f64,
    /// A major tick is longer and carries the numeral.
    pub major: bool,
    /// Kilometre numeral of a major tick.
    pub label: Option<String>,
}

/// Ticks of the km ruler inside `[d0, d1]` (see [`ruler_steps`]); at most 96, none for an empty
/// or broken window.
pub fn ruler_ticks(d0: f64, d1: f64) -> Vec<RulerTick> {
    let d0 = finite_or(d0, 0.0).max(0.0);
    let d1 = finite_or(d1, d0);
    let (lo, hi) = if d1 >= d0 { (d0, d1) } else { (d1, d0) };
    if hi - lo < 1e-9 {
        return Vec::new();
    }
    let (major, minor) = ruler_steps(hi - lo);
    let eps = minor * 1e-6;
    let mut out = Vec::new();
    let mut i = (lo / minor - 1e-6).ceil().max(0.0);
    while out.len() < RULER_MAX_TICKS {
        let dist = i * minor;
        if dist > hi + eps {
            break;
        }
        let ratio = dist / major;
        let is_major = (ratio - ratio.round()).abs() < 1e-6;
        out.push(RulerTick {
            dist,
            major: is_major,
            label: is_major.then(|| tick_label(dist, major)),
        });
        i += 1.0;
    }
    out
}

/// SVG `transform` that maps the rectangle `[x0, x1] × [y0, y1]` onto the `viewBox`
/// (`0..VIEW_W` horizontally, `0..VIEW_H` with y pointing down). Non-finite or collapsed
/// input is repaired so the attribute is always valid.
pub fn view_transform(x0: f64, x1: f64, y0: f64, y1: f64) -> String {
    let (x0, x1) = sane_span(x0, x1);
    let (y0, y1) = sane_span(y0, y1);
    let sx = VIEW_W / (x1 - x0);
    let sy = -VIEW_H / (y1 - y0);
    let tx = -x0 * sx;
    let ty = y1 * VIEW_H / (y1 - y0);
    format!("translate({tx:.4} {ty:.4}) scale({sx:.6} {sy:.6})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;

    fn themes() -> [Theme; 2] {
        [Theme::Dark, Theme::Light]
    }

    // ── x scale ────────────────────────────────────────────────────────────────

    #[test]
    fn x_scale_maps_distance_to_units_and_back() {
        let s = XScale::new(1_000.0, 4_000.0);
        assert_eq!(s.start(), 1_000.0);
        assert_eq!(s.end(), 4_000.0);
        assert_eq!(s.span(), 3_000.0);
        assert_eq!(s.frac(1_000.0), 0.0);
        assert_eq!(s.frac(2_500.0), 0.5);
        assert_eq!(s.frac(4_000.0), 1.0);
        assert_eq!(s.x(2_500.0), VIEW_W / 2.0);
        assert_eq!(s.pct(2_500.0), 50.0);
        // round trip: dist → x → dist
        for dist in [1_000.0, 1_750.0, 2_500.0, 3_333.0, 4_000.0] {
            let back = s.dist_at_x(s.x(dist));
            assert!((back - dist).abs() < 1e-6, "{dist} → {back}");
        }
        // a pointer outside the plot never seeks outside the window
        assert_eq!(s.dist_at_x(-50.0), 1_000.0);
        assert_eq!(s.dist_at_x(5_000.0), 4_000.0);
        assert_eq!(s.dist_at_frac(f64::NAN), 1_000.0);
        assert_eq!(s.frac(f64::NAN), 0.0);
    }

    #[test]
    fn x_scale_repairs_degenerate_windows() {
        let s = XScale::new(500.0, 500.0);
        assert!(s.span() > 0.0);
        assert_eq!(s.frac(500.0), 0.0);
        let s = XScale::new(-100.0, 100.0);
        assert_eq!(s.start(), 0.0, "negative distances are clamped away");
        let s = XScale::new(f64::NAN, f64::INFINITY);
        assert!(s.span().is_finite() && s.span() > 0.0);
    }

    // ── y range ────────────────────────────────────────────────────────────────

    #[test]
    fn y_range_pads_and_never_collapses() {
        let (lo, hi) = y_range(100.0, 200.0);
        assert!(lo < 100.0, "{lo} < 100");
        assert!(hi > 200.0, "{hi} > 200");
        assert!(
            (hi - lo) >= 100.0 * 1.15,
            "at least 15 % headroom, got {}",
            hi - lo
        );
        // more room above the summit (for the climb labels) than below the lowest point
        assert!((100.0 - lo) >= 0.05 * (hi - lo), "bottom padding");
        assert!((hi - 200.0) >= 0.12 * (hi - lo), "top padding");
        // the axis does not breathe while the data moves by a metre or two
        assert_eq!(y_range(100.0, 200.0), y_range(101.0, 199.0));
    }

    #[test]
    fn y_range_handles_flat_and_broken_elevations() {
        let (lo, hi) = y_range(140.0, 140.0);
        assert!(lo < 140.0 && hi > 140.0);
        assert!(hi - lo >= MIN_Y_SPAN_M - 1e-9);
        let (lo2, hi2) = y_range(f64::NAN, f64::NAN);
        assert!(lo2.is_finite() && hi2.is_finite() && hi2 > lo2);
        let (lo3, hi3) = y_range(f64::INFINITY, f64::NEG_INFINITY);
        assert!(lo3.is_finite() && hi3.is_finite() && hi3 > lo3);
        // reversed input is sorted, not swallowed
        let (lo4, hi4) = y_range(300.0, 120.0);
        assert!(lo4 < 120.0 && hi4 > 300.0);
    }

    #[test]
    fn elevation_fraction_is_clamped_and_nan_safe() {
        assert_eq!(elevation_frac(100.0, 100.0, 200.0), 0.0);
        assert_eq!(elevation_frac(150.0, 100.0, 200.0), 0.5);
        assert_eq!(elevation_frac(300.0, 100.0, 200.0), 1.0);
        assert_eq!(elevation_frac(f64::NAN, 100.0, 200.0), 0.5);
        assert_eq!(elevation_frac(150.0, 200.0, 200.0), 0.0, "empty span");
    }

    #[test]
    fn grid_values_are_interior_and_readable() {
        let values = grid_values(75.0, 225.0);
        assert!(!values.is_empty());
        assert!(values.len() <= 12);
        for v in &values {
            assert!(*v > 75.0 && *v < 225.0, "{v} is interior");
        }
        // strictly increasing with a constant step
        for w in values.windows(2) {
            assert!(w[1] > w[0]);
            assert!(((w[1] - w[0]) - (values[1] - values[0])).abs() < 1e-9);
        }
        assert!(grid_values(f64::NAN, f64::NAN).len() <= 12);
        // a small chart gets two or three hairlines, never a stack of seven
        for (lo, hi) in [(0.0, 280.0), (80.0, 140.0), (0.0, 110.0), (0.0, 18.0)] {
            let few = grid_values_n(lo, hi, 3);
            assert!(few.len() <= 2, "{lo}..{hi}: {few:?}");
            assert!(!few.is_empty(), "{lo}..{hi}");
            assert!(few.len() <= grid_values(lo, hi).len());
        }
        assert_eq!(grid_values_n(0.0, 280.0, 3), vec![100.0, 200.0]);
    }

    // ── ticks ──────────────────────────────────────────────────────────────────

    #[test]
    fn km_tick_step_fits_the_requested_count() {
        // a whole 24 km route in at most 10 ticks → 2.5 km steps
        assert_eq!(km_tick_step(24_000.0, 10), 2_500.0);
        // a 3 km look-ahead window → 500 m steps (6 ticks)
        assert_eq!(km_tick_step(3_000.0, 10), 500.0);
        // a 500 m window → 100 m steps
        assert_eq!(km_tick_step(500.0, 10), 100.0);
        for (span, max) in [(24_000.0, 6), (3_000.0, 4), (120_000.0, 8), (250.0, 3)] {
            let step = km_tick_step(span, max);
            assert!(
                span / step <= max as f64 + 1e-9,
                "{span} m / {step} m exceeds {max} ticks"
            );
        }
        assert_eq!(km_tick_step(f64::NAN, 5), 200.0);
    }

    #[test]
    fn km_ticks_are_aligned_inside_the_window() {
        let ticks = km_ticks(0.0, 24_000.0, 10);
        assert!(ticks.len() <= 10);
        assert_eq!(ticks.first().copied(), Some(0.0));
        assert_eq!(ticks.last().copied(), Some(22_500.0));
        for w in ticks.windows(2) {
            assert_eq!(w[1] - w[0], 2_500.0);
        }
        // a window that starts mid-step starts at the next multiple
        let ticks = km_ticks(1_180.0, 3_800.0, 10);
        assert_eq!(ticks.first().copied(), Some(1_500.0));
        assert!(ticks.iter().all(|d| *d >= 1_180.0 && *d <= 3_800.0));
        assert!(km_ticks(10.0, 10.0, 5).is_empty());
        assert!(km_ticks(f64::NAN, f64::NAN, 5).is_empty());
    }

    #[test]
    fn tick_labels_use_the_step_precision() {
        assert_eq!(tick_label(0.0, 2_500.0), "0.0");
        assert_eq!(tick_label(5_000.0, 2_500.0), "5.0");
        assert_eq!(tick_label(12_000.0, 1_000.0), "12");
        assert_eq!(tick_label(500.0, 100.0), "0.5");
        assert_eq!(tick_label(f64::NAN, 0.0), "0");
    }

    // ── down-sampling ──────────────────────────────────────────────────────────

    #[test]
    fn down_sample_caps_the_segment_count() {
        let route = straight_route(&[(24_000.0, 1.0)], 100.0);
        assert!(route.points.len() > MAX_POINTS);
        let sampled = down_sample(&route.points, MAX_POINTS);
        assert!(sampled.len() <= MAX_POINTS, "{} points", sampled.len());
        assert!(sampled.len() >= 2);
        assert_eq!(sampled.first().map(|p| p.dist), Some(0.0));
        assert_eq!(sampled.last().map(|p| p.dist), Some(route.total_dist));
        // strictly increasing distances, all points taken from the route
        for w in sampled.windows(2) {
            assert!(w[1].dist > w[0].dist, "ordered");
        }
        // a short route is returned unchanged
        let short = straight_route(&[(500.0, 1.0)], 0.0);
        assert_eq!(
            down_sample(&short.points, MAX_POINTS).len(),
            short.points.len()
        );
    }

    #[test]
    fn down_sample_keeps_the_peaks() {
        // a single 60 m spike in the middle of a flat route must survive
        let mut route = straight_route(&[(10_000.0, 0.0)], 100.0);
        let n = route.points.len();
        route.points[n / 2].ele = 260.0;
        let sampled = down_sample(&route.points, 41);
        assert!(sampled.len() <= 41);
        let peak = sampled.iter().map(|p| p.ele).fold(f64::MIN, f64::max);
        assert_eq!(peak, 260.0, "the spike has to survive the reduction");
    }

    #[test]
    fn down_sample_handles_degenerate_budgets() {
        let route = straight_route(&[(1_000.0, 2.0)], 50.0);
        for max in [0usize, 1, 2, 3, 4, 5, 6] {
            let sampled = down_sample(&route.points, max);
            assert!(!sampled.is_empty());
            assert!(sampled.len() <= max.max(2), "max {max}");
        }
        assert!(down_sample(&[], 400).is_empty());
        let single = [RoutePoint {
            lat: 0.0,
            lon: 0.0,
            ele: 1.0,
            dist: 0.0,
            grade: 0.0,
        }];
        assert_eq!(down_sample(&single, 400).len(), 1);
    }

    // ── windows ────────────────────────────────────────────────────────────────

    #[test]
    fn full_window_covers_the_route() {
        assert_eq!(
            window_bounds(24_000.0, ProfileMode::Full, 12_000.0, 3_000.0),
            (0.0, 24_000.0)
        );
        assert_eq!(
            window_bounds(0.0, ProfileMode::Full, 0.0, 3_000.0),
            (0.0, 1.0)
        );
        assert_eq!(
            window_bounds(f64::NAN, ProfileMode::Full, f64::NAN, 3_000.0),
            (0.0, 1.0)
        );
    }

    #[test]
    fn lookahead_window_keeps_the_rider_just_inside() {
        let total = 24_000.0;
        let look = 3_000.0;
        // at the start the window starts at zero and shows the first look-ahead metres
        assert_eq!(
            window_bounds(total, ProfileMode::Lookahead, 0.0, look),
            (0.0, 3_000.0)
        );
        // in the middle the rider sits just right of the left edge
        let (d0, d1) = window_bounds(total, ProfileMode::Lookahead, 12_000.0, look);
        assert_eq!(d1 - d0, look);
        assert!(d0 < 12_000.0 && 12_000.0 < d1);
        assert!((12_000.0 - d0 - lookahead_behind_m(look)).abs() < 1e-9);
        // near the end the window slides back but keeps its length
        let (d0, d1) = window_bounds(total, ProfileMode::Lookahead, 23_900.0, look);
        assert_eq!((d0, d1), (21_000.0, 24_000.0));
        assert_eq!(d1 - d0, look);
        // a window longer than the route covers the route
        assert_eq!(
            window_bounds(1_000.0, ProfileMode::Lookahead, 400.0, 3_000.0),
            (0.0, 1_000.0)
        );
        // broken input never produces an inverted or empty window
        for dist in [f64::NAN, -5.0, 1e9] {
            let (d0, d1) = window_bounds(total, ProfileMode::Lookahead, dist, f64::NAN);
            assert!(
                d0.is_finite() && d1.is_finite() && d1 > d0,
                "{dist}: {d0}..{d1}"
            );
        }
    }

    #[test]
    fn lookahead_behind_is_positive_and_bounded() {
        assert!((lookahead_behind_m(3_000.0) - 200.0).abs() < 1e-9);
        assert_eq!(lookahead_behind_m(100.0), 20.0, "never less than 20 m");
        assert_eq!(lookahead_behind_m(f64::NAN), 200.0);
        assert!(lookahead_behind_m(1e9) <= 200.0);
    }

    #[test]
    fn geometry_bounds_always_contain_the_live_window() {
        let total = 24_000.0;
        let look = 3_000.0;
        for dist in [
            0.0, 10.0, 249.0, 250.0, 999.0, 3_000.0, 11_997.0, 21_000.0, 23_999.0, 24_000.0,
        ] {
            let live = window_bounds(total, ProfileMode::Lookahead, dist, look);
            let bucket = bucket_index(dist);
            let (from, to) = geometry_bounds(total, ProfileMode::Lookahead, look, bucket, live);
            assert!(
                from <= live.0 + 1e-9,
                "dist {dist}: geometry {from} must reach the window start {}",
                live.0
            );
            assert!(
                to + 1e-9 >= live.1,
                "dist {dist}: geometry {to} must reach the window end {}",
                live.1
            );
            assert!(from >= 0.0 && to <= total);
            assert!(to > from);
            // and the geometry must also cover the whole bucket lifetime
            let next_bucket_dist = (bucket as f64 + 1.0) * GEOMETRY_BUCKET_M - 1.0;
            let live_next = window_bounds(
                total,
                ProfileMode::Lookahead,
                next_bucket_dist.min(total),
                look,
            );
            assert!(
                from <= live_next.0 + 1e-9 && to + 1e-9 >= live_next.1,
                "bucket lifetime at {dist}"
            );
        }
        assert_eq!(
            geometry_bounds(total, ProfileMode::Full, look, 3, (0.0, total)),
            (0.0, total)
        );
    }

    #[test]
    fn bucket_index_quantises_distance() {
        assert_eq!(bucket_index(0.0), 0);
        assert_eq!(bucket_index(249.9), 0);
        assert_eq!(bucket_index(250.0), 1);
        assert_eq!(bucket_index(12_000.0), 48);
        assert_eq!(bucket_index(-4.0), 0);
        assert_eq!(bucket_index(f64::NAN), 0);
    }

    // ── geometry ───────────────────────────────────────────────────────────────

    #[test]
    fn geometry_covers_the_window_with_at_most_400_segments() {
        let route = straight_route(&[(12_000.0, 4.0), (6_000.0, -2.0), (6_000.0, 3.0)], 120.0);
        for (from, to) in [
            (0.0, 24_000.0),
            (0.0, 3_000.0),
            (10_000.0, 13_000.0),
            (23_000.0, 24_000.0),
        ] {
            let geometry = build_geometry(&route, from, to, Theme::Dark);
            assert!(geometry.segments > 0, "{from}..{to}");
            assert!(geometry.segments <= MAX_SEGMENTS, "{}", geometry.segments);
            assert_eq!(geometry.from, from);
            assert_eq!(geometry.to, to);
            assert!(geometry.markup.starts_with("<polygon"), "fills first");
            assert!(!geometry.markup.contains("NaN"), "no NaN coordinates");
            // flat fill: one polygon per run of equal grade bucket, at most one per segment
            assert_eq!(geometry.markup.matches("<polygon").count(), geometry.runs);
            assert!(geometry.runs >= 1 && geometry.runs <= geometry.segments);
            // and exactly one crisp 2 px outline over the whole ridge, drawn last
            assert_eq!(geometry.markup.matches("<polyline").count(), 1);
            assert!(geometry.markup.ends_with("</polyline>"));
            assert!(geometry.markup.contains("stroke-width=\"2\""));
            assert!(geometry.markup.contains("var(--chart-ink)"));
        }
    }

    #[test]
    fn geometry_is_empty_without_a_drawable_route() {
        let mut route = straight_route(&[(1_000.0, 0.0)], 0.0);
        route.points.truncate(1);
        route.total_dist = 0.0;
        let geometry = build_geometry(&route, 0.0, 1.0, Theme::Dark);
        assert!(geometry.is_empty());
        assert!(geometry.markup.is_empty());
    }

    #[test]
    fn geometry_uses_the_grade_palette_of_the_theme() {
        let route = straight_route(&[(4_000.0, 8.0)], 100.0);
        for theme in themes() {
            let geometry = build_geometry(&route, 0.0, 4_000.0, theme);
            let expected = ride_ui_kit::palette::grade_bucket_color(
                ride_ui_kit::palette::grade_bucket(8.0),
                theme,
            );
            assert!(
                geometry.markup.contains(&expected),
                "{theme:?} should contain {expected}"
            );
        }
    }

    #[test]
    fn elevation_range_includes_the_interpolated_ends() {
        let route = straight_route(&[(1_000.0, 10.0), (1_000.0, -10.0)], 100.0);
        let (lo, hi) = elevation_range(&route, 0.0, 2_000.0).expect("range");
        assert!((lo - 100.0).abs() < 1e-6);
        assert!((hi - 200.0).abs() < 1e-6);
        // the ends are interpolated even when the slice is empty
        let (lo, hi) = elevation_range(&route, 100.0, 200.0).expect("range");
        assert!((lo - 110.0).abs() < 1e-6, "{lo}");
        assert!((hi - 120.0).abs() < 1e-6, "{hi}");
        let mut empty = route.clone();
        empty.points.clear();
        assert!(elevation_range(&empty, 0.0, 100.0).is_none());
    }

    #[test]
    fn view_transform_maps_the_window_onto_the_viewbox() {
        let t = view_transform(1_000.0, 4_000.0, 100.0, 300.0);
        assert!(t.starts_with("translate("));
        assert!(t.contains("scale("));
        assert!(!t.contains("NaN"));
        // re-parse the four numbers and check the corners
        let numbers: Vec<f64> = t
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        assert_eq!(numbers.len(), 4, "{t}");
        let (tx, ty, sx, sy) = (numbers[0], numbers[1], numbers[2], numbers[3]);
        let map = |x: f64, y: f64| (tx + x * sx, ty + y * sy);
        let (x, y) = map(1_000.0, 300.0);
        assert!(
            (x - 0.0).abs() < 0.02 && (y - 0.0).abs() < 0.02,
            "top left corner"
        );
        let (x, y) = map(4_000.0, 100.0);
        assert!((x - VIEW_W).abs() < 0.02, "right edge: {x}");
        assert!((y - VIEW_H).abs() < 0.02, "bottom edge: {y}");
        let (x, y) = map(2_500.0, 200.0);
        assert!(
            (x - VIEW_W / 2.0).abs() < 0.02 && (y - VIEW_H / 2.0).abs() < 0.02,
            "{x},{y}"
        );
        // broken input still yields a usable transform
        let t = view_transform(f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        assert!(!t.contains("NaN") && !t.contains("inf"));
    }

    // ── flat fill and outline ──────────────────────────────────────────────────

    #[test]
    fn fill_is_flat_opaque_and_merged_into_bucket_runs() {
        // a constant 8 % climb is one grade bucket: exactly one fill polygon, however many points
        let route = straight_route(&[(4_000.0, 8.0)], 100.0);
        for theme in themes() {
            let geometry = build_geometry(&route, 0.0, 4_000.0, theme);
            assert_eq!(geometry.runs, 1, "{theme:?}");
            assert!(geometry.segments > 1);
            assert!(
                !geometry.markup.contains("opacity"),
                "no transparency in the fill"
            );
            assert!(!geometry.markup.contains("gradient"), "no gradient");
            let color = ride_ui_kit::palette::grade_bucket_color(
                ride_ui_kit::palette::grade_bucket(8.0),
                theme,
            );
            assert!(
                geometry.markup.contains(&format!("fill=\"{color}\"")),
                "bucket colour {color}"
            );
        }
        // a profile that changes character gets hard colour steps: more runs than one, fewer than segments
        let route = straight_route(
            &[
                (3_000.0, 1.0),
                (3_000.0, 8.0),
                (3_000.0, -6.0),
                (3_000.0, 0.0),
            ],
            100.0,
        );
        let geometry = build_geometry(&route, 0.0, 12_000.0, Theme::Dark);
        assert!(geometry.runs >= 4, "{} runs", geometry.runs);
        assert!(geometry.runs < geometry.segments);
        // neighbouring runs share their boundary x, so the steps leave no hole
        let mut xs: Vec<(f64, f64)> = Vec::new();
        for poly in geometry.markup.split("<polygon points=\"").skip(1) {
            let pts = poly.split('"').next().unwrap_or("");
            let nums: Vec<f64> = pts
                .split([' ', ','])
                .filter_map(|s| s.parse().ok())
                .collect();
            let first_x = nums.first().copied().unwrap_or(f64::NAN);
            let last_x = nums
                .get(nums.len().saturating_sub(2))
                .copied()
                .unwrap_or(f64::NAN);
            xs.push((first_x, last_x));
        }
        for w in xs.windows(2) {
            assert!((w[0].1 - w[1].0).abs() < 0.2, "gap between runs: {:?}", w);
        }
    }

    #[test]
    fn outline_follows_the_ridge_with_a_crisp_non_scaling_stroke() {
        let route = straight_route(&[(2_000.0, 3.0), (2_000.0, -3.0)], 100.0);
        let geometry = build_geometry(&route, 0.0, 4_000.0, Theme::Light);
        let at = geometry.markup.find("<polyline").expect("outline");
        let outline = &geometry.markup[at..];
        assert!(outline.contains("fill=\"none\""));
        assert!(outline.contains("stroke=\"var(--chart-ink)\""));
        assert!(outline.contains("stroke-width=\"2\""));
        assert!(outline.contains("vector-effect=\"non-scaling-stroke\""));
        // it starts at the window start and ends at the window end
        let pts = outline
            .split("points=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("");
        let first = pts.split(' ').next().unwrap_or("");
        let last = pts.split(' ').next_back().unwrap_or("");
        assert!(first.starts_with("0.0,"), "{first}");
        assert!(last.starts_with("4000.0,"), "{last}");
    }

    // ── km ruler ───────────────────────────────────────────────────────────────

    #[test]
    fn ruler_has_a_tick_per_km_and_a_numeral_per_5_km_on_a_24_km_route() {
        let (major, minor) = ruler_steps(24_000.0);
        assert_eq!((major, minor), (5_000.0, 1_000.0));
        let ticks = ruler_ticks(0.0, 24_000.0);
        assert_eq!(ticks.len(), 25, "0..=24 km");
        let labelled: Vec<_> = ticks.iter().filter(|t| t.label.is_some()).collect();
        assert_eq!(labelled.len(), 5, "0 5 10 15 20");
        assert!(ticks.iter().all(|t| t.major == t.label.is_some()));
        assert_eq!(
            labelled
                .iter()
                .map(|t| t.label.clone().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["0", "5", "10", "15", "20"]
        );
        for w in ticks.windows(2) {
            assert!((w[1].dist - w[0].dist - 1_000.0).abs() < 1e-6);
        }
    }

    #[test]
    fn ruler_adapts_to_look_ahead_windows_and_long_routes() {
        // a 3 km look-ahead window: numeral per 500 m, tick per 100 m
        let (major, minor) = ruler_steps(3_000.0);
        assert_eq!((major, minor), (500.0, 100.0));
        let ticks = ruler_ticks(1_180.0, 4_180.0);
        assert_eq!(
            ticks.first().map(|t| t.dist),
            Some(1_200.0),
            "aligned to the minor step"
        );
        assert!(ticks
            .iter()
            .all(|t| t.dist >= 1_180.0 - 1e-6 && t.dist <= 4_180.0 + 1e-6));
        let labelled: Vec<String> = ticks.iter().filter_map(|t| t.label.clone()).collect();
        assert_eq!(labelled, ["1.5", "2.0", "2.5", "3.0", "3.5", "4.0"]);
        // a 2 km step is divided by four (0.5 km), 2.5 and 5 km by five
        assert_eq!(ruler_steps(14_000.0), (2_000.0, 500.0));
        assert_eq!(ruler_steps(18_000.0), (2_500.0, 500.0));
        // long and short routes never exceed the numeral budget or the tick cap
        for span in [
            150.0, 900.0, 3_000.0, 24_000.0, 61_000.0, 160_000.0, 900_000.0, 5.0e7,
        ] {
            let (major, minor) = ruler_steps(span);
            assert!(
                span / major <= RULER_MAX_MAJORS as f64 + 1e-9,
                "{span}: {major}"
            );
            assert!(minor < major);
            let ticks = ruler_ticks(0.0, span);
            assert!(ticks.len() <= 96, "{span}: {} ticks", ticks.len());
            assert!(ticks.iter().filter(|t| t.label.is_some()).count() <= RULER_MAX_MAJORS + 1);
        }
    }

    #[test]
    fn ruler_survives_broken_windows() {
        assert!(ruler_ticks(10.0, 10.0).is_empty());
        assert!(ruler_ticks(f64::NAN, f64::NAN).is_empty());
        assert!(
            !ruler_ticks(f64::NAN, 5_000.0).is_empty(),
            "NaN start is treated as 0"
        );
        let (major, minor) = ruler_steps(f64::NAN);
        assert!(major.is_finite() && minor.is_finite() && minor > 0.0);
        // reversed windows are sorted, not swallowed
        assert_eq!(
            ruler_ticks(5_000.0, 0.0).len(),
            ruler_ticks(0.0, 5_000.0).len()
        );
    }
}
