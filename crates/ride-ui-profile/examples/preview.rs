//! SSR preview of the elevation profile and the live charts, rendered into static HTML so it
//! can be screenshotted with headless Edge (the same engine as the app's WebView2):
//!
//!   cargo run -p ride-ui-profile --example preview
//!
//! Writes `target/preview/ride-ui-profile-<theme>.html` for both themes: the profile in
//! look-ahead and full mode (at the 230 px / full-width size of the shell's bottom panel),
//! the 380 px rail with the four charts, and the neutral placeholders. Realistic fake data
//! only — a ~24 km hilly Budapest-area route with two climbs and 300 recorded seconds.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{
    Bounds, Climb, ClimbCategory, ProfileMode, RecordPoint, RideSnapshot, RideState, RiderSettings,
    Route, RoutePoint, RouteSource, Theme,
};
use ride_ui_kit::html_document;
use ride_ui_profile::{ChartMetric, ElevationProfile, LiveChart, CSS};

/// Preview-only layout. Sized like the shell: the profile is full width and 230 px high, the
/// rail is 380 px wide.
const PREVIEW_CSS: &str = r#"
.prev { max-width: 1440px; margin: 0 auto; padding: 28px 32px 64px; display: flex; flex-direction: column; gap: 28px; }
.prev-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; padding-bottom: 16px; border-bottom: 2px solid var(--rule-ink); }
.prev-head h1 { font: 600 var(--t-28)/1.1 var(--font-display); }
.prev-head p { margin-top: 8px; max-width: 64ch; color: var(--text-dim); }
.prev-tag { display: inline-flex; align-items: center; gap: 8px; height: 20px; padding: 0 8px; border: 1px solid var(--rule-strong); border-radius: var(--radius-sm); font: 500 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.prev-sec { display: flex; flex-direction: column; gap: 12px; }
.prev-sec > h2 { font: 600 var(--t-15)/1 var(--font-ui); padding-bottom: 8px; border-bottom: 1px solid var(--rule); }
.prev-cap { font: 400 var(--t-13)/1.4 var(--font-ui); color: var(--text-dim); }
.prev-panel { position: relative; height: 232px; overflow: hidden; border: 1px solid var(--rule-strong); background: var(--bg-1); }
.prev-panel > * { width: 100%; height: 100%; }
.prev-pair { display: grid; grid-template-columns: minmax(0, 1fr) 380px; align-items: start; }
.prev-rail { width: 380px; display: flex; flex-direction: column; border: 1px solid var(--rule-strong); background: var(--bg-1); }
.prev-rail--narrow { width: 100%; }
.prev-charts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); }
.prev-empty { height: 168px; overflow: hidden; border: 1px solid var(--rule-strong); background: var(--bg-1); }
.prev-empty > * { width: 100%; height: 100%; }
@media (max-width: 1180px) {
  .prev-pair { grid-template-columns: minmax(0, 1fr); }
  .prev-rail { width: 100%; }
}
"#;

// ───────────────────────────── fake data ─────────────────────────────

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A ~24 km Budapest-area ride: 118 m at the start, a 6.8 km climb, a descent, a 3.4 km
/// second climb and a rolling run-in. Sampled every 10 m like the real pipeline.
fn fake_route() -> Route {
    let step = 10.0_f64;
    let total = 24_000.0_f64;
    let n = (total / step) as usize + 1;
    let mut points = Vec::with_capacity(n);
    let mut ascent = 0.0;
    let mut descent = 0.0;
    let ele_of = |d: f64| -> f64 {
        let climb = smooth(4_300.0, 11_100.0, d) * 462.0;
        let drop1 = smooth(11_100.0, 15_500.0, d) * 380.0;
        let climb2 = smooth(15_500.0, 18_900.0, d) * 208.0;
        let drop2 = smooth(18_900.0, 24_000.0, d) * 120.0;
        let roll = (d / 700.0).sin() * 4.0 + (d / 230.0).sin() * 1.6 + (d / 3_100.0).sin() * 6.0;
        118.0 + climb - drop1 + climb2 - drop2 + roll
    };
    let mut previous = ele_of(0.0);
    for i in 0..n {
        let d = i as f64 * step;
        let ele = ele_of(d);
        let t = d / total;
        // a plausible out-and-back loop over the Buda hills
        let lat = 47.5 + 0.018 * (t * std::f64::consts::TAU).sin();
        let lon = 19.0 + 0.03 * (t * std::f64::consts::TAU).cos();
        let (a, b) = (d - 50.0, d + 50.0);
        let grade = if d >= 50.0 && d + 50.0 <= total {
            ((ele_of(b) - ele_of(a)) / 100.0 * 100.0).clamp(-25.0, 25.0)
        } else {
            0.0
        };
        if i > 0 {
            if ele > previous {
                ascent += ele - previous;
            } else {
                descent += previous - ele;
            }
        }
        previous = ele;
        points.push(RoutePoint {
            lat,
            lon,
            ele,
            dist: d,
            grade,
        });
    }
    let min_ele = points.iter().map(|p| p.ele).fold(f64::INFINITY, f64::min);
    let max_ele = points
        .iter()
        .map(|p| p.ele)
        .fold(f64::NEG_INFINITY, f64::max);
    let climbs = vec![
        Climb {
            start_dist: 4_300.0,
            end_dist: 11_100.0,
            length: 6_800.0,
            gain: 462.0,
            avg_grade: 6.8,
            max_grade: 11.4,
            start_ele: 118.0,
            end_ele: 580.0,
            category: Some(ClimbCategory::Cat2),
        },
        Climb {
            start_dist: 15_500.0,
            end_dist: 18_900.0,
            length: 3_400.0,
            gain: 208.0,
            avg_grade: 6.1,
            max_grade: 9.2,
            start_ele: 236.0,
            end_ele: 444.0,
            category: Some(ClimbCategory::Cat4),
        },
    ];
    Route {
        id: "preview-budai-kor".into(),
        name: "Budai-hegység kör".into(),
        source: RouteSource::Demo,
        total_dist: total,
        points,
        ascent,
        descent,
        min_ele,
        max_ele,
        bounds: Bounds {
            south: 47.48,
            west: 18.97,
            north: 47.52,
            east: 19.03,
        },
        climbs,
        waypoints: Vec::new(),
        has_elevation: true,
        segments: Vec::new(),
        timeline: Vec::new(),
    }
}

/// A snapshot of a rider 11.4 km into the route, in the middle of the first climb.
fn fake_snapshot(dist: f64) -> RideSnapshot {
    let mut snapshot = RideSnapshot::empty();
    snapshot.state = RideState::Riding;
    snapshot.dist = dist;
    snapshot.pos.dist = dist;
    snapshot.pos.grade = 6.6;
    snapshot.pos.ele = 420.0;
    snapshot.speed_ms = 6.9;
    snapshot.power_w = Some(248.0);
    snapshot.power_3s = Some(239.0);
    snapshot.cadence_rpm = Some(83.0);
    snapshot.hr_bpm = Some(151.0);
    snapshot.elapsed_s = 1_284.0;
    snapshot.remaining_dist = 12_600.0;
    snapshot.eta_s = Some(2_940.0);
    snapshot.ascent_done = 318.0;
    snapshot.descent_done = 84.0;
    snapshot.ascent_left = 352.0;
    snapshot.avg_speed_ms = 8.9;
    snapshot.max_speed_ms = 12.6;
    snapshot.avg_power_w = Some(206.0);
    snapshot.max_power_w = Some(402.0);
    snapshot.normalized_power_w = Some(231.0);
    snapshot.work_kj = 264.0;
    snapshot.kcal = 264.0;
    snapshot.avg_hr = Some(142.0);
    snapshot.max_hr = Some(168.0);
    snapshot.power_zone = Some(4);
    snapshot.hr_zone = Some(3);
    snapshot.next_climb = Some(0);
    snapshot.next_climb_in = Some(0.0);
    snapshot.trainer_grade = 6.6;
    snapshot
}

/// 300 recorded seconds (1 Hz) with a realistic wave, a short stop and a heart rate that
/// lags the power.
fn fake_history() -> Vec<RecordPoint> {
    (0..300)
        .map(|i| {
            let t = i as f64;
            let effort = (t / 14.0).sin() * 0.5 + (t / 47.0).sin() * 0.3;
            let stop = if (150.0..158.0).contains(&t) {
                0.0
            } else {
                1.0
            };
            let power = (214.0 + effort * 68.0) * stop;
            let speed = (7.1 + effort * 1.4) * stop;
            RecordPoint {
                t,
                unix_s: 1_700_000_000.0 + t,
                dist: 8.9 * t,
                lat: 47.5 + 0.018 * (t / 300.0 * std::f64::consts::TAU).sin(),
                lon: 19.0 + 0.03 * (t / 300.0 * std::f64::consts::TAU).cos(),
                ele: 118.0 + t * 1.4,
                speed_ms: speed,
                power_w: Some(power),
                cadence_rpm: Some((82.0 + effort * 9.0) * stop),
                hr_bpm: Some(138.0 + (t / 52.0).sin() * 11.0 + effort * 4.0),
                grade: 3.4,
            }
        })
        .collect()
}

fn rider() -> RiderSettings {
    RiderSettings {
        ftp_w: 235.0,
        max_hr: 190.0,
        rider_kg: 75.0,
        bike_kg: 9.0,
        ..RiderSettings::default()
    }
}

// ───────────────────────────── demo wrappers ─────────────────────────────

#[derive(Clone, PartialEq, Props)]
struct ProfileDemoProps {
    route: Option<Arc<Route>>,
    snapshot: RideSnapshot,
    mode: ProfileMode,
    theme: Theme,
}

#[component]
fn ProfileDemo(props: ProfileDemoProps) -> Element {
    let route0 = props.route.clone();
    let route = use_signal(move || route0);
    let snapshot0 = props.snapshot.clone();
    let snapshot = use_signal(move || snapshot0);
    let mode0 = props.mode;
    let mode = use_signal(move || mode0);
    let lookahead = use_signal(|| 3_000.0_f64);
    let theme0 = props.theme;
    let theme = use_signal(move || theme0);
    rsx! {
        ElevationProfile {
            route,
            snapshot,
            mode,
            lookahead_m: lookahead,
            theme,
            on_seek: move |_| {},
            on_mode: move |_| {},
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct ChartDemoProps {
    history: Arc<Vec<RecordPoint>>,
    metric: ChartMetric,
    theme: Theme,
}

#[component]
fn ChartDemo(props: ChartDemoProps) -> Element {
    let history0 = props.history.clone();
    let history = use_signal(move || history0);
    let rider = use_signal(rider);
    let theme0 = props.theme;
    let theme = use_signal(move || theme0);
    rsx! {
        LiveChart { history, metric: props.metric, rider, theme }
    }
}

// ───────────────────────────── page ─────────────────────────────

#[component]
fn Page(theme: Theme) -> Element {
    let route = Arc::new(fake_route());
    let history = Arc::new(fake_history());
    // in-look-ahead state: inside the first climb, so the window shows the gradient
    let climbing = fake_snapshot(6_400.0);
    // full-route state: just over the top of the first climb
    let summit = fake_snapshot(11_400.0);
    let no_route: Option<Arc<Route>> = None;
    let empty_history: Arc<Vec<RecordPoint>> = Arc::new(Vec::new());
    let mode_tag = if theme == Theme::Dark {
        "Sötét téma"
    } else {
        "Világos téma"
    };

    rsx! {
        div { class: "prev",
            header { class: "prev-head",
                div {
                    div { class: "prev-tag", "ride-ui-profile · SSR előnézet" }
                    h1 { "Magassági profil és élő grafikonok" }
                    p { "SVG-s profilsáv a teljes útvonalhoz és 3 km-es előretekintő ablakhoz, valamint a jobb sáv négy 1 Hz-es grafikonja. A statikus geometria memoizált: tekerés közben csak a csoport-transzformáció és a jelölők mozognak." }
                }
                span { class: "prev-tag", "{mode_tag}" }
            }
            section { class: "prev-sec",
                h2 { "Előretekintés · 3 km-es ablak · 230 px" }
                div { class: "prev-panel",
                    ProfileDemo {
                        route: Some(route.clone()),
                        snapshot: climbing.clone(),
                        mode: ProfileMode::Lookahead,
                        theme,
                    }
                }
                div { class: "prev-cap", "A kerékpáros-jelölő 6,4 km-nél jár az első emelkedőn; a már megtett rész a felület színével takart, az emelkedő zászlaján a kategória száma, a km-vonalzón a sávja látszik." }
            }
            section { class: "prev-sec",
                h2 { "Teljes útvonal · 24 km" }
                div { class: "prev-panel",
                    ProfileDemo {
                        route: Some(route.clone()),
                        snapshot: summit.clone(),
                        mode: ProfileMode::Full,
                        theme,
                    }
                }
            }
            section { class: "prev-sec",
                h2 { "Élő grafikonok · 380 px sáv" }
                div { class: "prev-pair",
                    div { class: "prev-rail prev-rail--narrow",
                        ChartDemo { history: history.clone(), metric: ChartMetric::Power, theme }
                        ChartDemo { history: history.clone(), metric: ChartMetric::HeartRate, theme }
                        ChartDemo { history: history.clone(), metric: ChartMetric::Cadence, theme }
                        ChartDemo { history: history.clone(), metric: ChartMetric::Speed, theme }
                    }
                    div {
                        div { class: "prev-charts",
                            ChartDemo { history: history.clone(), metric: ChartMetric::Power, theme }
                            ChartDemo { history: history.clone(), metric: ChartMetric::HeartRate, theme }
                            ChartDemo { history: history.clone(), metric: ChartMetric::Cadence, theme }
                            ChartDemo { history: history.clone(), metric: ChartMetric::Speed, theme }
                        }
                        div { class: "prev-cap", style: "margin-top: 12px",
                            "Teljesítmény a Coggan-zónák, pulzus a pulzuszónák színeivel; a pedálütem és a sebesség akcentussal. A szüneteltetett szakasz a vonal megszakításaként látszik."
                        }
                    }
                }
            }
            section { class: "prev-sec",
                h2 { "Üres állapotok" }
                div { class: "prev-pair",
                    div { class: "prev-empty",
                        ProfileDemo {
                            route: no_route,
                            snapshot: RideSnapshot::empty(),
                            mode: ProfileMode::Lookahead,
                            theme,
                        }
                    }
                    div { class: "prev-rail prev-rail--narrow",
                        ChartDemo { history: empty_history.clone(), metric: ChartMetric::Power, theme }
                    }
                }
                div { class: "prev-cap", "Nincs útvonal és nincs felvétel: semleges magyar nyelvű helyettesítő szöveg, nincs pánik." }
            }
        }
    }
}

fn render(theme: Theme) -> String {
    let mut vdom = VirtualDom::new_with_props(Page, PageProps { theme });
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[CSS, PREVIEW_CSS], &body)
}

fn main() {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    std::fs::create_dir_all(&dir).expect("create target/preview");
    for (theme, name) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
        let path = dir.join(format!("ride-ui-profile-{name}.html"));
        std::fs::write(&path, render(theme)).expect("write preview");
        println!("{}", path.display());
    }
}
