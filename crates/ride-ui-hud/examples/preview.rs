//! SSR preview of the HUD, rendered to static HTML so it can be screenshotted with headless
//! Edge (the engine of the app's WebView2):
//!
//!   cargo run -p ride-ui-hud --example preview
//!
//! Writes `target/preview/ride-ui-hud-dark.html` and `target/preview/ride-ui-hud-light.html`:
//! the four components with realistic fake data (a mid-climb snapshot, a snapshot just before
//! a climb, and the empty `RideSnapshot::empty()` state), plus one transport bar per ride
//! state. Uses `ride_ui_kit::html_document`, so kit CSS + [`ride_ui_hud::CSS`] are inlined.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::testing::straight_route;
use ride_model::{
    Climb, ClimbCategory, RideSnapshot, RideState, RiderSettings, Route, RoutePosition, Theme,
};
use ride_ui_hud::{state_label, ClimbCard, MetricTiles, RideControls, StatsPanel, CSS};
use ride_ui_kit::{html_document, Icon, IconName};

/// Preview-only layout (not part of the component stylesheet).
const PREVIEW_CSS: &str = r#"
.hp { max-width: 1240px; margin: 0 auto; padding: 32px 40px 64px; display: flex; flex-direction: column; gap: 32px; }
.hp-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; padding-bottom: 16px; border-bottom: 2px solid var(--rule-ink); }
.hp-head h1 { font: 600 var(--t-28)/1.1 var(--font-display); }
.hp-head p { color: var(--text-dim); margin-top: 8px; max-width: 66ch; }
.hp-chips { display: flex; flex-wrap: wrap; gap: 8px; justify-content: flex-end; }
.hp-sec { display: flex; flex-direction: column; gap: 12px; }
.hp-sec > h2 { font: 600 var(--t-15)/1 var(--font-ui); padding-bottom: 8px; border-bottom: 1px solid var(--rule); }
.hp-sec > p { color: var(--text-dim); max-width: 78ch; }

/* the real arrangement: a flush 380 px rail next to the stage (the map), climb panel at its corner */
.hp-scene { display: grid; grid-template-columns: 380px minmax(0, 1fr); align-items: start; border: 1px solid var(--rule-strong); background: var(--bg-1); }
.hp-rail { display: flex; flex-direction: column; min-width: 0; border-right: 1px solid var(--rule-strong); }
.hp-stage { position: relative; min-height: 360px; display: flex; flex-direction: column; align-items: flex-start; background: var(--bg-2); }
.hp-stage__grid { display: none; }
.hp-stage__note { padding: 12px 16px; font: 500 var(--t-13)/1.4 var(--font-ui); color: var(--text-dim); }
.hp-stage__controls { margin-top: auto; width: 100%; border-top: 1px solid var(--rule-strong); background: var(--bg-1); }

.hp-controls { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.hp-cap { font: 500 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.hp-controls-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 24px; }

@media (max-width: 1100px) {
  .hp { padding: 24px; }
  .hp-scene { grid-template-columns: minmax(0, 1fr); }
  .hp-controls-grid { grid-template-columns: minmax(0, 1fr); }
}
"#;

// ───────────────────────────── fake data ─────────────────────────────

/// 22 km rolling Budapest-ish route with two climbs (6.8 km @ 6.8 % and 3.4 km @ 6.1 %).
fn demo_route() -> Arc<Route> {
    let mut route = straight_route(
        &[
            (6000.0, 2.0),
            (4000.0, 7.0),
            (3000.0, -2.0),
            (5000.0, 4.0),
            (4000.0, -1.0),
        ],
        118.0,
    );
    route.name = "Budai-hegység kör".to_string();
    route.climbs = vec![
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
            start_ele: 96.0,
            end_ele: 304.0,
            category: Some(ClimbCategory::Cat4),
        },
    ];
    Arc::new(route)
}

fn demo_rider() -> RiderSettings {
    RiderSettings {
        ftp_w: 235.0,
        max_hr: 190.0,
        ..RiderSettings::default()
    }
}

/// A moving snapshot. `inside` = the rider is on the first climb (6 200 m along the route),
/// otherwise 2 800 m along, 1 500 m before that climb starts.
fn riding_snapshot(dist: f64, inside: bool) -> RideSnapshot {
    RideSnapshot {
        state: RideState::Riding,
        pos: RoutePosition {
            dist,
            lat: 47.51,
            lon: 19.03,
            ele: if inside { 420.0 } else { 168.0 },
            grade: if inside { 6.8 } else { 1.4 },
            heading_deg: 92.0,
        },
        speed_ms: if inside { 7.2 } else { 8.9 },
        power_w: Some(252.0),
        power_3s: Some(238.0),
        cadence_rpm: Some(84.0),
        hr_bpm: Some(148.0),
        resistance: Some(42.0),
        elapsed_s: 1_264.0,
        dist,
        remaining_dist: 22_000.0 - dist,
        eta_s: Some(4_200.0),
        ascent_done: 312.0,
        descent_done: 148.0,
        ascent_left: 268.0,
        avg_speed_ms: 7.1,
        max_speed_ms: 12.4,
        avg_power_w: Some(214.0),
        max_power_w: Some(402.0),
        normalized_power_w: Some(236.0),
        work_kj: 264.0,
        kcal: 264.0,
        avg_hr: Some(139.0),
        max_hr: Some(168.0),
        power_zone: Some(4),
        hr_zone: Some(3),
        next_climb: Some(0),
        next_climb_in: Some(if inside { 0.0 } else { 1_500.0 }),
        trainer_grade: if inside { 6.6 } else { 1.2 },
        workout: None,
        ghost: None,
        segment: if inside {
            Some(ride_model::SegmentLive {
                climb: 0,
                elapsed_s: 272.0,
                pr_s: Some(280.0),
                delta_s: Some(-6.0),
            })
        } else {
            None
        },
    }
}

// ───────────────────────────── page components ─────────────────────────────

#[derive(Clone, PartialEq, Props)]
struct SceneProps {
    snapshot: RideSnapshot,
    route: Option<Arc<Route>>,
    theme: Theme,
    #[props(default = String::new())]
    note: String,
}

/// One rail (metric tiles + stats) next to a map-like stage holding the floating climb card
/// and the docked transport bar, i.e. the real shell arrangement at rail width 380 px.
#[component]
fn Scene(props: SceneProps) -> Element {
    let snapshot0 = props.snapshot.clone();
    let snapshot = use_signal(move || snapshot0);
    let route0 = props.route.clone();
    let route = use_signal(move || route0);
    let rider = use_signal(demo_rider);
    // the demo PR of the big climb: shown before the ride and live while riding it
    let prs = use_signal(|| vec![Some(280.0)]);
    let theme0 = props.theme;
    let theme = use_signal(move || theme0);
    let note = props.note.clone();

    rsx! {
        div { class: "hp-scene",
            div { class: "hp-rail",
                MetricTiles { snapshot, rider, theme }
                StatsPanel { snapshot }
            }
            div { class: "hp-stage",
                div { class: "hp-stage__grid", aria_hidden: "true" }
                ClimbCard { route, snapshot, theme, prs }
                if !note.is_empty() {
                    div { class: "hp-stage__note", "{note}" }
                }
                div { class: "hp-stage__controls",
                    RideControls {
                        snapshot,
                        on_start: move |_| {},
                        on_pause: move |_| {},
                        on_resume: move |_| {},
                        on_finish: move |_| {},
                        on_reset: move |_| {},
                    }
                }
            }
        }
    }
}

/// One transport bar for a given ride state (caption above it).
#[component]
fn ControlRow(state: RideState) -> Element {
    let mut snapshot = RideSnapshot::empty();
    snapshot.state = state;
    let snapshot = use_signal(move || snapshot);
    rsx! {
        div { class: "hp-controls",
            span { class: "hp-cap", "{state_label(state)}" }
            RideControls {
                snapshot,
                on_start: move |_| {},
                on_pause: move |_| {},
                on_resume: move |_| {},
                on_finish: move |_| {},
                on_reset: move |_| {},
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct HudPageProps {
    theme: Theme,
}

#[component]
fn HudPage(props: HudPageProps) -> Element {
    let theme = props.theme;
    let route = demo_route();
    let mid = riding_snapshot(6_200.0, true);
    let before = riding_snapshot(2_800.0, false);
    let empty = RideSnapshot::empty();
    let (theme_icon, theme_name) = match theme {
        Theme::Dark => (IconName::Moon, "Sötét téma"),
        Theme::Light => (IconName::Sun, "Világos téma"),
    };

    rsx! {
        div { class: "hp",
            header { class: "hp-head",
                div {
                    div { class: "t-eyebrow", "bike-osm-ride · HUD" }
                    h1 { "Pilótafülke" }
                    p { "Élő mérőszámok, menetadatok, a következő emelkedő és a menet vezérlése. Minden szám a ride-ui-kit formázó moduljából jön, hiányzó szenzor esetén „—” áll a helyén; a színek a design-kit tokenjeiből vagy a validált palettából származnak." }
                }
                div { class: "hp-chips",
                    span { class: "chip chip--accent",
                        Icon { name: theme_icon, size: 14 }
                        "{theme_name}"
                    }
                    span { class: "chip", "380 px jobb sáv" }
                    span { class: "chip", "10 Hz" }
                }
            }
            section { class: "hp-sec",
                h2 { "Emelkedőben" }
                p { "A negyedik Coggan-zóna köszöbön, 6,8 %-os emelkedőn; a kártya a hátralévő távot és szintet mutatja." }
                Scene { snapshot: mid, route: Some(route.clone()), theme }
            }
            section { class: "hp-sec",
                h2 { "Emelkedő előtt" }
                p { "Ugyanaz a pillanatkép a kaptató előtt: a kártya visszaszámlál, a zónasáv a 4. zónát jelöli." }
                Scene { snapshot: before, route: Some(route.clone()), theme }
            }
            section { class: "hp-sec",
                h2 { "Üres állapot" }
                p { "Nincs útvonal: a mérőszámok „—”-t mutatnak, a ClimbCard teljesen elrejtőzik, a Kezdés gomb letiltva." }
                Scene {
                    snapshot: empty,
                    route: None,
                    theme,
                    note: "Nincs betöltött útvonal — az emelkedő-kártya ilyenkor nem jelenik meg.",
                }
            }
            section { class: "hp-sec",
                h2 { "A menet vezérlése állapotonként" }
                div { class: "hp-controls-grid",
                    ControlRow { state: RideState::Ready }
                    ControlRow { state: RideState::Riding }
                    ControlRow { state: RideState::Paused }
                    ControlRow { state: RideState::Finished }
                }
            }
        }
    }
}

// ───────────────────────────── driver ─────────────────────────────

fn render(theme: Theme) -> String {
    let mut vdom = VirtualDom::new_with_props(HudPage, HudPageProps { theme });
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[CSS, PREVIEW_CSS], &body)
}

fn main() -> std::io::Result<()> {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    std::fs::create_dir_all(&dir)?;
    for (theme, name) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
        let path = dir.join(format!("ride-ui-hud-{name}.html"));
        std::fs::write(&path, render(theme))?;
        println!("{}", path.display());
    }
    Ok(())
}
