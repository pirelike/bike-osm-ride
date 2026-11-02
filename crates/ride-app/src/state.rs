//! Application state: the signals shared by the shell, the controller and the UI crates.
//!
//! UI crates receive these as `ReadSignal` props and never own business state. The
//! controller (see `controller.rs`) is the only writer of the ride-related signals.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dioxus::prelude::*;
use ride_model::{
    ConnState, DeviceInfo, DeviceKind, GhostStatus, MapStyle, RecentRoute, RecordPoint,
    RideComparison, RideSnapshot, RideSummary, Route, Settings, Theme, Workout,
};
use ride_ui_dialogs::{DeviceSlot, RideDetails};
use ride_ui_kit::{Toast, ToastKind};

use crate::dock::{Availability, Divider, Dock, DropTarget, PanelId, Workspace};
use crate::guard::guarded;
use crate::hooks::Hooks;

/// At most this many toasts are visible at once; the oldest one is dropped.
const MAX_TOASTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    Route,
    Devices,
    Settings,
    Workouts,
    About,
    Summary,
}

// ───────────────────────────── start-up snapshot ─────────────────────────────

/// How `RIDE_THEME` changed the persisted UI settings, so that the override is never
/// written to the settings file unless the user changes the theme / map style themselves.
#[derive(Debug, Clone, PartialEq)]
struct HookUi {
    theme_override: Theme,
    theme_persisted: Theme,
    style_override: MapStyle,
    style_persisted: MapStyle,
    workout_override: Option<String>,
    workout_persisted: Option<String>,
}

/// Read once at start-up (before the window exists): test hooks and persisted settings.
pub struct Boot {
    pub hooks: Hooks,
    pub settings: Settings,
    hook_ui: Option<HookUi>,
}

impl Boot {
    fn load() -> Self {
        let hooks = Hooks::from_env();
        let mut settings =
            guarded("store::load_settings", ride_route::store::load_settings).unwrap_or_default();
        let mut hook_ui: Option<HookUi> = None;
        if hooks.theme.is_some() || hooks.workout.is_some() {
            let mut h = HookUi {
                theme_override: settings.ui.theme,
                theme_persisted: settings.ui.theme,
                style_override: settings.ui.map_style,
                style_persisted: settings.ui.map_style,
                workout_override: settings.ui.workout_id.clone(),
                workout_persisted: settings.ui.workout_id.clone(),
            };
            if let Some(theme) = hooks.theme {
                settings.ui.theme = theme;
                if settings.ui.map_style != MapStyle::Osm {
                    settings.ui.map_style = match theme {
                        Theme::Dark => MapStyle::Dark,
                        Theme::Light => MapStyle::Light,
                    };
                }
                h.theme_override = theme;
                h.style_override = settings.ui.map_style;
            }
            if let Some(id) = &hooks.workout {
                settings.ui.workout_id = Some(id.clone());
                h.workout_override = Some(id.clone());
            }
            hook_ui = Some(h);
        }
        Self {
            hooks,
            settings,
            hook_ui,
        }
    }

    /// The settings as they should be written to disk.
    pub fn for_disk(&self, mut settings: Settings) -> Settings {
        if let Some(h) = &self.hook_ui {
            if settings.ui.theme == h.theme_override {
                settings.ui.theme = h.theme_persisted;
            }
            if settings.ui.map_style == h.style_override {
                settings.ui.map_style = h.style_persisted;
            }
            if settings.ui.workout_id == h.workout_override {
                settings.ui.workout_id = h.workout_persisted.clone();
            }
        }
        settings
    }
}

pub fn boot() -> &'static Boot {
    static BOOT: OnceLock<Boot> = OnceLock::new();
    BOOT.get_or_init(Boot::load)
}

// ───────────────────────────── layout ─────────────────────────────

/// What a mouse drag is changing; points are client (window) pixels from the press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragOp {
    /// A panel is carried by its title bar (`parked: false`) or a closed one by its chip in the
    /// top bar (`parked: true`). `travelled` says whether the pointer left the click radius (below
    /// it the release is a click); `target` is what a release here would do.
    Carry {
        id: PanelId,
        parked: bool,
        start_x: f64,
        start_y: f64,
        travelled: bool,
        target: Option<DropTarget>,
    },
    /// The divider between two panels is being dragged.
    Divider { divider: Divider },
}

// ───────────────────────────── signals ─────────────────────────────

#[derive(Clone, Copy)]
pub struct AppState {
    pub route: Signal<Option<Arc<Route>>>,
    /// 10 Hz.
    pub snapshot: Signal<RideSnapshot>,
    /// 1 Hz recording of the running ride.
    pub history: Signal<Arc<Vec<RecordPoint>>>,
    pub settings: Signal<Settings>,
    pub toasts: Signal<Vec<Toast>>,
    pub recents: Signal<Vec<RecentRoute>>,
    pub discovered: Signal<Vec<DeviceInfo>>,
    pub slots: Signal<Vec<DeviceSlot>>,
    pub scanning: Signal<bool>,
    /// A route is being loaded (welcome screen spinner).
    pub busy: Signal<bool>,
    pub welcome_error: Signal<Option<String>>,
    pub summary: Signal<Option<RideSummary>>,
    /// What the finished ride did against the history (ghost difference, segment PRs).
    pub comparison: Signal<Option<RideComparison>>,
    /// PR time of every climb of the loaded route, indexed like `Route::climbs`.
    pub segment_prs: Signal<Vec<Option<f64>>>,
    /// What the ride log of the loaded route holds (the settings dialog explains the ghost with it).
    pub ghost_status: Signal<GhostStatus>,
    /// Workouts imported from files (MyWhoosh / Zwift), on top of the embedded library.
    pub imported_workouts: Signal<Vec<Workout>>,
    pub dialog: Signal<Option<DialogKind>>,
    /// Power of the simulated rider, watts.
    pub virtual_power: Signal<f64>,
    /// The simulated rider is connected in at least one slot.
    pub sim_connected: Signal<bool>,
    /// A workout is selected and not finished yet: the trainer holds its watts (ERG), so the
    /// virtual gearbox has nothing to do and is not shown. Kept by the controller, so the layout
    /// does not have to read the 10 Hz snapshot.
    pub erg: Signal<bool>,
    pub fullscreen: Signal<bool>,
    /// A file is being dragged over the window.
    pub drag_active: Signal<bool>,
    /// Where every panel of the ride screen sits (persisted in `UiSettings::panels`).
    pub dock: Signal<Dock>,
    /// The panel drag in progress (a full-screen catcher follows the mouse).
    pub drag: Signal<Option<DragOp>>,
    /// Position and size of the workspace in CSS pixels (measured by a ResizeObserver).
    pub workspace: Signal<Workspace>,
    /// What the browser panel says instead of Firefox (not installed, starting, failed).
    pub browser_note: Signal<Option<String>>,
    /// What the rider says about the finished ride (title, sport, notes...), edited in the summary.
    pub ride_details: Signal<RideDetails>,
    /// A Strava upload is running (in the background, or in the visible view).
    pub strava_busy: Signal<bool>,
    /// The Strava view is shown (sign-in or a step that needs the rider) and its bar says this.
    pub strava_status: Signal<Option<String>>,
    /// A newer release found on GitHub (the top bar offers it).
    pub update: Signal<Option<crate::updater::UpdateInfo>>,
    /// The update is being downloaded / installed.
    pub updating: Signal<bool>,
}

impl AppState {
    /// Which conditional panels can be shown (read without subscribing).
    pub fn availability(self) -> Availability {
        Availability {
            workout: self.settings.peek().ui.workout_id.is_some(),
            rider: *self.sim_connected.peek(),
            gears: self.settings.peek().ui.virtual_gears && !*self.erg.peek(),
        }
    }

    /// Like [`AppState::availability`], re-rendering the caller when it changes.
    pub fn availability_live(self) -> Availability {
        Availability {
            workout: self.settings.read().ui.workout_id.is_some(),
            rider: *self.sim_connected.read(),
            gears: self.settings.read().ui.virtual_gears && !*self.erg.read(),
        }
    }
}

impl AppState {
    /// A Zwift Click / Play shifter is connected: it changes the gear, the screen only has to
    /// say which one.
    pub fn shifter_connected(self) -> bool {
        self.slots
            .read()
            .iter()
            .any(|s| s.kind == DeviceKind::Shifter && s.state == ConnState::Connected)
    }

    /// Creates all signals; call once, from the root component.
    pub fn new() -> Self {
        let settings = boot().settings.clone();
        let dock = Dock::from_settings(settings.ui.tiles.as_ref());
        // A fresh start asks for a route first; the hooks that load one themselves skip the dialog.
        let hooks = &boot().hooks;
        let first_dialog = if hooks.gpx.is_none() && !hooks.demo && hooks.sim_watts.is_none() {
            Some(DialogKind::Route)
        } else {
            None
        };
        Self {
            route: Signal::new(None),
            snapshot: Signal::new(RideSnapshot::empty()),
            history: Signal::new(Arc::new(Vec::new())),
            settings: Signal::new(settings),
            toasts: Signal::new(Vec::new()),
            recents: Signal::new(Vec::new()),
            discovered: Signal::new(Vec::new()),
            slots: Signal::new(default_slots()),
            scanning: Signal::new(false),
            busy: Signal::new(false),
            welcome_error: Signal::new(None),
            summary: Signal::new(None),
            comparison: Signal::new(None),
            segment_prs: Signal::new(Vec::new()),
            ghost_status: Signal::new(GhostStatus::default()),
            imported_workouts: Signal::new(Vec::new()),
            dialog: Signal::new(first_dialog),
            virtual_power: Signal::new(150.0),
            sim_connected: Signal::new(false),
            erg: Signal::new(false),
            fullscreen: Signal::new(false),
            drag_active: Signal::new(false),
            dock: Signal::new(dock),
            drag: Signal::new(None),
            workspace: Signal::new(Workspace::default()),
            browser_note: Signal::new(None),
            ride_details: Signal::new(RideDetails::default()),
            strava_busy: Signal::new(false),
            strava_status: Signal::new(None),
            update: Signal::new(None),
            updating: Signal::new(false),
        }
    }

    /// Shows a toast (Hungarian text); identical visible text is not repeated.
    pub fn toast(self, kind: ToastKind, text: impl Into<String>) {
        let lifetime = toast_lifetime(kind);
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let text = text.into();
        let mut toasts = self.toasts;
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        if !push_toast(&mut toasts.write(), Toast { id, kind, text }) {
            return;
        }
        spawn(async move {
            tokio::time::sleep(lifetime).await;
            toasts.write().retain(|t| t.id != id);
        });
    }

    pub fn dismiss_toast(self, id: u64) {
        let mut toasts = self.toasts;
        toasts.write().retain(|t| t.id != id);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── pure helpers ─────────────────────────────

pub fn toast_lifetime(kind: ToastKind) -> Duration {
    match kind {
        ToastKind::Error => Duration::from_secs(9),
        ToastKind::Warning => Duration::from_secs(7),
        ToastKind::Info | ToastKind::Success => Duration::from_millis(4500),
    }
}

/// Adds `toast` unless an identical text is already visible; keeps at most [`MAX_TOASTS`].
/// Returns whether it was added.
pub fn push_toast(list: &mut Vec<Toast>, toast: Toast) -> bool {
    if list.iter().any(|t| t.text == toast.text) {
        return false;
    }
    list.push(toast);
    while list.len() > MAX_TOASTS {
        list.remove(0);
    }
    true
}

pub const SLOT_ORDER: [DeviceKind; 5] = [
    DeviceKind::Trainer,
    DeviceKind::HeartRate,
    DeviceKind::PowerMeter,
    DeviceKind::Cadence,
    DeviceKind::Shifter,
];

pub fn default_slots() -> Vec<DeviceSlot> {
    SLOT_ORDER
        .iter()
        .map(|&kind| DeviceSlot {
            kind,
            state: ConnState::Disconnected,
            name: None,
        })
        .collect()
}

/// Updates the slot of `kind`. A new name replaces the old one; `Disconnected` forgets it.
pub fn apply_slot(
    slots: &mut Vec<DeviceSlot>,
    kind: DeviceKind,
    state: ConnState,
    name: Option<String>,
) {
    let forget = state == ConnState::Disconnected;
    match slots.iter_mut().find(|s| s.kind == kind) {
        Some(slot) => {
            slot.state = state;
            if forget {
                slot.name = None;
            } else if name.is_some() {
                slot.name = name;
            }
        }
        None => slots.push(DeviceSlot {
            kind,
            state,
            name: if forget { None } else { name },
        }),
    }
}

pub fn kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Trainer => "Trainer",
        DeviceKind::HeartRate => "Pulzusmérő",
        DeviceKind::PowerMeter => "Teljesítménymérő",
        DeviceKind::Cadence => "Cadence-szenzor",
        DeviceKind::Shifter => "Váltó",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(id: u64, text: &str) -> Toast {
        Toast {
            id,
            kind: ToastKind::Info,
            text: text.into(),
        }
    }

    #[test]
    fn toasts_are_deduplicated_and_capped() {
        let mut list = Vec::new();
        assert!(push_toast(&mut list, toast(1, "a")));
        assert!(!push_toast(&mut list, toast(2, "a")));
        for (i, text) in ["b", "c", "d", "e"].iter().enumerate() {
            assert!(push_toast(&mut list, toast(10 + i as u64, text)));
        }
        let texts: Vec<_> = list.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["b", "c", "d", "e"], "oldest toast dropped");
        // Once dropped, the same text may appear again.
        assert!(push_toast(&mut list, toast(99, "a")));
    }

    #[test]
    fn errors_stay_longer_than_confirmations() {
        assert!(toast_lifetime(ToastKind::Error) > toast_lifetime(ToastKind::Warning));
        assert!(toast_lifetime(ToastKind::Warning) > toast_lifetime(ToastKind::Success));
    }

    #[test]
    fn slots_start_disconnected_in_order() {
        let slots = default_slots();
        assert_eq!(slots.iter().map(|s| s.kind).collect::<Vec<_>>(), SLOT_ORDER);
        assert!(slots
            .iter()
            .all(|s| s.state == ConnState::Disconnected && s.name.is_none()));
    }

    #[test]
    fn slot_updates_keep_or_forget_the_name() {
        let mut slots = default_slots();
        apply_slot(
            &mut slots,
            DeviceKind::Trainer,
            ConnState::Connecting,
            Some("KICKR".into()),
        );
        apply_slot(&mut slots, DeviceKind::Trainer, ConnState::Connected, None);
        assert_eq!(slots[0].name.as_deref(), Some("KICKR"));
        assert_eq!(slots[0].state, ConnState::Connected);
        apply_slot(
            &mut slots,
            DeviceKind::Trainer,
            ConnState::Disconnected,
            Some("KICKR".into()),
        );
        assert_eq!(slots[0].name, None);
        apply_slot(
            &mut slots,
            DeviceKind::HeartRate,
            ConnState::Failed("időtúllépés".into()),
            None,
        );
        assert_eq!(slots[1].state, ConnState::Failed("időtúllépés".into()));
        assert_eq!(slots.len(), 5);
    }

    #[test]
    fn theme_override_is_not_persisted() {
        let hook = HookUi {
            theme_override: Theme::Light,
            theme_persisted: Theme::Dark,
            style_override: MapStyle::Light,
            style_persisted: MapStyle::Dark,
            workout_override: Some("hooked".into()),
            workout_persisted: None,
        };
        let boot = Boot {
            hooks: Hooks::default(),
            settings: Settings::default(),
            hook_ui: Some(hook),
        };

        let mut s = Settings::default();
        s.ui.theme = Theme::Light;
        s.ui.map_style = MapStyle::Light;
        let disk = boot.for_disk(s.clone());
        assert_eq!(
            (disk.ui.theme, disk.ui.map_style),
            (Theme::Dark, MapStyle::Dark)
        );

        // The user picked OSM in this session: that choice is kept.
        s.ui.map_style = MapStyle::Osm;
        assert_eq!(boot.for_disk(s.clone()).ui.map_style, MapStyle::Osm);
        // The user toggled the theme to dark in this session: kept as chosen.
        s.ui.theme = Theme::Dark;
        assert_eq!(boot.for_disk(s).ui.theme, Theme::Dark);
    }

    #[test]
    fn workout_hook_is_not_persisted_until_the_user_picks_one() {
        let boot = Boot {
            hooks: Hooks::default(),
            settings: Settings::default(),
            hook_ui: Some(HookUi {
                theme_override: Theme::Dark,
                theme_persisted: Theme::Dark,
                style_override: MapStyle::Dark,
                style_persisted: MapStyle::Dark,
                workout_override: Some("e-ramp-20w".into()),
                workout_persisted: None,
            }),
        };
        let mut s = Settings::default();
        s.ui.workout_id = Some("e-ramp-20w".into());
        assert_eq!(boot.for_disk(s.clone()).ui.workout_id, None);
        // choosing another workout in the session is a real change
        s.ui.workout_id = Some("c-threshold-2x20".into());
        assert_eq!(
            boot.for_disk(s).ui.workout_id.as_deref(),
            Some("c-threshold-2x20")
        );
    }

    #[test]
    fn without_hooks_settings_are_untouched() {
        let boot = Boot {
            hooks: Hooks::default(),
            settings: Settings::default(),
            hook_ui: None,
        };
        let mut s = Settings::default();
        s.ui.theme = Theme::Light;
        assert_eq!(boot.for_disk(s.clone()), s);
    }

    #[test]
    fn kind_labels_are_hungarian() {
        assert_eq!(kind_label(DeviceKind::HeartRate), "Pulzusmérő");
        assert_eq!(kind_label(DeviceKind::Cadence), "Cadence-szenzor");
        assert_eq!(kind_label(DeviceKind::Shifter), "Váltó");
    }
}
