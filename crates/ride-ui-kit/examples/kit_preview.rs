//! Design-system showcase, rendered with dioxus-ssr into static HTML so it can be screenshotted
//! with headless Edge (same engine as the app's WebView2):
//!
//!   cargo run -p ride-ui-kit --example kit_preview
//!
//! Writes `target/preview/ride-ui-kit-<page>-<theme>.html` for the pages `foundations`,
//! `controls`, `surfaces` and `headunit` in both themes. Uses realistic fake data only.
//!
//! The `headunit` page and the `profile_svg` / `map_svg` helpers are the REFERENCE RENDERINGS of
//! the design direction (power hero with zone bar, ruled 2x2 data grid, flat-fill elevation
//! profile with km ruler and climb flags, quiet map with cased route and a disc + wedge rider):
//! the other UI crates are measured against them.

use dioxus::prelude::*;
use ride_model::{testing::straight_route, ClimbCategory, RoutePoint, Theme};
use ride_ui_kit::color;
use ride_ui_kit::palette::{self, *};
use ride_ui_kit::*;
use std::path::PathBuf;

const PREVIEW_CSS: &str = r#"
:root { --pm-land: #1d1c19; --pm-road: #35322d; --pm-road2: #2a2824; --pm-park: #20231d; --pm-water: #1c2124; --pm-label: #8f887b; }
:root[data-theme="light"] { --pm-land: #e8e4d9; --pm-road: #f3f0e6; --pm-road2: #eeebe0; --pm-park: #dfe2d2; --pm-water: #d3dbdc; --pm-label: #6b655a; }

.kp { max-width: 1320px; margin: 0 auto; padding: 32px 40px 64px; display: flex; flex-direction: column; gap: 32px; }
.kp-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; padding-bottom: 16px; border-bottom: 2px solid var(--rule-ink); }
.kp-head h1 { font: 600 var(--t-28)/1 var(--font-display); }
.kp-head p { color: var(--text-dim); margin-top: 8px; max-width: 64ch; font-size: var(--t-15); }
.kp-sec { display: flex; flex-direction: column; gap: 12px; }
.kp-sec > h2 { font: 600 var(--t-15)/1 var(--font-ui); display: flex; align-items: baseline; gap: 12px; padding-bottom: 8px; border-bottom: 1px solid var(--rule); }
.kp-sec > h2 small { font: 400 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.kp-grid { display: grid; gap: 16px; }
.kp-g2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.kp-g3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.kp-g4 { grid-template-columns: repeat(4, minmax(0, 1fr)); }
@media (max-width: 1180px) { .kp { padding: 24px; } .kp-g3, .kp-g4 { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
.kp-wrap { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.kp-col { display: flex; flex-direction: column; gap: 16px; }
.kp-cap { font: 500 var(--t-11)/1.2 var(--font-ui); color: var(--text-faint); }
.kp-type { display: grid; grid-template-columns: 96px 1fr; gap: 12px 16px; align-items: baseline; padding: 12px 16px; }
.kp-swatch { height: 64px; padding: 8px 12px; display: flex; flex-direction: column; justify-content: flex-end; font: 600 var(--t-13)/1.3 var(--font-ui); }
.kp-swatch small { font: 400 var(--t-11)/1.2 var(--font-ui); opacity: 0.8; }
.kp-zones { display: grid; gap: 4px; padding: 12px 16px; }
.kp-zone { display: grid; grid-template-columns: 32px 1fr 128px; gap: 12px; align-items: center; }
.kp-zone b { font: 600 var(--t-15)/1 var(--font-num); }
.kp-zone .bar { height: 12px; background: var(--c); }
.kp-zone .nm { font-size: var(--t-13); color: var(--text-dim); text-align: right; }
.kp-ramp { display: flex; gap: 2px; height: 32px; }
.kp-ramp i { flex: 1; background: var(--c); }
.kp-gradebar { height: 16px; }
.kp-ticks { display: flex; justify-content: space-between; font: 500 var(--t-11)/1 var(--font-num); font-variant-numeric: tabular-nums; color: var(--text-dim); margin-top: 8px; }
.kp-stage { position: relative; transform: translateZ(0); overflow: hidden; border: 1px solid var(--rule); background: var(--bg); min-height: 400px; display: grid; }
.kp-icons { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); border-top: 1px solid var(--rule); border-left: 1px solid var(--rule); }
.kp-icon { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 16px 4px 8px; background: var(--bg-1); border-right: 1px solid var(--rule); border-bottom: 1px solid var(--rule); }
.kp-icon span { font: 500 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.kp-form { display: grid; gap: 16px; padding: 16px; }
.kp-rows { display: grid; gap: 12px; justify-items: start; }
.kp-rows > .switch { justify-self: stretch; }
.kp-bg { position: absolute; inset: 0; }
.kp-profile { padding: 8px 0 0; }
.kp-profile svg { display: block; width: 100%; height: auto; }

/* head unit composition */
.hu { display: grid; grid-template-columns: minmax(0, 1fr) 380px; grid-template-rows: 40px minmax(0, 1fr) 232px; height: 900px; background: var(--bg-1); border: 1px solid var(--rule-strong); }
.hu-top { grid-column: 1 / -1; display: flex; align-items: center; gap: 16px; padding: 0 12px 0 16px; border-bottom: 1px solid var(--rule-strong); background: var(--bg-1); }
.hu-brand { font: 700 var(--t-20)/1 var(--font-display); }
.hu-top .grow { display: flex; align-items: center; gap: 8px; }
.hu-map { position: relative; overflow: hidden; background: var(--pm-land); grid-column: 1; grid-row: 2; }
.hu-map svg.bg { position: absolute; inset: 0; width: 100%; height: 100%; }
.hu-climb { position: absolute; left: 0; top: 0; width: 264px; border-right: 1px solid var(--rule-strong); border-bottom: 1px solid var(--rule-strong); background: var(--bg-1); }
.hu-ctl { position: absolute; right: 12px; top: 12px; }
.hu-attr { position: absolute; right: 0; bottom: 0; padding: 4px 8px; background: var(--bg-1); border-top: 1px solid var(--rule-strong); border-left: 1px solid var(--rule-strong); font: 400 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.hu-rail { grid-column: 2; grid-row: 2; display: flex; flex-direction: column; border-left: 1px solid var(--rule-strong); background: var(--bg-1); min-height: 0; overflow: hidden; }
.hu-hero { padding: 12px 16px 16px; border-bottom: 1px solid var(--rule-strong); display: flex; flex-direction: column; gap: 8px; }
.hu-hero .head { display: flex; justify-content: space-between; align-items: baseline; font: 500 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.hu-hero .val { display: flex; align-items: baseline; gap: 8px; }
.hu-hero .zlabels { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 2px; font: 600 var(--t-11)/1 var(--font-num); color: var(--text-dim); }
.hu-hero .zlabels span.is-current { color: var(--text); }
.hu-hero .meta { display: flex; justify-content: space-between; align-items: baseline; font: 400 var(--t-13)/1.2 var(--font-ui); }
.hu-hero .meta b { font-weight: 600; }
.hu-chart { margin-top: auto; border-top: 1px solid var(--rule-strong); }
.hu-chart svg { display: block; width: 100%; height: auto; }
.hu-stats { border-top: 1px solid var(--rule-strong); }
.hu-profile { grid-column: 1 / -1; grid-row: 3; border-top: 1px solid var(--rule-strong); background: var(--bg-1); position: relative; display: flex; flex-direction: column; min-height: 0; }
.hu-profile .bar { display: flex; align-items: center; justify-content: space-between; height: 32px; padding: 0 12px 0 0; border-bottom: 1px solid var(--rule); flex: none; }
.hu-transport { display: flex; align-items: center; height: 32px; }
.hu-profile svg { display: block; width: 100%; flex: 1; min-height: 0; }
"#;

// ───────────────────────────── fake data ─────────────────────────────

/// Rolling Budapest-ish route: gentle start, a 7 % climb, a descent, a final kicker.
fn fake_route() -> Vec<RoutePoint> {
    let n = 400usize;
    let total = 24_000.0;
    let mut ele = Vec::with_capacity(n);
    for i in 0..n {
        let x = i as f64 / (n - 1) as f64;
        let climb = smooth(0.18, 0.46, x) * 210.0;
        let drop = smooth(0.52, 0.72, x) * -150.0;
        let kick = smooth(0.86, 0.95, x) * 55.0;
        let roll = (x * 31.0).sin() * 7.0 + (x * 11.0).sin() * 12.0;
        ele.push(118.0 + climb + drop + kick + roll);
    }
    (0..n)
        .map(|i| {
            let d = total * i as f64 / (n - 1) as f64;
            let (a, b) = (i.saturating_sub(2), (i + 2).min(n - 1));
            let grade = (ele[b] - ele[a]) / (total * (b - a) as f64 / (n - 1) as f64) * 100.0;
            RoutePoint {
                lat: 47.5 + d / 111_000.0 * 0.4,
                lon: 19.0 + d / 75_000.0,
                ele: ele[i],
                dist: d,
                grade,
            }
        })
        .collect()
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Runs of consecutive segments that share a grade bucket: `(bucket, first_point, last_point)`.
/// Flat, discrete colour blocks (15 buckets) read as a printed scale, a per-segment colour would
/// smear into a gradient wash.
fn bucket_runs(pts: &[RoutePoint]) -> Vec<(usize, usize, usize)> {
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for i in 0..pts.len().saturating_sub(1) {
        let b = grade_bucket((pts[i].grade + pts[i + 1].grade) / 2.0);
        match runs.last_mut() {
            Some(r) if r.0 == b => r.2 = i + 1,
            _ => runs.push((b, i, i + 1)),
        }
    }
    runs
}

/// Black or white, whichever reads better on `bg` (flag numerals sit on a data colour).
fn ink_on(bg: &str) -> &'static str {
    let c = color::parse_hex(bg).unwrap_or(color::Rgb(128, 128, 128));
    let on_dark = color::contrast_ratio(c, color::Rgb(16, 15, 13));
    let on_light = color::contrast_ratio(c, color::Rgb(255, 255, 255));
    if on_dark >= on_light {
        "#100f0d"
    } else {
        "#ffffff"
    }
}

/// REFERENCE elevation profile: flat grade-coloured fill, a crisp 2 px ink outline, a km ruler
/// along the bottom like a map scale, climbs as flags with category numerals, the rider as a solid
/// disc with a vertical hairline. `rider` indexes `pts`.
fn profile_svg(pts: &[RoutePoint], theme: Theme, rider: usize, w: f64, h: f64) -> String {
    let (pad_t, ruler) = (28.0, 36.0);
    let base = h - ruler;
    let (min_e, max_e) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p.ele), hi.max(p.ele))
    });
    let lo = (min_e / 50.0).floor() * 50.0 - 0.0;
    let hi = (max_e / 50.0).ceil() * 50.0;
    let total = pts.last().map_or(1.0, |p| p.dist);
    let y = |e: f64| base - (e - lo) / (hi - lo) * (base - pad_t);
    let x = |d: f64| d / total * w;
    let mut s = String::new();
    s.push_str(&format!("<svg viewBox=\"0 0 {w} {h}\" xmlns=\"http://www.w3.org/2000/svg\" font-family=\"Barlow Condensed\">"));
    // elevation grid: hairlines every 50 m, labels sit on the rule at the left edge
    let mut e = lo;
    while e <= hi + 0.1 {
        let gy = y(e);
        s.push_str(&format!("<line x1=\"0\" x2=\"{w}\" y1=\"{gy:.1}\" y2=\"{gy:.1}\" stroke=\"var(--chart-grid)\" stroke-width=\"1\"/>"));
        s.push_str(&format!("<text x=\"8\" y=\"{:.1}\" font-size=\"11\" font-weight=\"500\" fill=\"var(--text-faint)\" style=\"font-variant-numeric:tabular-nums\">{e:.0} m</text>", gy - 4.0));
        e += 50.0;
    }
    // flat fill: one polygon per run of equal grade bucket
    for (bucket, i0, i1) in bucket_runs(pts) {
        let c = grade_bucket_color(bucket, theme);
        let mut poly = format!("{:.2},{base}", x(pts[i0].dist));
        for p in &pts[i0..=i1] {
            poly.push_str(&format!(" {:.2},{:.2}", x(p.dist), y(p.ele)));
        }
        poly.push_str(&format!(" {:.2},{base}", x(pts[i1].dist)));
        s.push_str(&format!(
            "<polygon points=\"{poly}\" fill=\"{c}\" stroke=\"{c}\" stroke-width=\"0.6\"/>"
        ));
    }
    // what is already ridden is covered by the surface colour at 55 %: flat, no gradient
    let rp = pts[rider.min(pts.len() - 1)];
    s.push_str(&format!("<rect x=\"0\" y=\"{pad_t}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"var(--bg-1)\" fill-opacity=\"0.55\"/>", x(rp.dist), base - pad_t));
    // 2 px outline
    let line: Vec<String> = pts
        .iter()
        .map(|p| format!("{:.2},{:.2}", x(p.dist), y(p.ele)))
        .collect();
    s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"var(--chart-ink)\" stroke-width=\"2\" stroke-linejoin=\"round\"/>", line.join(" ")));
    // km ruler: baseline rule, a tick every km, a numeral every 5 km, climbs as bars on the ruler
    s.push_str(&format!("<line x1=\"0\" x2=\"{w}\" y1=\"{base}\" y2=\"{base}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>"));
    let total_km = (total / 1000.0).floor() as i32;
    for km in 0..=total_km {
        let tx = x(f64::from(km) * 1000.0);
        let major = km % 5 == 0;
        let len = if major { 10.0 } else { 5.0 };
        s.push_str(&format!("<line x1=\"{tx:.1}\" x2=\"{tx:.1}\" y1=\"{base}\" y2=\"{:.1}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>", base + len));
        if major {
            let anchor = if km == 0 { "start" } else { "middle" };
            let tx = if km == 0 { tx + 1.0 } else { tx };
            s.push_str(&format!("<text x=\"{tx:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"600\" text-anchor=\"{anchor}\" fill=\"var(--text)\" style=\"font-variant-numeric:tabular-nums\">{km}</text>", base + 24.0));
        }
    }
    s.push_str(&format!("<text x=\"{w}\" y=\"{:.1}\" font-size=\"11\" font-weight=\"500\" text-anchor=\"end\" fill=\"var(--text-faint)\">km</text>", base + 34.0));
    // climbs: a bar of the category colour on the ruler + a flag with the category numeral
    let climbs = [
        (4_300.0, 11_100.0, ClimbCategory::Cat2, "2"),
        (20_600.0, 22_900.0, ClimbCategory::Cat4, "4"),
    ];
    for (d0, d1, cat, label) in climbs {
        let cc = climb_category_color(Some(cat), theme);
        let (x0, x1) = (x(d0), x(d1));
        s.push_str(&format!(
            "<rect x=\"{x0:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"4\" fill=\"{cc}\"/>",
            base + 1.0,
            x1 - x0
        ));
        let idx = pts.iter().position(|p| p.dist >= d0).unwrap_or(0);
        let fy = y(pts[idx].ele);
        let top = (fy - 34.0).max(4.0);
        s.push_str(&format!("<line x1=\"{x0:.1}\" x2=\"{x0:.1}\" y1=\"{fy:.1}\" y2=\"{top:.1}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>"));
        s.push_str(&format!("<rect x=\"{x0:.1}\" y=\"{top:.1}\" width=\"24\" height=\"18\" fill=\"{cc}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>"));
        s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"15\" font-weight=\"700\" text-anchor=\"middle\" fill=\"{}\">{label}</text>", x0 + 12.0, top + 14.0, ink_on(&cc)));
    }
    // rider: vertical hairline + solid disc with a paper ring
    let (rx, ry) = (x(rp.dist), y(rp.ele));
    s.push_str(&format!("<line x1=\"{rx:.1}\" x2=\"{rx:.1}\" y1=\"{pad_t}\" y2=\"{base}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>"));
    s.push_str(&format!("<circle cx=\"{rx:.1}\" cy=\"{ry:.1}\" r=\"7\" fill=\"var(--accent)\" stroke=\"var(--marker-ring)\" stroke-width=\"2.5\"/>"));
    s.push_str("</svg>");
    s
}

/// REFERENCE map: desaturated and quiet, the route in solid grade colours over a dark / light
/// casing, the rider as a solid disc with a heading wedge, square ruled controls (HTML, not here).
fn map_svg(pts: &[RoutePoint], theme: Theme, rider: usize) -> String {
    let (w, h) = (1000.0_f64, 640.0_f64);
    let mut s = format!("<svg class=\"bg\" viewBox=\"0 0 {w} {h}\" preserveAspectRatio=\"xMidYMid slice\" xmlns=\"http://www.w3.org/2000/svg\" font-family=\"Barlow\">");
    s.push_str(&format!(
        "<rect width=\"{w}\" height=\"{h}\" fill=\"var(--pm-land)\"/>"
    ));
    s.push_str("<path d=\"M0 520 C 160 470 300 560 470 520 S 760 450 1000 520 L1000 640 L0 640z\" fill=\"var(--pm-water)\"/>");
    s.push_str("<path d=\"M640 70 C 760 40 900 90 940 170 S 860 290 720 250 S 590 130 640 70z\" fill=\"var(--pm-park)\"/>");
    s.push_str("<path d=\"M60 330 C 120 290 220 300 250 360 S 190 440 110 420 S 20 380 60 330z\" fill=\"var(--pm-park)\"/>");
    // street grid: two weights, the minor streets are barely there
    for i in 0..16 {
        let x0 = 20.0 + f64::from(i) * 68.0;
        s.push_str(&format!("<line x1=\"{x0}\" y1=\"0\" x2=\"{}\" y2=\"{h}\" stroke=\"var(--pm-road2)\" stroke-width=\"{}\"/>", x0 + 110.0, 1 + (i % 3) / 2));
    }
    for i in 0..10 {
        let y0 = 30.0 + f64::from(i) * 64.0;
        s.push_str(&format!("<line x1=\"0\" y1=\"{y0}\" x2=\"{w}\" y2=\"{}\" stroke=\"var(--pm-road)\" stroke-width=\"{}\"/>", y0 - 60.0, 2 + (i % 2)));
    }
    s.push_str("<text x=\"690\" y=\"170\" font-size=\"11\" fill=\"var(--pm-label)\">Városliget</text><text x=\"380\" y=\"556\" font-size=\"11\" fill=\"var(--pm-label)\" font-style=\"italic\">Duna</text>");
    // the route projected into the box
    let n = pts.len();
    let proj = |i: usize| -> (f64, f64) {
        let t = i as f64 / (n - 1) as f64;
        let x = 90.0 + t * 820.0;
        let y = 420.0 - (t * 6.0).sin() * 95.0 - t * 150.0 + (t * 17.0).sin() * 12.0;
        (x, y)
    };
    let path: Vec<String> = (0..n)
        .step_by(2)
        .map(|i| {
            let (x, y) = proj(i);
            format!("{x:.1},{y:.1}")
        })
        .collect();
    s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"var(--route-casing)\" stroke-width=\"10\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>", path.join(" ")));
    for (bucket, i0, i1) in bucket_runs(pts) {
        let c = grade_bucket_color(bucket, theme);
        let run: Vec<String> = (i0..=i1)
            .map(|i| {
                let (x, y) = proj(i);
                format!("{x:.1},{y:.1}")
            })
            .collect();
        s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"6\" stroke-linecap=\"butt\" stroke-linejoin=\"round\"/>", run.join(" ")));
    }
    // start (hollow ring) and finish (square)
    let (sx, sy) = proj(0);
    let (fx, fy) = proj(n - 1);
    s.push_str(&format!("<circle cx=\"{sx:.1}\" cy=\"{sy:.1}\" r=\"7\" fill=\"var(--bg-1)\" stroke=\"var(--chart-ink)\" stroke-width=\"3\"/>"));
    s.push_str(&format!("<rect x=\"{:.1}\" y=\"{:.1}\" width=\"14\" height=\"14\" fill=\"var(--chart-ink)\" stroke=\"var(--bg-1)\" stroke-width=\"2\"/>", fx - 7.0, fy - 7.0));
    // rider: heading wedge first, solid disc on top
    let r = rider.min(n - 3);
    let (rx, ry) = proj(r);
    let (nx, ny) = proj(r + 2);
    let ang = (ny - ry).atan2(nx - rx);
    let tip = (rx + 26.0 * ang.cos(), ry + 26.0 * ang.sin());
    let l = (rx + 11.0 * (ang + 1.9).cos(), ry + 11.0 * (ang + 1.9).sin());
    let rr = (rx + 11.0 * (ang - 1.9).cos(), ry + 11.0 * (ang - 1.9).sin());
    s.push_str(&format!("<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"var(--accent)\" stroke=\"var(--marker-ring)\" stroke-width=\"2\" stroke-linejoin=\"miter\"/>", tip.0, tip.1, l.0, l.1, rr.0, rr.1));
    s.push_str(&format!("<circle cx=\"{rx:.1}\" cy=\"{ry:.1}\" r=\"9\" fill=\"var(--accent)\" stroke=\"var(--marker-ring)\" stroke-width=\"3\"/>"));
    // printed scale bar, bottom left
    s.push_str("<g transform=\"translate(24 600)\"><path d=\"M0 -6V0H96V-6\" fill=\"none\" stroke=\"var(--text)\" stroke-width=\"2\"/><text x=\"0\" y=\"-12\" font-size=\"11\" fill=\"var(--text)\">0</text><text x=\"96\" y=\"-12\" font-size=\"11\" text-anchor=\"end\" fill=\"var(--text)\">500 m</text></g>");
    s.push_str("</svg>");
    s
}

/// REFERENCE live chart: the last 5 minutes of power as one flat area with a 2 px ink outline,
/// a zone strip along the bottom edge (one cell per 2 s in the zone colour), an FTP rule across
/// and the scale written out at the left edge. The colour stays in the strip, the chart is quiet.
fn power_history_svg(theme: Theme, w: f64, h: f64) -> String {
    let ftp = 235.0_f64;
    let (top, strip) = (8.0, 8.0);
    let base = h - strip - 4.0;
    let max_w = 420.0_f64;
    let y = |p: f64| base - (p / max_w) * (base - top);
    let n = 150usize;
    let bw = w / n as f64;
    let power = |i: usize| {
        let t = i as f64 / n as f64;
        (205.0 + 70.0 * (t * 9.0).sin() + 38.0 * (t * 37.0).sin() + 90.0 * smooth(0.7, 0.9, t))
            .clamp(40.0, max_w - 20.0)
    };
    let zone_of = |p: f64| match p / ftp * 100.0 {
        v if v < 55.0 => 1,
        v if v <= 75.0 => 2,
        v if v <= 90.0 => 3,
        v if v <= 105.0 => 4,
        v if v <= 120.0 => 5,
        v if v <= 150.0 => 6,
        _ => 7,
    };
    let mut s = format!("<svg viewBox=\"0 0 {w} {h}\" xmlns=\"http://www.w3.org/2000/svg\" font-family=\"Barlow Condensed\">");
    let line: Vec<String> = (0..n)
        .map(|i| format!("{:.1},{:.1}", (i as f64 + 0.5) * bw, y(power(i))))
        .collect();
    s.push_str(&format!(
        "<polygon points=\"0,{base} {} {w},{base}\" fill=\"var(--bg-3)\"/>",
        line.join(" ")
    ));
    for p in [100.0, 200.0, 300.0, 400.0] {
        s.push_str(&format!(
            "<line x1=\"0\" x2=\"{w}\" y1=\"{0:.1}\" y2=\"{0:.1}\" stroke=\"var(--chart-grid)\"/>",
            y(p)
        ));
        s.push_str(&format!("<text x=\"4\" y=\"{:.1}\" font-size=\"11\" font-weight=\"500\" fill=\"var(--text-faint)\">{p:.0}</text>", y(p) - 3.0));
    }
    s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"var(--chart-ink)\" stroke-width=\"2\" stroke-linejoin=\"round\"/>", line.join(" ")));
    for i in 0..n {
        let c = power_zone_color(zone_of(power(i)), theme);
        s.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.1}\" width=\"{:.2}\" height=\"{strip}\" fill=\"{c}\"/>",
            i as f64 * bw,
            h - strip,
            bw + 0.4
        ));
    }
    s.push_str(&format!("<line x1=\"0\" x2=\"{w}\" y1=\"{0:.1}\" y2=\"{0:.1}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\" stroke-dasharray=\"4 3\"/>", y(ftp)));
    s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" font-weight=\"600\" text-anchor=\"end\" fill=\"var(--text)\">FTP {ftp:.0}</text>", w - 4.0, y(ftp) - 4.0));
    s.push_str(&format!("<line x1=\"0\" x2=\"{w}\" y1=\"{0:.1}\" y2=\"{0:.1}\" stroke=\"var(--chart-ink)\" stroke-width=\"1\"/>", base));
    s.push_str("</svg>");
    s
}

fn grade_gradient_css(theme: Theme) -> String {
    // the grade scale as a strip of hard 0.5 % steps (not a gradient wash)
    let stops: Vec<String> = (0..56)
        .map(|i| {
            let g = -12.0 + f64::from(i) * 0.5;
            let c = grade_color(g + 0.25, theme);
            format!(
                "{c} {:.3}% {:.3}%",
                f64::from(i) / 56.0 * 100.0,
                f64::from(i + 1) / 56.0 * 100.0
            )
        })
        .collect();
    format!("linear-gradient(to right, {})", stops.join(", "))
}

// ───────────────────────────── pages ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum PageKind {
    Foundations,
    Controls,
    Surfaces,
    HeadUnit,
}

#[derive(Clone, PartialEq, Props)]
struct PageProps {
    kind: PageKind,
    theme: Theme,
}

#[component]
fn Page(props: PageProps) -> Element {
    let (title, lead) = match props.kind {
        PageKind::Foundations => ("Alapok", "Meleg grafit és papírfehér, egyetlen cinóbervörös hangsúly. A szerkezetet vékony vonalak adják, nem lebegő kártyák. A színek validált palettából jönnek, minden jelentést hordozó szín mellett szám vagy szó áll."),
        PageKind::Controls => ("Vezérlők", "Négyszögletes, vonalazott elemek: gombok, címkék, fülek, kapcsolók, csúszka, mezők és a számcellák. Az állapotjelzők az egyetlen kerek elemek."),
        PageKind::Surfaces => ("Párbeszéd, értesítések, ikonok", "Éles keret és kemény eltolású árnyék, 150 ms be- és kifutás. Az ikonok 2 px-es, szögletes végű vonalból épülnek."),
        PageKind::HeadUnit => ("Kerékpárszámítógép", "Referencia-megjelenítés: teljesítmény-hős zónasávval, vonalazott 2x2 adatrács, lapos kitöltésű magassági profil km-vonalzóval és emelkedő-zászlókkal, csendes térkép."),
    };
    rsx! {
        div { class: "kp",
            header { class: "kp-head",
                div {
                    div { class: "t-eyebrow", "bike-osm-ride · design rendszer" }
                    h1 { "{title}" }
                    p { "{lead}" }
                }
                div { class: "kp-wrap",
                    span { class: "tag",
                        Icon { name: if props.theme == Theme::Dark { IconName::Moon } else { IconName::Sun }, size: 16 }
                        if props.theme == Theme::Dark { "Meleg grafit" } else { "Papírfehér" }
                    }
                    span { class: "tag", "Barlow · OFL" }
                }
            }
            match props.kind {
                PageKind::Foundations => rsx! { Foundations { theme: props.theme } },
                PageKind::Controls => rsx! { Controls {} },
                PageKind::Surfaces => rsx! { Surfaces {} },
                PageKind::HeadUnit => rsx! { HeadUnit { theme: props.theme } },
            }
        }
    }
}

#[component]
fn Foundations(theme: Theme) -> Element {
    let pts = fake_route();
    let profile = profile_svg(&pts, theme, 170, 1238.0, 232.0);
    let grade_bar = grade_gradient_css(theme);
    let surfaces = [
        ("--bg", "bg", "ablak, térkép-keret"),
        ("--bg-1", "bg-1", "panelek, diagramfelület"),
        ("--bg-2", "bg-2", "emelt sor, táblázatfej"),
        ("--bg-3", "bg-3", "lenyomott, kijelölt"),
    ];
    let inks = [
        ("--text", "text"),
        ("--text-dim", "text-dim"),
        ("--text-faint", "text-faint"),
    ];
    rsx! {
        section { class: "kp-sec",
            h2 { "Tipográfia" small { "Barlow Condensed a számokhoz és címsorokhoz, Barlow a szöveghez · 11 / 13 / 15 / 20 / 28 / 56 / 96" } }
            div { class: "kp-grid kp-g2",
                div { class: "panel kp-type",
                    span { class: "kp-cap", "96 · hős" }
                    span { class: "num num--hero", "245" span { class: "unit", "W" } }
                    span { class: "kp-cap", "56 · érték" }
                    span { class: "num num--xl", "36,7" span { class: "unit", "km/h" } }
                    span { class: "kp-cap", "28 · főcím" }
                    span { class: "num num--lg", "1:05:09" }
                    span { class: "kp-cap", "20 · cím" }
                    span { class: "num num--md", "+7,4" span { class: "unit", "%" } "  0,42" span { class: "unit", "km" } "  190" span { class: "unit", "bpm" } }
                    span { class: "kp-cap", "15 · tabuláris" }
                    span { class: "num num--sm", "111  222  333  444  555  777  888  999" }
                }
                div { class: "panel kp-type",
                    span { class: "kp-cap", "28 · t-display" }
                    span { class: "t-display", "Őrségi körút, ÁÉÍÓÖŐÚÜŰ" }
                    span { class: "kp-cap", "20 · t-title" }
                    span { class: "t-title", "Következő emelkedő" }
                    span { class: "kp-cap", "15 · t-lead" }
                    span { class: "t-lead", "Húzd ide a GPX-fájlt, vagy illessz be egy bikerouter-linket." }
                    span { class: "kp-cap", "13 · szöveg" }
                    span { class: "t-body", "Az útvonal 24,1 km hosszú, 410 m szinttel; árvíztűrő tükörfúrógép, áéíóöőúüű." }
                    span { class: "kp-cap", "11 · címke" }
                    span { class: "t-eyebrow", "Teljesítmény, 3 mp átlag" }
                    span { class: "kp-cap", "kbd" }
                    span { class: "row", span { class: "kbd", "Szóköz" } span { class: "t-caption", "indítás / szünet" } span { class: "kbd", "F11" } span { class: "kbd", "Esc" } }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Felületek, tinta, jelzőszínek" small { "az egyetlen hangsúly: --accent" } }
            div { class: "cells cells--4",
                for (var , name , use_) in surfaces {
                    div { class: "kp-swatch", style: "background: var({var})", span { "{name}" } small { "{use_}" } }
                }
                for (var , name) in inks {
                    div { class: "kp-swatch", style: "background: var(--bg-1); color: var({var})", span { "{name}" } small { "Aa 0123456789" } }
                }
                div { class: "kp-swatch", style: "background: var(--accent); color: var(--accent-ink)", span { "accent" } small { "elsődleges gomb, fókusz, kerékpáros" } }
                div { class: "kp-swatch", style: "background: var(--bg-1); color: var(--ok)", span { "ok" } small { "Csatlakoztatva" } }
                div { class: "kp-swatch", style: "background: var(--bg-1); color: var(--warn)", span { "warn" } small { "Gyenge jel" } }
                div { class: "kp-swatch", style: "background: var(--bg-1); color: var(--crit)", span { "crit" } small { "Megszakadt" } }
                div { class: "kp-swatch", style: "background: var(--bg-1); color: var(--text-dim)", span { "rule / rule-strong" } small { "vonalak, mezőszegély" } }
            }
        }
        section { class: "kp-sec",
            h2 { "Zónák" small { "teljesítmény Z1-Z7 (Coggan), pulzus H1-H5" } }
            div { class: "kp-grid kp-g2",
                div { class: "panel",
                    div { class: "panel__head", span { "Teljesítményzónák, % FTP" } span { "Z1-Z7" } }
                    div { class: "kp-zones",
                        for (i , name) in POWER_ZONE_NAMES.iter().enumerate() {
                            div { class: "kp-zone", style: "--c: {power_zone_var((i + 1) as u8)}",
                                b { "Z{i + 1}" }
                                div { class: "bar", style: "width: {38 + i * 9}%" }
                                span { class: "nm", "{name}" }
                            }
                        }
                    }
                }
                div { class: "panel",
                    div { class: "panel__head", span { "Pulzuszónák, % max. pulzus" } span { "H1-H5" } }
                    div { class: "kp-zones",
                        for (i , name) in HR_ZONE_NAMES.iter().enumerate() {
                            div { class: "kp-zone", style: "--c: {hr_zone_var((i + 1) as u8)}",
                                b { "H{i + 1}" }
                                div { class: "bar", style: "width: {44 + i * 12}%" }
                                span { class: "nm", "{name}" }
                            }
                        }
                    }
                    div { class: "panel__head", style: "border-top: 1px solid var(--rule)", span { "Emelkedő-kategóriák" } span { "4 → HC" } }
                    div { class: "kp-wrap", style: "padding: 12px 16px",
                        for cat in [Some(ClimbCategory::Cat4), Some(ClimbCategory::Cat3), Some(ClimbCategory::Cat2), Some(ClimbCategory::Cat1), Some(ClimbCategory::Hc), None] {
                            span { class: "tag tag--data", style: "--c: {climb_category_var(cat)}",
                                span { class: "tag__dot" }
                                "{climb_category_label(cat)}"
                            }
                        }
                    }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Lejtőskála" small { "divergáló, semleges szürke 0 % körül; a térkép és a profil ugyanezt használja" } }
            div { class: "panel", style: "padding: 16px",
                div { class: "kp-gradebar", style: "background: {grade_bar}" }
                div { class: "kp-ticks",
                    for g in [-12, -8, -4, 0, 4, 8, 12, 16] {
                        span { "{g:+} %" }
                    }
                }
                div { class: "kp-wrap", style: "margin-top: 16px",
                    for (g , name) in [(-8.0, "lejtő -8 %"), (0.0, "sík"), (3.0, "+3 %"), (6.0, "+6 %"), (9.5, "+9,5 %"), (13.0, "+13 %")] {
                        span { class: "tag tag--data", style: "--c: {grade_color(g, theme)}",
                            span { class: "tag__dot" }
                            "{name}"
                        }
                    }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Magassági profil" small { "lapos lejtőszín-kitöltés, 2 px körvonal, km-vonalzó, emelkedő-zászlók, tömör kerékpáros-jelölő" } }
            div { class: "panel kp-profile", dangerous_inner_html: "{profile}" }
        }
    }
}

#[component]
fn Controls() -> Element {
    let mut mode = use_signal(|| "ahead".to_string());
    let mut style = use_signal(|| "dark".to_string());
    let mut tab = use_signal(|| "stats".to_string());
    let mut follow = use_signal(|| true);
    let mut sim = use_signal(|| true);
    let mut grade_scale = use_signal(|| 85.0_f64);
    let mut ftp = use_signal(|| 235.0_f64);
    let mut weight = use_signal(|| 75.0_f64);
    let mut cda = use_signal(|| 0.32_f64);
    let mut link = use_signal(String::new);
    rsx! {
        section { class: "kp-sec",
            h2 { "Gombok" small { "32 px, 3 px sarok, 1 px keret; kiválasztva: megfordított tinta; elsődleges: az egyetlen színes gomb" } }
            div { class: "kp-grid kp-g2",
                div { class: "panel kp-form",
                    div { class: "kp-wrap",
                        button { class: "btn btn--primary", Icon { name: IconName::Play, size: 16 } "Indítás" }
                        button { class: "btn", Icon { name: IconName::Folder, size: 16 } "GPX megnyitása" }
                        button { class: "btn btn--ghost", Icon { name: IconName::Link, size: 16 } "Link beillesztése" }
                        button { class: "btn btn--danger", Icon { name: IconName::Trash, size: 16 } "Törlés" }
                        button { class: "btn", disabled: true, "Letiltva" }
                    }
                    div { class: "kp-wrap",
                        button { class: "btn btn--primary btn--lg", Icon { name: IconName::Bike, size: 20 } "Tekerés" }
                        button { class: "btn btn--sm", Icon { name: IconName::Download, size: 16 } "TCX mentése" }
                        button { class: "btn btn--primary btn--sm", "Kész" }
                        button { class: "btn btn--icon", "aria-label": "Szünet", Icon { name: IconName::Pause, size: 16 } }
                        button { class: "btn btn--icon is-active", "aria-pressed": "true", "aria-label": "Követés", Icon { name: IconName::Locate, size: 20 } }
                        button { class: "btn btn--ghost btn--icon", "aria-label": "Beállítások", Icon { name: IconName::Settings, size: 20 } }
                    }
                    div { class: "kp-wrap",
                        div { class: "btn-group",
                            button { class: "btn btn--icon", "aria-label": "Nagyítás", Icon { name: IconName::Plus, size: 20 } }
                            button { class: "btn btn--icon", "aria-label": "Kicsinyítés", Icon { name: IconName::Minus, size: 20 } }
                            button { class: "btn btn--icon is-active", "aria-pressed": "true", "aria-label": "Követés", Icon { name: IconName::Locate, size: 20 } }
                            button { class: "btn btn--icon", "aria-label": "Rétegek", Icon { name: IconName::Layers, size: 20 } }
                        }
                        div { class: "btn-group",
                            button { class: "btn btn--icon", "aria-label": "Lejátszás", Icon { name: IconName::Play, size: 16 } }
                            button { class: "btn btn--icon", "aria-label": "Szünet", Icon { name: IconName::Pause, size: 16 } }
                            button { class: "btn btn--icon", "aria-label": "Leállítás", Icon { name: IconName::Stop, size: 16 } }
                        }
                    }
                }
                div { class: "panel kp-form",
                    div { class: "kp-wrap",
                        span { class: "tag", "Címke" }
                        span { class: "tag tag--solid", "Kijelölt" }
                        span { class: "tag tag--data", style: "--c: var(--z4)", span { class: "tag__dot" } "Z4 Küszöb" }
                        span { class: "tag tag--data", style: "--c: var(--h3)", span { class: "tag__dot" } "H3 Aerob" }
                        span { class: "tag tag--data", style: "--c: var(--cat-2)", span { class: "tag__dot" } "2. kat." }
                        button { class: "chip", "aria-label": "Útvonal váltása", Icon { name: IconName::Route, size: 16 } "Útvonal" Icon { name: IconName::ChevronDown, size: 16 } }
                    }
                    div { class: "kp-wrap",
                        span { class: "badge badge--ok", "Csatlakoztatva" }
                        span { class: "badge badge--warn", "Gyenge jel" }
                        span { class: "badge badge--crit", "Megszakadt" }
                        span { class: "badge", "Nincs eszköz" }
                    }
                    div { class: "kp-wrap",
                        span { class: "dot dot--ok" } span { class: "t-caption", "trainer" }
                        span { class: "dot dot--busy" } span { class: "t-caption", "pulzus (kapcsolódás)" }
                        span { class: "dot dot--crit" } span { class: "t-caption", "teljesítménymérő" }
                        span { class: "dot" } span { class: "t-caption", "nincs" }
                    }
                    div { class: "kp-wrap",
                        span { class: "kbd", "Szóköz" } span { class: "t-caption", "indítás" }
                        span { class: "kbd", "M" } span { class: "t-caption", "térkép követ" }
                        span { class: "kbd", "T" } span { class: "t-caption", "téma" }
                        span { class: "kbd", "F11" } span { class: "t-caption", "teljes képernyő" }
                    }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Számcellák" small { "közös 1 px vonalak, nincs kártya; a számok kondenzált, tabuláris számjegyek, az egység a számjegy alapvonalán" } }
            div { class: "kp-grid kp-g2",
                div { class: "cells cells--2",
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Ütem" } span { "átl. 86" } }
                        div { class: "cell__value", "88" span { class: "cell__unit", "rpm" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Pulzus" } span { "Z3" } }
                        div { class: "cell__value", "152" span { class: "cell__unit", "bpm" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Sebesség" } span { "max. 52,1" } }
                        div { class: "cell__value", "36,7" span { class: "cell__unit", "km/h" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Lejtő" } span { "10 m előre" } }
                        div { class: "cell__value", "+6,8" span { class: "cell__unit", "%" } }
                    }
                }
                div { class: "cells cells--4",
                    for (l , v , u) in [("Idő", "1:05:09", ""), ("Táv", "24,1", "km"), ("Szint", "410", "m"), ("Energia", "812", "kcal"), ("NP", "241", "W"), ("IF", "1,03", ""), ("TSS", "69", ""), ("Hátra", "0:34", "h")] {
                        div { class: "cell cell--sm",
                            div { class: "cell__label", "{l}" }
                            div { class: "cell__value", "{v}" if !u.is_empty() { span { class: "cell__unit", "{u}" } } }
                        }
                    }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Fülek, szegmentált vezérlő, kapcsolók, csúszka, mezők" }
            div { class: "kp-grid kp-g2",
                div { class: "panel kp-form",
                    Tabs {
                        options: vec![("stats".to_string(), "Statisztika".to_string()), ("zones".to_string(), "Zónák".to_string()), ("laps".to_string(), "Körök".to_string())],
                        value: tab(),
                        on_change: move |v| tab.set(v),
                        aria_label: "Panel".to_string(),
                    }
                    div { class: "kp-rows",
                        span { class: "label", "Profil nézet" }
                        Segmented {
                            options: vec![("full".to_string(), "Teljes".to_string()), ("ahead".to_string(), "Előretekintés".to_string())],
                            value: mode(),
                            on_change: move |v| mode.set(v),
                            aria_label: "Profil nézet".to_string(),
                        }
                        span { class: "label", "Térképstílus" }
                        Segmented {
                            options: vec![("dark".to_string(), "Sötét".to_string()), ("light".to_string(), "Világos".to_string()), ("osm".to_string(), "OSM".to_string())],
                            value: style(),
                            on_change: move |v| style.set(v),
                            aria_label: "Térképstílus".to_string(),
                            small: true,
                        }
                    }
                    div { class: "divider" }
                    div { class: "kp-rows",
                        Switch { checked: follow(), on_change: move |v| follow.set(v), label: "A térkép kövesse a kerékpárost".to_string(), hint: "M: be- és kikapcsolás".to_string(), row: true }
                        Switch { checked: sim(), on_change: move |v| sim.set(v), label: "Szimulált kerékpáros".to_string(), row: true }
                        Switch { checked: false, on_change: move |_| {}, label: "Trainer sebesség használata".to_string(), row: true }
                        Switch { checked: true, on_change: move |_| {}, label: "Letiltott kapcsoló".to_string(), disabled: true, row: true }
                    }
                }
                div { class: "panel kp-form",
                    Slider { label: "Lejtés nehézsége".to_string(), value: grade_scale(), min: 0.0, max: 150.0, step: 5.0, unit: "%".to_string(), on_change: move |v| grade_scale.set(v), hint: "A trainerre küldött lejtő aránya a valódihoz képest.".to_string() }
                    Slider { label: "Előretekintés".to_string(), value: 3000.0, min: 500.0, max: 10000.0, step: 100.0, unit: "m".to_string(), on_change: move |_| {} }
                    div { class: "kp-grid kp-g2",
                        NumberField { label: "FTP".to_string(), value: ftp(), min: 50.0, max: 600.0, step: 5.0, unit: "W".to_string(), on_change: move |v| ftp.set(v) }
                        NumberField { label: "Testsúly".to_string(), value: weight(), min: 30.0, max: 200.0, step: 0.5, unit: "kg".to_string(), on_change: move |v| weight.set(v) }
                        NumberField { label: "CdA".to_string(), value: cda(), min: 0.1, max: 1.0, step: 0.01, unit: "m²".to_string(), on_change: move |v| cda.set(v), hint: "Légellenállási tényező".to_string() }
                        NumberField { label: "Max. pulzus".to_string(), value: 190.0, min: 120.0, max: 230.0, step: 1.0, unit: "bpm".to_string(), on_change: move |_| {} }
                    }
                    TextField { label: "Bikerouter-link".to_string(), value: link(), on_input: move |v| link.set(v), placeholder: "https://bikerouter.de/#map=...".to_string(), hint: "Illessz be egy megosztott linket.".to_string() }
                    TextField { label: "Hibás link".to_string(), value: "https://example.com/valami".to_string(), on_input: move |_| {}, error: Some("Ez nem bikerouter.de link.".to_string()) }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Zónasáv" small { "7 egyenlő lépcső, az aktuális zóna magas, FTP-jelző; a szín sosem halványul" } }
            div { class: "panel kp-form",
                for (zone , ftp_pos) in [(2u8, 50), (4, 50), (7, 50)] {
                    div { class: "kp-rows",
                        div { class: "zonebar", style: "--ftp: {ftp_pos}",
                            for z in 1..=7u8 {
                                span { class: if z == zone { "zonebar__seg is-current" } else { "zonebar__seg" }, style: "--c: {power_zone_var(z)}" }
                            }
                            span { class: "zonebar__ftp" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Surfaces() -> Element {
    let mut toasts = use_signal(|| {
        vec![
            Toast::new(
                1,
                ToastKind::Success,
                "A TCX-fájl elmentve: Budai-hegység_2026-10-31.tcx",
            ),
            Toast::new(
                2,
                ToastKind::Info,
                "Szimulált kerékpáros csatlakoztatva, 235 W.",
            ),
            Toast::new(
                3,
                ToastKind::Warning,
                "A pulzusmérő jele gyenge, újracsatlakozás...",
            ),
            Toast::new(
                4,
                ToastKind::Error,
                "Nem sikerült megnyitni a fájlt: hibás GPX (3. sor).",
            ),
        ]
    });
    rsx! {
        section { class: "kp-sec",
            h2 { "Párbeszédablak és értesítések" small { "éles 1 px keret, 2 px tinta felső vonal, kemény eltolású árnyék" } }
            div { class: "kp-grid kp-g2",
                div { class: "kp-stage",
                    Dialog {
                        open: true,
                        title: "Beállítások".to_string(),
                        on_close: move |_| {},
                        foot: rsx! {
                            button { class: "btn btn--ghost", "Mégse" }
                            button { class: "btn btn--primary", "Kész" }
                        },
                        div { class: "stack",
                            span { class: "t-eyebrow", "Kerékpáros" }
                            div { class: "kp-grid kp-g2",
                                NumberField { label: "Testsúly".to_string(), value: 75.0, min: 30.0, max: 200.0, step: 0.5, unit: "kg".to_string(), on_change: move |_| {} }
                                NumberField { label: "Kerékpár".to_string(), value: 9.0, min: 3.0, max: 30.0, step: 0.1, unit: "kg".to_string(), on_change: move |_| {} }
                            }
                            Slider { label: "Lejtés nehézsége".to_string(), value: 100.0, min: 0.0, max: 150.0, step: 5.0, unit: "%".to_string(), on_change: move |_| {} }
                            Switch { checked: true, on_change: move |_| {}, label: "Azonnali alkalmazás".to_string(), hint: "A módosítások rögtön érvénybe lépnek.".to_string(), row: true }
                        }
                    }
                }
                div { class: "kp-stage", style: "align-content: end",
                    ToastHost { toasts, on_dismiss: move |id| toasts.write().retain(|t| t.id != id) }
                }
            }
        }
        section { class: "kp-sec",
            h2 { "Ikonok" small { "24 px rács, 2 px vonal, szögletes vég és csúcs, currentColor" } }
            div { class: "kp-icons",
                for n in IconName::ALL {
                    div { class: "kp-icon", Icon { name: n, size: 32 } span { "{n.name()}" } }
                }
            }
            div { class: "panel", style: "display:flex; gap: 24px; align-items: center; flex-wrap: wrap; padding: 12px 16px",
                span { class: "kp-cap", "16 · 20 · 24 · 32 · 48 px" }
                for s in [16u32, 20, 24, 32, 48] {
                    span { Icon { name: IconName::Mountain, size: s } }
                }
                span { style: "color: var(--accent)", Icon { name: IconName::Bolt, size: 32 } }
                span { style: "color: var(--crit)", Icon { name: IconName::Heart, size: 32 } }
                span { style: "color: var(--z4)", Icon { name: IconName::Gauge, size: 32 } }
            }
        }
        section { class: "kp-sec",
            h2 { "Állapotok: üres, betöltés, tooltip, haladás" }
            div { class: "kp-grid kp-g3",
                div { class: "panel empty",
                    Icon { name: IconName::Route, size: 32 }
                    div { class: "t-title", "Még nincs útvonal" }
                    div { class: "t-caption", "Tölts be egy GPX-fájlt vagy illessz be egy linket." }
                    button { class: "btn btn--primary btn--sm", style: "margin-top: 8px", Icon { name: IconName::Folder, size: 16 } "Fájl megnyitása" }
                }
                div { class: "panel kp-form",
                    div { class: "row", span { class: "spinner" } span { class: "t-body", "Útvonal feldolgozása..." } }
                    div { class: "progress", style: "--p: 72", i {} }
                    div { class: "progress", style: "--p: 38; --c: var(--z3)", i {} }
                    div { class: "progress", style: "--p: 91; --c: var(--crit)", i {} }
                }
                div { class: "panel", style: "display:grid; place-items:center; min-height: 160px",
                    div { class: "tip", style: "pointer-events:auto",
                        div { class: "tip__label", "12,40 km" }
                        div { class: "tip__value", "+6,8 %" }
                        div { class: "tip__label", "Magasság 263 m" }
                    }
                }
            }
        }
    }
}

#[component]
fn HeadUnit(theme: Theme) -> Element {
    let pts = fake_route();
    let rider = 170;
    let profile = profile_svg(&pts, theme, rider, 1238.0, 200.0);
    let map = map_svg(&pts, theme, rider);
    let history = power_history_svg(theme, 378.0, 104.0);
    let rp = pts[rider];
    let climb = format!("{:.1}", 3.9).replace('.', ",");
    let _ = rp;
    rsx! {
        div { class: "hu",
            div { class: "hu-top",
                span { class: "hu-brand", "bike-osm-ride" }
                div { class: "grow",
                    span { class: "tag tag--solid", Icon { name: IconName::Route, size: 16 } "Budai-hegység kör" }
                    span { class: "t-caption", "24,1 km · +410 m" }
                }
                span { class: "badge badge--ok", "Trainer" }
                span { class: "badge badge--ok", "Pulzus" }
                span { class: "badge", "Teljesítménymérő" }
                div { class: "btn-group",
                    button { class: "btn btn--icon btn--sm", "aria-label": "Beállítások", Icon { name: IconName::Settings, size: 16 } }
                    button { class: "btn btn--icon btn--sm", "aria-label": "Téma", Icon { name: IconName::Sun, size: 16 } }
                    button { class: "btn btn--icon btn--sm", "aria-label": "Névjegy", Icon { name: IconName::Info, size: 16 } }
                }
            }
            div { class: "hu-map",
                div { dangerous_inner_html: "{map}" }
                div { class: "hu-climb",
                    div { class: "panel__head", span { "Következő emelkedő" } span { class: "tag tag--data", style: "--c: var(--cat-2)", span { class: "tag__dot" } "2. kat." } }
                    div { style: "padding: 8px 12px 12px",
                        div { class: "num num--xl", "1,24" span { class: "unit", "km múlva" } }
                    }
                    div { class: "cells cells--3 cells--flush", style: "border-top: 1px solid var(--rule)",
                        div { class: "cell cell--sm", div { class: "cell__label", "Hossz" } div { class: "cell__value", "{climb}" span { class: "cell__unit", "km" } } }
                        div { class: "cell cell--sm", div { class: "cell__label", "Átlag" } div { class: "cell__value", "6,8" span { class: "cell__unit", "%" } } }
                        div { class: "cell cell--sm", div { class: "cell__label", "Szint" } div { class: "cell__value", "264" span { class: "cell__unit", "m" } } }
                    }
                    div { class: "progress", style: "--p: 34; --c: var(--cat-2)", i {} }
                }
                div { class: "hu-ctl btn-group btn-group--v",
                    button { class: "btn btn--icon", "aria-label": "Nagyítás", Icon { name: IconName::Plus, size: 20 } }
                    button { class: "btn btn--icon", "aria-label": "Kicsinyítés", Icon { name: IconName::Minus, size: 20 } }
                    button { class: "btn btn--icon is-active", "aria-pressed": "true", "aria-label": "Követés", Icon { name: IconName::Locate, size: 20 } }
                    button { class: "btn btn--icon", "aria-label": "Rétegek", Icon { name: IconName::Layers, size: 20 } }
                }
                div { class: "hu-attr", "© OpenStreetMap-közreműködők" }
            }
            div { class: "hu-rail",
                div { class: "hu-hero",
                    div { class: "head", span { "Teljesítmény" } span { "3 mp átlag 238 W" } }
                    div { class: "val",
                        span { class: "num num--hero", style: "color: var(--text)", "245" span { class: "unit", "W" } }
                    }
                    div { class: "zonebar", style: "--ftp: 50",
                        for z in 1..=7u8 {
                            span { class: if z == 4 { "zonebar__seg is-current" } else { "zonebar__seg" }, style: "--c: {power_zone_var(z)}" }
                        }
                        span { class: "zonebar__ftp" }
                    }
                    div { class: "zlabels",
                        for z in 1..=7u8 {
                            span { class: if z == 4 { "is-current" } else { "" }, "Z{z}" }
                        }
                    }
                    div { class: "meta",
                        span { b { "Z4" } " " { power_zone_name(4) } }
                        span { class: "num num--sm", "3,27" span { class: "unit", "W/kg" } }
                    }
                }
                div { class: "cells cells--2 cells--flush",
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Ütem" } span { "átl. 86" } }
                        div { class: "cell__value", "88" span { class: "cell__unit", "rpm" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Pulzus" } span { "H3 Aerob" } }
                        div { class: "cell__value", "152" span { class: "cell__unit", "bpm" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Sebesség" } span { "max. 52,1" } }
                        div { class: "cell__value", "36,7" span { class: "cell__unit", "km/h" } }
                    }
                    div { class: "cell cell--md",
                        div { class: "cell__label", span { "Lejtő" } span { "előre 10 m" } }
                        div { class: "cell__value", "+6,8" span { class: "cell__unit", "%" } }
                    }
                }
                div { class: "hu-chart",
                    div { class: "panel__head", span { "Teljesítmény, utolsó 5 perc" } span { "W" } }
                    div { dangerous_inner_html: "{history}" }
                }
                div { class: "cells cells--3 cells--flush hu-stats",
                    for (l , v , u) in [("Idő", "1:05:09", ""), ("Táv", "9,8", "km"), ("Szint", "264", "m"), ("Energia", "812", "kcal"), ("NP", "241", "W"), ("Hátra", "0:34", "h")] {
                        div { class: "cell cell--sm",
                            div { class: "cell__label", "{l}" }
                            div { class: "cell__value", "{v}" if !u.is_empty() { span { class: "cell__unit", "{u}" } } }
                        }
                    }
                }
            }
            div { class: "hu-profile",
                div { class: "bar",
                    div { class: "hu-transport",
                        div { class: "btn-group",
                            button { class: "btn btn--icon btn--sm", "aria-label": "Szünet", Icon { name: IconName::Pause, size: 16 } }
                            button { class: "btn btn--icon btn--sm", "aria-label": "Leállítás", Icon { name: IconName::Stop, size: 16 } }
                        }
                        span { class: "kbd", style: "margin-left: 12px", "Szóköz" }
                    }
                    Segmented {
                        options: vec![("full".to_string(), "Teljes".to_string()), ("ahead".to_string(), "Előretekintés".to_string())],
                        value: "full".to_string(),
                        on_change: move |_| {},
                        aria_label: "Profil nézet".to_string(),
                        small: true,
                    }
                }
                div { style: "flex: 1; min-height: 0; display: flex", dangerous_inner_html: "{profile}" }
            }
        }
    }
}

// ───────────────────────────── driver ─────────────────────────────

fn render(kind: PageKind, theme: Theme) -> String {
    let mut vdom = VirtualDom::new_with_props(Page, PageProps { kind, theme });
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[PREVIEW_CSS], &body)
}

fn main() {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    std::fs::create_dir_all(&dir).expect("create target/preview");
    for (kind, name) in [
        (PageKind::Foundations, "foundations"),
        (PageKind::Controls, "controls"),
        (PageKind::Surfaces, "surfaces"),
        (PageKind::HeadUnit, "headunit"),
    ] {
        for (theme, tname) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
            let path = dir.join(format!("ride-ui-kit-{name}-{tname}.html"));
            std::fs::write(&path, render(kind, theme)).expect("write preview");
            println!("{}", path.display());
        }
    }
    let _ = palette::GRADE_BUCKETS;
    let _ = straight_route(&[(100.0, 1.0)], 0.0);
}
