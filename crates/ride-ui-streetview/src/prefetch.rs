//! Resolving panorama ids: one bucket at a time ([`resolve_bucket`], [`resolve_live`]) and
//! ahead of the rider in the background ([`Prefetcher`]).
//!
//! The prefetcher only ever asks the *metadata* endpoint for ids (the explicitly allowed
//! thing to cache). It looks about 1.5 km ahead, one lookup per ~30 m, at most 4 requests at
//! a time and with a minimum gap between request starts; it stops by itself on key / request
//! errors, backs off on temporary ones and stops entirely when its handle is dropped (the
//! panel closing).

use std::sync::Arc;
use std::time::Duration;

use ride_model::Route;
use tokio::sync::{watch, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, Instant};

use crate::cache::{bucket_center, bucket_of, now_unix, Entry, PanoCache};
use crate::client::{MetadataClient, DEFAULT_RADIUS_M};
use crate::error::SvError;
use crate::planner::{smoothed_heading, PlannerConfig};

/// The route point, heading and coordinates a bucket is resolved for.
pub fn bucket_point(route: &Route, bucket: u32) -> (f64, f64, f64) {
    let d = bucket_center(bucket, route.total_dist);
    let pos = route.locate(d);
    let heading = smoothed_heading(route, d, &PlannerConfig::default().heading_offsets_m);
    (pos.lat, pos.lon, heading)
}

/// Asks Google for the bucket's panorama and stores the answer ("none" included).
/// Errors other than "no coverage" are returned and *not* cached.
pub async fn resolve_bucket(
    client: &MetadataClient,
    cache: &PanoCache,
    route: &Route,
    bucket: u32,
) -> Result<Option<String>, SvError> {
    let (lat, lng, heading) = bucket_point(route, bucket);
    match client.lookup(lat, lng, DEFAULT_RADIUS_M).await {
        Ok(info) => {
            cache.put(
                bucket,
                Entry::found(&info.pano_id, heading, lat, lng, now_unix()),
            );
            Ok(Some(info.pano_id))
        }
        Err(SvError::NoCoverage) => {
            cache.put(bucket, Entry::none(heading, lat, lng, now_unix()));
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Releases a bucket claim when dropped (also when the task is aborted).
struct ClaimGuard {
    cache: Arc<PanoCache>,
    bucket: u32,
}

impl Drop for ClaimGuard {
    fn drop(&mut self) {
        self.cache.release(self.bucket);
    }
}

/// The panel's lookup for the rider's bucket: cache first, then (waiting briefly for a
/// prefetch request already running for it) a request of its own.
pub async fn resolve_live(
    client: &MetadataClient,
    cache: &Arc<PanoCache>,
    route: &Route,
    bucket: u32,
) -> Result<Option<String>, SvError> {
    if let Some(e) = cache.get_at(bucket, now_unix()) {
        return Ok(e.pano);
    }
    if !cache.try_claim(bucket) {
        // The prefetcher is on it: give it a moment instead of asking twice.
        for _ in 0..40 {
            sleep(Duration::from_millis(100)).await;
            if let Some(e) = cache.get_at(bucket, now_unix()) {
                return Ok(e.pano);
            }
            if !cache.is_claimed(bucket) {
                break;
            }
        }
        if let Some(e) = cache.get_at(bucket, now_unix()) {
            return Ok(e.pano);
        }
        if !cache.try_claim(bucket) {
            // The original lookup still owns the claim. Never release its work.
            return Err(SvError::Network);
        }
    }
    let _guard = ClaimGuard {
        cache: Arc::clone(cache),
        bucket,
    };
    resolve_bucket(client, cache, route, bucket).await
}

// ───────────────────────────── prefetcher ─────────────────────────────

/// Prefetch tunables (defaults are the product policy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrefetchConfig {
    /// How far ahead of the rider ids are resolved, metres.
    pub window_m: f64,
    /// One lookup per this many metres of route.
    pub step_m: f64,
    pub max_concurrent: usize,
    /// Minimum time between two request starts (rate limit).
    pub min_start_gap: Duration,
    /// Check interval when everything ahead is known.
    pub idle_poll: Duration,
    /// Longest back-off after temporary failures.
    pub backoff_max: Duration,
}

impl Default for PrefetchConfig {
    fn default() -> Self {
        PrefetchConfig {
            window_m: 1500.0,
            step_m: 30.0,
            max_concurrent: 4,
            min_start_gap: Duration::from_millis(120),
            idle_poll: Duration::from_millis(500),
            backoff_max: Duration::from_secs(30),
        }
    }
}

/// Buckets to know ahead of `from_dist`: one per `step_m` up to `window_m`, nearest first,
/// unique, within the route.
pub fn window_buckets(total_dist: f64, from_dist: f64, cfg: &PrefetchConfig) -> Vec<u32> {
    if !from_dist.is_finite() || !total_dist.is_finite() || total_dist <= 0.0 {
        return Vec::new();
    }
    let step = cfg.step_m.max(1.0);
    let from = from_dist.clamp(0.0, total_dist);
    let to = (from + cfg.window_m.max(0.0)).min(total_dist);
    let mut out: Vec<u32> = Vec::new();
    let mut d = from;
    loop {
        let b = bucket_of(d.min(total_dist));
        if out.last() != Some(&b) {
            out.push(b);
        }
        if d >= to {
            break;
        }
        d += step;
    }
    let last = bucket_of(to);
    if out.last() != Some(&last) {
        out.push(last);
    }
    out
}

/// The buckets of `wanted` that still need a lookup.
pub fn missing_buckets(cache: &PanoCache, wanted: &[u32], now: u64) -> Vec<u32> {
    wanted
        .iter()
        .copied()
        .filter(|b| !cache.contains(*b, now) && !cache.is_claimed(*b))
        .collect()
}

/// How many of `wanted` are known (fresh) in the cache; buckets whose lookup is still in
/// flight do not count.
fn known_count(cache: &PanoCache, wanted: &[u32]) -> usize {
    let now = now_unix();
    wanted.iter().filter(|b| cache.contains(**b, now)).count()
}

/// What the background task reports (for the small "előtöltve" readout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Buckets in the window ahead.
    pub ahead_total: usize,
    /// Of those, already known.
    pub ahead_ready: usize,
    /// Metadata requests started so far.
    pub requests: usize,
    /// Set when the task gave up (key / request error).
    pub stopped: Option<SvError>,
}

/// Handle of the running background task. Dropping it (or [`PrefetchHandle::stop`]) ends the
/// task and any request in flight.
pub struct PrefetchHandle {
    pos: watch::Sender<f64>,
    progress: watch::Receiver<Progress>,
    task: Option<JoinHandle<()>>,
    cache: Arc<PanoCache>,
}

impl std::fmt::Debug for PrefetchHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrefetchHandle")
            .field("running", &self.is_running())
            .field("progress", &*self.progress.borrow())
            .finish()
    }
}

impl PrefetchHandle {
    /// Tells the task where the rider is (route distance, metres). Cheap; call on every tick.
    pub fn set_position(&self, dist_m: f64) {
        if dist_m.is_finite() {
            self.pos.send_replace(dist_m);
        }
    }

    pub fn progress(&self) -> Progress {
        *self.progress.borrow()
    }

    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Ends the task and writes the cache out.
    pub fn stop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        let _ = self.cache.flush();
    }
}

impl Drop for PrefetchHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts background resolution of panorama ids ahead of the rider.
pub struct Prefetcher;

impl Prefetcher {
    /// Spawns the task on the current tokio runtime. Without a runtime (SSR, plain unit
    /// tests) the returned handle is inert.
    pub fn spawn(
        client: MetadataClient,
        cache: Arc<PanoCache>,
        route: Arc<Route>,
        start_dist: f64,
        cfg: PrefetchConfig,
    ) -> PrefetchHandle {
        let (pos_tx, pos_rx) = watch::channel(if start_dist.is_finite() {
            start_dist
        } else {
            0.0
        });
        let (prog_tx, prog_rx) = watch::channel(Progress::default());
        let task = tokio::runtime::Handle::try_current().ok().map(|rt| {
            let cache = Arc::clone(&cache);
            rt.spawn(run(client, cache, route, cfg, pos_rx, prog_tx))
        });
        PrefetchHandle {
            pos: pos_tx,
            progress: prog_rx,
            task,
            cache,
        }
    }
}

type Done = (u32, Result<Option<String>, SvError>);

/// Folds one finished lookup into the failure bookkeeping. Returns the back-off to wait
/// after a temporary failure; a permanent one is recorded in `stopped`.
fn on_done(
    joined: Result<Done, tokio::task::JoinError>,
    failures: &mut u32,
    stopped: &mut Option<SvError>,
    backoff_max: Duration,
) -> Option<Duration> {
    match joined {
        Ok((_, Ok(_))) => {
            *failures = 0;
            None
        }
        Ok((_, Err(e))) if e.is_permanent() => {
            *stopped = Some(e);
            None
        }
        Ok((_, Err(_))) => {
            *failures = failures.saturating_add(1);
            Some(Duration::from_secs(1u64 << (*failures).min(5)).min(backoff_max))
        }
        Err(_) => None,
    }
}

async fn run(
    client: MetadataClient,
    cache: Arc<PanoCache>,
    route: Arc<Route>,
    cfg: PrefetchConfig,
    mut pos: watch::Receiver<f64>,
    progress: watch::Sender<Progress>,
) {
    let gate = Arc::new(Semaphore::new(cfg.max_concurrent.max(1)));
    let mut set: JoinSet<Done> = JoinSet::new();
    let mut requests = 0usize;
    let mut failures = 0u32;
    let mut last_start: Option<Instant> = None;
    let mut stopped: Option<SvError> = None;

    'main: loop {
        let from = *pos.borrow_and_update();
        let wanted = window_buckets(route.total_dist, from, &cfg);
        let missing = missing_buckets(&cache, &wanted, now_unix());
        progress.send_replace(Progress {
            ahead_total: wanted.len(),
            ahead_ready: known_count(&cache, &wanted),
            requests,
            stopped,
        });

        // Start as many lookups as the concurrency limit and the rate limit allow; finished
        // ones are reaped on the way so a key error is noticed before more are sent.
        let mut backoff = Duration::ZERO;
        for b in missing {
            while let Some(j) = set.try_join_next() {
                if let Some(w) = on_done(j, &mut failures, &mut stopped, cfg.backoff_max) {
                    backoff = backoff.max(w);
                }
            }
            if stopped.is_some() {
                break 'main;
            }
            if !backoff.is_zero() || gate.available_permits() == 0 {
                break;
            }
            if !cache.try_claim(b) {
                continue;
            }
            if let Some(t) = last_start {
                let ready = t + cfg.min_start_gap;
                if Instant::now() < ready {
                    tokio::time::sleep_until(ready).await;
                }
            }
            let Ok(permit) = Arc::clone(&gate).try_acquire_owned() else {
                cache.release(b);
                break;
            };
            last_start = Some(Instant::now());
            requests += 1;
            let (client, cache2, route2) = (client.clone(), Arc::clone(&cache), Arc::clone(&route));
            set.spawn(async move {
                let _permit = permit;
                let _claim = ClaimGuard {
                    cache: Arc::clone(&cache2),
                    bucket: b,
                };
                let res = resolve_bucket(&client, &cache2, &route2, b).await;
                (b, res)
            });
        }
        if !backoff.is_zero() {
            sleep(backoff).await;
            continue;
        }

        if set.is_empty() {
            // Everything ahead is known: wait for the rider to move (or poll for expiry).
            let _ = cache.flush();
            tokio::select! {
                _ = pos.changed() => {}
                _ = sleep(cfg.idle_poll) => {}
            }
            continue;
        }

        // Wait for one lookup to finish (or for the rider to move on).
        let joined = tokio::select! {
            j = set.join_next() => j,
            _ = pos.changed() => continue 'main,
        };
        if let Some(j) = joined {
            if let Some(w) = on_done(j, &mut failures, &mut stopped, cfg.backoff_max) {
                sleep(w).await;
            }
            if stopped.is_some() {
                break 'main;
            }
        }
    }

    // Gave up: stop what is running, keep what we learned, report why.
    set.abort_all();
    let _ = cache.flush();
    let from = *pos.borrow();
    let wanted = window_buckets(route.total_dist, from, &cfg);
    progress.send_replace(Progress {
        ahead_total: wanted.len(),
        ahead_ready: known_count(&cache, &wanted),
        requests,
        stopped,
    });
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_waiter_never_releases_another_lookups_claim() {
        use super::*;
        use crate::url::{ApiKey, Endpoints};
        let route = ride_model::testing::straight_route(&[(100.0, 0.0)], 0.0);
        let cache = Arc::new(PanoCache::memory(&route.id));
        assert!(cache.try_claim(0));
        let client = MetadataClient::new(
            Endpoints::local("http://127.0.0.1:9").unwrap(),
            ApiKey::new("TESTKEY").unwrap(),
        )
        .unwrap();
        assert_eq!(
            resolve_live(&client, &cache, &route, 0).await,
            Err(SvError::Network)
        );
        assert!(cache.is_claimed(0));
        assert!(!cache.try_claim(0));
    }

    use super::*;
    use crate::mock::{MockServer, StreetviewMock};
    use crate::url::{ApiKey, Endpoints};
    use ride_model::testing::straight_route;

    fn route(len: f64) -> Arc<Route> {
        let mut r = straight_route(&[(len, 1.0)], 100.0);
        r.id = "prefetchroute".into();
        Arc::new(r)
    }

    fn client(server: &MockServer, key: &str) -> MetadataClient {
        MetadataClient::new(
            Endpoints::local(&server.base).expect("local"),
            ApiKey::new(key).expect("key"),
        )
        .expect("client")
    }

    fn fast_cfg() -> PrefetchConfig {
        PrefetchConfig {
            min_start_gap: Duration::from_millis(2),
            idle_poll: Duration::from_millis(20),
            backoff_max: Duration::from_millis(50),
            ..PrefetchConfig::default()
        }
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

    #[test]
    fn window_covers_one_and_a_half_km_in_30_m_steps() {
        let cfg = PrefetchConfig::default();
        let w = window_buckets(10_000.0, 0.0, &cfg);
        // 0, 30, ..., 1500 -> 51 positions, all distinct buckets (30 m > 25 m)
        assert_eq!(w.len(), 51);
        assert_eq!(w[0], 0);
        assert_eq!(*w.last().expect("last"), bucket_of(1500.0));
        assert!(w.windows(2).all(|p| p[0] < p[1]));
        // moving on keeps the window length
        let w2 = window_buckets(10_000.0, 4_000.0, &cfg);
        assert_eq!(w2.len(), 51);
        assert_eq!(w2[0], bucket_of(4_000.0));
        // near the end the window shrinks and never leaves the route
        let w3 = window_buckets(10_000.0, 9_900.0, &cfg);
        assert!(w3.len() <= 5 && *w3.last().expect("last") == bucket_of(10_000.0));
        // at / beyond the end: just the last bucket; degenerate input: nothing
        assert_eq!(
            window_buckets(10_000.0, 20_000.0, &cfg),
            vec![bucket_of(10_000.0)]
        );
        assert!(window_buckets(10_000.0, f64::NAN, &cfg).is_empty());
        assert!(window_buckets(0.0, 0.0, &cfg).is_empty());
        // tiny steps never produce duplicates
        let fine = PrefetchConfig { step_m: 5.0, ..cfg };
        let w = window_buckets(10_000.0, 0.0, &fine);
        assert!(w.windows(2).all(|p| p[0] < p[1]));
    }

    #[test]
    fn missing_skips_known_and_claimed() {
        let cache = PanoCache::memory("r");
        let now = now_unix();
        cache.put(1, Entry::found("A", 0.0, 1.0, 2.0, now));
        cache.put(2, Entry::none(0.0, 1.0, 2.0, now));
        assert!(cache.try_claim(3));
        assert_eq!(missing_buckets(&cache, &[1, 2, 3, 4, 5], now), vec![4, 5]);
        // expired entries count as missing again
        let old = Entry::found("A", 0.0, 1.0, 2.0, now - crate::cache::TTL_FOUND_S - 10);
        cache.put(6, old);
        assert_eq!(missing_buckets(&cache, &[6], now), vec![6]);
    }

    #[tokio::test]
    async fn resolves_the_window_with_bounded_concurrency_then_repeat_rides_need_no_calls() {
        let server = MockServer::streetview(StreetviewMock {
            delay_ms: 60,
            ..Default::default()
        });
        let data = tempfile::tempdir().expect("tmp");
        let r = route(8_000.0);
        let cache = Arc::new(PanoCache::persistent(data.path(), &r.id));
        let mut h = Prefetcher::spawn(
            client(&server, "K"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            fast_cfg(),
        );

        assert!(
            wait_until(
                || {
                    let p = h.progress();
                    p.ahead_total == 51 && p.ahead_ready == 51
                },
                20
            )
            .await,
            "{:?}",
            h.progress()
        );
        assert_eq!(
            server.request_count(),
            51,
            "exactly one metadata call per bucket"
        );
        let peak = server.max_in_flight();
        assert!(peak <= 4, "never more than 4 at once, saw {peak}");
        assert!(peak >= 2, "but it does run in parallel, saw {peak}");
        // all requests were metadata calls, none for images/embeds
        assert!(server
            .request_lines()
            .iter()
            .all(|l| l.starts_with("GET /maps/api/streetview/metadata?")));

        // the rider moves 1 km: only the new part of the window is requested
        h.set_position(1_000.0);
        assert!(
            wait_until(
                || h.progress().ahead_ready == h.progress().ahead_total
                    && h.progress().ahead_total == 51
                    && server.request_count() > 51,
                20
            )
            .await
        );
        let w0 = window_buckets(r.total_dist, 0.0, &fast_cfg());
        let new = window_buckets(r.total_dist, 1_000.0, &fast_cfg())
            .into_iter()
            .filter(|b| !w0.contains(b))
            .count();
        assert_eq!(server.request_count(), 51 + new);

        h.stop();
        assert!(!h.is_running());
        drop(h);
        drop(cache);
        let calls_before = server.request_count();

        // second ride: ids come from the file, the metadata endpoint is not contacted
        let cache2 = Arc::new(PanoCache::persistent(data.path(), &r.id));
        assert!(cache2.len() >= 51 + new);
        let h2 = Prefetcher::spawn(
            client(&server, "K"),
            Arc::clone(&cache2),
            Arc::clone(&r),
            0.0,
            fast_cfg(),
        );
        assert!(wait_until(|| h2.progress().ahead_ready == 51, 5).await);
        sleep(Duration::from_millis(200)).await;
        assert_eq!(
            server.request_count(),
            calls_before,
            "repeat ride makes no calls"
        );
        // the live path answers from the cache too
        let c = client(&server, "K");
        let id = resolve_live(&c, &cache2, &r, bucket_of(300.0))
            .await
            .expect("cached");
        assert!(id.is_some());
        assert_eq!(server.request_count(), calls_before);
    }

    #[tokio::test]
    async fn rate_limit_spaces_request_starts() {
        let server = MockServer::streetview(Default::default());
        let r = route(8_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let cfg = PrefetchConfig {
            window_m: 270.0, // 10 buckets
            min_start_gap: Duration::from_millis(60),
            ..fast_cfg()
        };
        let t0 = Instant::now();
        let h = Prefetcher::spawn(
            client(&server, "K"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            cfg,
        );
        assert!(
            wait_until(
                || h.progress().ahead_total == 10 && h.progress().ahead_ready == 10,
                10
            )
            .await
        );
        let took = t0.elapsed();
        assert!(
            took >= Duration::from_millis(9 * 60 - 20),
            "10 starts with a 60 ms gap need >= ~540 ms, took {took:?}"
        );
    }

    #[tokio::test]
    async fn no_coverage_is_remembered_and_not_asked_again() {
        let server = MockServer::streetview(StreetviewMock {
            // roughly the first third of the east-going route has no panorama
            covered: Arc::new(|_, lng| lng > 19.0 + 0.0045),
            ..Default::default()
        });
        let r = route(3_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let h = Prefetcher::spawn(
            client(&server, "K"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            fast_cfg(),
        );
        assert!(
            wait_until(
                || h.progress().ahead_ready == h.progress().ahead_total
                    && h.progress().ahead_total > 0,
                10
            )
            .await
        );
        let n = server.request_count();
        let none = (0..60)
            .filter(|b| {
                matches!(
                    cache.get(f64::from(*b) * 25.0 + 1.0),
                    Some(Entry { pano: None, .. })
                )
            })
            .count();
        assert!(none > 5, "some buckets are negative entries: {none}");
        sleep(Duration::from_millis(200)).await;
        assert_eq!(server.request_count(), n, "negative answers are cached too");
    }

    #[tokio::test]
    async fn key_errors_stop_the_prefetcher() {
        let server = MockServer::streetview(Default::default());
        let r = route(8_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let cfg = PrefetchConfig {
            min_start_gap: Duration::from_millis(40),
            ..fast_cfg()
        };
        let h = Prefetcher::spawn(
            client(&server, "BADKEY"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            cfg,
        );
        assert!(wait_until(|| h.progress().stopped.is_some(), 10).await);
        assert_eq!(h.progress().stopped, Some(SvError::KeyRejected));
        assert!(wait_until(|| !h.is_running(), 5).await);
        let n = server.request_count();
        assert!(n <= 4, "at most the first batch was sent, got {n}");
        assert!(cache.is_empty(), "failures are not cached");
        sleep(Duration::from_millis(150)).await;
        assert_eq!(server.request_count(), n);
    }

    #[tokio::test]
    async fn transient_errors_back_off_and_recover_without_caching_failures() {
        let server = MockServer::streetview(Default::default());
        let r = route(2_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let h = Prefetcher::spawn(
            client(&server, "HTTP500KEY"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            fast_cfg(),
        );
        sleep(Duration::from_millis(400)).await;
        assert!(h.is_running(), "temporary errors do not end the task");
        assert_eq!(h.progress().stopped, None);
        assert!(cache.is_empty());
        // back-off keeps the call rate low (not a hot loop)
        assert!(server.request_count() < 40, "{}", server.request_count());
    }

    #[tokio::test]
    async fn dropping_the_handle_stops_the_requests() {
        let server = MockServer::streetview(StreetviewMock {
            delay_ms: 30,
            ..Default::default()
        });
        let r = route(8_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let h = Prefetcher::spawn(
            client(&server, "K"),
            Arc::clone(&cache),
            Arc::clone(&r),
            0.0,
            fast_cfg(),
        );
        assert!(wait_until(|| server.request_count() >= 5, 10).await);
        drop(h);
        sleep(Duration::from_millis(150)).await; // let requests that were already on the wire land
        let n = server.request_count();
        sleep(Duration::from_millis(300)).await;
        assert_eq!(
            server.request_count(),
            n,
            "no new requests after the panel closed"
        );
        assert!(n < 51);
    }

    #[tokio::test]
    async fn live_lookup_uses_the_cache_and_negative_answers() {
        let server = MockServer::streetview(Default::default());
        let r = route(2_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let c = client(&server, "K");
        let id = resolve_live(&c, &cache, &r, 10).await.expect("ok");
        assert!(id.is_some());
        assert_eq!(server.request_count(), 1);
        let again = resolve_live(&c, &cache, &r, 10).await.expect("ok");
        assert_eq!(again, id);
        assert_eq!(server.request_count(), 1, "second call is a cache hit");
        // errors are returned, not cached, and the claim is released
        let bad = client(&server, "BADKEY");
        assert_eq!(
            resolve_live(&bad, &cache, &r, 11).await.unwrap_err(),
            SvError::KeyRejected
        );
        assert!(!cache.is_claimed(11));
        assert!(cache.get_at(11, now_unix()).is_none());
    }

    #[tokio::test]
    async fn live_lookup_waits_for_a_running_prefetch_of_the_same_bucket() {
        let server = MockServer::streetview(StreetviewMock {
            delay_ms: 150,
            ..Default::default()
        });
        let r = route(2_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let c = client(&server, "K");
        assert!(cache.try_claim(7));
        let (c2, cache2, r2) = (c.clone(), Arc::clone(&cache), Arc::clone(&r));
        let background = tokio::spawn(async move {
            let _g = ClaimGuard {
                cache: Arc::clone(&cache2),
                bucket: 7,
            };
            resolve_bucket(&c2, &cache2, &r2, 7).await
        });
        sleep(Duration::from_millis(20)).await;
        let live = resolve_live(&c, &cache, &r, 7).await.expect("ok");
        assert!(live.is_some());
        assert!(background.await.expect("join").is_ok());
        assert_eq!(server.request_count(), 1, "only one request for the bucket");
    }

    #[test]
    fn spawn_without_a_runtime_gives_an_inert_handle() {
        let server = MockServer::streetview(Default::default());
        let r = route(1_000.0);
        let cache = Arc::new(PanoCache::memory(&r.id));
        let h = Prefetcher::spawn(
            client(&server, "K"),
            cache,
            r,
            0.0,
            PrefetchConfig::default(),
        );
        assert!(!h.is_running());
        h.set_position(100.0);
        assert_eq!(h.progress(), Progress::default());
        assert_eq!(server.request_count(), 0);
    }
}
