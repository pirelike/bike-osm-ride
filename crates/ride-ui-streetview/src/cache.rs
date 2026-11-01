//! Panorama-id cache, per route.
//!
//! Only **ids** (plus our own route coordinates, heading and a timestamp) are kept - never
//! imagery. Google's Street View policies forbid pre-fetching / storing imagery but
//! explicitly allow storing panorama ids; their technical pages add that ids can change over
//! time. Both are honoured: entries expire (30 days, "no panorama" answers 7 days) and the
//! embed URL always carries the route point as a `location` fallback, which Google uses when
//! an id is no longer found.
//!
//! Keys are route-distance *buckets* of [`BUCKET_M`] (25 m). The lookup point of a bucket is
//! always the route position at the bucket centre, so the same bucket asks the same
//! question on every ride.
//!
//! * [`PanoCache::memory`] - small in-memory LRU-ish map (used when the option is off).
//! * [`PanoCache::persistent`] - the same plus `<data dir>/streetview/<route id>.json`,
//!   written atomically (temp file + rename) every few inserts, on [`PanoCache::flush`] and
//!   when the cache is dropped.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::url::valid_pano_id;

/// Width of one cache bucket along the route, metres.
pub const BUCKET_M: f64 = 25.0;
/// Entries the in-memory-only cache keeps.
pub const MEMORY_CAP: usize = 96;
/// Entries one route file may hold (2 500 km of route).
pub const MAX_ENTRIES: usize = 100_000;
/// A found panorama id is trusted this long.
pub const TTL_FOUND_S: u64 = 30 * 24 * 3600;
/// A "no panorama here" answer is trusted this long.
pub const TTL_NONE_S: u64 = 7 * 24 * 3600;
/// Inserts between automatic writes of a persistent cache.
const FLUSH_EVERY: usize = 24;
const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const DIR_NAME: &str = "streetview";

/// Seconds since the Unix epoch.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Bucket index of a route distance (negative / NaN distances map to bucket 0).
pub fn bucket_of(dist_m: f64) -> u32 {
    if !dist_m.is_finite() || dist_m <= 0.0 {
        return 0;
    }
    (dist_m / BUCKET_M).floor().min(f64::from(u32::MAX)) as u32
}

/// Route distance of the lookup point of a bucket: its centre, clamped to the route end.
pub fn bucket_center(bucket: u32, total_dist_m: f64) -> f64 {
    let c = (f64::from(bucket) + 0.5) * BUCKET_M;
    if total_dist_m.is_finite() && total_dist_m > 0.0 {
        c.min(total_dist_m)
    } else {
        c
    }
}

/// Is `id` safe to use as a file name component (route ids are FNV hashes in hex)?
pub fn valid_route_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What we remember about one bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// `None` = Google had no panorama here.
    pub pano: Option<String>,
    /// Route heading at the bucket (degrees, one decimal).
    pub heading: f32,
    /// The point the question was asked for (a point of our own route).
    pub lat: f64,
    pub lng: f64,
    /// When it was resolved, Unix seconds.
    pub ts: u64,
}

impl Entry {
    pub fn found(pano: &str, heading: f64, lat: f64, lng: f64, ts: u64) -> Entry {
        Entry {
            pano: Some(pano.to_string()),
            heading: ((heading * 10.0).round() / 10.0) as f32,
            lat,
            lng,
            ts,
        }
    }

    pub fn none(heading: f64, lat: f64, lng: f64, ts: u64) -> Entry {
        Entry {
            pano: None,
            heading: ((heading * 10.0).round() / 10.0) as f32,
            lat,
            lng,
            ts,
        }
    }

    /// Still within its time to live at `now`?
    pub fn fresh(&self, now: u64) -> bool {
        let ttl = if self.pano.is_some() {
            TTL_FOUND_S
        } else {
            TTL_NONE_S
        };
        now.saturating_sub(self.ts) <= ttl
    }
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    route_id: String,
    bucket_m: f64,
    entries: BTreeMap<u32, Entry>,
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<u32, Entry>,
    /// Insertion order, used to evict in memory-only mode.
    order: VecDeque<u32>,
    dirty: usize,
    inflight: HashSet<u32>,
}

/// Panorama-id cache of one route. Share it as `Arc<PanoCache>`.
pub struct PanoCache {
    route_id: String,
    file: Option<PathBuf>,
    inner: Mutex<Inner>,
    /// Serialises file writes (see `write_out`).
    write_lock: Mutex<()>,
}

impl std::fmt::Debug for PanoCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanoCache")
            .field("route_id", &self.route_id)
            .field("persistent", &self.file.is_some())
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

/// `<data dir>/streetview`, the directory of the route files.
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR_NAME)
}

/// The cache directory below the app's data directory (`RIDE_DATA_DIR` aware).
pub fn default_cache_dir() -> PathBuf {
    cache_dir(&ride_route::store::data_dir())
}

impl PanoCache {
    /// In-memory only (option off): a small map, nothing touches the disk.
    pub fn memory(route_id: &str) -> PanoCache {
        PanoCache {
            route_id: route_id.to_string(),
            file: None,
            inner: Mutex::new(Inner::default()),
            write_lock: Mutex::new(()),
        }
    }

    /// Backed by `<data_dir>/streetview/<route_id>.json` (loaded now if present and sane).
    /// A route id that is not a plain identifier gives an in-memory cache.
    pub fn persistent(data_dir: &Path, route_id: &str) -> PanoCache {
        if !valid_route_id(route_id) {
            return PanoCache::memory(route_id);
        }
        let file = cache_dir(data_dir).join(format!("{route_id}.json"));
        let entries = Self::load(&file, route_id);
        let order = entries.keys().copied().collect();
        PanoCache {
            route_id: route_id.to_string(),
            file: Some(file),
            inner: Mutex::new(Inner {
                entries,
                order,
                ..Inner::default()
            }),
            write_lock: Mutex::new(()),
        }
    }

    /// [`PanoCache::persistent`] in the app's data directory.
    pub fn persistent_default(route_id: &str) -> PanoCache {
        PanoCache::persistent(&ride_route::store::data_dir(), route_id)
    }

    fn load(file: &Path, route_id: &str) -> BTreeMap<u32, Entry> {
        let Ok(meta) = std::fs::metadata(file) else {
            return BTreeMap::new();
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return BTreeMap::new();
        }
        let Ok(bytes) = std::fs::read(file) else {
            return BTreeMap::new();
        };
        let Ok(parsed) = serde_json::from_slice::<FileFormat>(&bytes) else {
            return BTreeMap::new();
        };
        if parsed.version != FORMAT_VERSION
            || parsed.route_id != route_id
            || (parsed.bucket_m - BUCKET_M).abs() > 1e-9
        {
            return BTreeMap::new();
        }
        parsed
            .entries
            .into_iter()
            .filter(|(_, e)| {
                e.lat.is_finite()
                    && e.lng.is_finite()
                    && e.heading.is_finite()
                    && e.pano.as_deref().is_none_or(valid_pano_id)
            })
            .take(MAX_ENTRIES)
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn route_id(&self) -> &str {
        &self.route_id
    }

    pub fn is_persistent(&self) -> bool {
        self.file.is_some()
    }

    /// Path of the route file (persistent caches only).
    pub fn file_path(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fresh entry of a bucket at time `now`.
    pub fn get_at(&self, bucket: u32, now: u64) -> Option<Entry> {
        self.lock()
            .entries
            .get(&bucket)
            .filter(|e| e.fresh(now))
            .cloned()
    }

    /// Fresh entry of the bucket containing `dist_m`.
    pub fn get(&self, dist_m: f64) -> Option<Entry> {
        self.get_at(bucket_of(dist_m), now_unix())
    }

    /// Is the bucket known (fresh) already?
    pub fn contains(&self, bucket: u32, now: u64) -> bool {
        self.get_at(bucket, now).is_some()
    }

    /// Stores an entry. Persistent caches write themselves out every few inserts.
    pub fn put(&self, bucket: u32, entry: Entry) {
        let need_write = {
            let mut g = self.lock();
            if !g.entries.contains_key(&bucket) {
                if self.file.is_some() {
                    if g.entries.len() >= MAX_ENTRIES {
                        return;
                    }
                } else {
                    while g.entries.len() >= MEMORY_CAP {
                        match g.order.pop_front() {
                            Some(old) => {
                                g.entries.remove(&old);
                            }
                            None => break,
                        }
                    }
                }
                g.order.push_back(bucket);
            }
            g.entries.insert(bucket, entry);
            g.dirty += 1;
            self.file.is_some() && g.dirty >= FLUSH_EVERY
        };
        if need_write {
            let _ = self.write_out(FLUSH_EVERY);
        }
    }

    /// Writes the file if at least `min_dirty` changes are pending. Writers are serialised
    /// and each takes its snapshot only after it holds the write lock, so a slower writer can
    /// never replace newer data with an older snapshot.
    fn write_out(&self, min_dirty: usize) -> io::Result<()> {
        let Some(path) = &self.file else {
            return Ok(());
        };
        let _w = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        let bytes = {
            let mut g = self.lock();
            if g.dirty == 0 || g.dirty < min_dirty {
                return Ok(());
            }
            g.dirty = 0;
            serde_json::to_vec(&FileFormat {
                version: FORMAT_VERSION,
                route_id: self.route_id.clone(),
                bucket_m: BUCKET_M,
                entries: g.entries.clone(),
            })
            .map_err(io::Error::other)?
        };
        write_atomic(path, &bytes).inspect_err(|_| {
            self.lock().dirty += 1; // try again on the next flush
        })
    }

    /// Writes pending changes now (no-op for memory caches or when nothing changed).
    pub fn flush(&self) -> io::Result<()> {
        self.write_out(1)
    }

    /// Marks a bucket as being resolved by someone; `false` if another task already is.
    pub fn try_claim(&self, bucket: u32) -> bool {
        self.lock().inflight.insert(bucket)
    }

    pub fn release(&self, bucket: u32) {
        self.lock().inflight.remove(&bucket);
    }

    pub fn is_claimed(&self, bucket: u32) -> bool {
        self.lock().inflight.contains(&bucket)
    }
}

impl Drop for PanoCache {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Size of what is saved on disk, for the settings dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SavedOverview {
    pub routes: usize,
    pub bytes: u64,
}

/// Counts the saved route files below `data_dir`.
pub fn saved_overview(data_dir: &Path) -> SavedOverview {
    let mut o = SavedOverview::default();
    if let Ok(rd) = std::fs::read_dir(cache_dir(data_dir)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                if let Ok(m) = e.metadata() {
                    o.routes += 1;
                    o.bytes += m.len();
                }
            }
        }
    }
    o
}

/// Deletes every saved route file; returns how many were removed.
pub fn clear_saved(data_dir: &Path) -> io::Result<usize> {
    let mut n = 0;
    let rd = match std::fs::read_dir(cache_dir(data_dir)) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json") && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

/// [`saved_overview`] in the app's data directory (for the settings dialog).
pub fn saved_overview_default() -> SavedOverview {
    saved_overview(&ride_route::store::data_dir())
}

/// [`clear_saved`] in the app's data directory.
pub fn clear_saved_default() -> io::Result<usize> {
    clear_saved(&ride_route::store::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn bucket_maths() {
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(24.999), 0);
        assert_eq!(bucket_of(25.0), 1);
        assert_eq!(bucket_of(49.9), 1);
        assert_eq!(bucket_of(12_345.0), 493);
        assert_eq!(bucket_of(-5.0), 0);
        assert_eq!(bucket_of(f64::NAN), 0);
        assert_eq!(bucket_of(f64::INFINITY), 0);
        assert_eq!(bucket_center(0, 1000.0), 12.5);
        assert_eq!(bucket_center(493, 1e6), 12_337.5);
        // clamped to the end of a short route
        assert_eq!(bucket_center(40, 1_003.0), 1_003.0);
        // every distance maps to a bucket whose centre is at most half a bucket away
        for i in 0..2000 {
            let d = f64::from(i) * 7.3;
            let c = bucket_center(bucket_of(d), 1e9);
            assert!((c - d).abs() <= BUCKET_M / 2.0 + 1e-9, "{d} {c}");
        }
    }

    #[test]
    fn expiry_depends_on_the_kind_of_entry() {
        let found = Entry::found("abc", 90.04, 47.5, 19.0, NOW);
        let none = Entry::none(90.0, 47.5, 19.0, NOW);
        assert_eq!(found.heading, 90.0);
        assert!(found.fresh(NOW + TTL_FOUND_S));
        assert!(!found.fresh(NOW + TTL_FOUND_S + 1));
        assert!(none.fresh(NOW + TTL_NONE_S));
        assert!(!none.fresh(NOW + TTL_NONE_S + 1));
        // a timestamp in the future (clock change) is not "expired"
        assert!(found.fresh(NOW - 100));

        let c = PanoCache::memory("r1");
        c.put(3, found);
        assert!(c.get_at(3, NOW + 10).is_some());
        assert!(c.get_at(3, NOW + TTL_FOUND_S + 5).is_none());
        assert!(c.get_at(4, NOW).is_none());
    }

    #[test]
    fn memory_cache_is_bounded_and_never_writes() {
        let tmp = tempfile::tempdir().expect("tmp");
        let c = PanoCache::memory("route");
        for b in 0..(MEMORY_CAP as u32 + 40) {
            c.put(b, Entry::found(&format!("p{b}"), 0.0, 47.0, 19.0, NOW));
        }
        assert_eq!(c.len(), MEMORY_CAP);
        assert!(c.get_at(0, NOW).is_none(), "oldest evicted");
        assert!(c.get_at(MEMORY_CAP as u32 + 39, NOW).is_some());
        assert!(!c.is_persistent() && c.file_path().is_none());
        c.flush().expect("flush");
        assert_eq!(std::fs::read_dir(tmp.path()).expect("dir").count(), 0);
    }

    #[test]
    fn persistence_round_trip() {
        let tmp = tempfile::tempdir().expect("tmp");
        {
            let c = PanoCache::persistent(tmp.path(), "abcd1234");
            assert!(c.is_empty() && c.is_persistent());
            c.put(10, Entry::found("PANO_A-1", 123.44, 47.5, 19.04, NOW));
            c.put(11, Entry::none(124.0, 47.5001, 19.0401, NOW));
            c.flush().expect("flush");
            // second flush without changes is a no-op
            c.flush().expect("flush");
        }
        let path = tmp.path().join("streetview").join("abcd1234.json");
        assert!(path.is_file());
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            text.contains("PANO_A-1") && text.contains("\"bucket_m\":25.0"),
            "{text}"
        );
        let c = PanoCache::persistent(tmp.path(), "abcd1234");
        assert_eq!(c.len(), 2);
        let e = c.get_at(10, NOW).expect("entry");
        assert_eq!(e.pano.as_deref(), Some("PANO_A-1"));
        assert_eq!(e.heading, 123.4);
        assert_eq!((e.lat, e.lng, e.ts), (47.5, 19.04, NOW));
        assert_eq!(c.get_at(11, NOW).expect("none entry").pano, None);
        // no temp files left
        let left: Vec<_> = std::fs::read_dir(tmp.path().join("streetview"))
            .expect("dir")
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(left.is_empty());
    }

    #[test]
    fn dropping_a_persistent_cache_saves_pending_entries() {
        let tmp = tempfile::tempdir().expect("tmp");
        {
            let c = PanoCache::persistent(tmp.path(), "r");
            c.put(1, Entry::found("X", 0.0, 1.0, 2.0, NOW));
        }
        assert_eq!(PanoCache::persistent(tmp.path(), "r").len(), 1);
    }

    #[test]
    fn automatic_flush_after_a_batch_of_inserts() {
        let tmp = tempfile::tempdir().expect("tmp");
        let c = PanoCache::persistent(tmp.path(), "auto");
        for b in 0..(FLUSH_EVERY as u32) {
            c.put(b, Entry::found("Y", 0.0, 1.0, 2.0, NOW));
        }
        assert!(tmp.path().join("streetview").join("auto.json").is_file());
        std::mem::forget(c); // no flush on drop: the file must already exist
    }

    #[test]
    fn damaged_or_foreign_files_are_ignored() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("streetview");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("bad.json"), b"{ nope").expect("write");
        assert!(PanoCache::persistent(tmp.path(), "bad").is_empty());
        // file for another route id inside this route's file name
        let other = r#"{"version":1,"route_id":"other","bucket_m":25.0,"entries":{"1":{"pano":"A","heading":0.0,"lat":1.0,"lng":2.0,"ts":5}}}"#;
        std::fs::write(dir.join("mine.json"), other).expect("write");
        assert!(PanoCache::persistent(tmp.path(), "mine").is_empty());
        // wrong bucket size / version
        let old = r#"{"version":1,"route_id":"old","bucket_m":50.0,"entries":{}}"#;
        std::fs::write(dir.join("old.json"), old).expect("write");
        assert!(PanoCache::persistent(tmp.path(), "old").is_empty());
        // an entry with a hostile pano id is dropped, the sane one stays
        let mixed = r#"{"version":1,"route_id":"mixed","bucket_m":25.0,"entries":{
            "1":{"pano":"../../x","heading":0.0,"lat":1.0,"lng":2.0,"ts":5},
            "2":{"pano":"GOOD_id-1","heading":0.0,"lat":1.0,"lng":2.0,"ts":5}}}"#;
        std::fs::write(dir.join("mixed.json"), mixed).expect("write");
        let c = PanoCache::persistent(tmp.path(), "mixed");
        assert_eq!(c.len(), 1);
        assert!(c.get_at(2, 5).is_some());
    }

    #[test]
    fn route_ids_cannot_escape_the_directory() {
        let tmp = tempfile::tempdir().expect("tmp");
        for bad in ["../x", "a/b", "a\\b", "", "..", "x.json", &"a".repeat(65)] {
            let c = PanoCache::persistent(tmp.path(), bad);
            assert!(!c.is_persistent(), "{bad}");
            c.put(1, Entry::found("A", 0.0, 1.0, 2.0, NOW));
            c.flush().expect("flush");
        }
        assert!(!tmp.path().join("x").exists());
        assert!(!tmp.path().join("streetview").exists());
    }

    #[test]
    fn claims_prevent_duplicate_work() {
        let c = PanoCache::memory("r");
        assert!(c.try_claim(5));
        assert!(!c.try_claim(5));
        assert!(c.is_claimed(5));
        c.release(5);
        assert!(c.try_claim(5));
    }

    #[test]
    fn overview_and_clear() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert_eq!(saved_overview(tmp.path()), SavedOverview::default());
        assert_eq!(clear_saved(tmp.path()).expect("clear"), 0);
        for id in ["a", "b"] {
            let c = PanoCache::persistent(tmp.path(), id);
            c.put(1, Entry::found("P", 0.0, 1.0, 2.0, NOW));
        }
        let o = saved_overview(tmp.path());
        assert_eq!(o.routes, 2);
        assert!(o.bytes > 50);
        assert_eq!(clear_saved(tmp.path()).expect("clear"), 2);
        assert_eq!(saved_overview(tmp.path()).routes, 0);
    }

    #[test]
    fn debug_output_has_no_entries() {
        let c = PanoCache::memory("rid");
        c.put(1, Entry::found("SECRETPANO", 0.0, 1.0, 2.0, NOW));
        let s = format!("{c:?}");
        assert!(s.contains("rid") && !s.contains("SECRETPANO"));
    }
}
