//! Explicit download of a route's Street View images (the opt-in offline cache).
//!
//! Nothing here runs by itself: the panel starts a [`Downloader`] only when the rider asks
//! for it (the "Letöltés" button), and the task stops when the panel closes or the download
//! is cancelled. Every image is a Street View Static API request (billed to the user's own
//! Google project), so the walk is paced ([`MIN_START_GAP`]) and bounded by the route's
//! image plan ([`crate::imagestore`]). Progress goes through a `watch` channel; partial
//! results are kept, so a cancelled download can be resumed later.

use std::sync::Arc;
use std::time::Duration;

use ride_i18n::{tf, tn};
use ride_model::Route;
use tokio::sync::{watch, Mutex};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep_until, Instant};

use crate::cache::{bucket_of, now_unix, PanoCache};
use crate::client::MetadataClient;
use crate::error::SvError;
use crate::image::ImageFetcher;
use crate::imagestore::{self, ImageStore};
use crate::planner::{smoothed_heading, PlannerConfig};
use crate::prefetch::resolve_live;
use crate::url::{EmbedTarget, View};

/// Minimum time between two image request starts (about 8 images/s).
pub const MIN_START_GAP: Duration = Duration::from_millis(120);
/// Image requests in flight at any moment.
pub const MAX_CONCURRENT: usize = 4;
/// Camera pitch of a downloaded image (the same as the live panel).
pub const IMAGE_PITCH: f64 = 2.0;
/// Camera field of view of a downloaded image.
pub const IMAGE_FOV: f64 = 90.0;

/// Where a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Image buckets of the route.
    pub total: usize,
    /// Buckets already handled (downloaded, already present, or with no panorama).
    pub done: usize,
    /// Buckets whose request failed (they stay missing).
    pub failed: usize,
    /// Actual images available (excludes buckets without a panorama).
    pub stored: usize,
    /// Bytes written in this run.
    pub bytes: u64,
    pub running: bool,
    /// Set when the run stopped early (key / quota problem).
    pub stopped: Option<SvError>,
}

impl Progress {
    /// 0.0–1.0, 0.0 for an empty plan.
    pub fn fraction(self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }

    /// One-liner for the panel footer, in the UI language.
    pub fn note(self) -> String {
        if let Some(e) = self.stopped {
            return tf(
                "ui_streetview.download.a_letoltes_leallt",
                &[("reason", e.short())],
            );
        }
        if self.running {
            return tf(
                "ui_streetview.download.kepek_letoltese_0",
                &[
                    ("done", &self.done.to_string()),
                    ("total", &self.total.to_string()),
                    ("pct", &format!("{:.0}", self.fraction() * 100.0)),
                ],
            );
        }
        if self.total > 0 && self.done >= self.total {
            let count = self.stored;
            return tn(
                "ui_streetview.download.offline_street_view_kesz_kep",
                count as i64,
                &[("count", &count.to_string())],
            );
        }
        tf(
            "ui_streetview.download.offline_street_view_kep",
            &[
                ("done", &self.done.to_string()),
                ("total", &self.total.to_string()),
            ],
        )
    }
}

/// Handle of the running download task. Dropping it cancels the download.
pub struct DownloadHandle {
    progress: watch::Receiver<Progress>,
    task: Option<JoinHandle<()>>,
    store: Arc<ImageStore>,
}

impl std::fmt::Debug for DownloadHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadHandle")
            .field("running", &self.is_running())
            .field("progress", &*self.progress.borrow())
            .finish()
    }
}

impl DownloadHandle {
    pub fn progress(&self) -> Progress {
        *self.progress.borrow()
    }

    /// A receiver the caller can await, so the UI can follow the download without polling
    /// the handle (which stays owned by the panel session).
    pub fn subscribe(&self) -> watch::Receiver<Progress> {
        self.progress.clone()
    }

    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Number of images stored so far (available even while running).
    pub fn stored(&self) -> usize {
        self.store.len()
    }

    pub fn cancel(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

impl Drop for DownloadHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Paces the request starts so a long route does not hammer the API.
struct Pacer {
    next: Mutex<Instant>,
    gap: Duration,
}

impl Pacer {
    fn new(gap: Duration) -> Self {
        Pacer {
            next: Mutex::new(Instant::now()),
            gap,
        }
    }

    async fn wait(&self) {
        let at = {
            let mut next = self.next.lock().await;
            let now = Instant::now();
            let at = (*next).max(now);
            *next = at + self.gap;
            at
        };
        sleep_until(at).await;
    }
}

/// What one finished download task reports.
type Done = (bool, u64);

/// Starts the download of every missing image of `route`.
pub struct Downloader;

impl Downloader {
    /// Spawns the task on the current tokio runtime. Without a runtime (SSR, plain unit
    /// tests) the returned handle is inert and reports `running: false`.
    pub fn spawn(
        meta: MetadataClient,
        images: ImageFetcher,
        cache: Arc<PanoCache>,
        store: Arc<ImageStore>,
        route: Arc<Route>,
    ) -> DownloadHandle {
        let total = store.plan_len();
        let already = store.len();
        let (tx, rx) = watch::channel(Progress {
            total,
            done: already.min(total),
            failed: 0,
            stored: already.min(total),
            bytes: 0,
            running: tokio::runtime::Handle::try_current().is_ok(),
            stopped: None,
        });
        let task = tokio::runtime::Handle::try_current().ok().map(|rt| {
            let store = Arc::clone(&store);
            rt.spawn(run(meta, images, cache, store, route, tx))
        });
        DownloadHandle {
            progress: rx,
            task,
            store,
        }
    }
}

async fn run(
    meta: MetadataClient,
    images: ImageFetcher,
    cache: Arc<PanoCache>,
    store: Arc<ImageStore>,
    route: Arc<Route>,
    progress: watch::Sender<Progress>,
) {
    let total = store.plan_len();
    let mut state = Progress {
        total,
        done: 0,
        failed: 0,
        stored: 0,
        bytes: 0,
        running: true,
        stopped: None,
    };
    for bucket in 0..total as u32 {
        if store.get_at(bucket, now_unix()).is_some() {
            state.done += 1;
            state.stored += 1;
        }
    }
    progress.send_replace(state);

    let pacer = Pacer::new(MIN_START_GAP);
    let mut set: JoinSet<(u32, Result<Done, SvError>)> = JoinSet::new();
    let mut next_bucket = 0u32;

    loop {
        // Fill the pipeline up to the concurrency limit.
        while set.len() < MAX_CONCURRENT && next_bucket < total as u32 && state.stopped.is_none() {
            let bucket = next_bucket;
            next_bucket += 1;
            if store.get_at(bucket, now_unix()).is_some() {
                continue;
            }
            pacer.wait().await;
            let (meta, images, cache, store, route) = (
                meta.clone(),
                images.clone(),
                Arc::clone(&cache),
                Arc::clone(&store),
                Arc::clone(&route),
            );
            set.spawn(async move {
                let result = download_one(&meta, &images, &cache, &store, &route, bucket).await;
                (bucket, result)
            });
        }

        let Some(joined) = set.join_next().await else {
            break;
        };
        match joined {
            Ok((_, Ok((stored, bytes)))) => {
                state.done += 1;
                if stored {
                    state.stored += 1;
                    state.bytes = state.bytes.saturating_add(bytes);
                }
            }
            Ok((_, Err(e))) if e.is_permanent() || e == SvError::QuotaExceeded => {
                state.stopped = Some(e);
            }
            Ok((_, Err(_))) => {
                state.done += 1;
                state.failed += 1;
            }
            Err(_) => {
                // A task was cancelled (should not happen except on abort).
            }
        }
        progress.send_replace(state);
        if state.stopped.is_some() {
            break;
        }
    }

    // Stop queued requests immediately on a permanent failure.
    if state.stopped.is_some() {
        set.abort_all();
    }
    // Drain the remaining tasks before publishing the final storage count.
    while set.join_next().await.is_some() {}
    state.stored = store.len();
    state.running = false;
    progress.send_replace(state);
    if let Err(e) = store.flush() {
        state.stopped = Some(SvError::Storage(e.kind()));
        progress.send_replace(state);
    }
}

/// One bucket: panorama id (cache first), then the image, then the file.
async fn download_one(
    meta: &MetadataClient,
    images: &ImageFetcher,
    cache: &Arc<PanoCache>,
    store: &ImageStore,
    route: &Route,
    bucket: u32,
) -> Result<Done, SvError> {
    let dist = store.bucket_center(bucket);
    let pos = route.locate(dist);
    let b25 = bucket_of(dist);
    let pano = match resolve_live(meta, cache, route, b25).await? {
        Some(id) => id,
        None => return Ok((false, 0)), // no panorama here: nothing to download
    };
    let heading = smoothed_heading(route, dist, &PlannerConfig::default().heading_offsets_m);
    let view = View {
        heading,
        pitch: IMAGE_PITCH,
        fov: IMAGE_FOV,
    };
    let target = EmbedTarget::Pano {
        id: pano.clone(),
        fallback: Some((pos.lat, pos.lon)),
    };
    let bytes = images
        .fetch(&target, view, imagestore::IMAGE_SIZE_PX)
        .await?;
    store
        .put(bucket, &pano, heading, &bytes)
        .map_err(|e| SvError::Storage(e.kind()))?;
    Ok((true, bytes.len() as u64))
}

/// Buckets that still need an image (the panel's estimate before starting).
pub fn missing(store: &ImageStore) -> usize {
    let now = now_unix();
    (0..store.plan_len() as u32)
        .filter(|b| store.get_at(*b, now).is_none())
        .count()
}

/// Removes every downloaded image and id of one route.
pub fn remove_route(data_dir: &std::path::Path, route_id: &str) -> std::io::Result<usize> {
    imagestore::clear_route(data_dir, route_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockServer, StreetviewMock};
    use crate::url::{ApiKey, Endpoints};
    use ride_model::testing::straight_route;
    use tokio::time::sleep;

    fn route(len: f64) -> Arc<Route> {
        let mut r = straight_route(&[(len, 1.0)], 100.0);
        r.id = "downloadroute".into();
        Arc::new(r)
    }

    fn clients(server: &MockServer, key: &str) -> (MetadataClient, ImageFetcher) {
        let ep = Endpoints::local(&server.base).expect("local");
        (
            MetadataClient::new(ep.clone(), ApiKey::new(key).expect("key")).expect("meta"),
            ImageFetcher::new(ep, ApiKey::new(key).expect("key")).expect("images"),
        )
    }

    async fn wait_until(mut f: impl FnMut() -> bool, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if f() {
                return true;
            }
            sleep(Duration::from_millis(10)).await;
        }
        f()
    }

    #[tokio::test]
    async fn downloads_every_image_once_then_repeat_needs_no_requests() {
        let server = MockServer::streetview(StreetviewMock::default());
        let data = tempfile::tempdir().expect("tmp");
        let r = route(600.0);
        let (meta, images) = clients(&server, "GOODKEY");
        let cache = Arc::new(PanoCache::persistent(data.path(), &r.id));
        let store = Arc::new(ImageStore::open(data.path(), &r.id, r.total_dist).expect("store"));
        assert_eq!(store.plan_len(), 9);
        let mut h = Downloader::spawn(
            meta.clone(),
            images.clone(),
            Arc::clone(&cache),
            Arc::clone(&store),
            Arc::clone(&r),
        );
        assert!(
            wait_until(|| !h.is_running(), 10).await,
            "{:?}",
            h.progress()
        );
        let p = h.progress();
        assert_eq!(p.done, 9);
        assert_eq!(p.failed, 0);
        assert_eq!(p.stopped, None);
        assert_eq!(store.len(), 9);
        assert!(p.bytes >= 9 * MOCK_JPEG_LEN);
        // the requests are metadata + image, nothing else
        let lines = server.request_lines();
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with("GET /maps/api/streetview")),
            "{lines:?}"
        );
        let image_calls = lines.iter().filter(|l| !l.contains("/metadata?")).count();
        assert_eq!(image_calls, 9);
        assert!(image_calls <= p.total);

        // second run: every image is there, no request at all
        let before = server.request_count();
        let h2 = Downloader::spawn(meta, images, Arc::clone(&cache), Arc::clone(&store), r);
        assert!(wait_until(|| !h2.is_running(), 5).await);
        assert_eq!(server.request_count(), before);
        assert_eq!(h2.progress().done, 9);
        assert_eq!(h2.progress().failed, 0);
        h.cancel();
    }

    const MOCK_JPEG_LEN: u64 = crate::mock::MOCK_JPEG.len() as u64;

    #[tokio::test]
    async fn permanent_errors_stop_the_run_and_keep_what_was_saved() {
        let server = MockServer::streetview(StreetviewMock::default());
        let data = tempfile::tempdir().expect("tmp");
        let r = route(600.0);
        let (meta, images) = clients(&server, "BADKEY");
        let cache = Arc::new(PanoCache::memory(&r.id));
        let store = Arc::new(ImageStore::open(data.path(), &r.id, r.total_dist).expect("store"));
        let mut h = Downloader::spawn(meta, images, cache, Arc::clone(&store), r);
        assert!(
            wait_until(|| h.progress().stopped.is_some(), 10).await,
            "{:?}",
            h.progress()
        );
        assert_eq!(h.progress().stopped, Some(SvError::KeyRejected));
        assert!(wait_until(|| !h.is_running(), 5).await);
        assert!(store.is_empty());
        let n = server.request_count();
        sleep(Duration::from_millis(150)).await;
        assert_eq!(server.request_count(), n);
        h.cancel();
    }

    #[tokio::test]
    async fn a_storage_failure_stops_before_spending_the_whole_quota() {
        let server = MockServer::streetview(StreetviewMock::default());
        let data = tempfile::tempdir().unwrap();
        let r = route(2000.0);
        let (meta, images) = clients(&server, "GOODKEY");
        let cache = Arc::new(PanoCache::memory(&r.id));
        let store = Arc::new(ImageStore::open(data.path(), &r.id, r.total_dist).unwrap());
        let folder = data.path().join("streetview/img").join(&r.id);
        if folder.is_dir() {
            std::fs::remove_dir_all(&folder).unwrap();
        }
        std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
        std::fs::write(&folder, b"blocks the image directory").unwrap();
        let h = Downloader::spawn(meta, images, cache, Arc::clone(&store), r);
        assert!(
            h.progress().running,
            "running is published before spawn returns"
        );
        assert!(wait_until(|| !h.is_running(), 10).await);
        assert!(matches!(h.progress().stopped, Some(SvError::Storage(_))));
        assert_eq!(h.progress().stored, 0);
        assert!(
            server
                .request_lines()
                .iter()
                .filter(|line| !line.contains("metadata?"))
                .count()
                <= MAX_CONCURRENT
        );
    }

    #[tokio::test]
    async fn cancelling_stops_the_requests() {
        let server = MockServer::streetview(StreetviewMock {
            delay_ms: 40,
            ..Default::default()
        });
        let data = tempfile::tempdir().expect("tmp");
        let r = route(2_000.0);
        let (meta, images) = clients(&server, "GOODKEY");
        let cache = Arc::new(PanoCache::memory(&r.id));
        let store = Arc::new(ImageStore::open(data.path(), &r.id, r.total_dist).expect("store"));
        let mut h = Downloader::spawn(meta, images, cache, Arc::clone(&store), r);
        assert!(wait_until(|| h.progress().done >= 3, 10).await);
        h.cancel();
        sleep(Duration::from_millis(200)).await;
        let n = server.request_count();
        sleep(Duration::from_millis(400)).await;
        assert_eq!(server.request_count(), n);
        assert!(!h.is_running());
        assert!(!store.is_empty());
    }

    #[test]
    fn progress_notes_are_hungarian_and_bounded() {
        let p = Progress {
            total: 10,
            done: 4,
            failed: 1,
            stored: 3,
            bytes: 100,
            running: true,
            stopped: None,
        };
        assert!(p.note().contains("4/10"));
        assert!((p.fraction() - 0.4).abs() < 1e-9);
        let done = Progress {
            total: 10,
            done: 10,
            failed: 1,
            stored: 9,
            running: false,
            ..p
        };
        assert!(done.note().contains("9 kép"));
        let stopped = Progress {
            stopped: Some(SvError::QuotaExceeded),
            ..p
        };
        assert!(stopped.note().contains("Kvóta"));
        assert_eq!(Progress::default().fraction(), 0.0);
    }
}
