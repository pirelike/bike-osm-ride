//! Offline Street View: after a route's images were downloaded, the panel shows them with
//! **no API key and no request**, and a missing bucket falls back to the live embed when a
//! key is configured.
//!
//! One test function on purpose: it sets process-wide environment variables.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use ride_model::testing::straight_route;
use ride_model::{RideSnapshot, RideState, Route, UiSettings};
use ride_ui_streetview::imagestore::{self, ImageStore};
use ride_ui_streetview::mock::{MockServer, StreetviewMock};
use ride_ui_streetview::panel::set_load_timeout;
use ride_ui_streetview::url::{ApiKey, Endpoints};
use ride_ui_streetview::{Downloader, ImageFetcher, MetadataClient, PanoCache, StreetViewPanel};

fn route() -> Arc<Route> {
    let mut r = straight_route(&[(600.0, 1.0)], 100.0);
    r.id = "offlinetest".into();
    Arc::new(r)
}

fn settings(key: &str) -> UiSettings {
    UiSettings {
        google_maps_key: key.to_string(),
        streetview_offline: true,
        streetview_cache_panos: true,
        ..UiSettings::default()
    }
}

#[derive(Clone, PartialEq)]
struct HostProps {
    route: Option<Arc<Route>>,
    settings: UiSettings,
    dist: f64,
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

fn start(props: HostProps) -> VirtualDom {
    let mut vdom = VirtualDom::new_with_props(Host, props);
    vdom.rebuild_in_place();
    vdom
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downloaded_images_ride_without_a_key_or_a_request() {
    let data = tempfile::tempdir().expect("tmp");
    let server = MockServer::streetview(StreetviewMock::default());
    std::env::set_var("RIDE_STREETVIEW_BASE", &server.base);
    std::env::set_var("RIDE_DATA_DIR", data.path());
    set_load_timeout(Duration::from_millis(300));

    let route = route();
    let ep = Endpoints::local(&server.base).expect("local");
    let meta = MetadataClient::new(ep.clone(), ApiKey::new("GOODKEY").expect("key")).expect("meta");
    let fetcher = ImageFetcher::new(ep, ApiKey::new("GOODKEY").expect("key")).expect("images");
    let cache = Arc::new(PanoCache::persistent(data.path(), &route.id));
    let store =
        Arc::new(ImageStore::open(data.path(), &route.id, route.total_dist).expect("store"));

    // Download the whole route (9 image buckets of 75 m).
    let h = Downloader::spawn(
        meta,
        fetcher,
        Arc::clone(&cache),
        Arc::clone(&store),
        Arc::clone(&route),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.is_running() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(h.progress().done, 9, "{:?}", h.progress());
    assert_eq!(store.len(), 9);
    drop(h);
    cache.flush().expect("flush the id cache");

    // 1. no key, cached image: the panel shows the local file and sends nothing.
    let requests_before = server.request_count();
    let mut vdom = start(HostProps {
        route: Some(Arc::clone(&route)),
        settings: settings(""),
        dist: 100.0,
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("data-state=\"ok\"")).await;
    assert!(html.contains("data-state=\"ok\""), "{html}");
    // 100 m / 75 m = bucket 1
    assert!(
        html.contains("streetview/offlinetest/1.jpg"),
        "the panel shows the cached image through the svimg protocol: {html}"
    );
    assert!(
        html.contains(ride_ui_kit::protocol::base("svimg").as_str()),
        "{html}"
    );
    assert!(
        !html.contains("<iframe"),
        "no embed while a local image exists: {html}"
    );
    assert_eq!(
        server.request_count(),
        requests_before,
        "offline ride makes no Google request at all"
    );
    drop(vdom);

    // 2. a missing bucket: offline message, still no request.
    std::fs::remove_file(imagestore::image_path(data.path(), &route.id, 6).expect("path"))
        .expect("remove one image");
    let mut vdom = start(HostProps {
        route: Some(Arc::clone(&route)),
        settings: settings(""),
        dist: 475.0, // / 75 = bucket 6
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("data-state=\"offline\"")).await;
    assert!(html.contains("Ez a szakasz még nincs letöltve"), "{html}");
    // without a key the download cannot start: the message offers the settings instead
    assert!(html.contains("Beállítások megnyitása"), "{html}");
    assert!(!html.contains("Képek letöltése"), "{html}");
    assert_eq!(server.request_count(), requests_before);
    drop(vdom);

    // 3. with a key the missing bucket falls back to the live embed (id already cached).
    let mut vdom = start(HostProps {
        route: Some(Arc::clone(&route)),
        settings: settings("GOODKEY"),
        dist: 475.0,
    });
    let html = run_until(&mut vdom, 10, |h| h.contains("pano=MOCK")).await;
    assert!(html.contains("<iframe"), "{html}");
    assert!(html.contains("pano=MOCK"), "{html}");
    assert!(
        !html.contains("svimg.localhost"),
        "the missing bucket uses the embed: {html}"
    );
    assert_eq!(
        server.request_count(),
        requests_before,
        "the id was in the cache, no lookup"
    );
}
