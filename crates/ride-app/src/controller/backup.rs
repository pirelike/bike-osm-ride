//! The controller side of the backup: "Mentés fájlba" and "Visszaállítás fájlból" in the settings.
//!
//! The zip itself is made and read by `ride_route::backup`; this file asks where to put it (or
//! where it is), runs that on a blocking thread and, after a restore, brings every signal that
//! mirrors the disk up to date. Nothing here touches the network (`docs/PRIVACY.md`).
//!
//! Both are refused while a ride runs: a ride that finishes during the copy would write into the
//! folder being read, and the ghost of a running ride is not rebuilt.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use ride_model::{RideState, Settings};
use ride_route::backup;
use ride_route::store::Store;
use ride_ui_kit::ToastKind;

use super::{boot, guarded, Controller, Inner};

fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Writes the backup to `path` (the blocking part of a save).
fn write_backup(path: &Path, settings: &Settings) -> Result<backup::ExportReport, String> {
    let file = File::create(path).map_err(|err| format!("A fájl nem hozható létre: {err}"))?;
    let mut out = BufWriter::new(file);
    let done = backup::export(
        &Store::default_location(),
        settings,
        env!("CARGO_PKG_VERSION"),
        now_unix() as u64,
        &mut out,
    )
    .and_then(|report| {
        out.flush()
            .map(|()| report)
            .map_err(|err| format!("A mentés írása nem sikerült: {err}"))
    });
    if done.is_err() {
        // a half-written zip must not look like a backup
        drop(out);
        let _ = std::fs::remove_file(path);
    }
    done
}

/// Reads the backup at `path` into the data folder (the blocking part of a restore).
fn read_backup(path: &Path) -> Result<backup::RestoreReport, String> {
    let file = File::open(path).map_err(|err| format!("A fájl nem nyitható meg: {err}"))?;
    backup::restore(&Store::default_location(), BufReader::new(file))
}

impl Inner {
    /// Imported workouts live in one signal; a restore may have added some.
    fn reload_imported_workouts(&mut self) {
        if let Ok(list) = guarded("store::list_workouts", || {
            Store::default_location().list_workouts()
        }) {
            let mut signal = self.st.imported_workouts;
            signal.set(list);
        }
    }
}

impl Controller {
    /// A running or paused ride must be ended first.
    fn backup_allowed(self) -> bool {
        let riding = self.with(|i| matches!(i.core.state(), RideState::Riding | RideState::Paused));
        if riding {
            self.st.toast(
                ToastKind::Warning,
                "Tekerés közben nem lehet mentést készíteni vagy visszaállítani. Előbb fejezd be a túrát.",
            );
        }
        !riding
    }

    /// The settings as the backup should carry them: the live ones, not the file that may lag
    /// behind the debounced save. The Google key is blanked by the backup itself.
    fn backup_settings(self) -> Settings {
        boot().for_disk(self.st.settings.peek().clone())
    }

    /// "Mentés fájlba": a save dialog, then the zip is written.
    #[cfg(not(target_os = "android"))]
    pub fn backup_save(self) {
        if !self.backup_allowed() {
            return;
        }
        if self.with(|i| std::mem::replace(&mut i.file_dialog_open, true)) {
            return;
        }
        let Some(win) = self.window() else {
            self.with(|i| i.file_dialog_open = false);
            return;
        };
        let settings = self.backup_settings();
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Biztonsági mentés mentése")
                .set_file_name(backup::suggested_filename(now_unix()))
                .add_filter("Mentés (zip)", &["zip"])
                .set_parent(&*win.window)
                .save_file()
                .await
                .map(|file| file.path().to_path_buf());
            self.with(|i| i.file_dialog_open = false);
            let Some(mut path) = picked else { return };
            if path.extension().is_none() {
                path.set_extension("zip");
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let joined = tokio::task::spawn_blocking(move || {
                guarded("backup export", || write_backup(&path, &settings))
            })
            .await;
            match joined {
                Ok(Ok(Ok(report))) => self.st.toast(
                    ToastKind::Success,
                    format!(
                        "Mentve: {name} ({} útvonal, {} túra, {} edzés)",
                        report.routes, report.rides, report.workouts
                    ),
                ),
                Ok(Ok(Err(message))) => self.st.toast(ToastKind::Error, message),
                _ => self.st.toast(ToastKind::Error, "A mentés nem sikerült."),
            }
        });
    }

    /// Android: the zip is written into a private folder, then the system's "save as" dialog
    /// takes over (`MainActivity.saveBackup` copies it and deletes it). The answer comes through
    /// [`Controller::backup_saved`].
    #[cfg(target_os = "android")]
    pub fn backup_save(self) {
        if !self.backup_allowed() {
            return;
        }
        let settings = self.backup_settings();
        spawn(async move {
            let name = backup::suggested_filename(now_unix());
            let temp = crate::paths::data_dir().join("backup-tmp");
            let path = temp.join(&name);
            let written = {
                let path = path.clone();
                tokio::task::spawn_blocking(move || {
                    guarded("backup export", || {
                        std::fs::create_dir_all(&temp)
                            .map_err(|err| format!("A mappa nem hozható létre: {err}"))?;
                        write_backup(&path, &settings)
                    })
                })
                .await
            };
            match written {
                Ok(Ok(Ok(_))) => {}
                Ok(Ok(Err(message))) => {
                    self.st.toast(ToastKind::Error, message);
                    return;
                }
                _ => {
                    self.st.toast(ToastKind::Error, "A mentés nem sikerült.");
                    return;
                }
            }
            if let Err(err) = crate::android_picker::save_backup(&path.to_string_lossy(), &name) {
                let _ = std::fs::remove_file(&path);
                self.st.toast(
                    ToastKind::Error,
                    format!("A fájlválasztó nem nyílt meg: {err}"),
                );
            }
        });
    }

    /// Android: how the "save as" dialog ended (polled by the shell).
    #[cfg(target_os = "android")]
    pub fn backup_saved(self, how: crate::android_picker::BackupSaved) {
        use crate::android_picker::BackupSaved;
        match how {
            BackupSaved::Saved => self.st.toast(ToastKind::Success, "A mentés elkészült."),
            BackupSaved::Cancelled => {}
            BackupSaved::Failed => self.st.toast(
                ToastKind::Error,
                "A mentést nem sikerült a kiválasztott helyre írni.",
            ),
        }
    }

    /// "Visszaállítás fájlból": a file dialog, then the zip is merged into the data.
    #[cfg(not(target_os = "android"))]
    pub fn backup_restore(self) {
        if !self.backup_allowed() {
            return;
        }
        if self.with(|i| std::mem::replace(&mut i.file_dialog_open, true)) {
            return;
        }
        let Some(win) = self.window() else {
            self.with(|i| i.file_dialog_open = false);
            return;
        };
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Visszaállítás mentésből")
                .add_filter("Mentés (zip)", &["zip"])
                .add_filter("Minden fájl", &["*"])
                .set_parent(&*win.window)
                .pick_file()
                .await;
            self.with(|i| i.file_dialog_open = false);
            if let Some(file) = picked {
                self.backup_restore_path(file.path().to_path_buf(), false);
            }
        });
    }

    /// Android: the system picker answers through [`Controller::backup_restore_path`].
    #[cfg(target_os = "android")]
    pub fn backup_restore(self) {
        if !self.backup_allowed() {
            return;
        }
        if let Err(err) = crate::android_picker::pick_backup() {
            self.st.toast(
                ToastKind::Error,
                format!("A fájlválasztó nem nyílt meg: {err}"),
            );
        }
    }

    /// Merges the backup at `path` into the data. `delete_after`: the file is a copy in the cache
    /// (Android), not the rider's own file.
    pub fn backup_restore_path(self, path: PathBuf, delete_after: bool) {
        if !self.backup_allowed() {
            if delete_after {
                let _ = std::fs::remove_file(&path);
            }
            return;
        }
        spawn(async move {
            let joined = tokio::task::spawn_blocking(move || {
                let result = guarded("backup restore", || read_backup(&path));
                if delete_after {
                    let _ = std::fs::remove_file(&path);
                }
                result
            })
            .await;
            match joined {
                Ok(Ok(Ok(report))) => self.finish_restore(report),
                Ok(Ok(Err(message))) => self.st.toast(ToastKind::Error, message),
                _ => self
                    .st
                    .toast(ToastKind::Error, "A visszaállítás nem sikerült."),
            }
        });
    }

    /// Everything that mirrors the disk is read again, then the rider is told what was added.
    fn finish_restore(self, mut report: backup::RestoreReport) {
        let settings_restored = if let Some(mut restored) = report.settings.take() {
            let current = self.st.settings.peek().clone();
            // The key never comes from a file, and picking another workout would start it.
            restored.ui.google_maps_key = current.ui.google_maps_key;
            restored.ui.workout_id = current.ui.workout_id;
            self.apply_settings(restored);
            true
        } else {
            false
        };
        self.with(|i| {
            i.refresh_recents();
            i.reload_imported_workouts();
            i.reload_ghost();
        });
        if *self.st.review.open.peek() {
            self.review_refresh(None, Vec::new(), None);
        }

        let added = report.routes + report.rides + report.workouts;
        let mut text = if added == 0 {
            "Minden megvolt már, nem került be új útvonal, túra vagy edzés.".to_string()
        } else {
            format!(
                "Visszaállítva: {} útvonal, {} túra, {} edzés.",
                report.routes, report.rides, report.workouts
            )
        };
        if settings_restored {
            text.push_str(
                " A beállítások is a mentésből valók (a Street View kulcs a régi maradt).",
            );
        }
        if report.skipped > 0 {
            text.push_str(&format!(
                " {} elem kimaradt (ismeretlen vagy sérült).",
                report.skipped
            ));
        }
        self.st.toast(ToastKind::Success, text);
    }
}
