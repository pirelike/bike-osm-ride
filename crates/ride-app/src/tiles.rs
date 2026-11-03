//! Tile proxy: the Dioxus custom protocol `tiles` (the webview sees
//! `http://tiles.localhost/<style>/<z>/<x>/<y>.png` on Windows, `tiles://localhost/...` on
//! macOS and Linux and `https://tiles.localhost/...` on Android — see `ride_ui_kit::protocol`;
//! the handler only reads the path, so one registration serves all three).
//!
//! Request path: `ride_ui_map::parse_tile_path` -> disk cache
//! (`<data>/tiles/<style>/<z>/<x>/<y>.png`, 30 days) -> otherwise one `reqwest` GET of
//! `ride_ui_map::upstream_url` (rustls, 10 s timeout, at most 6 in flight, identical tiles
//! are fetched once) -> written to the cache -> returned as `image/png`.
//!
//! The map must never show broken-image icons, so every failure (offline, HTTP error,
//! timeout, non-PNG answer) returns a transparent 256x256 PNG with status 200 and
//! `Cache-Control: no-store`; an expired cache entry is served in preference to that when
//! the network fails. Failed tiles are remembered for a short time and a transport error
//! pauses all network access briefly, so an offline session does not queue up 10 s
//! timeouts. Tiles are only fetched when the map asks for them (no prefetching), as the
//! OSM tile usage policy requires.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use dioxus::desktop::wry::http::{Method, Response};
use dioxus::desktop::Config;
use ride_model::MapStyle;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

pub const USER_AGENT: &str = "bike-osm-ride/0.1 (desktop app)";
pub const CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
pub const MAX_CONCURRENT: usize = 6;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// A failed tile (HTTP error, bad body) is not requested again for this long.
const FAILURE_MEMORY: Duration = Duration::from_secs(30);
/// After a transport error (no network, DNS, timeout) no tile is requested for this long.
const OFFLINE_PAUSE: Duration = Duration::from_secs(8);
const MAX_TILE_BYTES: usize = 1_500_000;
const MAX_ZOOM: u8 = 19;
const TILE_PX: u32 = 256;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const TILE_CACHE_CONTROL: &str = "public, max-age=86400";

type UpstreamFn = Box<dyn Fn(MapStyle, u8, u32, u32) -> String + Send + Sync>;

/// What the proxy answers for one request.
#[derive(Debug, Clone, PartialEq)]
pub struct TileResponse {
    pub status: u16,
    pub body: Cow<'static, [u8]>,
    pub cache_control: &'static str,
}

impl TileResponse {
    fn tile(bytes: Vec<u8>) -> Self {
        Self {
            status: 200,
            body: Cow::Owned(bytes),
            cache_control: TILE_CACHE_CONTROL,
        }
    }

    /// The "no tile" answer: a transparent PNG with status 200 that is never cached.
    pub fn transparent() -> Self {
        Self {
            status: 200,
            body: Cow::Borrowed(transparent_png()),
            cache_control: "no-store",
        }
    }

    fn error(status: u16) -> Self {
        Self {
            status,
            body: Cow::Borrowed(&[]),
            cache_control: "no-store",
        }
    }

    pub fn into_http(self) -> Response<Cow<'static, [u8]>> {
        let content_type = if self.body.is_empty() {
            "text/plain"
        } else {
            "image/png"
        };
        Response::builder()
            .status(self.status)
            .header("Content-Type", content_type)
            .header("Cache-Control", self.cache_control)
            .header("Access-Control-Allow-Origin", "*")
            .body(self.body)
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
    }
}

#[derive(Debug)]
enum FetchError {
    /// No client could be built.
    NoClient,
    /// The request never completed (offline, DNS, TLS, timeout).
    Transport,
    /// The server answered with an HTTP error.
    Status(u16),
    /// The answer was not a PNG or implausibly large.
    BadBody,
    /// Skipped on purpose (recent failure / offline pause).
    Skipped,
}

pub struct TileService {
    client: Option<reqwest::Client>,
    root: PathBuf,
    upstream: UpstreamFn,
    ttl: Duration,
    permits: Semaphore,
    inflight: Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>,
    failures: Mutex<HashMap<PathBuf, Instant>>,
    offline_until: Mutex<Option<Instant>>,
    tmp_counter: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl TileService {
    pub fn new(root: PathBuf, upstream: UpstreamFn, ttl: Duration) -> Self {
        let client = ride_route::http::client_builder()
            .user_agent(USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|err| {
                eprintln!("[ride-app] tile proxy: HTTP client could not be created: {err}")
            })
            .ok();
        Self {
            client,
            root,
            upstream,
            ttl,
            permits: Semaphore::new(MAX_CONCURRENT),
            inflight: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            offline_until: Mutex::new(None),
            tmp_counter: AtomicU64::new(0),
        }
    }

    /// The production service: real upstream servers, cache under the data directory.
    pub fn standard() -> Self {
        let upstream: UpstreamFn = match crate::state::boot().hooks.tile_base.clone() {
            // Test hook (RIDE_TILE_BASE): a stand-in tile server, or an unreachable one for "offline".
            Some(base) => Box::new(move |style, z, x, y| {
                format!("{base}/{}/{z}/{x}/{y}.png", style_dir(style))
            }),
            None => Box::new(ride_ui_map::upstream_url),
        };
        Self::new(crate::paths::tiles_dir(), upstream, CACHE_TTL)
    }

    /// Answers a request for `path` (the URL path, e.g. `/dark/14/8923/5701.png`).
    pub async fn respond(&self, path: &str) -> TileResponse {
        let Some((style, z, x, y)) = ride_ui_map::parse_tile_path(path) else {
            return TileResponse::error(400);
        };
        if !tile_in_range(z, x, y) {
            return TileResponse::transparent();
        }
        match self.fetch(style, z, x, y).await {
            Ok(bytes) => TileResponse::tile(bytes),
            Err(_) => TileResponse::transparent(),
        }
    }

    fn cache_path(&self, style: MapStyle, z: u8, x: u32, y: u32) -> PathBuf {
        cache_path(&self.root, style, z, x, y)
    }

    async fn fetch(&self, style: MapStyle, z: u8, x: u32, y: u32) -> Result<Vec<u8>, FetchError> {
        let path = self.cache_path(style, z, x, y);

        let stale = match read_cached(&path, self.ttl).await {
            Some((bytes, true)) => return Ok(bytes),
            Some((bytes, false)) => Some(bytes),
            None => None,
        };
        if self.network_paused(&path) {
            return stale.ok_or(FetchError::Skipped);
        }

        // One download per tile: later requests for the same tile wait here and then
        // find the file in the cache.
        let flight = self.flight_lock(&path);
        let result = {
            let _turn = flight.lock().await;
            match read_cached(&path, self.ttl).await {
                Some((bytes, true)) => Ok(bytes),
                // The request we waited for may just have failed: do not retry right away.
                _ if self.network_paused(&path) => Err(FetchError::Skipped),
                _ => self.download_and_store(&path, style, z, x, y).await,
            }
        };
        self.release_flight(&path, flight);

        match result {
            Ok(bytes) => Ok(bytes),
            Err(err) => {
                self.remember_failure(&path, &err);
                stale.ok_or(err)
            }
        }
    }

    async fn download_and_store(
        &self,
        path: &Path,
        style: MapStyle,
        z: u8,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, FetchError> {
        let client = self.client.as_ref().ok_or(FetchError::NoClient)?;
        let url = (self.upstream)(style, z, x, y);
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| FetchError::Skipped)?;

        let response = client
            .get(&url)
            .header("Accept", "image/png,image/*;q=0.8")
            .send()
            .await
            .map_err(|_| FetchError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|len| len > MAX_TILE_BYTES as u64)
        {
            return Err(FetchError::BadBody);
        }
        let bytes = response.bytes().await.map_err(|_| FetchError::Transport)?;
        if bytes.len() > MAX_TILE_BYTES || !is_png(&bytes) {
            return Err(FetchError::BadBody);
        }
        let bytes = bytes.to_vec();
        if let Err(err) = self.write_atomic(path, &bytes).await {
            // Not fatal: the tile is still shown, it is just not cached.
            eprintln!(
                "[ride-app] tile cache write failed for {}: {err}",
                path.display()
            );
        }
        Ok(bytes)
    }

    async fn write_atomic(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let n = self.tmp_counter.fetch_add(1, Ordering::Relaxed);
        let tmp = path.with_extension(format!("tmp{}-{n}", std::process::id()));
        tokio::fs::write(&tmp, bytes).await?;
        if let Err(err) = tokio::fs::rename(&tmp, path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(err);
        }
        Ok(())
    }

    fn flight_lock(&self, path: &Path) -> Arc<AsyncMutex<()>> {
        lock(&self.inflight)
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    fn release_flight(&self, path: &Path, flight: Arc<AsyncMutex<()>>) {
        let mut map = lock(&self.inflight);
        // The map holds one reference and `flight` another; any more means waiters remain.
        if Arc::strong_count(&flight) <= 2 {
            map.remove(path);
        }
    }

    fn network_paused(&self, path: &Path) -> bool {
        let now = Instant::now();
        if lock(&self.offline_until).is_some_and(|until| now < until) {
            return true;
        }
        lock(&self.failures)
            .get(path)
            .is_some_and(|at| now.saturating_duration_since(*at) < FAILURE_MEMORY)
    }

    fn remember_failure(&self, path: &Path, err: &FetchError) {
        let now = Instant::now();
        match err {
            FetchError::Transport => {
                let mut until = lock(&self.offline_until);
                if until.is_none_or(|u| now >= u) {
                    eprintln!("[ride-app] tile proxy: no network, serving blank tiles (next attempt in {} s)", OFFLINE_PAUSE.as_secs());
                }
                *until = Some(now + OFFLINE_PAUSE);
            }
            FetchError::Status(_) | FetchError::BadBody => {
                if let FetchError::Status(code @ (403 | 429)) = err {
                    // The tile server refuses us (usage policy / rate limit): worth a log line.
                    eprintln!(
                        "[ride-app] tile server answered HTTP {code}: {}",
                        path.display()
                    );
                }
                let mut failures = lock(&self.failures);
                if failures.len() > 2048 {
                    failures.retain(|_, at| now.saturating_duration_since(*at) < FAILURE_MEMORY);
                }
                failures.insert(path.to_path_buf(), now);
            }
            FetchError::NoClient | FetchError::Skipped => {}
        }
    }
}

pub fn style_dir(style: MapStyle) -> &'static str {
    match style {
        MapStyle::Dark => "dark",
        MapStyle::Light => "light",
        MapStyle::Osm => "osm",
    }
}

/// `<root>/<style>/<z>/<x>/<y>.png`
pub fn cache_path(root: &Path, style: MapStyle, z: u8, x: u32, y: u32) -> PathBuf {
    root.join(style_dir(style))
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"))
}

/// Is `(z, x, y)` a tile that exists in the Web-Mercator grid up to the supported zoom?
pub fn tile_in_range(z: u8, x: u32, y: u32) -> bool {
    z <= MAX_ZOOM && u64::from(x) < (1u64 << z) && u64::from(y) < (1u64 << z)
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.len() > PNG_MAGIC.len() && bytes.starts_with(PNG_MAGIC)
}

/// `(bytes, fresh)` of a cached tile; corrupt files are removed.
async fn read_cached(path: &Path, ttl: Duration) -> Option<(Vec<u8>, bool)> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    let bytes = tokio::fs::read(path).await.ok()?;
    if !is_png(&bytes) {
        let _ = tokio::fs::remove_file(path).await;
        return None;
    }
    let fresh = match meta.modified() {
        Ok(modified) => match SystemTime::now().duration_since(modified) {
            Ok(age) => age < ttl,
            Err(_) => true, // modification time in the future (clock change)
        },
        Err(_) => false,
    };
    Some((bytes, fresh))
}

// ───────────────────────── transparent fallback tile ─────────────────────────

/// A fully transparent 256x256 PNG (1-bit indexed with a single fully transparent
/// palette entry, stored without compression: about 8.6 kB).
pub fn transparent_png() -> &'static [u8] {
    static PNG: OnceLock<Vec<u8>> = OnceLock::new();
    PNG.get_or_init(|| build_transparent_png(TILE_PX))
        .as_slice()
}

fn build_transparent_png(size: u32) -> Vec<u8> {
    let mut png = PNG_MAGIC.to_vec();

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[1, 3, 0, 0, 0]); // 1 bit per pixel, indexed colour, no interlace
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"PLTE", &[0, 0, 0]);
    push_chunk(&mut png, b"tRNS", &[0]); // palette entry 0 is fully transparent

    // Each scanline: filter type 0 + ceil(size / 8) zero bytes (all pixels = palette entry 0).
    let row_bytes = 1 + size.div_ceil(8) as usize;
    let raw = vec![0u8; row_bytes * size as usize];
    push_chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// zlib container around "stored" (uncompressed) deflate blocks.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if raw.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none())); // BFINAL, BTYPE = 00 (stored)
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

// ───────────────────────── protocol registration ─────────────────────────

static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();

/// A small runtime of its own so tile traffic never competes with the UI thread.
fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("tile-proxy")
                .enable_all()
                .build()
                .map_err(|err| {
                    eprintln!("[ride-app] tile proxy: runtime could not be started: {err}")
                })
                .ok()
        })
        .as_ref()
}

/// Registers the `tiles` protocol on the desktop config.
pub fn register(config: Config) -> Config {
    let service = Arc::new(TileService::standard());
    config.with_asynchronous_custom_protocol("tiles", move |_webview, request, responder| {
        if request.method() != Method::GET && request.method() != Method::HEAD {
            responder.respond(TileResponse::error(405).into_http());
            return;
        }
        let Some(rt) = runtime() else {
            responder.respond(TileResponse::transparent().into_http());
            return;
        };
        let path = request.uri().path().to_string();
        let service = service.clone();
        rt.spawn(async move {
            let response = service.respond(&path).await;
            responder.respond(response.into_http());
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use std::net::TcpListener;

    fn service(root: &Path, base: &str) -> TileService {
        let base = base.to_string();
        TileService::new(
            root.to_path_buf(),
            Box::new(move |style, z, x, y| format!("{base}/{}/{z}/{x}/{y}.png", style_dir(style))),
            CACHE_TTL,
        )
    }

    /// Serves `body` as image/png to the first `count` connections, then stops listening.
    fn serve(
        count: usize,
        status: &'static str,
        body: Vec<u8>,
    ) -> (String, std::thread::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let handle = std::thread::spawn(move || {
            let mut served = 0;
            for stream in listener.incoming().take(count) {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let mut seen = Vec::new();
                while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                served += 1;
            }
            served
        });
        (base, handle)
    }

    fn decode_alpha(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info().expect("valid PNG");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("frame");
        assert_eq!(
            info.color_type,
            png::ColorType::Rgba,
            "tRNS expands to RGBA"
        );
        let alpha = buf[..info.buffer_size()]
            .chunks(4)
            .map(|px| px[3])
            .collect();
        (info.width, info.height, alpha)
    }

    #[test]
    fn fallback_tile_is_a_valid_transparent_256_png() {
        let (w, h, alpha) = decode_alpha(transparent_png());
        assert_eq!((w, h), (256, 256));
        assert_eq!(alpha.len(), 256 * 256);
        assert!(alpha.iter().all(|&a| a == 0));
        assert!(transparent_png().len() < 16_000);
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn zlib_stored_splits_large_inputs() {
        let raw = vec![7u8; 70_000];
        let z = zlib_stored(&raw);
        // 2 header bytes + (5 + 65535) + (5 + 4465) + 4 adler bytes
        assert_eq!(z.len(), 2 + 5 + 65_535 + 5 + 4_465 + 4);
        assert_eq!(z[2], 0, "first block is not final");
        assert_eq!(z[2 + 5 + 65_535], 1, "second block is final");
    }

    #[test]
    fn tile_range() {
        assert!(tile_in_range(0, 0, 0));
        assert!(!tile_in_range(0, 1, 0));
        assert!(tile_in_range(14, 8923, 5701));
        assert!(!tile_in_range(14, 16384, 0));
        assert!(tile_in_range(19, 524_287, 524_287));
        assert!(!tile_in_range(20, 0, 0));
        assert!(!tile_in_range(200, 0, 0));
    }

    #[test]
    fn cache_layout() {
        let p = cache_path(Path::new("C:/d/tiles"), MapStyle::Dark, 14, 8923, 5701);
        assert_eq!(
            p,
            PathBuf::from("C:/d/tiles")
                .join("dark")
                .join("14")
                .join("8923")
                .join("5701.png")
        );
    }

    #[test]
    fn http_response_headers() {
        let r = TileResponse::tile(vec![1, 2, 3]).into_http();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["Content-Type"], "image/png");
        assert_eq!(r.headers()["Cache-Control"], TILE_CACHE_CONTROL);
        let t = TileResponse::transparent().into_http();
        assert_eq!(t.status(), 200);
        assert_eq!(t.headers()["Cache-Control"], "no-store");
        assert_eq!(TileResponse::error(405).into_http().status(), 405);
    }

    #[tokio::test]
    async fn bad_paths_are_rejected_without_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path(), "http://127.0.0.1:9");
        assert_eq!(svc.respond("/nope/1/2/3.png").await.status, 400);
        assert_eq!(svc.respond("/dark/x/2/3.png").await.status, 400);
        assert_eq!(svc.respond("").await.status, 400);
        // Outside the tile grid: transparent tile, nothing fetched, nothing cached.
        let out = svc.respond("/dark/3/9/0.png").await;
        assert_eq!(out, TileResponse::transparent());
        assert!(!dir.path().join("dark").exists());
    }

    #[tokio::test]
    async fn downloads_once_then_serves_from_the_disk_cache() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A distinguishable but valid PNG: the transparent tile is fine for the proxy.
        let body = transparent_png().to_vec();
        let (base, server) = serve(1, "200 OK", body.clone());
        let svc = service(dir.path(), &base);

        let first = svc.respond("/dark/14/8923/5701.png").await;
        assert_eq!(first.status, 200);
        assert_eq!(first.body.as_ref(), body.as_slice());
        assert_eq!(first.cache_control, TILE_CACHE_CONTROL);
        let cached = cache_path(dir.path(), MapStyle::Dark, 14, 8923, 5701);
        assert_eq!(std::fs::read(&cached).expect("tile cached"), body);

        // The server has served its only connection; a second answer must come from disk.
        let second = svc.respond("/dark/14/8923/5701.png").await;
        assert_eq!(second.body.as_ref(), body.as_slice());
        assert_eq!(second.cache_control, TILE_CACHE_CONTROL);
        assert_eq!(server.join().expect("server thread"), 1);
    }

    #[tokio::test]
    async fn concurrent_requests_for_one_tile_share_a_download() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body = transparent_png().to_vec();
        let (base, server) = serve(1, "200 OK", body.clone());
        let svc = Arc::new(service(dir.path(), &base));

        let jobs: Vec<_> = (0..5)
            .map(|_| {
                let svc = svc.clone();
                tokio::spawn(async move { svc.respond("/osm/10/1/2.png").await })
            })
            .collect();
        for job in jobs {
            let r = job.await.expect("task");
            assert_eq!(r.body.as_ref(), body.as_slice());
        }
        assert_eq!(server.join().expect("server thread"), 1);
    }

    #[tokio::test]
    async fn offline_returns_the_transparent_tile_and_pauses_the_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Nothing listens on this port: the connection is refused immediately.
        let svc = service(dir.path(), "http://127.0.0.1:9");
        let started = Instant::now();
        let first = svc.respond("/dark/5/1/1.png").await;
        assert_eq!(first, TileResponse::transparent());
        assert!(
            lock(&svc.offline_until).is_some(),
            "transport error pauses the network"
        );
        // Other tiles are now answered immediately without trying the network.
        let second = svc.respond("/dark/5/2/2.png").await;
        assert_eq!(second, TileResponse::transparent());
        assert!(started.elapsed() < Duration::from_secs(8));
        assert!(!cache_path(dir.path(), MapStyle::Dark, 5, 1, 1).exists());
    }

    #[tokio::test]
    async fn http_errors_and_non_png_answers_fall_back_and_are_remembered() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (base, server) = serve(1, "404 Not Found", b"not found".to_vec());
        let svc = service(dir.path(), &base);
        assert_eq!(
            svc.respond("/light/6/3/3.png").await,
            TileResponse::transparent()
        );
        // Remembered: no second connection (the server accepts only one).
        assert_eq!(
            svc.respond("/light/6/3/3.png").await,
            TileResponse::transparent()
        );
        assert_eq!(server.join().expect("server thread"), 1);

        let (base, server) = serve(1, "200 OK", b"<html>captive portal</html>".to_vec());
        let svc = service(dir.path(), &base);
        assert_eq!(
            svc.respond("/light/6/4/4.png").await,
            TileResponse::transparent()
        );
        assert!(
            !cache_path(dir.path(), MapStyle::Light, 6, 4, 4).exists(),
            "HTML must not be cached"
        );
        assert_eq!(server.join().expect("server thread"), 1);
    }

    #[tokio::test]
    async fn expired_cache_entry_is_served_when_the_network_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = cache_path(dir.path(), MapStyle::Osm, 7, 5, 6);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, transparent_png()).expect("write");
        let old = SystemTime::now() - Duration::from_secs(60 * 24 * 3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open")
            .set_modified(old)
            .expect("set mtime");

        let svc = service(dir.path(), "http://127.0.0.1:9");
        let r = svc.respond("/osm/7/5/6.png").await;
        assert_eq!(r.body.as_ref(), transparent_png());
        assert_eq!(
            r.cache_control, TILE_CACHE_CONTROL,
            "stale tile is a real tile, not the fallback"
        );
    }

    #[tokio::test]
    async fn corrupt_cache_files_are_discarded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = cache_path(dir.path(), MapStyle::Dark, 8, 1, 1);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"garbage").expect("write");
        let svc = service(dir.path(), "http://127.0.0.1:9");
        assert_eq!(
            svc.respond("/dark/8/1/1.png").await,
            TileResponse::transparent()
        );
        assert!(!path.exists());
    }
}
