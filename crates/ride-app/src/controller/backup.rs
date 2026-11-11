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
use ride_i18n::{t, tf};

fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Writes the backup to `path` (the blocking part of a save).
fn write_backup(path: &Path, settings: &Settings) -> Result<backup::ExportReport, String> {
    let file = File::create(path).map_err(|err| {
        tf(
            "app.controller.backup.a_fajl_nem_hozhato_letre_err",
            &[("err", &err.to_string())],
        )
    })?;
    let mut out = BufWriter::new(file);
    let done = backup::export(
        &Store::default_location(),
        settings,
        env!("CARGO_PKG_VERSION"),
        now_unix() as u64,
        &mut out,
    )
    .and_then(|report| {
        out.flush().map(|()| report).map_err(|err| {
            tf(
                "app.controller.backup.a_mentes_irasa_nem_sikerult_err",
                &[("err", &err.to_string())],
            )
        })
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
    let file = File::open(path).map_err(|err| {
        tf(
            "app.controller.backup.a_fajl_nem_nyithato_meg_err",
            &[("err", &err.to_string())],
        )
    })?;
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
                t("app.controller.backup.tekeres_kozben_nem_lehet_mentest_kesziteni_vagy_viss"),
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
        let Some(lease) = self.acquire_file_dialog() else {
            return;
        };
        let Some(win) = self.window() else {
            return;
        };
        let settings = self.backup_settings();
        spawn(async move {
            let _lease = lease;
            let picked = rfd::AsyncFileDialog::new()
                .set_title(t("app.controller.backup.biztonsagi_mentes_mentese"))
                .set_file_name(backup::suggested_filename(now_unix()))
                .add_filter(t("app.controller.backup.mentes_zip"), &["zip"])
                .set_parent(&*win.window)
                .save_file()
                .await
                .map(|file| file.path().to_path_buf());

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
                Ok(Ok(Ok(report))) => {
                    let mut text = tf(
                        "app.controller.backup.mentve_name_utvonal_tura_edzes",
                        &[
                            ("name", &name),
                            ("routes", &report.routes.to_string()),
                            ("rides", &report.rides.to_string()),
                            ("workouts", &report.workouts.to_string()),
                        ],
                    );
                    // A backup that is missing files must not look complete.
                    let kind = if report.omitted > 0 {
                        text.push_str(&tf(
                            "app.controller.backup.fajl_kimaradt_a_mentesbol",
                            &[("n", &report.omitted.to_string())],
                        ));
                        ToastKind::Warning
                    } else {
                        ToastKind::Success
                    };
                    self.st.toast(kind, text);
                }
                Ok(Ok(Err(message))) => self.st.toast(ToastKind::Error, message),
                _ => self.st.toast(
                    ToastKind::Error,
                    t("app.controller.backup.a_mentes_nem_sikerult"),
                ),
            }
        });
    }

    /// Android: the zip is written into a private folder, then the system's "save as" dialog
    /// takes over (`MainActivity.saveFile` copies it and deletes it). The answer comes through
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
                        std::fs::create_dir_all(&temp).map_err(|err| {
                            tf(
                                "app.controller.backup.a_mappa_nem_hozhato_letre_err",
                                &[("err", &err.to_string())],
                            )
                        })?;
                        write_backup(&path, &settings)
                    })
                })
                .await
            };
            match written {
                Ok(Ok(Ok(report))) => {
                    if report.omitted > 0 {
                        self.st.toast(
                            ToastKind::Warning,
                            tf(
                                "app.controller.backup.fajl_kimaradt_a_mentesbol",
                                &[("n", &report.omitted.to_string())],
                            )
                            .trim()
                            .to_string(),
                        );
                    }
                }
                Ok(Ok(Err(message))) => {
                    self.st.toast(ToastKind::Error, message);
                    return;
                }
                _ => {
                    self.st.toast(
                        ToastKind::Error,
                        t("app.controller.backup.a_mentes_nem_sikerult"),
                    );
                    return;
                }
            }
            if let Err(err) =
                crate::android_picker::save_file(&path.to_string_lossy(), &name, "application/zip")
            {
                let _ = std::fs::remove_file(&path);
                self.st.toast(
                    ToastKind::Error,
                    tf(
                        "app.controller.backup.a_fajlvalaszto_nem_nyilt_meg_err",
                        &[("error", &err.to_string())],
                    ),
                );
            }
        });
    }

    /// Android: how the "save as" dialog ended (polled by the shell).
    #[cfg(target_os = "android")]
    pub fn backup_saved(self, how: crate::android_picker::BackupSaved) {
        use crate::android_picker::BackupSaved;
        match how {
            BackupSaved::Saved => self.st.toast(
                ToastKind::Success,
                t("app.controller.backup.a_mentes_elkeszult"),
            ),
            BackupSaved::Cancelled => {}
            BackupSaved::Failed => self.st.toast(ToastKind::Error, {
                t("app.controller.backup.a_mentest_nem_sikerult_a_kivalasztott_helyre_irni")
            }),
        }
    }

    /// "Visszaállítás fájlból": a file dialog, then the zip is merged into the data.
    #[cfg(not(target_os = "android"))]
    pub fn backup_restore(self) {
        if !self.backup_allowed() {
            return;
        }
        let Some(lease) = self.acquire_file_dialog() else {
            return;
        };
        let Some(win) = self.window() else {
            return;
        };
        spawn(async move {
            let _lease = lease;
            let picked = rfd::AsyncFileDialog::new()
                .set_title(t("app.controller.backup.visszaallitas_mentesbol"))
                .add_filter(t("app.controller.backup.mentes_zip"), &["zip"])
                .add_filter(t("app.controller.backup.minden_fajl"), &["*"])
                .set_parent(&*win.window)
                .pick_file()
                .await;

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
                tf(
                    "app.controller.backup.a_fajlvalaszto_nem_nyilt_meg_err",
                    &[("error", &err.to_string())],
                ),
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
                _ => self.st.toast(
                    ToastKind::Error,
                    t("app.controller.backup.a_visszaallitas_nem_sikerult"),
                ),
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
            t("app.controller.backup.minden_megvolt_mar_nem_kerult_be_uj_utvonal_tura_vag")
                .to_string()
        } else {
            tf(
                "app.controller.backup.visszaallitva_utvonal_tura_edzes",
                &[
                    ("routes", &report.routes.to_string()),
                    ("rides", &report.rides.to_string()),
                    ("workouts", &report.workouts.to_string()),
                ],
            )
        };
        if settings_restored {
            text.push_str(t(
                "app.controller.backup.a_beallitasok_is_a_mentesbol_valok_a_street_view_kul",
            ));
        }
        if report.skipped > 0 {
            text.push_str(&tf(
                "app.controller.backup.elem_kimaradt_ismeretlen_vagy_serult",
                &[("n", &report.skipped.to_string())],
            ));
        }
        let kind = if report.failed > 0 {
            text.push_str(&tf(
                "app.controller.backup.fajlt_nem_sikerult_kiirni",
                &[("n", &report.failed.to_string())],
            ));
            ToastKind::Warning
        } else {
            ToastKind::Success
        };
        self.st.toast(kind, text);
    }
}
