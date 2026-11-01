//! End-to-end run of the stateful panel against the local mock: a real `VirtualDom`, real
//! effects, real HTTP to `MockServer` (selected through `RIDE_STREETVIEW_BASE`).
//!
//! One test function on purpose: it sets process-wide environment variables.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use ride_model::testing::straight_route;
use ride_model::{RideSnapshot, RideState, Route, UiSettings};
use ride_ui_streetview::mock::{MockServer, StreetviewMock};
use ride_ui_streetview::panel::set_load_timeout;
use ride_ui_streetview::StreetViewPanel;

#[derive(Clone)]
struct HostProps {
    route: Option<Arc<Route>>,
    settings: UiSettings,
    dist: f64,
}

impl PartialEq for HostProps {
    fn eq(&self, other: &Self) -> bool {
        self.dist == other.dist && self.settings == other.settings
    }
}

#[allow(non_snake_case)]
fn Host(props: HostProps) -> Element {
    let route = use_signal(|| props.route.clone());
    let snapshot = use_signal(|| {
        let mut s = RideSnapshot::empty();
        s.state = RideState::Ready;
        s.dist = props.dist;
        s
    });
    let settings = use_signal(|| props.settings.clone());
    rsx! {
        StreetViewPanel {
            route,
            snapshot,
            settings,
            on_close: move |_| {},
            on_toggle_size: move |_| {},
            on_open_settings: move |_| {},
        }
    }
}

async fn run_until(vdom: &mut VirtualDom, secs: u64, mut done: impl FnMut(&str) -> bool) -> String {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        let html = dioxus_ssr::render(vdom);
        if done(&html) || Instant::now() > end {
            return html;
        }
        let _ = tokio::time::timeout(Duration::from_millis(100), vdom.wait_for_work()).await;
        vdom.render_immediate(&mut dioxus::core::NoOpMutations);
    }
}

fn route() -> Arc<Route> {
    let mut r = straight_route(&[(6000.0, 1.0)], 100.0);
    r.id = "panelflowroute".into();
    Arc::new(r)
}

fn settings(key: &str, cache: bool) -> UiSettings {
    UiSettings {
        google_maps_key: key.to_string(),
        streetview_cache_panos: cache,
        // These scenarios exercise the online embed path; offline images have their own test.
        streetview_offline: false,
        ..UiSettings::default()
    }
}

fn start(props: HostProps) -> VirtualDom {
    let mut vdom = VirtualDom::new_with_props(Host, props);
    vdom.rebuild_in_place();
    vdom
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_follows_the_ride_against_the_mock() {
    let data = tempfile::tempdir().expect("tmp");
    let server = MockServer::streetview(StreetviewMock::default());
    std::env::set_var("RIDE_STREETVIEW_BASE", &server.base);
    std::env::set_var("RIDE_DATA_DIR", data.path());
    set_load_timeout(Duration::from_millis(300));

    // 1. no key: message, no request
    let mut vdom = start(HostProps {
        route: Some(route()),
        settings: settings("", false),
        dist: 100.0,
    });
    let html = run_until(&mut vdom, 5, |h| h.contains("data-state=\"nokey\"")).await;
    assert!(html.contains("API-kulcs szükséges"), "{html}");
    assert!(html.contains("Maps Embed API"));
    assert_eq!(server.request_count(), 0);
    drop(vdom);

    // 2. rejected key: Hungarian error, panel stays usable
    let mut vdom = start(HostProps {
        route: Some(route()),
        settings: settings("BADKEY", false),
        dist: 100.0,
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("data-state=\"error\"")).await;
    assert!(html.contains("A Google elutasította a kulcsot"), "{html}");
    drop(vdom);
    let calls = server.request_count();
    assert_eq!(calls, 1);

    // 3. good key, option off: one metadata call, the panorama goes into the hidden iframe
    //    and is swapped to the front; nothing is written to disk.
    let mut vdom = start(HostProps {
        route: Some(route()),
        settings: settings("GOODKEY", false),
        dist: 100.0,
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("data-state=\"ok\"")).await;
    assert!(html.contains("data-state=\"ok\""), "{html}");
    assert!(
        html.contains("pano=MOCK"),
        "embed src carries the panorama id: {html}"
    );
    assert!(html.contains("key=GOODKEY"));
    assert!(html.contains("heading=90"), "route runs east");
    assert!(
        html.contains("location=47.5"),
        "route point as fallback location"
    );
    assert!(html.contains("pitch=2") && html.contains("fov=90"));
    assert!(
        !html.contains("sv__msg"),
        "no overlay once the picture shows"
    );
    assert_eq!(server.request_count(), calls + 1);
    assert!(
        server
            .request_lines()
            .iter()
            .all(|l| l.starts_with("GET /maps/api/streetview/metadata?")),
        "the host only talks to the metadata endpoint, never fetches imagery"
    );
    assert!(
        !data.path().join("streetview").exists(),
        "option off: nothing saved"
    );
    drop(vdom);

    // 4. option on: ids are resolved ahead of the rider and saved per route
    let before = server.request_count();
    let mut vdom = start(HostProps {
        route: Some(route()),
        settings: settings("GOODKEY", true),
        dist: 100.0,
    });
    let file = data.path().join("streetview").join("panelflowroute.json");
    let html = run_until(&mut vdom, 30, |h| {
        h.contains("data-state=\"ok\"") && file.is_file()
    })
    .await;
    assert!(html.contains("data-state=\"ok\""), "{html}");
    assert!(file.is_file(), "route file written");
    assert!(
        html.contains("Panoráma-azonosítók: "),
        "footer note shows the progress: {html}"
    );
    let text = std::fs::read_to_string(&file).expect("read");
    assert!(text.contains("\"route_id\":\"panelflowroute\"") && text.contains("MOCK"));
    assert!(
        !text.to_lowercase().contains("goodkey"),
        "the key is never written to disk"
    );
    assert!(server.request_count() > before + 10, "prefetch ran ahead");
    drop(vdom);

    // 5. repeat ride: ids come from the file, the metadata endpoint stays quiet
    let known = ride_ui_streetview::PanoCache::persistent(data.path(), "panelflowroute").len();
    assert!(known >= 24, "{known}");
    let calls_before_repeat = server.request_count();
    tokio::time::sleep(Duration::from_millis(400)).await; // let aborted requests land
    let settled = server.request_count();
    assert!(settled >= calls_before_repeat);
    let mut vdom = start(HostProps {
        route: Some(route()),
        settings: settings("GOODKEY", true),
        dist: 100.0,
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("data-state=\"ok\"")).await;
    assert!(html.contains("data-state=\"ok\""), "{html}");
    // the first buckets (near the start) were cached by the previous ride: no call for the
    // rider's own bucket; any new calls are prefetch for buckets not saved yet
    let lines = server.request_lines();
    let own = route().locate(ride_ui_streetview::cache::bucket_center(
        ride_ui_streetview::cache::bucket_of(100.0),
        6000.0,
    ));
    let own_location = format!("location={:.6}%2C{:.6}", own.lat, own.lon);
    let own_bucket_calls = lines[settled..]
        .iter()
        .filter(|l| l.contains(&own_location))
        .count();
    assert_eq!(own_bucket_calls, 0, "{:?}", &lines[settled..]);
    drop(vdom);

    // 6. no route: message instead of a panorama
    let mut vdom = start(HostProps {
        route: None,
        settings: settings("GOODKEY", false),
        dist: 0.0,
    });
    let html = run_until(&mut vdom, 5, |h| h.contains("Nincs betöltött útvonal")).await;
    assert!(html.contains("Nincs betöltött útvonal"), "{html}");
}
