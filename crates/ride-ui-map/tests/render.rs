//! Server-side rendering smoke tests.
//!
//! The map must render without a browser, without a route, with a route that has no points
//! and with an `Empty` ride state — and it must never panic on any of them.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{testing::straight_route, MapStyle, RideSnapshot, RideState, Route};
use ride_ui_map::MapView;

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    route: Option<Arc<Route>>,
    snapshot: RideSnapshot,
    follow: bool,
    style: MapStyle,
}

#[component]
fn Harness(props: HarnessProps) -> Element {
    let initial_route = props.route.clone();
    let initial_snapshot = props.snapshot.clone();
    let initial_style = props.style;
    let initial_follow = props.follow;
    // exactly how the shell feeds the map: owned signals for the ride data, memos derived
    // from the settings for the map style and the follow flag
    let route = use_signal(move || initial_route);
    let snapshot = use_signal(move || initial_snapshot);
    let style = use_memo(move || initial_style);
    let follow = use_memo(move || initial_follow);
    rsx! {
        MapView {
            route,
            snapshot,
            style,
            follow,
            on_style: move |_| {},
            on_follow: move |_| {},
        }
    }
}

fn render(route: Option<Arc<Route>>, snapshot: RideSnapshot, follow: bool) -> String {
    let props = HarnessProps {
        route,
        snapshot,
        follow,
        style: MapStyle::Dark,
    };
    let mut vdom = VirtualDom::new_with_props(Harness, props);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn riding_snapshot(route: &Route, dist: f64) -> RideSnapshot {
    let mut snapshot = RideSnapshot::empty();
    snapshot.state = RideState::Riding;
    snapshot.dist = dist;
    snapshot.pos = route.locate(dist);
    snapshot.speed_ms = 7.2;
    snapshot.power_w = Some(223.0);
    snapshot.hr_bpm = Some(149.0);
    snapshot
}

#[test]
fn renders_a_full_map() {
    let route = Arc::new(straight_route(&[(1200.0, 3.0), (800.0, -2.0)], 120.0));
    let snapshot = riding_snapshot(&route, 900.0);
    let html = render(Some(Arc::clone(&route)), snapshot, false);

    // structure
    assert!(html.contains(r#"class="map""#), "map root missing:\n{html}");
    assert!(html.contains("map__layer"), "tile layer missing");
    assert!(html.contains("map__canvas"), "canvas missing");
    assert!(html.contains("map-route"), "route overlay missing");
    assert!(html.contains("map-route__seg"), "no grade-bucket path");
    assert!(html.contains("map-marker--start"), "start marker missing");
    assert!(html.contains("map-marker--finish"), "finish marker missing");
    assert!(html.contains("map-marker--rider"), "rider marker missing");
    assert!(html.contains("map-attrib"), "attribution missing");
    assert!(html.contains("map-controls"), "controls missing");

    // Hungarian user facing text
    for text in [
        "Rajt",
        "Cél",
        "Jelenlegi pozíció",
        "Követés",
        "Vissza a kerékpároshoz",
        "Nagyítás",
        "Kicsinyítés",
        "Sötét",
        "Világos",
        "OSM",
        "Térkép",
        "Térképstílus",
    ] {
        assert!(html.contains(text), "missing Hungarian text {text:?}");
    }

    // tiles come from the app's own proxy and the attribution is visible
    // (the base of the proxy differs per webview: `http://tiles.localhost` on Windows,
    // `tiles://localhost` on macOS / Linux)
    assert!(
        html.contains(&ride_ui_kit::protocol::base("tiles")),
        "tile proxy URLs missing"
    );
    assert!(html.contains("map-tile"), "no tile elements");
    assert!(
        html.contains("© OpenStreetMap contributors"),
        "attribution text missing"
    );
    // every style is OpenStreetMap; the look comes from the local CSS filter
    assert!(!html.contains("CARTO"), "stale CARTO attribution");

    // the rider arrow carries the heading as a rotation
    assert!(html.contains("rotate("), "heading arrow not rotated");

    // no emoji and no em dash in the map itself (the HUD owns "—" for missing values)
    assert!(!html.contains("😀"), "emoji found");
}

#[test]
fn renders_without_a_route() {
    let html = render(None, RideSnapshot::empty(), false);
    assert!(html.contains("map--empty"), "empty state missing");
    assert!(
        html.contains("Nincs betöltött útvonal"),
        "empty state text missing"
    );
    assert!(
        html.contains("Tölts be egy GPX-fájlt"),
        "empty state hint missing"
    );
    assert!(!html.contains("map-tile"), "no tiles without a route");
    assert!(!html.contains("Rajt"), "no start marker without a route");
    assert!(
        !html.contains("map-controls"),
        "no controls without a route"
    );
}

#[test]
fn renders_a_route_without_points() {
    let mut route = straight_route(&[(100.0, 1.0)], 0.0);
    route.points.clear();
    route.total_dist = 0.0;
    let html = render(Some(Arc::new(route)), RideSnapshot::empty(), true);
    assert!(html.contains("map__layer"), "tiles still render");
    assert!(html.contains("map-attrib"), "attribution still visible");
    assert!(!html.contains("Rajt"), "no start marker for an empty route");
    assert!(!html.contains("Cél"), "no finish marker for an empty route");
    assert!(
        !html.contains("map-marker--rider"),
        "no rider for an empty route"
    );
}

#[test]
fn hides_the_rider_before_the_ride_starts() {
    let route = Arc::new(straight_route(&[(500.0, 4.0)], 80.0));
    let html = render(Some(route), RideSnapshot::empty(), false);
    assert!(html.contains("Rajt"), "start marker present");
    assert!(!html.contains("Jelenlegi pozíció"), "rider must be hidden");
    assert!(html.contains("map-attrib"));
}

#[test]
fn follow_mode_marks_the_map_and_the_button() {
    let route = Arc::new(straight_route(&[(500.0, 4.0)], 80.0));
    let html = render(
        Some(Arc::clone(&route)),
        riding_snapshot(&route, 200.0),
        true,
    );
    assert!(html.contains("is-following"), "follow state class missing");
    assert!(
        html.contains(r#"aria-pressed="true""#),
        "follow button not pressed"
    );
}

#[test]
fn the_ghost_of_an_earlier_ride_gets_its_own_marker() {
    let route = Arc::new(straight_route(&[(500.0, 4.0)], 80.0));
    let mut snapshot = riding_snapshot(&route, 200.0);
    snapshot.ghost = Some(ride_model::GhostLive {
        dist: 260.0,
        pos: route.locate(260.0),
        delta_s: Some(12.0),
    });
    let html = render(Some(Arc::clone(&route)), snapshot.clone(), false);
    assert!(html.contains("map-marker--ghost"), "ghost marker missing");
    assert!(html.contains("Szellem"), "ghost label missing");
    assert!(html.contains("map-ghost__wedge"), "ghost wedge missing");
    assert_eq!(html.matches("map-ghost__dot").count(), 1, "one ghost dot");
    // the rider keeps its own marker next to it
    assert!(html.contains("map-marker--rider"));
    assert!(html.contains("Jelenlegi pozíció"));

    // without a ghost the map stays exactly as it was
    snapshot.ghost = None;
    let plain = render(Some(route), snapshot, false);
    assert!(!plain.contains("map-marker--ghost"), "no ghost, no marker");
    assert!(plain.contains("map-marker--rider"));
}
