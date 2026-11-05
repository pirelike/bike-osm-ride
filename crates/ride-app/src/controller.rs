//! The controller: wires the UI signals, the ride loop ([`RideCore`]), the sensor hub,
//! persistence, file dialogs, toasts and the summary / TCX export together.
//!
//! Everything here runs on the UI thread, inside Dioxus tasks and event handlers. State
//! lives in one [`Inner`] behind a `RefCell` that is leaked once (`Controller` is a
//! `Copy` handle, so it can be used freely in `rsx!` closures). The rule that keeps the
//! `RefCell` safe: a `Controller` method borrows `Inner` exactly once, for the duration
//! of one synchronous call, and never across an `await`.
//!
//! Every call into another workstream's crate (route loader, store, engine, sensor hub,
//! TCX writer) is panic-contained and reports failures as Hungarian toasts, so the
//! window never dies because of them.

use std::cell::RefCell;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use dioxus::desktop::DesktopContext;
use dioxus::prelude::*;
use ride_ble::{spawn_sensor_hub, HubOptions, SensorLink};
use ride_model::{
    ConnState, DeviceKind, GhostStatus, MapStyle, ProfileMode, ProfileView, RideRecord, RideState,
    Route, RouteSource, SensorCommand, SensorEvent, Settings, Theme, Workout, WorkoutLibrary,
    RIDE_RECORD_VERSION,
};
use ride_ui_kit::format::{fmt_km, fmt_m};
use ride_ui_kit::ToastKind;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::MissedTickBehavior;

use crate::browser;
use crate::dock::{Dock, DropTarget, PanelId};
use crate::guard::guarded;
use crate::paths;
use crate::ride_core::{step_dt, FinishedRide, Output, RideCore};
use crate::state::{apply_slot, boot, kind_label, AppState, DialogKind};
use crate::wake;
use ride_route::rides as ride_log;
use ride_sim::ghost::GhostSet;

/// Ride loop period.
pub const TICK: std::time::Duration = std::time::Duration::from_millis(100);
/// The id under which the simulated rider is offered by the sensor hub.
pub const SIM_DEVICE_ID: &str = "sim:rider";
/// A BLE scan that nobody stopped ends after this long.
const SCAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);
const MAX_VIRTUAL_POWER: f64 = 2000.0;
const WINDOW_TITLE: &str = "bike-osm-ride";

/// The docked browser: where its slot is, whether it may be shown, whether it is starting.
struct BrowserHost {
    slot: Option<browser::Slot>,
    visible: bool,
    starting: bool,
}

static BROWSER: Mutex<BrowserHost> = Mutex::new(BrowserHost {
    slot: None,
    visible: true,
    starting: false,
});

/// The native handle of the app window, the parent of the docked Firefox window.
#[cfg(windows)]
fn parent_window(win: &DesktopContext) -> Option<isize> {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    Some(win.window.hwnd())
}

#[cfg(not(windows))]
fn parent_window(_win: &DesktopContext) -> Option<isize> {
    None
}

/// Settings waiting to be written (flushed on exit so a change made <500 ms before closing
/// the window is not lost).
static PENDING_SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

pub fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Writes settings that are still waiting for the debounce timer. Called when the app exits.
pub fn flush_pending_settings() {
    let pending = PENDING_SETTINGS.lock().ok().and_then(|mut p| p.take());
    if let Some(settings) = pending {
        let disk = boot().for_disk(settings);
        let _ = guarded("save_settings (exit)", || {
            ride_route::store::save_settings(&disk)
        });
    }
}

// ───────────────────────────── pure helpers ─────────────────────────────

/// The toast (if any) for a device connection state change.
pub fn state_change_toast(
    kind: DeviceKind,
    prev: &ConnState,
    new: &ConnState,
    name: Option<&str>,
    expected_disconnect: bool,
) -> Option<(ToastKind, String)> {
    let label = kind_label(kind);
    match new {
        ConnState::Connected if *prev != ConnState::Connected => Some((
            ToastKind::Success,
            match name {
                Some(n) if !n.is_empty() => format!("{label} csatlakoztatva: {n}"),
                _ => format!("{label} csatlakoztatva"),
            },
        )),
        ConnState::Failed(reason) if prev != new => Some((
            ToastKind::Error,
            if reason.trim().is_empty() {
                format!("{label}: a kapcsolódás nem sikerült")
            } else {
                format!("{label}: a kapcsolódás nem sikerült ({reason})")
            },
        )),
        ConnState::Disconnected if *prev == ConnState::Connected && !expected_disconnect => Some((
            ToastKind::Warning,
            format!("{label}: megszakadt a kapcsolat"),
        )),
        _ => None,
    }
}

/// "Útvonal betöltve: Name – 24.10 km, 312 m emelkedő"
pub fn route_loaded_text(route: &Route) -> String {
    format!(
        "Útvonal betöltve: {} – {} km, {} m emelkedő",
        route.name,
        fmt_km(route.total_dist),
        fmt_m(route.ascent)
    )
}

pub fn window_title(route: Option<&Route>) -> String {
    match route {
        Some(r) if !r.name.trim().is_empty() => format!("{} – {WINDOW_TITLE}", r.name.trim()),
        _ => WINDOW_TITLE.to_string(),
    }
}

/// Virtual rider power after pressing ↑/↓ or moving the slider.
pub fn clamp_virtual_power(watts: f64) -> f64 {
    if watts.is_finite() {
        watts.clamp(0.0, MAX_VIRTUAL_POWER).round()
    } else {
        0.0
    }
}

/// Whether the test hooks alone make every ride of this session a simulated one: `RIDE_DEMO`
/// and `RIDE_SIM` exist to produce runs without a rider, so their rides never become PRs.
pub fn simulated_hooks() -> bool {
    let hooks = &boot().hooks;
    hooks.demo || hooks.sim_watts.is_some()
}

// ───────────────────────────── state ─────────────────────────────

pub struct Inner {
    st: AppState,
    win: DesktopContext,
    core: RideCore,
    cmd_tx: Option<UnboundedSender<SensorCommand>>,
    last_tick: Instant,
    awake: bool,
    time_scale: f64,
    finished: Option<FinishedRide>,
    /// Slots (by kind) that were connected to the simulated rider.
    sim_kinds: HashSet<DeviceKind>,
    expected_disconnect: HashSet<DeviceKind>,
    save_gen: u64,
    saved: Settings,
    scan_gen: u64,
    hub_lost_notified: bool,
    file_dialog_open: bool,
    /// Sticky per ride: the simulated rider took part (`RIDE_DEMO` / `RIDE_SIM` count as one).
    /// Such a ride is stored but never becomes a PR or the "Best" ghost.
    simulated: bool,
    /// The ghost/PR plan of the loaded route, applied to the engine at the start.
    ghost_plan: Option<GhostSet>,
    /// Generation of the ghost plan, so a plan that arrives after a route change is dropped.
    ghost_gen: u64,
    /// The `Copy` handle of this controller, filled in right after the cell has been leaked:
    /// the async tasks started from an `Inner` method need it to come back after their `await`.
    cell: Option<&'static RefCell<Inner>>,
}

/// `Copy` handle to the controller; see the module docs.
#[derive(Clone, Copy)]
pub struct Controller {
    pub st: AppState,
    inner: &'static RefCell<Inner>,
}

impl Controller {
    /// Creates the (single) controller. Call once from the root component.
    pub fn new(st: AppState, win: DesktopContext) -> Self {
        let b = boot();
        // Workouts imported earlier sit on top of the embedded library, also at start-up, so a
        // selected imported workout is found again after a restart.
        let imported_workouts = ride_route::store::Store::default_location().list_workouts();
        let library = WorkoutLibrary::with_imported(imported_workouts.clone());
        {
            let mut signal = st.imported_workouts;
            signal.set(imported_workouts);
        }
        let mut core = RideCore::new();
        let selected_workout = b
            .settings
            .ui
            .workout_id
            .as_deref()
            .and_then(|id| library.get(id))
            .map(|w| Arc::new(w.clone()));
        let _ = core.set_workout(selected_workout, Instant::now());
        let inner = Inner {
            st,
            win,
            core,
            cmd_tx: None,
            last_tick: Instant::now(),
            awake: false,
            time_scale: b.hooks.time_scale,
            finished: None,
            sim_kinds: HashSet::new(),
            expected_disconnect: HashSet::new(),
            save_gen: 0,
            saved: b.for_disk(b.settings.clone()),
            scan_gen: 0,
            hub_lost_notified: false,
            file_dialog_open: false,
            simulated: false,
            ghost_plan: None,
            ghost_gen: 0,
            cell: None,
        };
        // One controller per process: leaking it makes the handle `Copy`.
        let inner: &'static RefCell<Inner> = Box::leak(Box::new(RefCell::new(inner)));
        inner.borrow_mut().cell = Some(inner);
        Self { st, inner }
    }

    /// Runs `f` on the controller state.
    ///
    /// A panic inside `f` never leaves this function. On Android the whole UI event, render and
    /// asset path of `dioxus-desktop` runs under one global mutex (`android_sync_lock`), taken
    /// with `.lock().unwrap()`: a panic would poison it, and from then on *every* event, render
    /// and tile request panics too — the app dies on the next tap, whatever it is. That is what
    /// turns one unknown bug into "nothing works any more". Catching it here keeps the rest of
    /// the app alive and puts the message in the crash banner instead.
    fn with<R: Default>(self, f: impl FnOnce(&mut Inner) -> R) -> R {
        match guarded("ui", || f(&mut self.inner.borrow_mut())) {
            Ok(value) => value,
            Err(message) => {
                self.st.report_crash(message);
                R::default()
            }
        }
    }

    /// The window handle for the native dialogs and the WebView panels. Not routed through
    /// [`Controller::with`]: `DesktopContext` has no `Default` to fall back to, and a handler that
    /// cannot get the window has nothing left to do.
    fn window(self) -> Option<DesktopContext> {
        guarded("ui.window", || self.inner.borrow_mut().win.clone()).ok()
    }

    // ───── start-up ─────

    /// Starts the sensor hub, the ride loop and the environment test hooks.
    pub fn boot(self) {
        spawn(async move {
            let rx = self.with(|i| {
                i.refresh_recents();
                let boot_settings = i.st.settings.peek().clone();
                i.sync_gear(&boot_settings);
                i.start_hub()
            });
            spawn(async move { self.run(rx).await });
            spawn(async move { self.check_update().await });
            self.run_hooks().await;
        });
    }

    // ───── updates ─────

    /// Looks for a newer GitHub release a few seconds after the start (silent on any failure).
    async fn check_update(self) {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if let Some(update) = crate::updater::check().await {
            let mut slot = self.st.update;
            let text = format!("Elérhető az új verzió: {}.", update.version);
            slot.set(Some(update));
            self.st.toast(ToastKind::Info, text);
        }
    }

    /// The top bar's "Frissítés" button.
    pub fn install_update(self) {
        let Some(update) = self.st.update.peek().clone() else {
            return;
        };
        if *self.st.updating.peek() {
            return;
        }
        #[cfg(target_os = "android")]
        {
            // An app cannot replace itself on Android: the browser downloads the APK and the
            // system installer takes over.
            let url = update.asset_url.clone();
            if let Some(document) = try_consume_context::<std::rc::Rc<dyn document::Document>>() {
                // `dioxus-desktop` `expect`s a live window here; a panic would poison the whole
                // Android UI (see `Controller::with`), and a failed download prompt is survivable.
                let _ = guarded("document::eval", || {
                    document.eval(format!(
                        "window.open({}, '_blank');",
                        serde_json::to_string(&url).unwrap_or_default()
                    ))
                });
            }
            self.st.toast(
                ToastKind::Info,
                format!("Töltsd le és telepítsd az új APK-t: {url}"),
            );
        }
        #[cfg(not(target_os = "android"))]
        {
            let platform =
                crate::updater::Platform::current().unwrap_or(crate::updater::Platform::Windows);
            // A .deb installation (Linux without APPIMAGE) and the macOS .dmg cannot replace
            // themselves: the release page opens and the rider installs the package — the same
            // shape as the Android branch above.
            if crate::updater::current_install_mode(platform)
                == crate::updater::InstallMode::OpenReleasePage
            {
                let opened = crate::updater::open_release_page(&update);
                let hint = crate::updater::manual_install_hint(platform);
                self.st.toast(
                    ToastKind::Info,
                    if opened {
                        hint.to_string()
                    } else {
                        format!("{hint} {}", update.page_url)
                    },
                );
                return;
            }
            let mut updating = self.st.updating;
            updating.set(true);
            self.st.toast(
                ToastKind::Info,
                format!("A {} letöltése folyamatban…", update.version),
            );
            spawn(async move {
                match crate::updater::install(&update, platform).await {
                    Ok(exe) => {
                        flush_pending_settings();
                        match std::process::Command::new(&exe).spawn() {
                            Ok(_) => std::process::exit(0),
                            Err(_) => self.st.toast(
                                ToastKind::Success,
                                "A frissítés telepítve; indítsd újra az alkalmazást.",
                            ),
                        }
                    }
                    Err(err) => {
                        updating.set(false);
                        self.st
                            .toast(ToastKind::Error, format!("A frissítés nem sikerült: {err}"));
                    }
                }
            });
        }
    }

    /// The ride loop (10 Hz) and the sensor event pump.
    async fn run(self, mut rx: Option<UnboundedReceiver<SensorEvent>>) {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        self.with(|i| i.last_tick = Instant::now());
        loop {
            tokio::select! {
                _ = ticker.tick() => self.with(|i| i.tick(Instant::now())),
                event = next_event(&mut rx) => match event {
                    Some(event) => self.on_sensor_event(event),
                    None => {
                        rx = None;
                        self.with(|i| i.hub_lost());
                    }
                },
            }
        }
    }

    /// `RIDE_*` environment hooks (see `hooks.rs`).
    async fn run_hooks(self) {
        let hooks = &boot().hooks;
        if let Some(path) = hooks.gpx.clone() {
            self.run_load(ride_route::load_route_from_path(path, true))
                .await;
        } else if hooks.demo || hooks.sim_watts.is_some() {
            self.load_demo();
        }
        if let Some(watts) = hooks.sim_watts {
            self.with(|i| i.connect_simulated(watts));
            if self.st.route.peek().is_some() {
                // The ghost/PR plan of the route is read on a blocking thread; a scripted start
                // would otherwise always race it and begin without the ghost.
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                self.start();
            }
        }
        if let Some(dialog) = hooks.dialog {
            self.open_dialog(dialog);
        }
        if hooks.review {
            self.review_open();
        }
    }

    // ───── ride controls ─────

    pub fn start(self) {
        self.with(|i| i.start());
    }
    pub fn pause(self) {
        self.with(|i| i.pause());
    }
    pub fn finish(self) {
        self.with(|i| i.finish());
    }
    pub fn reset(self) {
        self.with(|i| i.reset());
    }
    /// Space: start / pause / resume.
    pub fn toggle_ride(self) {
        self.with(|i| {
            if i.core.state() == RideState::Riding {
                i.pause()
            } else {
                i.start()
            }
        });
    }
    /// Click on the elevation profile (ignored by the engine while riding).
    pub fn seek(self, dist_m: f64) {
        self.with(|i| i.seek(dist_m));
    }

    // ───── routes ─────

    /// Android has no native file dialog: the system picker is started through the activity
    /// (`android/MainActivity.kt`) and the answer arrives later, see `shell::use_android_picker`.
    #[cfg(target_os = "android")]
    pub fn pick_file(self) {
        if let Err(err) = crate::android_picker::pick_gpx() {
            self.st.toast(
                ToastKind::Error,
                format!("A fájlválasztó nem nyílt meg: {err}"),
            );
        }
    }

    /// Native "open" dialog, then loads the chosen file.
    #[cfg(not(target_os = "android"))]
    pub fn pick_file(self) {
        if self.with(|i| std::mem::replace(&mut i.file_dialog_open, true)) {
            return;
        }
        let Some(win) = self.window() else {
            // no window, no dialog: do not leave the flag set and lock the next attempt out
            self.with(|i| i.file_dialog_open = false);
            return;
        };
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("GPX-útvonal megnyitása")
                .add_filter("GPX-útvonal", &["gpx"])
                .add_filter("Minden fájl", &["*"])
                .set_parent(&*win.window)
                .pick_file()
                .await;
            self.with(|i| i.file_dialog_open = false);
            if let Some(file) = picked {
                self.open_path(file.path().to_path_buf());
            }
        });
    }

    /// Loads a GPX file (dialog, drag & drop, `RIDE_GPX`).
    pub fn open_path(self, path: PathBuf) {
        spawn(async move {
            self.run_load(ride_route::load_route_from_path(path, true))
                .await
        });
    }

    // ───── workout import ─────

    /// Native "open" dialog for a workout file (MyWhoosh / Zwift), then imports it.
    #[cfg(not(target_os = "android"))]
    pub fn import_workout(self) {
        if self.with(|i| std::mem::replace(&mut i.file_dialog_open, true)) {
            return;
        }
        let Some(win) = self.window() else {
            // no window, no dialog: do not leave the flag set and lock the next attempt out
            self.with(|i| i.file_dialog_open = false);
            return;
        };
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Edzés importálása")
                .add_filter("MyWhoosh / Zwift edzés", &["xml", "zwo", "json"])
                .add_filter("Minden fájl", &["*"])
                .set_parent(&*win.window)
                .pick_file()
                .await;
            self.with(|i| i.file_dialog_open = false);
            if let Some(file) = picked {
                self.with(|i| i.import_workout_path(file.path().to_path_buf()));
            }
        });
    }

    /// Android has no native file dialog (see [`Controller::pick_file`]).
    #[cfg(target_os = "android")]
    pub fn import_workout(self) {
        self.st.toast(
            ToastKind::Info,
            "Androidon az edzésimport még nem elérhető.",
        );
    }

    /// Imports a bikerouter.de link (or raw BRouter URL).
    pub fn load_link(self, link: String) {
        let link = link.trim().to_string();
        if link.is_empty() {
            self.st
                .welcome_error_set(Some("Illessz be egy bikerouter-linket.".to_string()));
            return;
        }
        spawn(async move {
            self.run_load(async move { ride_route::load_route_from_bikerouter(&link).await })
                .await
        });
    }

    pub fn load_demo(self) {
        self.with(|i| {
            if !i.can_replace_route() {
                return;
            }
            match guarded("demo_route", ride_route::demo_route) {
                Ok(route) => i.install_route(route),
                Err(_) => i.fail_load("A bemutató útvonal most nem érhető el.".to_string()),
            }
        });
    }

    pub fn open_recent(self, id: String) {
        self.with(|i| {
            if !i.can_replace_route() {
                return;
            }
            match guarded("store::load_recent", || ride_route::store::load_recent(&id)) {
                Ok(Some(route)) => i.install_route(route),
                Ok(None) => {
                    i.fail_load(
                        "Ez az útvonal már nem érhető el, ezért eltávolítottuk a listából."
                            .to_string(),
                    );
                    i.delete_recent(&id);
                }
                Err(_) => i.fail_load("A legutóbbi útvonal betöltése nem sikerült.".to_string()),
            }
        });
    }

    pub fn delete_recent(self, id: String) {
        self.with(|i| i.delete_recent(&id));
    }

    /// Summary "Új út": drops the finished route and asks for the next one.
    pub fn new_ride(self) {
        let unloaded = self.with(|i| {
            let allowed = i.can_replace_route();
            if allowed {
                i.unload_route();
            }
            allowed
        });
        if unloaded {
            self.open_dialog(DialogKind::Route);
        }
    }

    /// Loads the route produced by `fut` without blocking the UI thread. Shows the
    /// welcome-screen spinner and turns every failure into a message.
    async fn run_load<F>(self, fut: F)
    where
        F: Future<Output = ride_route::Result<Route>> + Send + 'static,
    {
        let allowed = self.with(|i| {
            if *i.st.busy.peek() {
                i.toast(
                    ToastKind::Info,
                    "Egy útvonal betöltése már folyamatban van.",
                );
                return false;
            }
            i.can_replace_route()
        });
        if !allowed {
            return;
        }
        let (mut busy, mut error) = (self.st.busy, self.st.welcome_error);
        busy.set(true);
        error.set(None);
        if self.st.route.peek().is_some() {
            self.st.toast(ToastKind::Info, "Útvonal betöltése…");
        }
        // The route pipeline may take a while on big files: run it on a worker thread.
        let joined = tokio::spawn(fut).await;
        busy.set(false);
        match joined {
            Ok(Ok(route)) => self.with(|i| i.install_route(route)),
            Ok(Err(err)) => self.with(|i| i.fail_load(err.to_string())),
            Err(join) => {
                let text = if join.is_panic() {
                    "Az útvonal feldolgozása közben belső hiba történt."
                } else {
                    "Az útvonal betöltése megszakadt."
                };
                self.with(|i| i.fail_load(text.to_string()));
            }
        }
    }

    // ───── settings ─────

    pub fn apply_settings(self, settings: Settings) {
        if self.with(|i| i.apply_settings(settings)) {
            self.schedule_save();
        }
    }

    fn update_settings(self, change: impl FnOnce(&mut Settings)) {
        let mut next = self.st.settings.peek().clone();
        change(&mut next);
        self.apply_settings(next);
    }

    pub fn toggle_theme(self) {
        self.update_settings(|s| {
            s.ui.theme = match s.ui.theme {
                Theme::Dark => Theme::Light,
                Theme::Light => Theme::Dark,
            };
            // Keep the CARTO tiles in step with the theme; the OSM style is a deliberate choice.
            s.ui.map_style = match (s.ui.map_style, s.ui.theme) {
                (MapStyle::Osm, _) => MapStyle::Osm,
                (_, Theme::Dark) => MapStyle::Dark,
                (_, Theme::Light) => MapStyle::Light,
            };
        });
    }

    pub fn toggle_follow(self) {
        self.update_settings(|s| s.ui.follow_rider = !s.ui.follow_rider);
    }

    pub fn set_follow(self, follow: bool) {
        self.update_settings(|s| s.ui.follow_rider = follow);
    }

    pub fn set_map_style(self, style: MapStyle) {
        self.update_settings(|s| s.ui.map_style = style);
    }

    // ───── browser panel (docked Firefox) ─────

    /// Starts Firefox with the app's own profile and docks its window into the app window.
    /// Does nothing while one is running or starting.
    pub fn browser_start(self) {
        if browser::is_docked()
            || std::mem::replace(&mut BROWSER.lock().expect("lock").starting, true)
        {
            return;
        }
        let mut note = self.st.browser_note;
        let Some(exe) = browser::firefox_path() else {
            BROWSER.lock().expect("lock").starting = false;
            note.set(Some(
                "A Firefox nincs telepítve. Telepítsd a mozilla.org/firefox oldalról, majd nyisd meg újra ezt a panelt."
                    .to_string(),
            ));
            return;
        };
        let parent = self.with(|i| parent_window(&i.win));
        let profile = paths::data_dir().join("firefox-profile");
        note.set(Some("A Firefox indul…".to_string()));
        spawn(async move {
            let before = browser::firefox_windows();
            if let Err(err) = browser::launch(&exe, &profile) {
                eprintln!("[ride-app] firefox: {err}");
                BROWSER.lock().expect("lock").starting = false;
                note.set(Some(format!("A Firefox nem indult el: {err}")));
                return;
            }
            // The window shows up a moment later (the launcher process may hand over to another
            // one), so the new Firefox window is the one that was not there before.
            for _ in 0..160 {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let slot = BROWSER.lock().expect("lock").slot;
                let found = browser::firefox_windows()
                    .into_iter()
                    .find(|h| !before.contains(h));
                if let (Some(hwnd), Some(slot), Some(parent)) = (found, slot, parent) {
                    browser::dock(hwnd, parent, slot);
                    {
                        let mut host = BROWSER.lock().expect("lock");
                        host.starting = false;
                        if !host.visible {
                            browser::hide();
                        }
                    }
                    note.set(None);
                    return;
                }
            }
            BROWSER.lock().expect("lock").starting = false;
            note.set(Some("A Firefox ablaka nem jelent meg.".to_string()));
        });
    }

    /// Where the panel's body is now (`None`: the panel is gone or too small).
    pub fn browser_place(self, slot: Option<browser::Slot>) {
        let visible = {
            let mut host = BROWSER.lock().expect("lock");
            host.slot = slot;
            host.visible
        };
        match slot {
            Some(slot) => browser::place(slot, visible),
            None => browser::hide(),
        }
    }

    /// Firefox is covered by a dialog or a drag: hide it (`false`) or show it again.
    pub fn browser_visible(self, visible: bool) {
        let slot = {
            let mut host = BROWSER.lock().expect("lock");
            host.visible = visible;
            host.slot
        };
        match (slot, visible) {
            (Some(slot), true) => browser::place(slot, true),
            _ => browser::hide(),
        }
    }

    /// The panel was closed: so is Firefox.
    pub fn browser_stop(self) {
        {
            let mut host = BROWSER.lock().expect("lock");
            host.slot = None;
            host.starting = false;
        }
        browser::close();
        let mut note = self.st.browser_note;
        note.set(None);
    }

    // ───── panels ─────

    /// Closes a panel (its close button): the neighbour takes the space, a chip appears in the
    /// top bar.
    pub fn hide_panel(self, id: PanelId) {
        let mut dock = self.st.dock.peek().clone();
        dock.hide(id);
        self.save_dock(dock, true);
    }

    /// Brings a closed panel back: where it was dropped, or (a click on its chip) where it was.
    pub fn show_panel(self, id: PanelId, at: Option<DropTarget>) {
        let mut dock = self.st.dock.peek().clone();
        dock.show(id, at, self.st.availability());
        self.save_dock(dock, true);
    }

    /// Moves a shown panel next to another one (or swaps the two).
    pub fn move_panel(self, id: PanelId, target: DropTarget) {
        let mut dock = self.st.dock.peek().clone();
        dock.relocate(id, target, self.st.availability());
        self.save_dock(dock, true);
    }

    /// Street View on the map tile on/off (persisted; the panel and its id prefetcher live only
    /// while it is on).
    pub fn set_streetview(self, on: bool) {
        self.update_settings(|s| s.ui.streetview_enabled = on);
    }

    pub fn toggle_streetview(self) {
        self.update_settings(|s| s.ui.streetview_enabled = !s.ui.streetview_enabled);
    }

    /// Swaps the big and the small view of the map tile. Turns Street View on (and big) when off.
    pub fn toggle_streetview_size(self) {
        self.update_settings(|s| {
            if !s.ui.streetview_enabled {
                s.ui.streetview_enabled = true;
                s.ui.streetview_large = true;
            } else {
                s.ui.streetview_large = !s.ui.streetview_large;
            }
        });
    }

    /// A sensor event. Gear steps need the settings and the save, so the controller handles
    /// them; everything else goes to the inner state.
    fn on_sensor_event(self, event: SensorEvent) {
        match event {
            SensorEvent::Shift(delta) => self.on_shift(delta),
            other => self.with(|i| i.on_event(other)),
        }
    }

    /// A gear step from a Zwift controller. The first step switches the virtual gearbox on,
    /// since a shifter does nothing without it, and says so once.
    fn on_shift(self, delta: i32) {
        if !self.st.settings.peek().ui.virtual_gears {
            self.update_settings(|s| s.ui.virtual_gears = true);
            self.st.toast(
                ToastKind::Success,
                "Zwift váltó csatlakoztatva, a virtuális váltót bekapcsoltuk.",
            );
        }
        self.shift_gear(delta);
    }

    /// Shifts the virtual gearbox one gear up (`+1`, harder) or down (`-1`, easier).
    /// Ignored while a workout holds the trainer in ERG: the gear would change nothing there
    /// but would surprise the rider after the workout.
    pub fn shift_gear(self, delta: i32) {
        let s = self.st.settings.peek().clone();
        if !s.ui.virtual_gears || *self.st.erg.peek() {
            return;
        }
        let now = crate::gearbox::index_from_saved(s.ui.gear);
        let next = crate::gearbox::shifted(now, delta);
        if next != now {
            self.update_settings(|s| s.ui.gear = next as u8);
        }
    }

    /// Persists the tiling the user made and applies it. `now` writes the file at once (a drag
    /// lives in a component that unmounts on mouse-up, and a debounced save spawned from there
    /// would be cancelled with it); otherwise it follows the usual debounce.
    pub fn save_dock(self, dock: Dock, now: bool) {
        let mut dock_sig = self.st.dock;
        if *dock_sig.peek() != dock {
            dock_sig.set(dock.clone());
        }
        // into the layout of the situation in force: with a workout selected that is the
        // workout's own, so the free-ride arrangement stays as it was
        self.update_settings(|s| s.ui.set_active_tiles(dock.to_settings()));
        if now {
            self.with(|i| i.flush_settings());
        }
    }

    pub fn toggle_profile_mode(self) {
        self.update_settings(|s| {
            let next = match s.ui.active_profile_mode() {
                ProfileMode::Full => ProfileMode::Lookahead,
                ProfileMode::Lookahead => ProfileMode::Full,
            };
            s.ui.set_active_profile_mode(next);
        });
    }

    pub fn set_profile_mode(self, mode: ProfileMode) {
        self.update_settings(|s| s.ui.set_active_profile_mode(mode));
    }

    pub fn set_profile_view(self, view: ProfileView) {
        self.update_settings(|s| s.ui.profile_view = view);
    }

    /// A page of the phone's profile was chosen (by swiping or by its dot): view and mode at
    /// once, and the swipe hint has done its job.
    pub fn set_profile_page(self, view: ProfileView, mode: ProfileMode) {
        self.update_settings(|s| {
            s.ui.profile_view = view;
            s.ui.set_active_profile_mode(mode);
            s.ui.profile_swipe_learned = true;
        });
    }

    /// Settings are written 500 ms after the last change.
    fn schedule_save(self) {
        let gen = self.with(|i| {
            i.save_gen += 1;
            i.save_gen
        });
        if let Ok(mut pending) = PENDING_SETTINGS.lock() {
            *pending = Some(self.st.settings.peek().clone());
        }
        spawn(async move {
            tokio::time::sleep(SAVE_DEBOUNCE).await;
            self.with(|i| {
                if i.save_gen == gen {
                    i.flush_settings();
                }
            });
        });
    }

    // ───── sensors ─────

    /// A command from the devices dialog / virtual rider panel.
    pub fn command(self, cmd: SensorCommand) {
        // Android: the permission dialog is easy to dismiss at startup; ask again when the rider
        // actually wants to search (the activity is the only one who can show it).
        #[cfg(target_os = "android")]
        if matches!(cmd, SensorCommand::StartScan) {
            crate::android_picker::request_ble_permissions();
        }
        if let Some(scan) = self.with(|i| i.command(cmd)) {
            spawn(async move {
                tokio::time::sleep(SCAN_TIMEOUT).await;
                self.with(|i| {
                    if i.scan_gen == scan && *i.st.scanning.peek() {
                        i.command(SensorCommand::StopScan);
                    }
                });
            });
        }
    }

    pub fn set_virtual_power(self, watts: f64) {
        self.with(|i| i.set_virtual_power(watts));
    }

    /// Picks (or clears) the workout from the picker. Ignored during a ride.
    pub fn select_workout(self, id: Option<String>) {
        let changed = self.with(|i| {
            if i.core.state() == RideState::Riding {
                i.toast(
                    ToastKind::Warning,
                    "Tekerés közben nem lehet edzést váltani. Előbb tarts szünetet.",
                );
                return false;
            }
            let workout: Option<Arc<Workout>> = match id.as_deref() {
                Some(id) => {
                    let library =
                        WorkoutLibrary::with_imported(i.st.imported_workouts.peek().clone());
                    match library.get(id) {
                        Some(w) => Some(Arc::new(w.clone())),
                        None => {
                            i.toast(ToastKind::Error, "Ez az edzés nem található.");
                            return false;
                        }
                    }
                }
                None => None,
            };
            let out = i.core.set_workout(workout, Instant::now());
            i.apply(out);
            true
        });
        if changed {
            self.update_settings(|s| s.ui.workout_id = id);
        }
    }

    /// ↑ / ↓ keys: ±10 W for the simulated rider.
    pub fn adjust_virtual_power(self, delta: f64) {
        self.with(|i| {
            if *i.st.sim_connected.peek() {
                let next = *i.st.virtual_power.peek() + delta;
                i.set_virtual_power(next);
            }
        });
    }

    // ───── dialogs, window, keyboard ─────

    pub fn open_dialog(self, dialog: DialogKind) {
        if dialog == DialogKind::Route && !*self.st.busy.peek() {
            // an old failure should not greet the next attempt
            self.st.welcome_error_set(None);
        }
        let mut d = self.st.dialog;
        d.set(Some(dialog));
    }

    pub fn close_dialog(self) {
        let mut d = self.st.dialog;
        d.set(None);
    }

    pub fn toggle_fullscreen(self) {
        self.with(|i| i.toggle_fullscreen());
    }

    /// Keys forwarded by the page script (see `shell.rs`).
    pub fn on_key(self, key: &str) {
        let open_now = *self.st.dialog.peek();
        let dialog_open = open_now.is_some();
        let route_dialog = open_now == Some(DialogKind::Route);
        // the review page covers the ride screen: its keys must not start or steer a ride
        let review_open = *self.st.review.open.peek();
        match key {
            "esc" => {
                if dialog_open {
                    self.close_dialog();
                } else if review_open {
                    self.review_close();
                } else if *self.st.fullscreen.peek() {
                    self.toggle_fullscreen();
                }
            }
            "f11" => self.toggle_fullscreen(),
            "ctrl+o" if !review_open && (!dialog_open || route_dialog) => self.pick_file(),
            _ if dialog_open || review_open => {}
            "space" => self.toggle_ride(),
            "m" => self.toggle_follow(),
            "p" => self.toggle_profile_mode(),
            "t" => self.toggle_theme(),
            "v" => self.toggle_streetview(),
            "x" => self.toggle_streetview_size(),
            "right" => self.shift_gear(1),
            "left" => self.shift_gear(-1),
            "w" => self.open_dialog(DialogKind::Workouts),
            "up" => self.adjust_virtual_power(10.0),
            "down" => self.adjust_virtual_power(-10.0),
            _ => {}
        }
    }

    // ───── summary / export ─────

    /// "Mentés TCX-ként": native save dialog, then the file is written.
    pub fn save_tcx(self) {
        let Some(finished) = self.with(|i| i.finished.clone()) else {
            self.st.toast(ToastKind::Warning, "Nincs mentendő túra.");
            return;
        };
        if self.with(|i| std::mem::replace(&mut i.file_dialog_open, true)) {
            return;
        }
        let Some(win) = self.window() else {
            // no window, no dialog: do not leave the flag set and lock the next attempt out
            self.with(|i| i.file_dialog_open = false);
            return;
        };
        spawn(async move {
            let rendered = guarded("tcx", || {
                (
                    ride_sim::tcx::to_tcx(&finished.summary, &finished.points),
                    ride_sim::tcx::suggested_filename(&finished.summary),
                )
            });
            let Ok((xml, suggested)) = rendered else {
                self.with(|i| i.file_dialog_open = false);
                self.st
                    .toast(ToastKind::Error, "A TCX-fájl előállítása nem sikerült.");
                return;
            };
            #[cfg(not(target_os = "android"))]
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Túra mentése TCX-fájlként")
                .set_file_name(suggested)
                .add_filter("TCX (Garmin)", &["tcx"])
                .set_parent(&*win.window)
                .save_file()
                .await
                .map(|file| file.path().to_path_buf());
            // No save dialog on Android: the shared Download folder, else the app's own folder.
            #[cfg(target_os = "android")]
            let picked = {
                let _ = &win;
                Some(crate::paths::export_dir().join(suggested))
            };
            self.with(|i| i.file_dialog_open = false);
            let Some(mut path) = picked else { return };
            if path.extension().is_none() {
                path.set_extension("tcx");
            }
            match tokio::fs::write(&path, xml.as_bytes()).await {
                Ok(()) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.st.toast(ToastKind::Success, format!("Mentve: {name}"));
                }
                Err(err) => self
                    .st
                    .toast(ToastKind::Error, format!("A mentés nem sikerült: {err}")),
            }
        });
    }
}

/// Next sensor event, or never if the hub is gone.
async fn next_event(rx: &mut Option<UnboundedReceiver<SensorEvent>>) -> Option<SensorEvent> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

// ───────────────────────────── Inner: synchronous logic ─────────────────────────────

impl Inner {
    /// The `Copy` controller handle (see [`Inner::cell`]).
    fn controller(&self) -> Controller {
        Controller {
            st: self.st,
            inner: self
                .cell
                .expect("the controller cell is set in Controller::new"),
        }
    }

    /// Parses one workout file, stores it and adds it to the library.
    fn import_workout_path(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "edzés".to_string());
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("[ride-app] workout import: {err}");
                self.toast(ToastKind::Error, "A fájlt nem sikerült megnyitni.");
                return;
            }
        };
        let workout = match ride_sim::workout_import::from_bytes(&bytes, &name) {
            Ok(workout) => workout,
            Err(message) => {
                self.toast(ToastKind::Error, message);
                return;
            }
        };
        if let Err(err) = ride_route::store::Store::default_location().save_workout(&workout) {
            eprintln!("[ride-app] save_workout: {err}");
            self.toast(ToastKind::Error, "Az edzést nem sikerült elmenteni.");
            return;
        }
        let mut imported = self.st.imported_workouts.peek().clone();
        imported.retain(|other| other.id != workout.id);
        imported.push(workout.clone());
        imported.sort_by(|a, b| a.name.cmp(&b.name));
        let mut signal = self.st.imported_workouts;
        signal.set(imported);
        self.toast(
            ToastKind::Success,
            format!(
                "„{}” importálva: {} blokk, {} perc.",
                workout.name,
                workout.steps.len(),
                workout.total_seconds() / 60
            ),
        );
    }

    fn toast(&self, kind: ToastKind, text: impl Into<String>) {
        self.st.toast(kind, text);
    }

    // ───── sensor hub ─────

    fn start_hub(&mut self) -> Option<UnboundedReceiver<SensorEvent>> {
        // Always on (rotated at 8 MB): the only evidence when a trainer misbehaves at home.
        let hex_log = Some(paths::ble_log_path());
        if let Some(dir) = hex_log.as_ref().and_then(|p| p.parent()) {
            let _ = std::fs::create_dir_all(dir);
        }
        // `..Default::default()` keeps this compiling when the BLE crate adds option fields.
        #[allow(clippy::needless_update)]
        let options = HubOptions {
            hex_log,
            ..Default::default()
        };
        match guarded("spawn_sensor_hub", || spawn_sensor_hub(options)) {
            Ok(SensorLink { tx, rx }) => {
                self.cmd_tx = Some(tx);
                Some(rx)
            }
            Err(_) => {
                self.toast(
                    ToastKind::Error,
                    "A Bluetooth-szolgáltatás nem indult el, ezért eszközök most nem csatlakoztathatók.",
                );
                None
            }
        }
    }

    fn hub_lost(&mut self) {
        self.cmd_tx = None;
        if !std::mem::replace(&mut self.hub_lost_notified, true) {
            self.toast(
                ToastKind::Error,
                "A szenzorkezelő leállt. Indítsd újra az alkalmazást az eszközök használatához.",
            );
        }
    }

    /// Silent send (internal traffic). `false` = there is no hub.
    fn send(&mut self, cmd: SensorCommand) -> bool {
        match &self.cmd_tx {
            Some(tx) => tx.send(cmd).is_ok(),
            None => false,
        }
    }

    /// A command from the UI. Returns the scan generation when a scan was started.
    fn command(&mut self, cmd: SensorCommand) -> Option<u64> {
        let mut started = None;
        let mut scanning = self.st.scanning;
        match &cmd {
            SensorCommand::StartScan => {
                self.scan_gen += 1;
                started = Some(self.scan_gen);
                scanning.set(true);
            }
            SensorCommand::StopScan => scanning.set(false),
            SensorCommand::Connect { id, kind } => {
                if *scanning.peek() {
                    self.send(SensorCommand::StopScan);
                    scanning.set(false);
                }
                if id == SIM_DEVICE_ID {
                    self.sim_kinds.insert(*kind);
                } else {
                    self.sim_kinds.remove(kind);
                }
                self.expected_disconnect.remove(kind);
            }
            SensorCommand::Disconnect(kind) => {
                self.expected_disconnect.insert(*kind);
                self.sim_kinds.remove(kind);
            }
            _ => {}
        }
        if !self.send(cmd) {
            scanning.set(false);
            self.toast(
                ToastKind::Warning,
                "A Bluetooth-szolgáltatás nem érhető el.",
            );
            return None;
        }
        started
    }

    fn connect_simulated(&mut self, watts: f64) {
        let watts = clamp_virtual_power(watts);
        let mut vp = self.st.virtual_power;
        vp.set(watts);
        self.command(SensorCommand::Connect {
            id: SIM_DEVICE_ID.to_string(),
            kind: DeviceKind::Trainer,
        });
        self.send(SensorCommand::SetVirtualPower(watts));
    }

    fn set_virtual_power(&mut self, watts: f64) {
        let watts = clamp_virtual_power(watts);
        let mut vp = self.st.virtual_power;
        if (*vp.peek() - watts).abs() > f64::EPSILON {
            vp.set(watts);
        }
        self.send(SensorCommand::SetVirtualPower(watts));
    }

    fn on_event(&mut self, event: SensorEvent) {
        match event {
            SensorEvent::Discovered(list) => {
                let mut d = self.st.discovered;
                d.set(list);
            }
            SensorEvent::State { kind, state, name } => self.on_device_state(kind, state, name),
            SensorEvent::Sample(sample) => self.core.set_sample(sample),
            SensorEvent::Notice(text) => self.toast(ToastKind::Info, text),
            // routed by `Controller::on_sensor_event`, which owns the settings
            SensorEvent::Shift(_) => {}
        }
    }

    fn on_device_state(&mut self, kind: DeviceKind, state: ConnState, name: Option<String>) {
        let prev = self
            .st
            .slots
            .peek()
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.state.clone())
            .unwrap_or(ConnState::Disconnected);
        let expected = state == ConnState::Disconnected && self.expected_disconnect.remove(&kind);
        if let Some((tone, text)) =
            state_change_toast(kind, &prev, &state, name.as_deref(), expected)
        {
            self.toast(tone, text);
        }
        let mut slots = self.st.slots;
        apply_slot(&mut slots.write(), kind, state.clone(), name);

        match state {
            ConnState::Connected => {
                if self.sim_kinds.contains(&kind) {
                    let watts = *self.st.virtual_power.peek();
                    self.send(SensorCommand::SetVirtualPower(watts));
                }
                if kind == DeviceKind::Trainer {
                    // A (re)connected trainer needs the current state again.
                    self.core.resend_simulation();
                    if self.core.state() == RideState::Riding {
                        self.send(SensorCommand::TrainerStart);
                    }
                }
            }
            ConnState::Disconnected | ConnState::Failed(_) => {
                self.sim_kinds.remove(&kind);
            }
            ConnState::Connecting => {}
        }
        let sim_connected = self
            .st
            .slots
            .peek()
            .iter()
            .any(|s| s.state == ConnState::Connected && self.sim_kinds.contains(&s.kind));
        // A virtual rider joining during the ride makes the whole ride a simulated one.
        if sim_connected && matches!(self.core.state(), RideState::Riding | RideState::Paused) {
            self.simulated = true;
        }
        let mut flag = self.st.sim_connected;
        if *flag.peek() != sim_connected {
            flag.set(sim_connected);
        }
    }

    // ───── ride loop ─────

    fn tick(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let out = self
            .core
            .tick(step_dt(elapsed, self.time_scale), unix_now(), now);
        self.apply(out);
    }

    fn start(&mut self) {
        match self.core.state() {
            RideState::Ready => {
                // A ride counts as simulated when the virtual rider takes part, or when a test
                // hook loaded the demo route / started the simulated rider.
                self.simulated = simulated_hooks() || *self.st.sim_connected.peek();
                // The ghost plan may have arrived while the previous ride was still running.
                self.core.set_ghost(self.ghost_plan.clone());
                if self.core.workout().is_some() && !self.trainer_controllable() {
                    self.toast(
                        ToastKind::Warning,
                        "Az edzés célwattját nincs mire küldeni: nincs Trainer szerepben csatlakoztatott okos trainer. Az Eszközök ablakban a trainert Trainer szerepben csatlakoztasd (a teljesítménymérő szerep csak adatot olvas).",
                    );
                }
                let out = self.core.start(unix_now(), Instant::now());
                self.apply(out);
            }
            RideState::Paused => {
                let out = self.core.start(unix_now(), Instant::now());
                self.apply(out);
            }
            RideState::Riding => {}
            // The ride is over: Space brings the summary (and its TCX export) back.
            RideState::Finished => {
                let mut dialog = self.st.dialog;
                dialog.set(Some(DialogKind::Summary));
            }
            RideState::Empty => self.toast(ToastKind::Info, "Először válassz egy útvonalat."),
        }
    }

    /// A trainer (or the simulated rider) is connected in the Trainer role, so ERG targets and
    /// grades have a receiver. A trainer connected only as a power meter is read, not driven.
    fn trainer_controllable(&self) -> bool {
        *self.st.sim_connected.peek()
            || self
                .st
                .slots
                .peek()
                .iter()
                .any(|s| s.kind == DeviceKind::Trainer && s.state == ConnState::Connected)
    }

    fn pause(&mut self) {
        let out = self.core.pause(Instant::now());
        self.apply(out);
    }

    fn finish(&mut self) {
        let out = self.core.finish(Instant::now());
        self.apply(out);
    }

    fn reset(&mut self) {
        self.finished = None;
        let mut summary = self.st.summary;
        summary.set(None);
        let mut comparison = self.st.comparison;
        comparison.set(None);
        let out = self.core.reset(Instant::now());
        self.apply(out);
    }

    fn seek(&mut self, dist_m: f64) {
        let out = self.core.seek(dist_m, Instant::now());
        self.apply(out);
    }

    /// Pushes an engine result into the signals and the sensor hub.
    fn apply(&mut self, out: Output) {
        let Output {
            snapshot,
            commands,
            history,
            transition,
            finished,
            workout_finished,
            failure,
        } = out;

        if failure.is_some() {
            self.toast(
                ToastKind::Error,
                "A szimuláció belső hiba miatt leállt. Töltsd be újra az útvonalat.",
            );
            self.drop_route_signals();
        }
        // ERG while a selected workout is not finished: the gearbox is hidden meanwhile
        let erg = snapshot.workout.as_ref().is_some_and(|w| !w.finished);
        if *self.st.erg.peek() != erg {
            let mut e = self.st.erg;
            e.set(erg);
        }
        if *self.st.snapshot.peek() != snapshot {
            let mut s = self.st.snapshot;
            s.set(snapshot);
        }
        if let Some(history) = history {
            let mut h = self.st.history;
            h.set(Arc::new(history));
        }
        for cmd in commands {
            self.send(cmd);
        }
        if let Some((_, to)) = transition {
            self.set_awake(to == RideState::Riding);
        }
        if workout_finished {
            self.toast(
                ToastKind::Success,
                "Az edzés véget ért, a trainer ismét a terepet követi.",
            );
        }
        if let Some(finished) = finished {
            self.on_finished(finished);
        }
    }

    fn on_finished(&mut self, finished: FinishedRide) {
        let reached_end = self.st.snapshot.peek().remaining_dist < 5.0;
        let mut summary = self.st.summary;
        summary.set(Some(finished.summary.clone()));
        // The dialog opens now; its comparison arrives when the ride has been stored, so a
        // stale comparison of the previous ride must not be shown in between.
        let mut comparison = self.st.comparison;
        comparison.set(None);
        self.finished = Some(finished.clone());
        let mut dialog = self.st.dialog;
        dialog.set(Some(DialogKind::Summary));
        self.toast(
            ToastKind::Success,
            if reached_end {
                "Célba értél! Nézd meg az összegzést."
            } else {
                "A túra véget ért. Nézd meg az összegzést."
            },
        );
        self.store_finished_ride(finished);
    }

    /// Stores the finished ride in the ride log and publishes its comparison with the history.
    /// Both run on a blocking thread: a two-hour ride is a few thousand points and the index is
    /// rewritten. A failure is a warning only — the TCX export of the finished ride still works.
    fn store_finished_ride(&mut self, finished: FinishedRide) {
        let Some(route) = self.st.route.peek().clone() else {
            return;
        };
        let record = RideRecord {
            version: RIDE_RECORD_VERSION,
            route_id: route.id.clone(),
            route_name: finished.summary.route_name.clone(),
            simulated: self.simulated,
            summary: finished.summary,
            points: finished.points,
            source: ride_model::RideSource::Recorded,
            power_estimated: false,
            estimate: None,
        };
        // The comparison must use the PRs *before* this ride is saved, so the read, the compare
        // and the write happen in this order on the same blocking thread.
        let ghost_delta_s = self
            .st
            .snapshot
            .peek()
            .ghost
            .as_ref()
            .and_then(|ghost| ghost.delta_s);
        let mut comparison = self.st.comparison;
        let saved_route = route;
        let ctl = self.controller();
        spawn(async move {
            let joined = tokio::task::spawn_blocking(move || {
                let store = ride_route::store::Store::default_location();
                let previous = ride_log::list_rides(&saved_route.id);
                let best = ride_sim::segments::best_times(&previous);
                let comparison =
                    crate::history::compare(&record, &saved_route, &best, ghost_delta_s);
                let saved = store.save_ride(&record, &saved_route);
                (comparison, saved)
            })
            .await;
            match joined {
                Ok((value, saved)) => {
                    comparison.set(Some(value));
                    match saved {
                        Ok(Some(meta)) => ctl.st.toast(
                            ToastKind::Success,
                            if meta.simulated {
                                "Menet elmentve a tekerés-naplóba. Szimulált menet: nem lesz PR és nem lehet Legjobb szellem."
                            } else {
                                "Menet elmentve a tekerés-naplóba: a következő menetnél már szellemként megy előtted."
                            },
                        ),
                        // Too short to keep: say so, otherwise the missing ghost looks broken.
                        Ok(None) => ctl.st.toast(
                            ToastKind::Info,
                            "A menet rövid volt a naplózáshoz (legalább 1 perc és 200 m kell), ezért nem lesz belőle szellem.",
                        ),
                        Err(err) => {
                            eprintln!("[ride-app] save_ride: {err}");
                            ctl.st.toast(
                                ToastKind::Warning,
                                "A menetet nem sikerült a tekerés-naplóba menteni.",
                            );
                        }
                    }
                }
                Err(_) => ctl.st.toast(
                    ToastKind::Warning,
                    "A menetet nem sikerült a tekerés-naplóba menteni.",
                ),
            }
            // The next ride should see the PRs and the ghost of the ride that just ended.
            ctl.with(|i| i.reload_ghost());
        });
    }

    /// Rebuilds the ghost and the PR times of the loaded route (off the UI thread).
    /// Ignored while a ride is running: a ride compares against the history it started with.
    fn reload_ghost(&mut self) {
        let Some(route) = self.st.route.peek().clone() else {
            self.ghost_plan = None;
            self.core.set_ghost(None);
            let mut prs = self.st.segment_prs;
            prs.set(Vec::new());
            let mut status = self.st.ghost_status;
            status.set(GhostStatus::default());
            return;
        };
        if matches!(self.core.state(), RideState::Riding | RideState::Paused) {
            return;
        }
        let mode = self.st.settings.peek().ui.ghost;
        self.ghost_gen += 1;
        let generation = self.ghost_gen;
        let route_id = route.id.clone();
        let mut prs_signal = self.st.segment_prs;
        let mut status_signal = self.st.ghost_status;
        let ctl = self.controller();
        spawn(async move {
            let joined = tokio::task::spawn_blocking(move || {
                guarded("history::plan", || crate::history::plan(&route, mode)).unwrap_or_default()
            })
            .await;
            let history = joined.unwrap_or_default();
            let crate::history::HistoryPlan { ghost, prs } = history.plan;
            let applied = ctl.with(|i| {
                let same_route =
                    i.st.route
                        .peek()
                        .as_ref()
                        .is_some_and(|route| route.id == route_id);
                if i.ghost_gen != generation || !same_route {
                    return false;
                }
                i.ghost_plan = ghost;
                i.core.set_ghost(i.ghost_plan.clone());
                true
            });
            // A plan for a route that has been replaced meanwhile must not leak into the new one.
            if applied {
                prs_signal.set(prs);
                status_signal.set(history.status);
            }
        });
    }

    /// Keeps the PC awake while riding.
    fn set_awake(&mut self, on: bool) {
        if self.awake != on {
            self.awake = on;
            if !wake::set(on) {
                eprintln!("[ride-app] keep-awake could not be toggled to {on}");
            }
        }
    }

    // ───── routes ─────

    /// A running or paused ride must be ended before another route is loaded.
    fn can_replace_route(&self) -> bool {
        if matches!(self.core.state(), RideState::Riding | RideState::Paused) {
            self.toast(
                ToastKind::Warning,
                "Tekerés közben nem lehet útvonalat váltani. Előbb fejezd be a túrát.",
            );
            return false;
        }
        true
    }

    fn fail_load(&mut self, message: String) {
        eprintln!("[ride-app] route not loaded: {message}");
        let mut error = self.st.welcome_error;
        error.set(Some(message.clone()));
        self.toast(ToastKind::Error, message);
    }

    fn install_route(&mut self, route: Route) {
        eprintln!(
            "[ride-app] route loaded: {} ({} points, {:.1} km)",
            route.name,
            route.points.len(),
            route.total_dist / 1000.0
        );
        let route = Arc::new(route);
        let rider = self.st.settings.peek().rider.clone();
        match self.core.load(route.clone(), rider, Instant::now()) {
            Ok(out) => {
                self.finished = None;
                let mut summary = self.st.summary;
                summary.set(None);
                // the route chooser has done its job, and a finished ride's summary is stale
                if matches!(
                    *self.st.dialog.peek(),
                    Some(DialogKind::Summary | DialogKind::Route)
                ) {
                    let mut dialog = self.st.dialog;
                    dialog.set(None);
                }
                let mut error = self.st.welcome_error;
                error.set(None);
                let mut r = self.st.route;
                r.set(Some(route.clone()));
                self.apply(out);
                self.win.set_title(&window_title(Some(&route)));
                self.remember_recent(&route);
                // The PRs of the previous route must not show against the new climbs while the
                // plan is being read.
                let mut segment_prs = self.st.segment_prs;
                segment_prs.set(Vec::new());
                let mut ghost_status = self.st.ghost_status;
                ghost_status.set(GhostStatus::default());
                self.reload_ghost();
                self.toast(ToastKind::Success, route_loaded_text(&route));
                if !route.has_elevation {
                    self.toast(
                        ToastKind::Warning,
                        "Ehhez az útvonalhoz nincs magassági adat, ezért a profil lapos.",
                    );
                }
            }
            Err(_) => self.fail_load(
                "Ezzel az útvonallal nem sikerült elindítani a szimulációt.".to_string(),
            ),
        }
    }

    fn unload_route(&mut self) {
        let out = self.core.unload(Instant::now());
        self.drop_route_signals();
        self.apply(out);
    }

    /// Clears everything that belongs to the loaded route.
    fn drop_route_signals(&mut self) {
        self.finished = None;
        self.ghost_plan = None;
        self.ghost_gen += 1;
        self.core.set_ghost(None);
        let (mut route, mut summary, mut history, mut dialog, mut comparison, mut segment_prs) = (
            self.st.route,
            self.st.summary,
            self.st.history,
            self.st.dialog,
            self.st.comparison,
            self.st.segment_prs,
        );
        let mut ghost_status = self.st.ghost_status;
        route.set(None);
        summary.set(None);
        history.set(Arc::new(Vec::new()));
        comparison.set(None);
        segment_prs.set(Vec::new());
        ghost_status.set(GhostStatus::default());
        if *dialog.peek() == Some(DialogKind::Summary) {
            dialog.set(None);
        }
        self.win.set_title(&window_title(None));
    }

    fn remember_recent(&mut self, route: &Route) {
        if route.source == RouteSource::Demo {
            return;
        }
        match guarded("store::save_recent", || {
            ride_route::store::save_recent(route)
        }) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                eprintln!("[ride-app] save_recent: {err}");
                self.toast(
                    ToastKind::Warning,
                    "Az útvonalat nem sikerült a legutóbbiak közé menteni.",
                );
            }
            Err(_) => {}
        }
        self.refresh_recents();
    }

    fn refresh_recents(&mut self) {
        if let Ok(list) = guarded("store::list_recent", ride_route::store::list_recent) {
            let mut recents = self.st.recents;
            recents.set(list);
        }
    }

    fn delete_recent(&mut self, id: &str) {
        match guarded("store::delete_recent", || {
            ride_route::store::delete_recent(id)
        }) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                eprintln!("[ride-app] delete_recent: {err}");
                self.toast(
                    ToastKind::Warning,
                    "Az útvonalat nem sikerült törölni a listából.",
                );
            }
            Err(_) => {}
        }
        self.refresh_recents();
    }

    // ───── settings ─────

    /// Applies new settings; `true` if they differ from the current ones.
    fn apply_settings(&mut self, new: Settings) -> bool {
        let old = self.st.settings.peek().clone();
        if old == new {
            return false;
        }
        if old.rider != new.rider {
            self.core.set_rider(new.rider.clone());
        }
        if old.ui.workout_id != new.ui.workout_id {
            let current = self.core.workout().map(|w| w.id.clone());
            if current != new.ui.workout_id {
                let workout = new
                    .ui
                    .workout_id
                    .as_deref()
                    .and_then(|id| WorkoutLibrary::embedded().get(id))
                    .map(|w| Arc::new(w.clone()));
                let out = self.core.set_workout(workout, Instant::now());
                self.apply(out);
            }
        }
        let mut settings = self.st.settings;
        settings.set(new.clone());
        self.sync_gear(&new);
        if old.ui.ghost != new.ui.ghost {
            self.reload_ghost();
        }
        if self.st.drag.peek().is_none() {
            let dock = Dock::from_settings(new.ui.active_tiles());
            if *self.st.dock.peek() != dock {
                self.st.dock.set(dock);
            }
        }
        true
    }

    /// Tells the ride loop how much the virtual gear shifts the trainer's grade.
    fn sync_gear(&mut self, settings: &Settings) {
        let offset = if settings.ui.virtual_gears {
            crate::gearbox::grade_offset(crate::gearbox::index_from_saved(settings.ui.gear))
        } else {
            0.0
        };
        self.core.set_gear_offset(offset);
    }

    fn flush_settings(&mut self) {
        let pending = PENDING_SETTINGS.lock().ok().and_then(|mut p| p.take());
        let Some(settings) = pending else { return };
        let disk = boot().for_disk(settings);
        if disk == self.saved {
            return;
        }
        match guarded("store::save_settings", || {
            ride_route::store::save_settings(&disk)
        }) {
            Ok(Ok(())) => self.saved = disk,
            Ok(Err(err)) => {
                eprintln!("[ride-app] save_settings: {err}");
                self.toast(ToastKind::Error, "A beállításokat nem sikerült elmenteni.");
            }
            Err(_) => self.toast(ToastKind::Error, "A beállításokat nem sikerült elmenteni."),
        }
    }

    // ───── window ─────

    fn toggle_fullscreen(&mut self) {
        let on = self.win.window.fullscreen().is_none();
        self.win.set_fullscreen(on);
        let mut fullscreen = self.st.fullscreen;
        fullscreen.set(on);
    }
}

impl AppState {
    pub fn welcome_error_set(self, message: Option<String>) {
        let mut error = self.welcome_error;
        error.set(message);
    }
}

mod backup;
mod review;

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;

    #[test]
    fn connection_toasts() {
        use ConnState::*;
        let t = |prev: &ConnState, new: &ConnState, name: Option<&str>, expected| {
            state_change_toast(DeviceKind::Trainer, prev, new, name, expected)
        };
        assert_eq!(
            t(&Connecting, &Connected, Some("KICKR"), false),
            Some((
                ToastKind::Success,
                "Trainer csatlakoztatva: KICKR".to_string()
            ))
        );
        assert_eq!(
            t(&Connecting, &Connected, None, false),
            Some((ToastKind::Success, "Trainer csatlakoztatva".to_string()))
        );
        assert_eq!(
            t(&Connected, &Connected, Some("KICKR"), false),
            None,
            "no repeat"
        );
        assert_eq!(
            t(&Connecting, &Failed("időtúllépés".into()), None, false),
            Some((
                ToastKind::Error,
                "Trainer: a kapcsolódás nem sikerült (időtúllépés)".to_string()
            ))
        );
        assert_eq!(
            t(&Connecting, &Failed(String::new()), None, false),
            Some((
                ToastKind::Error,
                "Trainer: a kapcsolódás nem sikerült".to_string()
            ))
        );
        assert_eq!(
            t(&Failed("x".into()), &Failed("x".into()), None, false),
            None
        );
        assert_eq!(
            t(&Connected, &Disconnected, None, false),
            Some((
                ToastKind::Warning,
                "Trainer: megszakadt a kapcsolat".to_string()
            ))
        );
        assert_eq!(
            t(&Connected, &Disconnected, None, true),
            None,
            "user-initiated disconnect is silent"
        );
        assert_eq!(t(&Disconnected, &Disconnected, None, false), None);
        assert_eq!(t(&Disconnected, &Connecting, None, false), None);
    }

    #[test]
    fn route_texts() {
        let route = straight_route(&[(10_000.0, 2.0)], 100.0);
        let text = route_loaded_text(&route);
        assert!(
            text.starts_with("Útvonal betöltve: Tesztútvonal – 10.00 km, "),
            "{text}"
        );
        assert!(text.ends_with(" m emelkedő"), "{text}");
        assert_eq!(window_title(Some(&route)), "Tesztútvonal – bike-osm-ride");
        assert_eq!(window_title(None), "bike-osm-ride");
        let mut unnamed = route;
        unnamed.name = "  ".into();
        assert_eq!(window_title(Some(&unnamed)), "bike-osm-ride");
    }

    #[test]
    fn virtual_power_is_bounded_and_whole_watts() {
        assert_eq!(clamp_virtual_power(-30.0), 0.0);
        assert_eq!(clamp_virtual_power(249.6), 250.0);
        assert_eq!(clamp_virtual_power(5000.0), 2000.0);
        assert_eq!(clamp_virtual_power(f64::NAN), 0.0);
    }

    #[test]
    fn tick_period_is_ten_hertz() {
        assert_eq!(TICK.as_millis(), 100);
    }
}
