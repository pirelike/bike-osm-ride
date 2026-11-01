//! QA helper: pre-download the first N Street View images of the demo route.
//!
//! Point the endpoints at the local mock and use a throw-away data directory:
//!
//! ```powershell
//! node scripts/streetview_mock.mjs 9300
//! $env:RIDE_STREETVIEW_BASE='http://127.0.0.1:9300'; $env:RIDE_DATA_DIR="$env:TEMP\ride-sv"
//! cargo run -p ride-ui-streetview --example offline_demo -- 40
//! ```
//!
//! Then start the app with the same `RIDE_STREETVIEW_BASE` / `RIDE_DATA_DIR` and a
//! `settings.json` that enables Street View (`RIDE_SV_KEY` sets the key, default `GOODKEY`).

use std::sync::Arc;
use std::time::Duration;

use ride_ui_streetview::{Downloader, ImageFetcher, ImageStore, MetadataClient, PanoCache};

#[tokio::main]
async fn main() {
    let limit: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let key = std::env::var("RIDE_SV_KEY").unwrap_or_else(|_| "GOODKEY".into());
    let route = Arc::new(ride_route::demo_route());
    let data = ride_route::store::data_dir();
    let cache = Arc::new(PanoCache::persistent(&data, &route.id));
    let store = Arc::new(ImageStore::open(&data, &route.id, route.total_dist).expect("store"));
    let meta = MetadataClient::from_settings_key(&key).expect("metadata client");
    let images = ImageFetcher::from_settings_key(&key).expect("image client");

    let mut handle = Downloader::spawn(meta, images, cache, Arc::clone(&store), Arc::clone(&route));
    while handle.is_running() && store.len() < limit {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    handle.cancel();
    println!(
        "route {} · spacing {:.0} m · images {}/{} · {:.1} MB",
        route.id,
        store.step_m(),
        store.len(),
        store.plan_len(),
        store.bytes() as f64 / 1_048_576.0
    );
}
