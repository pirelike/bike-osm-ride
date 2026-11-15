//! A small line / bar plot for the views that are not on the distance axis: the ride's time
//! chart, the performance management chart and the power curve.
//!
//! The markup is SVG in data space, drawn inside the group transform of
//! [`ride_ui_profile::geom::view_transform`], exactly like the distance chart; the axis numerals
//! are HTML laid over it, so they keep their size whatever the plot's aspect ratio.

use std::fmt::Write as _;

use dioxus::prelude::*;
use ride_ui_profile::geom;

use crate::{edge_class, pct_text};

/// One line of a plot: `(x, value)`, `None` breaks the line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// A colour token, `var(--...)`.
    pub color: String,
    pub dash: &'static str,
    pub width: f64,
    pub points: Vec<(f64, Option<f64>)>,
}

/// Bars from zero: `(x, width, value)` in data space.
#[derive(Clone, Debug, PartialEq)]
pub struct Bars {
    pub color: String,
    pub bars: Vec<(f64, f64, f64)>,
}

/// Averages `values` (one per x step of 1, starting at `x0`) down to at most `max` points; each
/// point sits at the middle of its bucket.
pub fn bucket_mean(values: &[Option<f64>], x0: f64, max: usize) -> Vec<(f64, Option<f64>)> {
    let max = max.max(1);
    let size = values.len().div_ceil(max).max(1);
    values
        .chunks(size)
        .enumerate()
        .map(|(i, chunk)| {
            let (sum, n) = chunk
                .iter()
                .flatten()
                .filter(|v| v.is_finite())
                .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
            let x = x0 + (i * size) as f64 + chunk.len() as f64 / 2.0;
            (
                x,
                (n * 2 >= chunk.len().max(1)).then(|| sum / n.max(1) as f64),
            )
        })
        .collect()
}

/// The SVG body of `lines` and `bars`.
pub fn markup(lines: &[Line], bars: &[Bars]) -> String {
    let mut out = String::new();
    for group in bars {
        for &(x, w, v) in &group.bars {
            if !(x.is_finite() && w.is_finite() && v.is_finite()) || v == 0.0 {
                continue;
            }
            let (y, h) = if v >= 0.0 { (0.0, v) } else { (v, -v) };
            let _ = write!(
                out,
                "<rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{w:.3}\" height=\"{h:.3}\" fill=\"{}\"></rect>",
                group.color
            );
        }
    }
    for line in lines {
        let mut run: Vec<(f64, f64)> = Vec::new();
        let flush = |out: &mut String, run: &mut Vec<(f64, f64)>| {
            if run.len() >= 2 {
                out.push_str("<polyline points=\"");
                for (x, y) in run.iter() {
                    let _ = write!(out, "{x:.3},{y:.3} ");
                }
                let _ = write!(
                    out,
                    "\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\" vector-effect=\"non-scaling-stroke\"",
                    line.color, line.width
                );
                if !line.dash.is_empty() {
                    let _ = write!(out, " stroke-dasharray=\"{}\"", line.dash);
                }
                out.push_str("></polyline>");
            }
            run.clear();
        };
        for (x, v) in &line.points {
            match v.filter(|v| v.is_finite()) {
                Some(v) if x.is_finite() => run.push((*x, v)),
                _ => flush(&mut out, &mut run),
            }
        }
        flush(&mut out, &mut run);
    }
    out
}

/// A y range that holds every value of `lines` and `bars`, rounded to nice steps; `floor_zero`
/// keeps zero at the bottom when everything is positive.
pub fn y_range(lines: &[Line], bars: &[Bars], floor_zero: bool, min_span: f64) -> (f64, f64) {
    let values = lines
        .iter()
        .flat_map(|l| l.points.iter().filter_map(|(_, v)| *v))
        .chain(bars.iter().flat_map(|b| b.bars.iter().map(|(_, _, v)| *v)))
        .filter(|v| v.is_finite());
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in values {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        return (0.0, min_span.max(1.0));
    }
    if floor_zero {
        lo = lo.min(0.0);
    }
    if hi - lo < min_span {
        hi = lo + min_span;
    }
    let pad = (hi - lo) * 0.05;
    let (lo, hi) = (
        if floor_zero && lo >= 0.0 {
            0.0
        } else {
            lo - pad
        },
        hi + pad,
    );
    let step = geom::nice_step(hi - lo, 4);
    ((lo / step).floor() * step, (hi / step).ceil() * step)
}

/// "1:05" for 65 minutes, "0:30" for half an hour.
pub fn clock_label(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        (seconds.max(0.0) / 60.0).round() as u64
    } else {
        0
    };
    format!("{}:{:02}", total / 60, total % 60)
}

/// Ticks of a time axis from 0 to `x1` seconds, at most `max` of them.
pub fn time_ticks(x1: f64, max: usize) -> Vec<f64> {
    const STEPS: [f64; 10] = [
        60.0, 120.0, 300.0, 600.0, 900.0, 1_200.0, 1_800.0, 3_600.0, 7_200.0, 14_400.0,
    ];
    if !(x1.is_finite() && x1 > 0.0) {
        return Vec::new();
    }
    let step = STEPS
        .iter()
        .copied()
        .find(|s| x1 / s <= max.max(1) as f64)
        .unwrap_or(28_800.0);
    (0..)
        .map(|i| i as f64 * step)
        .take_while(|v| *v <= x1 + 1e-6)
        .take(64)
        .collect()
}

/// The plot itself: the SVG, the horizontal rules with their numerals, the x axis.
#[component]
pub fn PlotFrame(
    markup: String,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    /// Numerals of the y axis; the top one carries the unit.
    unit: String,
    y_text: Callback<f64, String>,
    /// `(x, label)` of the x axis.
    x_ticks: Vec<(f64, String)>,
    /// Shown over an empty plot.
    empty: Option<String>,
    #[props(default)] class: String,
) -> Element {
    let transform = geom::view_transform(x0, x1, y0, y1);
    let y_pct = |v: f64| (1.0 - geom::elevation_frac(v, y0, y1)) * 100.0;
    let scale = geom::XScale::new(x0, x1);
    let rules = geom::grid_values_n(y0, y1, 4);
    let x_pct = move |x: f64| {
        if x1 > x0 {
            ((x - x0) / (x1 - x0) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    };
    let _ = scale;
    rsx! {
        div { class: "rv-chart {class}",
            div { class: "rv-plot",
                svg {
                    class: "rv-svg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    g { transform: "{transform}", dangerous_inner_html: "{markup}" }
                }
                for (i , v) in rules.iter().enumerate() {
                    div {
                        key: "grid-{v:.3}",
                        class: "rv-grid",
                        style: "top: {pct_text(y_pct(*v))}",
                        span { class: "rv-ytick",
                            if i + 1 == rules.len() {
                                "{y_text.call(*v)} {unit}"
                            } else {
                                "{y_text.call(*v)}"
                            }
                        }
                    }
                }
                if let Some(note) = empty {
                    div { class: "rv-note", "{note}" }
                }
            }
            div { class: "rv-xaxis", "aria-hidden": "true",
                for (x , label) in x_ticks.iter() {
                    span {
                        key: "x-{x:.3}",
                        class: edge_class("rv-xtick", x_pct(*x)),
                        style: "left: {pct_text(x_pct(*x))}",
                        "{label}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_average_and_drop_mostly_empty_ones() {
        let values: Vec<Option<f64>> = (0..10).map(|i| Some(f64::from(i))).collect();
        let b = bucket_mean(&values, 0.0, 5);
        assert_eq!(b.len(), 5);
        assert_eq!(b[0], (1.0, Some(0.5)));
        let holes = vec![None, None, None, Some(4.0)];
        assert_eq!(bucket_mean(&holes, 0.0, 1), vec![(2.0, None)]);
        assert!(bucket_mean(&[], 0.0, 10).is_empty());
    }

    #[test]
    fn the_markup_breaks_at_gaps_and_draws_bars_both_ways() {
        let line = Line {
            color: "var(--accent)".into(),
            dash: "",
            width: 2.0,
            points: vec![
                (0.0, Some(1.0)),
                (1.0, Some(2.0)),
                (2.0, None),
                (3.0, Some(1.0)),
            ],
        };
        let bars = Bars {
            color: "var(--fill-3)".into(),
            bars: vec![(0.0, 1.0, 5.0), (1.0, 1.0, -3.0), (2.0, 1.0, f64::NAN)],
        };
        let svg = markup(&[line], &[bars]);
        assert_eq!(
            svg.matches("<polyline").count(),
            1,
            "a lone point draws nothing"
        );
        assert_eq!(svg.matches("<rect").count(), 2);
        assert!(svg.contains("y=\"-3.000\" width=\"1.000\" height=\"3.000\""));
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn ranges_and_ticks() {
        let line = Line {
            color: String::new(),
            dash: "",
            width: 1.0,
            points: vec![(0.0, Some(-12.0)), (1.0, Some(80.0))],
        };
        let (lo, hi) = y_range(&[line], &[], true, 10.0);
        assert!(lo <= -12.0 && hi >= 80.0);
        assert_eq!(y_range(&[], &[], true, 10.0), (0.0, 10.0));
        assert_eq!(
            time_ticks(3_600.0, 6),
            vec![0.0, 600.0, 1_200.0, 1_800.0, 2_400.0, 3_000.0, 3_600.0]
        );
        assert!(time_ticks(f64::NAN, 6).is_empty());
        assert_eq!(clock_label(3_900.0), "1:05");
        assert_eq!(clock_label(1_800.0), "0:30");
    }
}
