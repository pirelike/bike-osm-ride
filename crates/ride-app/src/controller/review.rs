//! The controller side of the review page: which route and rides are shown, importing a GPX / TCX
//! of another app onto a route, re-estimating the watts of such a ride, deleting a ride.
//!
//! Everything that touches the disk runs on a blocking thread; the result is applied only if no
//! newer request was made meanwhile (`GENERATION`), so a slow list from a route the user already
//! left cannot overwrite the page. The import is a file the user picked or dropped, handled on
//! this machine: nothing leaves it (`docs/PRIVACY.md`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{EstimateParams, RideRecord, RiderSettings, Route, RouteRides};
use ride_route::store::Store;
use ride_route::{activity, import, rides};
use ride_sim::power_curve;
use ride_ui_kit::ToastKind;

use super::{guarded, Controller};
use ride_i18n::{t, tf};

/// Rides compared at once (the number of series colours of the page).
const MAX_TICKED: usize = 4;
/// Rides ticked when a route is opened: its two newest, so there is something to compare.
const DEFAULT_TICKED: usize = 2;
/// Activity files larger than this are refused before reading.
const MAX_ACTIVITY_BYTES: u64 = 64 * 1024 * 1024;

fn now_unix_s() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Counts the refreshes; a result that is not the latest one is dropped.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Everything one refresh puts on the page.
struct Page {
    routes: Vec<RouteRides>,
    route_id: Option<String>,
    route: Option<Arc<Route>>,
    selected: Vec<String>,
    records: Vec<Arc<RideRecord>>,
}

/// Reads the ride log and chooses what to show: the wanted route, else the loaded one, else the
/// one ridden last; the rides to keep ticked (when they still exist), the one to add, else the two
/// newest.
fn collect(
    store: &Store,
    current: Option<Arc<Route>>,
    wanted_route: Option<String>,
    keep: Vec<String>,
    prefer: Option<String>,
) -> Page {
    let mut routes = store.list_all_rides();
    // the loaded route is offered even before its first ride: an import needs a route to land on
    if let Some(route) = &current {
        if !routes.iter().any(|r| r.route_id == route.id) {
            routes.push(RouteRides {
                route_id: route.id.clone(),
                route_name: route.name.clone(),
                rides: Vec::new(),
            });
        }
    }
    let route_id = wanted_route
        .filter(|id| routes.iter().any(|r| &r.route_id == id))
        .or_else(|| {
            current
                .as_ref()
                .map(|r| r.id.clone())
                .filter(|id| routes.iter().any(|r| &r.route_id == id))
        })
        .or_else(|| routes.first().map(|r| r.route_id.clone()));

    let route = route_id.as_ref().and_then(|id| match &current {
        Some(route) if &route.id == id => Some(route.clone()),
        _ => store.load_recent(id).map(Arc::new),
    });

    let available: Vec<String> = route_id
        .as_ref()
        .and_then(|id| routes.iter().find(|r| &r.route_id == id))
        .map(|r| r.rides.iter().map(|m| m.stem.clone()).collect())
        .unwrap_or_default();
    let mut selected: Vec<String> = keep
        .into_iter()
        .filter(|stem| available.contains(stem))
        .collect();
    if let Some(stem) = prefer.filter(|s| available.contains(s)) {
        selected.retain(|s| s != &stem);
        selected.truncate(MAX_TICKED - 1);
        selected.push(stem);
    }
    if selected.is_empty() {
        selected = available.iter().take(DEFAULT_TICKED).cloned().collect();
    }
    selected.truncate(MAX_TICKED);

    let mut records = Vec::new();
    let mut loadable = Vec::new();
    if let Some(id) = &route_id {
        for stem in selected {
            if let Some(record) = store.load_ride(id, &stem) {
                loadable.push(stem);
                records.push(Arc::new(record));
            }
        }
    }
    Page {
        routes,
        route_id,
        route,
        selected: loadable,
        records,
    }
}

/// Reads an activity file (bounded), parses it, puts it on `route` and stores it.
fn import_file(
    path: &PathBuf,
    route: &Route,
    rider: &RiderSettings,
) -> Result<(import::Imported, String), String> {
    let meta = std::fs::metadata(path)
        .map_err(|_| t("app.controller.review.a_fajlt_nem_sikerult_megnyitni").to_string())?;
    if !meta.is_file() {
        return Err(t("app.controller.review.a_kivalasztott_elem_nem_fajl").into());
    }
    if meta.len() > MAX_ACTIVITY_BYTES {
        return Err(tf(
            "app.controller.review.a_fajl_tul_nagy_legfeljebb_mb_engedelyezett",
            &[("mb", &(MAX_ACTIVITY_BYTES / (1024 * 1024)).to_string())],
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|_| t("app.controller.review.a_fajlt_nem_sikerult_megnyitni").to_string())?;
    let parsed = activity::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let done = import::import_activity(&parsed, route, rider, None).map_err(|e| e.to_string())?;
    let started = done.record.summary.started_unix_s;
    if rides::has_ride(&route.id, started) {
        return Err(t(
            "app.controller.review.ezt_az_edzest_mar_importaltad_erre_az_utvonalra_ugya",
        )
        .into());
    }
    let saved = rides::save_ride(&done.record, route)
        .map_err(|e| {
            tf(
                "app.controller.review.az_edzest_nem_sikerult_elmenteni_e",
                &[("e", &e.to_string())],
            )
        })?
        .ok_or_else(|| {
            t("app.controller.review.az_edzes_tul_rovid_legalabb_1_perc_es_200_m_kell_eze")
                .to_string()
        })?;
    Ok((done, saved.stem))
}

impl Controller {
    /// Opens the page on the loaded route (else the route ridden last).
    pub fn review_open(self) {
        let mut open = self.st.review.open;
        let mut notes = self.st.review.notes;
        notes.set(Vec::new());
        open.set(true);
        self.review_refresh(None, Vec::new(), None);
    }

    pub fn review_close(self) {
        let mut open = self.st.review.open;
        open.set(false);
    }

    pub fn review_dismiss_notes(self) {
        let mut notes = self.st.review.notes;
        notes.set(Vec::new());
    }

    pub fn review_select_route(self, id: String) {
        self.review_refresh(Some(id), Vec::new(), None);
    }

    /// Ticks or unticks a ride (at most four).
    pub fn review_toggle(self, stem: String) {
        let mut keep = self.st.review.selected.peek().clone();
        if let Some(at) = keep.iter().position(|s| s == &stem) {
            keep.remove(at);
            // unticking the last ride must not fall back to the default pair
            if keep.is_empty() {
                let mut selected = self.st.review.selected;
                let mut loaded = self.st.review.loaded;
                selected.set(Vec::new());
                loaded.set(Arc::new(Vec::new()));
                return;
            }
        } else if keep.len() >= MAX_TICKED {
            self.st.toast(
                ToastKind::Info,
                t("app.controller.review.legfeljebb_negy_edzest_lehet_egyszerre_osszehasonlit"),
            );
            return;
        } else {
            keep.push(stem);
        }
        let route_id = self.st.review.route_id.peek().clone();
        self.review_refresh(route_id, keep, None);
    }

    /// A mutation refresh must not undo a route choice made while its disk work ran.
    fn refresh_after_mutation(self, route_id: String, keep: Vec<String>, prefer: Option<String>) {
        if self.st.review.route_id.peek().as_ref() == Some(&route_id) {
            self.review_refresh(Some(route_id), keep, prefer);
        } else {
            let mut busy = self.st.review.busy;
            busy.set(false);
        }
    }

    /// Deletes one stored ride.
    pub fn review_delete(self, stem: String) {
        let Some(route_id) = self.st.review.route_id.peek().clone() else {
            return;
        };
        let keep: Vec<String> = self
            .st
            .review
            .selected
            .peek()
            .iter()
            .filter(|s| **s != stem)
            .cloned()
            .collect();
        let mut busy = self.st.review.busy;
        busy.set(true);
        spawn(async move {
            let id = route_id.clone();
            let joined = tokio::task::spawn_blocking(move || {
                guarded("review delete", || rides::delete_ride(&id, &stem))
            })
            .await;
            match joined {
                Ok(Ok(Ok(true))) => self.st.toast(
                    ToastKind::Success,
                    t("app.controller.review.az_edzest_toroltuk"),
                ),
                Ok(Ok(Ok(false))) => {}
                _ => self.st.toast(
                    ToastKind::Error,
                    t("app.controller.review.az_edzest_nem_sikerult_torolni"),
                ),
            }
            self.refresh_after_mutation(route_id, keep, None);
        });
    }

    /// The "import" button: a file dialog, then the file goes onto the route shown.
    #[cfg(not(target_os = "android"))]
    pub fn review_import(self) {
        let Some(lease) = self.acquire_file_dialog() else {
            return;
        };
        let Some(win) = self.window() else {
            return;
        };
        spawn(async move {
            let _lease = lease;
            let picked = rfd::AsyncFileDialog::new()
                .set_title(t("app.controller.review.edzes_importalasa_gpx_vagy_tcx"))
                .add_filter(
                    t("app.controller.review.edzesfajl_gpx_tcx"),
                    &["gpx", "tcx"],
                )
                .add_filter(t("app.controller.backup.minden_fajl"), &["*"])
                .set_parent(&*win.window)
                .pick_file()
                .await;

            if let Some(file) = picked {
                self.review_import_path(file.path().to_path_buf());
            }
        });
    }

    /// Android has no native file dialog and the system picker is wired to the route loader only.
    #[cfg(target_os = "android")]
    pub fn review_import(self) {
        self.st.toast(ToastKind::Info, {
            t("app.controller.review.androidon_az_edzesimport_meg_nem_elerheto")
        });
    }

    /// Puts the activity file at `path` on the route shown and stores it as a ride.
    pub fn review_import_path(self, path: PathBuf) {
        let Some(route) = self.st.review.route.peek().clone() else {
            self.st.toast(
                ToastKind::Warning,
                t("app.controller.review.elobb_valassz_egy_utvonalat_amelynek_megvan_a_fajlja"),
            );
            return;
        };
        let rider = self.st.settings.peek().rider.clone();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| t("app.controller.review.edzes").to_string());
        let mut busy = self.st.review.busy;
        busy.set(true);
        let keep = self.st.review.selected.peek().clone();
        spawn(async move {
            let route_for_job = route.clone();
            let joined = tokio::task::spawn_blocking(move || {
                guarded("review import", || {
                    import_file(&path, &route_for_job, &rider)
                })
            })
            .await;
            match joined {
                Ok(Ok(Ok((done, stem)))) => {
                    let display_name = if done.name.is_empty() {
                        name.as_str()
                    } else {
                        done.name.as_str()
                    };
                    let mut notes = vec![tf(
                        "app.controller.review.importalva_1_km",
                        &[
                            ("name", display_name),
                            ("km", &format!("{:.1}", done.record.summary.dist / 1000.0)),
                            (
                                "time",
                                &ride_ui_kit::format::fmt_duration(done.record.summary.elapsed_s),
                            ),
                        ],
                    )];
                    notes.extend(done.warnings);
                    let mut signal = self.st.review.notes;
                    signal.set(notes);
                    self.refresh_after_mutation(route.id.clone(), keep, Some(stem));
                }
                Ok(Ok(Err(message))) => {
                    self.st.toast(ToastKind::Error, message);
                    let mut busy = self.st.review.busy;
                    busy.set(false);
                }
                _ => {
                    self.st.toast(
                        ToastKind::Error,
                        t("app.controller.review.az_edzes_importalasa_nem_sikerult"),
                    );
                    let mut busy = self.st.review.busy;
                    busy.set(false);
                }
            }
        });
    }

    /// Estimates the watts of a stored ride again with another rider / bike setup.
    pub fn review_reestimate(self, stem: String, params: EstimateParams) {
        let (Some(route_id), Some(route)) = (
            self.st.review.route_id.peek().clone(),
            self.st.review.route.peek().clone(),
        ) else {
            return;
        };
        let rider = self.st.settings.peek().rider.clone();
        let keep = self.st.review.selected.peek().clone();
        let mut busy = self.st.review.busy;
        busy.set(true);
        spawn(async move {
            let id = route_id.clone();
            let joined = tokio::task::spawn_blocking(move || {
                guarded("review reestimate", || -> Result<(), String> {
                    let record = rides::load_ride(&id, &stem)
                        .ok_or(t("app.controller.review.az_edzes_nem_toltheto_be"))?;
                    let redone =
                        import::reestimate(&record, params, &rider).map_err(|e| e.to_string())?;
                    rides::replace_ride(&redone, &route, &stem).map_err(|e| {
                        tf(
                            "app.controller.review.az_edzest_nem_sikerult_elmenteni_e",
                            &[("e", &e.to_string())],
                        )
                    })?;
                    Ok(())
                })
            })
            .await;
            match joined {
                Ok(Ok(Ok(()))) => self.st.toast(
                    ToastKind::Success,
                    t("app.controller.review.a_wattokat_ujraszamoltuk"),
                ),
                Ok(Ok(Err(message))) => self.st.toast(ToastKind::Error, message),
                _ => self.st.toast(
                    ToastKind::Error,
                    t("app.controller.review.az_ujraszamolas_nem_sikerult"),
                ),
            }
            self.refresh_after_mutation(route_id, keep, None);
        });
    }

    /// "FTP beállítása N W-ra" on the power view: the setting changes to the suggestion. The TSS
    /// stored with older rides keeps the FTP of its day.
    pub fn review_apply_ftp(self, watts: f64) {
        if !watts.is_finite() {
            return;
        }
        let ftp = watts
            .round()
            .clamp(power_curve::FTP_MIN_W, power_curve::FTP_MAX_W);
        self.update_settings(|s| s.rider.ftp_w = ftp);
        self.st.toast(
            ToastKind::Success,
            tf(
                "app.controller.review.az_ftp_beallitva_ftp_0_w",
                &[("ftp", &format!("{ftp:.0}"))],
            ),
        );
    }

    /// Reads the ride log and puts the result on the page.
    pub(super) fn review_refresh(
        self,
        wanted_route: Option<String>,
        keep: Vec<String>,
        prefer: Option<String>,
    ) {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let mut busy = self.st.review.busy;
        busy.set(true);
        let current = self.st.route.peek().clone();
        spawn(async move {
            let joined = tokio::task::spawn_blocking(move || {
                guarded("review collect", || {
                    let store = Store::default_location();
                    // rides saved before the power page existed get their numbers once, from
                    // their records; the result stays in the index
                    store.backfill_analysis();
                    let page = collect(&store, current, wanted_route, keep, prefer);
                    let analysis = power_curve::analyze(&page.routes, now_unix_s());
                    (page, analysis)
                })
            })
            .await;
            if GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let r = self.st.review;
            let (mut routes, mut route_id, mut route) = (r.routes, r.route_id, r.route);
            let (mut selected, mut loaded, mut busy) = (r.selected, r.loaded, r.busy);
            let mut analysis_signal = r.analysis;
            match joined {
                Ok(Ok((page, analysis))) => {
                    analysis_signal.set(Arc::new(analysis));
                    routes.set(page.routes);
                    route_id.set(page.route_id);
                    route.set(page.route);
                    selected.set(page.selected);
                    loaded.set(Arc::new(page.records));
                }
                _ => self.st.toast(
                    ToastKind::Error,
                    t("app.controller.review.az_edzesek_listajat_nem_sikerult_beolvasni"),
                ),
            }
            busy.set(false);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::{RecordPoint, RideSource, RideSummary};

    fn route(id: &str) -> Route {
        let mut route = straight_route(&[(2_000.0, 1.0)], 100.0);
        route.id = id.into();
        route.name = format!("Út {id}");
        route
    }

    fn ride(route: &Route, started: f64) -> RideRecord {
        let points: Vec<RecordPoint> = (0..=300)
            .map(|t| RecordPoint {
                t: f64::from(t),
                unix_s: started + f64::from(t),
                dist: f64::from(t) * 6.0,
                lat: 47.5,
                lon: 19.0,
                ele: 100.0,
                speed_ms: 6.0,
                power_w: Some(200.0),
                cadence_rpm: Some(85.0),
                hr_bpm: Some(140.0),
                grade: 1.0,
            })
            .collect();
        RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: route.id.clone(),
            route_name: route.name.clone(),
            simulated: false,
            summary: RideSummary {
                route_name: route.name.clone(),
                route_id: route.id.clone(),
                started_unix_s: started,
                elapsed_s: 300.0,
                dist: 1_800.0,
                ascent: 18.0,
                avg_speed_ms: 6.0,
                max_speed_ms: 6.0,
                avg_power_w: Some(200.0),
                max_power_w: Some(200.0),
                normalized_power_w: Some(200.0),
                intensity_factor: None,
                tss: None,
                work_kj: 60.0,
                kcal: 60.0,
                avg_hr: Some(140.0),
                max_hr: Some(140.0),
                avg_cadence: Some(85.0),
                power_zone_secs: [0.0; 7],
                hr_zone_secs: [0.0; 5],
            },
            points,
            source: RideSource::Recorded,
            power_estimated: false,
            estimate: None,
            measured_power: Vec::new(),
        }
    }

    #[test]
    fn the_loaded_route_is_offered_even_without_rides() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::at(dir.path());
        let route = Arc::new(route("a"));
        let page = collect(&store, Some(route), None, Vec::new(), None);
        assert_eq!(page.routes.len(), 1);
        assert_eq!(page.route_id.as_deref(), Some("a"));
        assert!(page.route.is_some(), "the loaded route needs no disk read");
        assert!(page.selected.is_empty() && page.records.is_empty());
        // nothing loaded and nothing stored: an empty page, not a failure
        let page = collect(&store, None, None, Vec::new(), None);
        assert!(page.routes.is_empty() && page.route_id.is_none());
    }

    #[test]
    fn the_two_newest_rides_are_ticked_and_choices_are_kept() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::at(dir.path());
        let a = route("a");
        for started in [1_700_000_000.0, 1_700_100_000.0, 1_700_200_000.0] {
            store.save_ride(&ride(&a, started), &a).unwrap();
        }
        let page = collect(&store, Some(Arc::new(a.clone())), None, Vec::new(), None);
        assert_eq!(
            page.selected,
            ["1700200000", "1700100000"],
            "the two newest"
        );
        assert_eq!(page.records.len(), 2);

        // an explicit choice stays, a vanished one is dropped, a new one is added
        let page = collect(
            &store,
            Some(Arc::new(a.clone())),
            None,
            vec!["1700000000".into(), "999".into()],
            Some("1700200000".into()),
        );
        assert_eq!(page.selected, ["1700000000", "1700200000"]);
        assert_eq!(
            page.records
                .iter()
                .map(|r| r.summary.started_unix_s)
                .collect::<Vec<_>>(),
            [1_700_000_000.0, 1_700_200_000.0]
        );
        // never more than four
        for started in [1_700_300_000.0, 1_700_400_000.0, 1_700_500_000.0] {
            store.save_ride(&ride(&a, started), &a).unwrap();
        }
        let many: Vec<String> = [
            "1700000000",
            "1700100000",
            "1700200000",
            "1700300000",
            "1700400000",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let page = collect(&store, Some(Arc::new(a)), None, many, None);
        assert_eq!(page.selected.len(), MAX_TICKED);
    }

    #[test]
    fn another_route_is_chosen_by_id_and_a_missing_one_falls_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::at(dir.path());
        let (a, b) = (route("a"), route("b"));
        store.save_ride(&ride(&a, 1_700_000_000.0), &a).unwrap();
        store.save_ride(&ride(&b, 1_700_500_000.0), &b).unwrap();
        // b was ridden last; asking for a gets a
        let page = collect(&store, None, Some("a".into()), Vec::new(), None);
        assert_eq!(page.route_id.as_deref(), Some("a"));
        let page = collect(&store, None, None, Vec::new(), None);
        assert_eq!(
            page.route_id.as_deref(),
            Some("b"),
            "the newest ride's route"
        );
        let page = collect(&store, None, Some("nincs".into()), Vec::new(), None);
        assert_eq!(page.route_id.as_deref(), Some("b"));
        // the route file is not in this store, so the page has rides but no geometry
        assert!(page.route.is_none());
        assert_eq!(page.selected.len(), 1);
    }
}
