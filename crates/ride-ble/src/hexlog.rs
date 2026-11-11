//! Raw hex log of everything that crosses the GATT boundary (`<data>/ble.log`, always on).
//! It is bounded: past [`ROTATE_AT_BYTES`] the file moves to `ble.log.1` (replacing the older
//! one), at start-up and while it is being written.
//!
//! One line per packet: `timestamp  direction  characteristic  hex`, e.g.
//! `2026-10-31T16:20:11.123Z  NOTIFY  2ad2  44009b077c005200`. Directions are
//! `NOTIFY`, `INDICATE`, `READ` (values read from the device) and `WRITE` (what we sent).
//! This is the evidence trail for debugging a trainer we cannot test in the lab.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::transport::short_id;

/// The log file is rotated (renamed to `<name>.1`) beyond this size.
pub const ROTATE_AT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Notify,
    Indicate,
    Read,
    Write,
}

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Notify => "NOTIFY",
            Dir::Indicate => "INDICATE",
            Dir::Read => "READ",
            Dir::Write => "WRITE",
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    if bytes.is_empty() {
        return "-".to_string();
    }
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Format one log line (without the trailing newline). `unix_ms` is UTC.
pub fn format_line(unix_ms: i64, dir: Dir, characteristic: &str, bytes: &[u8]) -> String {
    format!(
        "{}  {}  {}  {}",
        format_utc(unix_ms),
        dir.as_str(),
        characteristic,
        hex(bytes)
    )
}

/// `2023-11-14T22:13:20.123Z` from milliseconds since the Unix epoch (proleptic Gregorian,
/// Howard Hinnant's civil-from-days algorithm; no external crate needed).
pub fn format_utc(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let ms = unix_ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Cloneable handle; a disabled log makes every call a no-op.
#[derive(Clone, Default)]
pub struct HexLog {
    inner: Option<Arc<Mutex<LogFile>>>,
}

/// The open file and how many bytes it holds, so it can be rotated while the app runs.
struct LogFile {
    path: PathBuf,
    writer: BufWriter<File>,
    len: u64,
    limit: u64,
}

impl LogFile {
    fn write_line(&mut self, line: &str) {
        if writeln!(self.writer, "{line}").is_ok() {
            let _ = self.writer.flush();
            self.len += line.len() as u64 + 1;
        }
        if self.len > self.limit {
            let _ = self.writer.flush();
            rotate(&self.path);
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                self.writer = BufWriter::new(file);
                self.len = 0;
            }
        }
    }
}

/// Moves `path` to `<path>.1`, replacing an older rotation.
fn rotate(path: &Path) {
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(".1");
    let _ = fs::remove_file(&rotated);
    let _ = fs::rename(path, &rotated);
}

impl HexLog {
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Open (append) the log file, creating parent directories. An oversized old file is
    /// moved to `<name>.1` first.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Self::open_with_limit(path, ROTATE_AT_BYTES)
    }

    fn open_with_limit(path: &Path, limit: u64) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::metadata(path).is_ok_and(|meta| meta.len() > limit) {
            rotate(path);
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata().map_or(0, |m| m.len());
        Ok(Self {
            inner: Some(Arc::new(Mutex::new(LogFile {
                path: path.to_path_buf(),
                writer: BufWriter::new(file),
                len,
                limit,
            }))),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// Record one packet. Never panics and never blocks the caller for long; an I/O error
    /// simply drops the line.
    pub fn record(&self, dir: Dir, characteristic: &Uuid, bytes: &[u8]) {
        self.record_at(now_ms(), dir, characteristic, bytes);
    }

    pub fn record_at(&self, unix_ms: i64, dir: Dir, characteristic: &Uuid, bytes: &[u8]) {
        let Some(inner) = &self.inner else { return };
        let name = match short_id(characteristic) {
            Some(s) => format!("{s:04x}"),
            None => characteristic.to_string(),
        };
        let line = format_line(unix_ms, dir, &name, bytes);
        if let Ok(mut file) = inner.lock() {
            file.write_line(&line);
        }
    }

    /// A free-form marker line (`# text`), e.g. connection events.
    pub fn comment(&self, text: &str) {
        let Some(inner) = &self.inner else { return };
        if let Ok(mut file) = inner.lock() {
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            file.write_line(&format!("{}  #  {clean}", format_utc(now_ms())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::sig_uuid;

    #[test]
    fn utc_formatting_known_values() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_utc(1_000_000_000_000), "2001-09-09T01:46:40.000Z");
        assert_eq!(format_utc(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
        // leap day
        assert_eq!(format_utc(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
        // before the epoch must not panic
        assert_eq!(format_utc(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn line_format_has_four_columns() {
        let line = format_line(0, Dir::Notify, "2ad2", &[0x44, 0x00, 0x9b]);
        assert_eq!(line, "1970-01-01T00:00:00.000Z  NOTIFY  2ad2  44009b");
        assert_eq!(
            format_line(0, Dir::Write, "2ad9", &[]).split("  ").count(),
            4
        );
    }

    #[test]
    fn the_log_rotates_while_it_is_written() {
        let dir = std::env::temp_dir().join(format!("ride-ble-hexrot-{}", std::process::id()));
        let path = dir.join("ble.log");
        let _ = fs::remove_dir_all(&dir);
        let log = HexLog::open_with_limit(&path, 1_000).unwrap();
        for i in 0..200 {
            log.record_at(i, Dir::Notify, &sig_uuid(0x2AD2), &[0x44; 16]);
        }
        let current = fs::metadata(&path).unwrap().len();
        let mut rotated = path.as_os_str().to_owned();
        rotated.push(".1");
        let older = fs::metadata(&rotated).unwrap().len();
        assert!(current <= 1_100, "{current} bytes");
        assert!(older <= 1_100, "{older} bytes");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_appends_and_disabled_is_noop() {
        let dir = std::env::temp_dir().join(format!("ride-ble-hexlog-{}", std::process::id()));
        let path = dir.join("nested").join("ble.log");
        let log = HexLog::open(&path).unwrap();
        assert!(log.is_enabled());
        log.record_at(0, Dir::Indicate, &sig_uuid(0x2AD9), &[0x80, 0x00, 0x01]);
        log.record_at(1000, Dir::Write, &sig_uuid(0x2AD9), &[0x00]);
        drop(log);
        // re-open appends
        let log = HexLog::open(&path).unwrap();
        log.record_at(2000, Dir::Notify, &Uuid::nil(), &[1, 2]);
        drop(log);
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "1970-01-01T00:00:00.000Z  INDICATE  2ad9  800001");
        assert_eq!(lines[1], "1970-01-01T00:00:01.000Z  WRITE  2ad9  00");
        assert!(lines[2].contains("00000000-0000-0000-0000-000000000000  0102"));
        let _ = fs::remove_dir_all(&dir);

        let off = HexLog::disabled();
        off.record_at(0, Dir::Read, &Uuid::nil(), &[1]);
        off.comment("semmi");
        assert!(!off.is_enabled());
    }
}
