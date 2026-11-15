//! Persistent calendar entries.

use std::fs::{File, OpenOptions};
use std::io;

use ride_model::CalendarEntry;

use crate::store::{self, Store};

pub(crate) const CALENDAR_FILE: &str = "calendar.json";

/// Serializes calendar mutations across app processes as well as threads.
pub(crate) fn lock_calendar(store: &Store) -> io::Result<File> {
    std::fs::create_dir_all(store.dir())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(store.dir().join("calendar.lock"))?;
    file.lock()?;
    Ok(file)
}

impl Store {
    /// Read scheduled workouts. An unreadable file is reported instead of being overwritten.
    pub fn load_calendar(&self) -> io::Result<Vec<CalendarEntry>> {
        let _guard = store::lock();
        self.load_calendar_unlocked()
    }

    pub(crate) fn load_calendar_unlocked(&self) -> io::Result<Vec<CalendarEntry>> {
        let path = self.dir().join(CALENDAR_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = store::read_limited(&path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "calendar unreadable"))?;
        let entries: Vec<CalendarEntry> = serde_json::from_slice(&bytes)?;
        if entries.iter().any(|entry| !entry.is_valid()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid calendar entry",
            ));
        }
        Ok(entries)
    }

    /// Change the latest calendar snapshot while holding the store lock.
    pub fn update_calendar<T>(
        &self,
        update: impl FnOnce(&mut Vec<CalendarEntry>) -> io::Result<T>,
    ) -> io::Result<T> {
        let _guard = store::lock();
        let _calendar_guard = lock_calendar(self)?;
        let mut entries = self.load_calendar_unlocked()?;
        let result = update(&mut entries)?;
        if entries.iter().any(|entry| !entry.is_valid()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid calendar entry",
            ));
        }
        entries.sort_by(|a, b| (&a.date, &a.time, &a.id).cmp(&(&b.date, &b.time, &b.id)));
        let bytes = serde_json::to_vec_pretty(&entries)?;
        store::write_atomic(&self.dir().join(CALENDAR_FILE), &bytes)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::WorkoutLibrary;

    fn entry(id: &str, date: &str) -> CalendarEntry {
        CalendarEntry {
            id: id.to_string(),
            date: date.to_string(),
            time: Some("09:30".to_string()),
            workout: WorkoutLibrary::embedded().all()[0].clone(),
        }
    }

    #[test]
    fn calendar_roundtrip_sorts_and_keeps_workout_snapshot() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        let later = entry("later", "2028-02-29");
        let earlier = entry("earlier", "2027-01-01");
        store
            .update_calendar(|items| {
                items.push(later.clone());
                items.push(earlier.clone());
                Ok(())
            })
            .expect("save calendar");
        assert_eq!(
            store.load_calendar().expect("load calendar"),
            vec![earlier, later]
        );
    }

    #[test]
    fn invalid_dates_and_damaged_files_are_not_overwritten() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        assert!(!entry("x", "2027-02-29").is_valid());
        assert!(!entry("x", "2027-0é-01").is_valid());
        std::fs::write(dir.path().join(CALENDAR_FILE), b"broken").expect("write damaged file");
        assert!(store
            .update_calendar(|items| {
                items.push(entry("x", "2028-01-01"));
                Ok(())
            })
            .is_err());
        assert_eq!(
            std::fs::read(dir.path().join(CALENDAR_FILE)).expect("read"),
            b"broken"
        );
    }

    #[test]
    fn calendar_lock_child() {
        let Some(dir) = std::env::var_os("RIDE_CALENDAR_LOCK_TEST_DIR") else {
            return;
        };
        let store = Store::at(dir);
        store
            .update_calendar(|items| {
                items.push(entry("child", "2028-01-01"));
                Ok(())
            })
            .expect("child calendar update");
    }

    #[test]
    fn calendar_updates_wait_for_other_processes() {
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        let guard = lock_calendar(&store).expect("lock calendar");
        let mut child = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "calendar::tests::calendar_lock_child"])
            .env("RIDE_CALENDAR_LOCK_TEST_DIR", dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn child test");
        std::thread::sleep(Duration::from_millis(300));
        let early_exit = child.try_wait().expect("check child");
        drop(guard);
        let result = child.wait().expect("wait for child");
        assert!(early_exit.is_none(), "child did not wait for calendar lock");
        assert!(result.success(), "child update failed");
        assert_eq!(store.load_calendar().expect("load calendar").len(), 1);
    }
}
