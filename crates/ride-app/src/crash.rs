//! Where a crash goes, and how the rider sees it.
//!
//! On Android the app has no console: a panic prints nowhere, the process is gone before anything
//! can be read, and all the rider sees is "the app closed". Every panic is therefore written to
//! `<data>/crash.log`, sent to logcat (`adb logcat -s bike-osm-ride`) and kept in memory so the
//! shell can show it in a banner.
//!
//! This matters more than it looks. `dioxus-desktop` runs every Android UI event, render and
//! asset request under **one global mutex** (`android_sync_lock`), locked with `.lock().unwrap()`
//! (the wry/Android port is not thread safe). A panic inside an event handler poisons that mutex,
//! and from that moment *every* interaction panics as well — the app then "crashes on everything"
//! after one real bug. Two consequences, both implemented here and in `guard`/`controller`:
//!
//! * never let a panic escape our own event handlers (see `Controller::with`), and
//! * always leave a report behind, so the first, real panic can be found.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// A crash log bigger than this is rotated to `crash.log.1` (one previous copy is kept).
const MAX_BYTES: u64 = 256 * 1024;

/// The most recent report, for the shell banner and the tests.
static LAST: Mutex<Option<String>> = Mutex::new(None);
/// Bumped for every report, so the shell notices a new one without comparing the text.
static SEQ: AtomicU64 = AtomicU64::new(0);
/// The log file, resolved once (a panic hook must not do this work).
static LOG: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Installs the panic hook: every panic of this process is logged (file, logcat, stderr) and kept
/// for the banner. Call this first thing in `main`; the previous hook still runs afterwards, so a
/// console build keeps printing to stderr as before.
pub fn install() {
    let _ = log_path();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_raw(&describe(info));
        previous(info);
    }));
}

/// Counter of the reports so far (the shell polls this to pick a new one up).
pub fn sequence() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// The most recent report, if there is one.
pub fn last() -> Option<String> {
    LAST.lock().ok().and_then(|last| last.clone())
}

/// Records an error the app caught (a bug it survived), with the call site that saw it.
pub fn record(what: &str, message: &str) {
    record_raw(&format!(
        "[{}] {what}: {}",
        utc_stamp(now_secs()),
        message.trim_end()
    ));
}

/// The text of a panic: thread, place, message and a backtrace (which a release build resolves to
/// symbols only with debug information, but the addresses alone already show the call path from
/// the crash log of one run against the same APK).
pub fn describe(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = crate::guard::panic_message(info.payload());
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("?").to_string();
    let place = match info.location() {
        Some(loc) => format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        None => "?".to_string(),
    };
    let mut out = format!(
        "[{}] panic in \"{name}\" at {place}: {message}",
        utc_stamp(now_secs())
    );
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let trace = trace.trim_end();
    if !trace.is_empty() {
        out.push('\n');
        out.push_str(trace);
    }
    out
}

/// Writes the report to every sink and remembers it.
fn record_raw(text: &str) {
    let text = text.trim_end();
    if let Ok(mut last) = LAST.lock() {
        *last = Some(text.to_string());
    }
    SEQ.fetch_add(1, Ordering::Relaxed);
    eprintln!("[ride-app] {text}");
    let line = format!("{text}\n");
    if let Some(path) = log_path() {
        append(path, &line);
    }
    log_to_logcat(&line);
}

/// `<data>/crash.log`, created on first use.
fn log_path() -> Option<&'static Path> {
    LOG.get_or_init(|| {
        #[cfg(not(test))]
        let dir = crate::paths::data_dir();
        #[cfg(test)]
        let dir = std::env::temp_dir().join(format!("bike-osm-ride-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join("crash.log"))
    })
    .as_deref()
}

/// Appends `text`, rotating the file once it grows past [`MAX_BYTES`].
fn append(path: &Path, text: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let old = path.with_extension("log.1");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(path, &old);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Android keeps the report where `adb logcat -s bike-osm-ride` shows it, even when the process
/// dies before it could write the file. Every failure here is silent: this runs while the process
/// is already going down.
#[cfg(target_os = "android")]
fn log_to_logcat(text: &str) {
    // liblog works even after the activity/JVM context has been destroyed. A nested panic
    // inside a panic hook aborts before catch_unwind can run, so never use ndk_context here.
    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(
            priority: i32,
            tag: *const std::ffi::c_char,
            text: *const std::ffi::c_char,
        ) -> i32;
    }
    if let Ok(message) = std::ffi::CString::new(text.replace('\0', " ")) {
        // SAFETY: both C strings are NUL terminated and live for the duration of the call.
        unsafe {
            __android_log_write(6, c"bike-osm-ride".as_ptr(), message.as_ptr());
        }
    }
}

#[cfg(not(target_os = "android"))]
fn log_to_logcat(_text: &str) {}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Seconds since the epoch as `YYYY-MM-DDTHH:MM:SSZ` (no clock crate, and no local time zone:
/// the log of one run of one APK is compared as it is).
pub fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to `(year, month, day)` (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_the_reference_point() {
        assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_stamp(59), "1970-01-01T00:00:59Z");
        // a leap day, and a time that is not midnight
        assert_eq!(utc_stamp(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(utc_stamp(1_709_164_800 + 45_296), "2024-02-29T12:34:56Z");
        // the last second of a year, the first of the next
        assert_eq!(utc_stamp(1_735_689_599), "2024-12-31T23:59:59Z");
        assert_eq!(utc_stamp(1_735_689_600), "2025-01-01T00:00:00Z");
        // before the epoch the clock was set back; the log must not panic on it
        assert!(utc_stamp(0).starts_with("1970"));
    }

    #[test]
    fn every_second_of_a_day_formats_without_panicking() {
        for secs in (0..86_400).step_by(997) {
            let stamp = utc_stamp(secs);
            assert_eq!(stamp.len(), 20, "{stamp}");
            assert!(stamp.ends_with('Z'));
        }
    }

    #[test]
    fn a_report_can_be_recorded_and_read_back() {
        let before = sequence();
        record("test", "valami elromlott");
        assert!(
            sequence() > before,
            "the sequence moves so the shell notices"
        );
        let first = last().expect("report");
        assert!(first.contains("test: valami elromlott"), "{first}");
        assert!(first.starts_with('['), "stamped: {first}");
        assert!(!first.ends_with('\n'), "{first}");

        // trailing blanks are trimmed, the message itself is kept verbatim
        record("store::load_settings", "  szóközök  ");
        let trimmed = last().expect("report");
        assert!(trimmed.contains("store::load_settings"), "{trimmed}");
        assert!(trimmed.ends_with("szóközök"), "{trimmed}");
    }
}
