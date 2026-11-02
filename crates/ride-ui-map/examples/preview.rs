//! Server-side rendered map preview for both themes, written to
//! `target/preview/ride-ui-map-<theme>.html`:
//!
//!   cargo run -p ride-ui-map --example preview
//!
//! The map is rendered with a realistic fake ride: a ~24 km Budapest-area loop with rolling
//! hills and a 4.3 km / ~6.6 % climb, a rider 11.4 km into the ride. The tiles point at the
//! app's local proxy (the `tiles` custom protocol), so they simply fail to load in a static
//! page: nothing here touches the network and the preview asserts nothing about tiles.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{
    Bounds, Climb, ClimbCategory, MapStyle, RideSnapshot, RideState, Route, RoutePoint,
    RouteSource, Theme,
};
use ride_ui_kit::html_document;
use ride_ui_map::MapView;

/// Page frame around the map; the map itself is styled by `ride_ui_map::CSS`.
const PREVIEW_CSS: &str = r#"
.pv { min-height: 100vh; display: grid; justify-items: center; align-content: center; gap: 16px;
      padding: 24px; background: var(--bg); }
.pv-stage { position: relative; width: 1280px; height: 720px; overflow: hidden;
            border-radius: var(--radius-xl); border: 1px solid var(--border-strong);
            box-shadow: var(--shadow-2); background: var(--bg-1); }
.pv-cap { display: flex; align-items: center; gap: 12px; width: 1280px; }
.pv-cap b { font: 600 13px/1.2 var(--font-ui); color: var(--text); }
.pv-cap span { font: 500 12px/1.2 var(--font-ui); color: var(--text-dim); }
"#;

// ───────────────────────────── fake ride data ─────────────────────────────

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    if edge1 <= edge0 {
        return 0.0;
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Elevation of the fake route at `d` metres, with a 4.3 km climb, a long descent and a kicker.
fn elevation(d: f64) -> f64 {
    let climb = smooth(3_500.0, 7_800.0, d) * 285.0;
    let descent = smooth(8_400.0, 12_400.0, d) * -240.0;
    let kicker = smooth(18_000.0, 19_500.0, d) * 62.0;
    let roll = (d / 1_900.0 * std::f64::consts::TAU).sin() * 6.0
        + (d / 700.0 * std::f64::consts::TAU).sin() * 3.5;
    118.0 + climb + descent + kicker + roll
}

/// Smoothed grade at `d` in percent (central difference over 100 m, like the route pipeline).
fn grade_at(d: f64) -> f64 {
    (elevation(d + 50.0) - elevation(d - 50.0)).clamp(-25.0, 25.0)
}

/// Plausible Budapest-area road: a wide arc with gentle S-curves from the Buda side towards
/// Gödöllő, sampled densely; `dist` is the real cumulative distance of the polyline.
fn fake_route() -> Route {
    const STEPS: usize = 3_000;
    const TARGET_M: f64 = 24_000.0;
    let mut points: Vec<RoutePoint> = Vec::with_capacity(STEPS);
    let mut lat: f64 = 47.4400;
    let mut lon: f64 = 18.9400;
    let mut dist: f64 = 0.0;
    for step in 0..=STEPS {
        let t = step as f64 / STEPS as f64;
        let next_lat = 47.4400 + 0.1600 * t + 0.0060 * (3.0 * std::f64::consts::TAU * t).sin();
        let next_lon =
            18.9400 + 0.1700 * t + 0.0110 * (2.0 * std::f64::consts::TAU * t + 0.6).sin();
        if step > 0 {
            // haversine is overkill for a preview, an equirectangular step is accurate enough
            let metres_per_deg_lat = 111_320.0;
            let metres_per_deg_lon = 111_320.0 * lat.to_radians().cos();
            let dy = (next_lat - lat) * metres_per_deg_lat;
            let dx = (next_lon - lon) * metres_per_deg_lon;
            dist += (dx * dx + dy * dy).sqrt();
        }
        points.push(RoutePoint {
            lat: next_lat,
            lon: next_lon,
            ele: elevation(dist),
            dist,
            grade: grade_at(dist),
        });
        lat = next_lat;
        lon = next_lon;
    }
    // Trim the tail so the preview route is ~24 km regardless of the exact curve length.
    if let Some(cut) = points.iter().position(|p| p.dist >= TARGET_M) {
        let keep = cut.max(2);
        points.truncate(keep);
        if let Some(last) = points.last_mut() {
            last.dist = dist.min(TARGET_M);
            last.grade = grade_at(last.dist);
        }
    }
    let total = points.last().map_or(0.0, |p| p.dist);
    let mut south = f64::MAX;
    let mut north = f64::MIN;
    let mut west = f64::MAX;
    let mut east = f64::MIN;
    let mut ascent = 0.0;
    let mut descent = 0.0;
    let mut min_ele = f64::MAX;
    let mut max_ele = f64::MIN;
    let mut previous_ele: Option<f64> = None;
    for point in &points {
        south = south.min(point.lat);
        north = north.max(point.lat);
        west = west.min(point.lon);
        east = east.max(point.lon);
        min_ele = min_ele.min(point.ele);
        max_ele = max_ele.max(point.ele);
        if let Some(previous) = previous_ele {
            let delta = point.ele - previous;
            if delta > 0.0 {
                ascent += delta;
            } else {
                descent -= delta;
            }
        }
        previous_ele = Some(point.ele);
    }
    let climb_start = 3_500.0_f64.min(total * 0.9);
    let climb_end = 7_800.0_f64.min(total * 0.95);
    let kicker_start = 18_000.0_f64.min(total * 0.95);
    let kicker_end = 19_500.0_f64.min(total * 0.98);
    let climbs = vec![
        Climb {
            start_dist: climb_start,
            end_dist: climb_end,
            length: (climb_end - climb_start).max(0.0),
            gain: 285.0,
            avg_grade: 6.6,
            max_grade: 9.9,
            start_ele: elevation(climb_start),
            end_ele: elevation(climb_end),
            category: Some(ClimbCategory::Cat3),
        },
        Climb {
            start_dist: kicker_start,
            end_dist: kicker_end,
            length: (kicker_end - kicker_start).max(0.0),
            gain: 62.0,
            avg_grade: 4.1,
            max_grade: 6.4,
            start_ele: elevation(kicker_start),
            end_ele: elevation(kicker_end),
            category: None,
        },
    ];
    Route {
        id: "preview-budapest-hills".into(),
        name: "Budapest–Gödöllő dombjai".into(),
        source: RouteSource::Demo,
        total_dist: total,
        ascent,
        descent,
        min_ele: if min_ele.is_finite() { min_ele } else { 0.0 },
        max_ele: if max_ele.is_finite() { max_ele } else { 0.0 },
        bounds: Bounds {
            south: if south.is_finite() { south } else { 0.0 },
            west: if west.is_finite() { west } else { 0.0 },
            north: if north.is_finite() { north } else { 0.0 },
            east: if east.is_finite() { east } else { 0.0 },
        },
        climbs,
        waypoints: vec![],
        has_elevation: true,
        segments: Vec::new(),
        timeline: Vec::new(),
        points,
    }
}

/// Mid-ride snapshot: 11.4 km in, on the descent after the big climb.
fn fake_snapshot(route: &Route) -> RideSnapshot {
    let dist = 11_400.0_f64.min(route.total_dist);
    let pos = route.locate(dist);
    RideSnapshot {
        state: RideState::Riding,
        pos,
        speed_ms: 7.6,
        power_w: Some(231.0),
        power_3s: Some(226.0),
        cadence_rpm: Some(84.0),
        hr_bpm: Some(151.0),
        resistance: Some(3.0),
        elapsed_s: 1_884.0,
        dist,
        remaining_dist: (route.total_dist - dist).max(0.0),
        eta_s: Some(1_960.0),
        ascent_done: 312.0,
        descent_done: 118.0,
        ascent_left: 128.0,
        avg_speed_ms: 6.05,
        max_speed_ms: 13.4,
        avg_power_w: Some(214.0),
        max_power_w: Some(486.0),
        normalized_power_w: Some(238.0),
        work_kj: 402.0,
        kcal: 405.0,
        avg_hr: Some(146.0),
        max_hr: Some(168.0),
        power_zone: Some(4),
        hr_zone: Some(4),
        next_climb: Some(1),
        next_climb_in: Some((18_000.0 - dist).max(0.0)),
        trainer_grade: -2.4,
        workout: None,
        ghost: Some(ride_model::GhostLive {
            dist: (dist + 240.0).min(route.total_dist),
            pos: route.locate((dist + 240.0).min(route.total_dist)),
            delta_s: Some(12.0),
        }),
        segment: None,
    }
}

// ───────────────────────────── page ─────────────────────────────

#[derive(Clone, PartialEq, Props)]
struct PreviewProps {
    theme: Theme,
}

#[component]
fn Preview(props: PreviewProps) -> Element {
    // Signals are created here so `MapView` receives exactly what the shell passes it.
    let route_data: Arc<Route> = use_hook(|| Arc::new(fake_route()));
    let initial_snapshot = fake_snapshot(&route_data);
    let route_signal_data = Arc::clone(&route_data);
    let route = use_signal(move || Some(route_signal_data));
    let snapshot = use_signal(move || initial_snapshot);
    let style = use_signal(|| match props.theme {
        Theme::Dark => MapStyle::Dark,
        Theme::Light => MapStyle::Light,
    });
    let follow = use_signal(|| false);
    let style_label = match props.theme {
        Theme::Dark => "Sötét térkép",
        Theme::Light => "Világos térkép",
    };
    rsx! {
        div { class: "pv",
            div { class: "pv-stage",
                MapView {
                    route,
                    snapshot,
                    style,
                    follow,
                    on_style: move |_| {},
                    on_follow: move |_| {},
                }
            }
            div { class: "pv-cap",
                b { "ride-ui-map" }
                span { "{style_label} · 24 km-es budapesti kör, 11,4 km-nél · csempék a helyi proxyból (statikus előnézetben nem töltődnek be)" }
            }
        }
    }
}

fn render(theme: Theme) -> String {
    let mut vdom = VirtualDom::new_with_props(Preview, PreviewProps { theme });
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[ride_ui_map::CSS, PREVIEW_CSS], &body)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    std::fs::create_dir_all(&dir)?;
    for (theme, name) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
        let path = dir.join(format!("ride-ui-map-{name}.html"));
        std::fs::write(&path, render(theme))?;
        println!("{}", path.display());
    }
    Ok(())
}
