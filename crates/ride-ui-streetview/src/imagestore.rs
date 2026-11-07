//! Offline Street View images: one JPEG per *image bucket* along a route.
//!
//! This is the opt-in counterpart of [`crate::cache`]: when the rider asks, the app
//! downloads the Street View Static API image of evenly spaced points of the route and keeps
//! it on disk. A later ride of the same route then shows those files and needs **no Google
//! request at all** (not even metadata, so no API key is used).
//!
//! Layout: `<data dir>/streetview/img/<route id>/<bucket>.jpg` plus `manifest.json` with the
//! panorama id, the heading and the capture time of every image. The bucket size is chosen
//! per route ([`step_for`]): 75 m for a short route, coarser (up to 250 m) for a long one, so
//! one route never needs more than [`MAX_IMAGES`] requests. `manifest.json` records the step
//! so lookups stay stable if the policy ever changes.
//!
//! Retention: an image is used for [`IMAGE_TTL_S`] (30 days, the caching window of the
//! Google Maps Platform terms); after that it counts as missing and can be downloaded again.
//! Files that vanished from the disk are dropped when the store is opened, corrupt or
//! foreign manifests are ignored (never trusted), and path components are validated so a
//! crafted route id or bucket can never address a file outside the image directory.
//!
//! The local protocol that serves these files is registered by `ride-app`; [`parse_image_path`]
//! is its path parser and [`image_url`] builds the matching webview URL.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::cache::{now_unix, valid_route_id};
use crate::url::valid_pano_id;
use ride_i18n::t;

/// Subdirectory of `<data>/streetview/` that holds the image store.
pub const IMAGE_DIR: &str = "img";
/// Closest spacing of downloaded images, metres.
pub const STEP_MIN_M: f64 = 75.0;
/// Coarsest spacing, metres (long routes).
pub const STEP_MAX_M: f64 = 250.0;
/// Images one route may keep. 900 × 640 × 640 JPEG is roughly 45–110 MB.
pub const MAX_IMAGES: usize = 900;
/// Edge of the square image requested from the Street View Static API, pixels.
pub const IMAGE_SIZE_PX: u32 = 640;
/// How long a downloaded image may be shown (Google's caching window).
pub const IMAGE_TTL_S: u64 = 30 * 24 * 3600;
/// Manifest size cap; a larger file is not ours.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const FORMAT_VERSION: u32 = 1;
const MANIFEST_NAME: &str = "manifest.json";

/// Spacing of the image buckets of a route: `total_dist / MAX_IMAGES`, clamped and rounded up
/// to 5 m.
pub fn step_for(total_dist: f64) -> f64 {
    if !total_dist.is_finite() || total_dist <= 0.0 {
        return STEP_MIN_M;
    }
    let raw = total_dist / MAX_IMAGES as f64;
    let clamped = raw.clamp(STEP_MIN_M, STEP_MAX_M);
    (clamped / 5.0).ceil() * 5.0
}

/// Number of image buckets a route of `total_dist` needs (0 m is one bucket).
pub fn plan_len(total_dist: f64, step_m: f64) -> usize {
    if !total_dist.is_finite() || total_dist < 0.0 || !step_m.is_finite() || step_m <= 0.0 {
        return 0;
    }
    let n = (total_dist / step_m).floor() as i64 + 1;
    n.clamp(0, MAX_IMAGES as i64 + 1) as usize
}

/// Image bucket of a route distance (negative / NaN distances map to bucket 0).
pub fn bucket_of(dist_m: f64, step_m: f64) -> u32 {
    if !dist_m.is_finite() || dist_m <= 0.0 || !step_m.is_finite() || step_m <= 0.0 {
        return 0;
    }
    (dist_m / step_m).floor().min(f64::from(u32::MAX)) as u32
}

/// Route distance of the middle of an image bucket, clamped to the route end.
pub fn bucket_center(bucket: u32, step_m: f64, total_dist: f64) -> f64 {
    if !step_m.is_finite() || step_m <= 0.0 {
        return 0.0;
    }
    let c = (f64::from(bucket) + 0.5) * step_m;
    if total_dist.is_finite() && total_dist > 0.0 {
        c.min(total_dist)
    } else {
        c
    }
}

/// `<data>/streetview/img`, the parent of the per-route image folders.
pub fn image_root(data_dir: &Path) -> PathBuf {
    crate::cache::cache_dir(data_dir).join(IMAGE_DIR)
}

/// `<data>/streetview/img/<route id>`; `None` for a route id that is not a plain identifier.
pub fn route_dir(data_dir: &Path, route_id: &str) -> Option<PathBuf> {
    valid_route_id(route_id).then(|| image_root(data_dir).join(route_id))
}

/// Path of one image file; `None` for a hostile route id or an absurd bucket.
pub fn image_path(data_dir: &Path, route_id: &str, bucket: u32) -> Option<PathBuf> {
    route_dir(data_dir, route_id).map(|d| d.join(format!("{bucket}.jpg")))
}

/// Webview URL of a downloaded image (served by the `svimg` protocol of the shell). The base
/// (`http://svimg.localhost`, `svimg://localhost` or `https://svimg.localhost`) depends on the
/// webview; the handler only reads `uri().path()`.
pub fn image_url(route_id: &str, bucket: u32) -> String {
    ride_ui_kit::protocol::url("svimg", &format!("streetview/{route_id}/{bucket}.jpg"))
}

/// Parses the protocol path `/streetview/<route id>/<bucket>.jpg`.
pub fn parse_image_path(path: &str) -> Option<(String, u32)> {
    let rest = path.trim_start_matches('/').strip_prefix("streetview/")?;
    let (route_id, file) = rest.split_once('/')?;
    if !valid_route_id(route_id) || file.contains('/') || file.contains('\\') {
        return None;
    }
    let stem = file.strip_suffix(".jpg")?;
    if stem.is_empty() || stem.len() > 10 || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((route_id.to_string(), stem.parse().ok()?))
}

/// What one stored image remembers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageEntry {
    /// Panorama the image was taken from (validated when loaded).
    pub pano: String,
    /// Camera heading, degrees (one decimal).
    pub heading: f32,
    /// When it was saved, Unix seconds.
    pub ts: u64,
    /// Size of the file, bytes (for the storage readout).
    pub bytes: u64,
}

impl ImageEntry {
    /// Still inside the allowed caching window at `now`?
    pub fn fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.ts) <= IMAGE_TTL_S
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    route_id: String,
    step_m: f64,
    #[serde(default)]
    entries: BTreeMap<u32, ImageEntry>,
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<u32, ImageEntry>,
    dirty: bool,
}

/// Images of one route. Share it as `Arc<ImageStore>`.
pub struct ImageStore {
    route_id: String,
    dir: PathBuf,
    step_m: f64,
    total_dist: f64,
    inner: Mutex<Inner>,
    write_lock: Mutex<()>,
}

impl std::fmt::Debug for ImageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageStore")
            .field("route_id", &self.route_id)
            .field("step_m", &self.step_m)
            .field("entries", &self.len())
            .finish()
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map_or_else(|| "data".to_string(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

impl ImageStore {
    /// Opens (or creates empty) the image store of `route_id`.
    ///
    /// `None` for a route id that is not a safe file name component. The directory is not
    /// created until the first image is written, so an unused store never touches the disk.
    pub fn open(data_dir: &Path, route_id: &str, total_dist: f64) -> Option<ImageStore> {
        let dir = route_dir(data_dir, route_id)?;
        let (step_m, entries) = Self::load(&dir, route_id, total_dist);
        Some(ImageStore {
            route_id: route_id.to_string(),
            dir,
            step_m,
            total_dist: if total_dist.is_finite() && total_dist > 0.0 {
                total_dist
            } else {
                0.0
            },
            inner: Mutex::new(Inner {
                entries,
                dirty: false,
            }),
            write_lock: Mutex::new(()),
        })
    }

    fn load(dir: &Path, route_id: &str, total_dist: f64) -> (f64, BTreeMap<u32, ImageEntry>) {
        let default_step = step_for(total_dist);
        let Ok(meta) = std::fs::metadata(dir.join(MANIFEST_NAME)) else {
            return (default_step, BTreeMap::new());
        };
        if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
            return (default_step, BTreeMap::new());
        }
        let Ok(bytes) = std::fs::read(dir.join(MANIFEST_NAME)) else {
            return (default_step, BTreeMap::new());
        };
        let Ok(parsed) = serde_json::from_slice::<Manifest>(&bytes) else {
            return (default_step, BTreeMap::new());
        };
        if parsed.version != FORMAT_VERSION
            || parsed.route_id != route_id
            || !parsed.step_m.is_finite()
            || !(STEP_MIN_M..=STEP_MAX_M).contains(&parsed.step_m)
        {
            return (default_step, BTreeMap::new());
        }
        // Keep only entries we could serve: a sane id, a fresh timestamp and an existing file.
        let now = now_unix();
        let entries = parsed
            .entries
            .into_iter()
            .filter(|(bucket, e)| {
                *bucket <= MAX_IMAGES as u32
                    && valid_pano_id(&e.pano)
                    && e.heading.is_finite()
                    && e.fresh(now)
                    && dir.join(format!("{bucket}.jpg")).is_file()
            })
            .collect();
        (parsed.step_m, entries)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn route_id(&self) -> &str {
        &self.route_id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn step_m(&self) -> f64 {
        self.step_m
    }

    /// Number of image buckets of this route (`0` for a zero-length route).
    pub fn plan_len(&self) -> usize {
        plan_len(self.total_dist, self.step_m)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bytes(&self) -> u64 {
        self.lock().entries.values().map(|e| e.bytes).sum()
    }

    /// Fresh entry of a bucket (the file was verified when the store was opened).
    pub fn get(&self, bucket: u32) -> Option<ImageEntry> {
        self.lock().entries.get(&bucket).cloned()
    }

    pub fn get_at(&self, bucket: u32, now: u64) -> Option<ImageEntry> {
        self.lock()
            .entries
            .get(&bucket)
            .filter(|e| e.fresh(now))
            .cloned()
    }

    /// Image bucket of a route distance.
    pub fn bucket_of(&self, dist_m: f64) -> u32 {
        bucket_of(dist_m, self.step_m)
    }

    /// Route distance the image of `bucket` was taken at.
    pub fn bucket_center(&self, bucket: u32) -> f64 {
        bucket_center(bucket, self.step_m, self.total_dist)
    }

    /// Path of a stored image; `None` for a bucket outside the plan.
    pub fn path(&self, bucket: u32) -> Option<PathBuf> {
        (bucket <= MAX_IMAGES as u32).then(|| self.dir.join(format!("{bucket}.jpg")))
    }

    /// Writes the image (atomically) and records it in the manifest.
    pub fn put(
        &self,
        bucket: u32,
        pano: &str,
        heading: f64,
        bytes: &[u8],
    ) -> io::Result<ImageEntry> {
        if !valid_pano_id(pano) || bucket > MAX_IMAGES as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                t("ui_streetview.imagestore.ervenytelen_panorama_azonosito_vagy_sav"),
            ));
        }
        let path = self.dir.join(format!("{bucket}.jpg"));
        write_atomic(&path, bytes)?;
        let entry = ImageEntry {
            pano: pano.to_string(),
            heading: ((heading * 10.0).round() / 10.0) as f32,
            ts: now_unix(),
            bytes: bytes.len() as u64,
        };
        {
            let mut g = self.lock();
            g.entries.insert(bucket, entry.clone());
            g.dirty = true;
        }
        let _ = self.flush();
        Ok(entry)
    }

    /// Forgets an image that could not be served (a corrupt file, a rejected download).
    pub fn remove(&self, bucket: u32) {
        let mut g = self.lock();
        if g.entries.remove(&bucket).is_some() {
            g.dirty = true;
        }
        drop(g);
        let _ = std::fs::remove_file(self.dir.join(format!("{bucket}.jpg")));
        let _ = self.flush();
    }

    fn write_manifest(&self) -> io::Result<()> {
        let _w = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        let bytes = {
            let mut g = self.lock();
            if !g.dirty {
                return Ok(());
            }
            g.dirty = false;
            serde_json::to_vec(&Manifest {
                version: FORMAT_VERSION,
                route_id: self.route_id.clone(),
                step_m: self.step_m,
                entries: g.entries.clone(),
            })
            .map_err(io::Error::other)?
        };
        write_atomic(&self.dir.join(MANIFEST_NAME), &bytes).inspect_err(|_| {
            self.lock().dirty = true;
        })
    }

    /// Writes the manifest if anything changed.
    pub fn flush(&self) -> io::Result<()> {
        self.write_manifest()
    }

    /// Deletes the images and the manifest of this route.
    pub fn clear(&self) -> io::Result<usize> {
        let mut g = self.lock();
        let n = g.entries.len();
        g.entries.clear();
        g.dirty = false;
        drop(g);
        let mut removed = 0;
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let p = e.path();
                let is_image = p.extension().is_some_and(|x| x == "jpg");
                let is_manifest = p.file_name().is_some_and(|n| n == MANIFEST_NAME);
                if (is_image || is_manifest) && std::fs::remove_file(&p).is_ok() && is_image {
                    removed += 1;
                }
            }
        }
        Ok(removed.max(n))
    }
}

impl Drop for ImageStore {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Storage used by the downloaded images, for the settings dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageOverview {
    pub routes: usize,
    pub images: usize,
    pub bytes: u64,
}

/// Counts the images below `data_dir`.
pub fn overview(data_dir: &Path) -> ImageOverview {
    let mut o = ImageOverview::default();
    let Ok(rd) = std::fs::read_dir(image_root(data_dir)) else {
        return o;
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        o.routes += 1;
        if let Ok(files) = std::fs::read_dir(&dir) {
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_some_and(|x| x == "jpg") {
                    o.images += 1;
                    o.bytes += f.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    o
}

/// Deletes every downloaded image of every route. Returns how many routes were cleared.
pub fn clear_all(data_dir: &Path) -> io::Result<usize> {
    let root = image_root(data_dir);
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut n = 0;
    for entry in rd.flatten() {
        let dir = entry.path();
        if dir.is_dir() && std::fs::remove_dir_all(&dir).is_ok() {
            n += 1;
        }
    }
    // Drop the now empty `img` folder as well, so an untouched data directory stays clean.
    let _ = std::fs::remove_dir(&root);
    Ok(n)
}

/// Deletes the images of one route and forgets its panorama ids as well.
pub fn clear_route(data_dir: &Path, route_id: &str) -> io::Result<usize> {
    if !valid_route_id(route_id) {
        return Ok(0);
    }
    let n = std::fs::remove_dir_all(image_root(data_dir).join(route_id))
        .map(|_| 1)
        .or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(0)
            } else {
                Err(e)
            }
        })?;
    let _ =
        std::fs::remove_file(crate::cache::cache_dir(data_dir).join(format!("{route_id}.json")));
    Ok(n)
}

/// [`overview`] in the app's data directory (for the settings dialog).
pub fn overview_default() -> ImageOverview {
    overview(&ride_route::store::data_dir())
}

/// [`clear_all`] in the app's data directory (for the settings dialog).
pub fn clear_all_default() -> io::Result<usize> {
    clear_all(&ride_route::store::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_grows_with_the_route_but_stays_in_bounds() {
        assert_eq!(step_for(0.0), STEP_MIN_M);
        assert_eq!(step_for(f64::NAN), STEP_MIN_M);
        assert_eq!(step_for(1_000.0), STEP_MIN_M);
        // 24 km: 26.7 m raw -> the 75 m floor
        assert_eq!(step_for(24_000.0), 75.0);
        // 100 km: 111.1 m raw -> 115 m
        assert_eq!(step_for(100_000.0), 115.0);
        // very long routes: capped at 200 m
        assert_eq!(step_for(1_000_000.0), STEP_MAX_M);
        // one route never needs more than the cap of images
        for total in [0.0, 500.0, 24_000.0, 100_000.0, 250_000.0, 1e7] {
            let step = step_for(total);
            assert!(plan_len(total, step) <= MAX_IMAGES + 1, "{total} {step}");
        }
    }

    #[test]
    fn buckets_are_centred_and_clamped() {
        assert_eq!(bucket_of(0.0, 40.0), 0);
        assert_eq!(bucket_of(39.999, 40.0), 0);
        assert_eq!(bucket_of(40.0, 40.0), 1);
        assert_eq!(bucket_of(-5.0, 40.0), 0);
        assert_eq!(bucket_of(f64::NAN, 40.0), 0);
        assert_eq!(bucket_center(0, 40.0, 1000.0), 20.0);
        assert_eq!(bucket_center(24, 40.0, 1000.0), 980.0);
        // clamped to the route end
        assert_eq!(bucket_center(100, 40.0, 1000.0), 1000.0);
        assert_eq!(plan_len(0.0, 40.0), 1);
        assert_eq!(plan_len(1000.0, 40.0), 26);
        assert_eq!(plan_len(f64::NAN, 40.0), 0);
    }

    #[test]
    fn paths_and_urls_round_trip_and_reject_traversal() {
        assert_eq!(
            image_url("abc123", 7),
            format!(
                "{}/streetview/abc123/7.jpg",
                ride_ui_kit::protocol::base("svimg")
            )
        );
        assert_eq!(
            parse_image_path("/streetview/abc123/7.jpg"),
            Some(("abc123".to_string(), 7))
        );
        assert_eq!(
            parse_image_path("streetview/abc123/7.jpg"),
            Some(("abc123".to_string(), 7))
        );
        for bad in [
            "/streetview/../x/7.jpg",
            "/streetview/abc/7.png",
            "/streetview/abc/-1.jpg",
            "/streetview/abc/.jpg",
            "/streetview/abc/7.jpg/",
            "/streetview/abc/99999999999.jpg",
            "/other/abc/7.jpg",
            "/streetview/abc",
        ] {
            assert!(parse_image_path(bad).is_none(), "{bad}");
        }
        assert!(image_path(Path::new("C:/d"), "../x", 1).is_none());
        assert!(route_dir(Path::new("C:/d"), "a/b").is_none());
        assert!(image_path(Path::new("C:/d"), "abc", 3)
            .expect("path")
            .ends_with("3.jpg"));
    }

    #[test]
    fn round_trip_put_and_reload() {
        let tmp = tempfile::tempdir().expect("tmp");
        let bytes = [0xFFu8, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        {
            let store = ImageStore::open(tmp.path(), "route1", 1000.0).expect("store");
            assert!(store.is_empty() && store.step_m() == 75.0);
            store.put(3, "PANO-1", 91.24, &bytes).expect("put");
            assert_eq!(store.len(), 1);
            assert_eq!(store.bytes(), bytes.len() as u64);
            assert_eq!(store.get(3).expect("entry").heading, 91.2);
            assert!(store.path(3).expect("path").is_file());
        }
        let store = ImageStore::open(tmp.path(), "route1", 1000.0).expect("store");
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(3).expect("entry").pano, "PANO-1");
        assert_eq!(store.step_m(), 75.0);
        // the manifest carries no key-like text and is in the route directory
        let manifest = std::fs::read_to_string(
            tmp.path()
                .join("streetview")
                .join("img")
                .join("route1")
                .join("manifest.json"),
        )
        .expect("manifest");
        assert!(manifest.contains("\"route_id\":\"route1\""));
        assert!(manifest.contains("\"step_m\":75.0"));
        // no temp files left behind
        let left: Vec<_> =
            std::fs::read_dir(tmp.path().join("streetview").join("img").join("route1"))
                .expect("dir")
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
                .collect();
        assert!(left.is_empty());
    }

    #[test]
    fn missing_files_and_hostile_manifests_are_dropped() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("streetview").join("img").join("r");
        std::fs::create_dir_all(&dir).expect("dir");
        // entry whose file does not exist
        let now = now_unix();
        let manifest = format!(
            r#"{{"version":1,"route_id":"r","step_m":75.0,"entries":{{
                "1":{{"pano":"OK_ID","heading":10.0,"ts":{now},"bytes":5}},
                "2":{{"pano":"../evil","heading":10.0,"ts":{now},"bytes":5}}
            }}}}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).expect("write");
        let store = ImageStore::open(tmp.path(), "r", 1000.0).expect("store");
        assert!(store.is_empty(), "no file exists yet");
        std::fs::write(dir.join("1.jpg"), [1u8, 2]).expect("jpg");
        let store = ImageStore::open(tmp.path(), "r", 1000.0).expect("store");
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(1).expect("entry").pano, "OK_ID");

        // wrong version / wrong route / a step outside the policy
        for bad in [
            r#"{"version":9,"route_id":"r","step_m":75.0,"entries":{}}"#,
            r#"{"version":1,"route_id":"other","step_m":75.0,"entries":{}}"#,
            r#"{"version":1,"route_id":"r","step_m":1.0,"entries":{}}"#,
        ] {
            std::fs::write(dir.join("manifest.json"), bad).expect("write");
            let store = ImageStore::open(tmp.path(), "r", 1000.0).expect("store");
            assert!(store.is_empty(), "{bad}");
            assert_eq!(store.step_m(), 75.0);
        }
    }

    #[test]
    fn expired_entries_are_not_served() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("streetview").join("img").join("r");
        std::fs::create_dir_all(&dir).expect("dir");
        let old = now_unix() - IMAGE_TTL_S - 100;
        std::fs::write(dir.join("1.jpg"), [1u8, 2]).expect("jpg");
        let manifest = format!(
            r#"{{"version":1,"route_id":"r","step_m":75.0,"entries":{{"1":{{"pano":"A","heading":0.0,"ts":{old},"bytes":2}}}}}}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).expect("write");
        let store = ImageStore::open(tmp.path(), "r", 1000.0).expect("store");
        assert!(store.is_empty());
        assert!(store.get_at(1, now_unix()).is_none());
    }

    #[test]
    fn clear_removes_files_and_overview_counts_them() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert_eq!(overview(tmp.path()), ImageOverview::default());
        assert_eq!(clear_all(tmp.path()).expect("clear"), 0);
        {
            let store = ImageStore::open(tmp.path(), "a", 1000.0).expect("store");
            store.put(0, "A", 0.0, &[1, 2, 3]).expect("put");
            store.put(1, "B", 0.0, &[1, 2, 3, 4]).expect("put");
        }
        let o = overview(tmp.path());
        assert_eq!((o.routes, o.images), (1, 2));
        assert_eq!(o.bytes, 7);
        let store = ImageStore::open(tmp.path(), "a", 1000.0).expect("store");
        assert_eq!(store.clear().expect("clear"), 2);
        assert!(store.is_empty());
        assert!(!store.path(0).expect("p").exists());
        assert_eq!(overview(tmp.path()).images, 0);
        assert_eq!(clear_all(tmp.path()).expect("clear"), 1);
        assert!(!tmp.path().join("streetview").join("img").exists());
    }

    #[test]
    fn invalid_puts_are_rejected() {
        let tmp = tempfile::tempdir().expect("tmp");
        let store = ImageStore::open(tmp.path(), "r", 1000.0).expect("store");
        assert!(store.put(0, "../evil", 0.0, &[1]).is_err());
        assert!(store.put(MAX_IMAGES as u32 + 1, "A", 0.0, &[1]).is_err());
        assert!(store.is_empty());
        assert!(!tmp.path().join("streetview").exists());
    }
}
