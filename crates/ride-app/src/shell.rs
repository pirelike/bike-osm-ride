//! The shell: layout, theme attribute, CSS injection, keyboard bridge and drag & drop.
//!
//! ```text
//! top bar   mark · name | route · km | parked panels (chips) ........ Edzés | Útvonal | Eszközök (3 dots) | Beállítások | theme | full | about
//! workspace the ride screen: a 24 x 16 grid of panels (map, profile, controls, metrics, ...)
//! ```
//! The language is a printed race sheet: every panel is a flush rectangle drawn with 1 px rules,
//! no blur, no shadows. Every panel has the same frame (a title bar to move it by, a close button,
//! eight resize handles) and always sits on the grid, see `dock.rs`. A closed panel waits as a chip
//! in the top bar; pressing it puts the panel back, dragging it drops the panel wherever it is
//! released. Nothing but the top bar is fixed: even the map is a panel. The route chooser, the
//! workout picker, the devices and the settings are dialogs of one common size on top of the
//! screen, which is always there, empty until a route is loaded. Components only read the signals
//! they display; the high-frequency ones (snapshot, history) are read inside the UI crates.

use std::rc::Rc;

use dioxus::desktop::use_window;
use dioxus::document;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use ride_model::{ConnState, DeviceKind, ProfileMode, ProfileView, Theme, WorkoutLibrary};
use ride_ui_dialogs::{
    AboutDialog, DeviceSlot, DevicesDialog, RouteDialog, SettingsDialog, SummaryDialog,
    VirtualRiderPanel, WorkoutsDialog,
};
use ride_ui_hud::{ClimbCard, MetricTiles, RideControls, RideDash, StatsPanel, WorkoutCard};
use ride_ui_kit::format::fmt_km;
use ride_ui_kit::{Icon, IconName, ToastHost};
use ride_ui_map::MapView;
use ride_ui_profile::{ChartMetric, ElevationProfile, LiveChart, Pages, WorkoutProfile};
use ride_ui_streetview::StreetViewPanel;

use crate::browser;
use crate::controller::Controller;
use crate::dock::{self, PanelId, Workspace, CLICK_SLOP};
use crate::gearbox;
use crate::state::{kind_label, AppState, DialogKind, DragOp};

// ───────────────────────────── CSS / head ─────────────────────────────

/// Shell layout. Tokens only (the kit owns the values); class names are prefixed `shell-` / `dock-`.
pub const CSS: &str = r##"
:root { color-scheme: dark; }
:root[data-theme="light"] { color-scheme: light; }
html, body { margin: 0; height: 100%; overflow: hidden; background: var(--bg); }

.shell {
  --top-h: 40px;
  position: fixed; inset: 0;
  display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: calc(var(--top-h) + var(--safe-top)) minmax(0, 1fr); grid-auto-rows: 0;
  background: var(--bg); color: var(--text);
  font: 400 var(--t-13)/1.4 var(--font-ui);
  overflow: hidden;
  -webkit-user-select: none; user-select: none;
}
.shell *, .shell *::before, .shell *::after { box-sizing: border-box; }
.shell input, .shell textarea, .shell [contenteditable] { -webkit-user-select: text; user-select: text; }

/* ── top bar: 40 px, cells separated by rules ──
   Below the safe area: on a phone drawn edge-to-edge (Android 15 with targetSdk 35+, or a
   WebView that reports the camera cutout) the status bar, the punch hole or the notch sit over
   the top of the page, and the bar starts under them. Everywhere else the insets are 0. */
.shell { --safe-top: env(safe-area-inset-top, 0px); }
.shell-top {
  grid-row: 1;
  position: relative; z-index: 10;
  display: flex; align-items: stretch;
  height: calc(var(--top-h) + var(--safe-top));
  padding: var(--safe-top) env(safe-area-inset-right, 0px) 0 env(safe-area-inset-left, 0px);
  background: var(--bg-1);
  border-bottom: 1px solid var(--rule-strong);
}
.shell-logo {
  display: flex; align-items: center; gap: var(--space-3);
  padding-right: var(--space-4);
  border-right: 1px solid var(--rule);
  font: 600 var(--t-20)/1 var(--font-display);
  white-space: nowrap;
}
.shell-logo__mark {
  display: grid; place-items: center; width: var(--top-h); height: var(--top-h);
  background: var(--text); color: var(--bg-1);
}
.shell-route {
  display: flex; align-items: center; gap: var(--space-3);
  min-width: 0; max-width: min(28vw, 400px);
  padding: 0 var(--space-4);
  border-right: 1px solid var(--rule);
}
.shell-route__name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.shell-route__meta { flex: none; font: 600 var(--t-15)/1 var(--font-num); font-variant-numeric: tabular-nums; color: var(--text-dim); white-space: nowrap; }
.shell-route__meta small { font: 500 var(--t-11)/1 var(--font-num); margin-left: 3px; }

/* the parked panels: a chip per closed panel; press = bring it back, drag = drop it somewhere */
.shell-tray {
  flex: 1 1 0; min-width: 0;
  display: flex; align-items: center; gap: var(--space-2);
  padding: 0 var(--space-3);
  overflow-x: auto; overflow-y: hidden;
  scrollbar-width: none;
}
.shell-tray::-webkit-scrollbar { display: none; }
.shell-chip {
  flex: none;
  display: inline-flex; align-items: center; gap: var(--space-2);
  height: 26px; padding: 0 var(--space-3) 0 var(--space-2);
  border: 1px solid var(--rule-strong); border-radius: 0;
  background: var(--bg-2); color: var(--text-dim);
  font: 600 var(--t-11)/1 var(--font-ui); white-space: nowrap;
  cursor: grab;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.shell-chip:hover { background: var(--hover); border-color: var(--accent); color: var(--text); }
.shell-chip:focus-visible { outline-offset: 1px; }
.shell-chip[data-dragging="true"] { border-color: var(--accent); color: var(--text); cursor: grabbing; }
.shell-nav { display: flex; align-items: stretch; margin-left: auto; flex: none; }
.shell-tb--update { color: var(--accent); }
.shell-tb {
  display: inline-flex; align-items: center; gap: var(--space-2);
  height: var(--top-h); padding: 0 var(--space-3);
  border: 0; border-left: 1px solid var(--rule); border-radius: 0;
  background: transparent; color: var(--text-dim);
  font: 600 var(--t-13)/1 var(--font-ui); white-space: nowrap;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.shell-tb:hover { background: var(--hover); color: var(--text); }
.shell-tb:active { background: var(--fill-3); }
.shell-tb:focus-visible { outline-offset: -2px; }
.shell-tb--icon { width: var(--top-h); padding: 0; justify-content: center; }
.shell-dots { display: inline-flex; align-items: center; gap: 4px; margin-left: var(--space-1); }
.shell-dot {
  width: 8px; height: 8px; border-radius: 50%;
  border: 1.5px solid var(--text-faint); background: transparent;
  transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
}
.shell-dot[data-state="ok"] { background: var(--ok); border-color: var(--ok); }
.shell-dot[data-state="busy"] { background: var(--warn); border-color: var(--warn); animation: shell-pulse 1.2s var(--ease) infinite; }
.shell-dot[data-state="bad"] { background: var(--crit); border-color: var(--crit); }
@keyframes shell-pulse { 50% { opacity: 0.35; } }

/* a grip: two rows of three dots (panel title bars, chips) */
.shell-grip {
  flex: none; width: 12px; height: 10px;
  display: grid; grid-template-columns: repeat(3, 2px); grid-template-rows: repeat(2, 2px);
  gap: 2px; align-content: center; justify-content: center;
  color: var(--text-faint);
}
.shell-grip i { width: 2px; height: 2px; background: currentColor; }

/* ── workspace: the grid of panels ── */
.dock { grid-row: 2; position: relative; min-width: 0; min-height: 0; overflow: hidden; background: var(--bg); }
.dock-panel {
  position: absolute;
  display: flex; flex-direction: column;
  min-width: 0; min-height: 0;
  background: var(--bg-1);
  border: 1px solid var(--rule-strong);
  overflow: hidden;
}
.dock-panel[data-dragging="true"] { border-color: var(--accent); opacity: 0.55; }
.dock-bar {
  flex: none;
  display: flex; align-items: center; gap: var(--space-2);
  height: 24px; padding-left: var(--space-2);
  background: var(--bg-2);
  border-bottom: 1px solid var(--rule);
  cursor: grab;
}
.dock-panel[data-dragging="true"] .dock-bar { cursor: grabbing; }
.dock-title {
  flex: 1; min-width: 0;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  font: 600 var(--t-11)/1 var(--font-ui); color: var(--text-dim);
}
.dock-close {
  flex: none; display: grid; place-items: center;
  width: 24px; height: 24px; padding: 0;
  border: 0; border-radius: 0;
  background: transparent; color: var(--text-dim);
  cursor: pointer;
}
.dock-close:hover { background: var(--hover); color: var(--text); }
.dock-close:focus-visible { outline-offset: -2px; }
.dock-body {
  position: relative; flex: 1 1 0; min-width: 0; min-height: 0;
  overflow-x: hidden; overflow-y: auto;
  container: dock-body / size;
}
/* map, profile and Street View fill their body and manage their own scrolling */
.dock-body--fill { overflow: hidden; }
.dock-fill { position: absolute; inset: 0; }
.dock-fill > * { width: 100%; height: 100%; }
.dock-note { padding: var(--space-3); color: var(--text-dim); font: 400 var(--t-13)/1.45 var(--font-ui); }

/* the divider between two panels: a thin hit area on the line, an accent rule while hovered */
.dock-divider { position: absolute; z-index: 5; }
.dock-divider--cols { width: 9px; margin-left: -4px; cursor: ew-resize; }
.dock-divider--rows { height: 9px; margin-top: -4px; cursor: ns-resize; }
.dock-divider::after { content: ""; position: absolute; background: var(--accent); opacity: 0; transition: opacity var(--dur-fast) var(--ease); }
.dock-divider--cols::after { top: 0; bottom: 0; left: 3px; width: 3px; }
.dock-divider--rows::after { left: 0; right: 0; top: 3px; height: 3px; }
.dock-divider:hover::after, .dock-divider[data-dragging="true"]::after { opacity: 1; }

/* what a release would do: the half (edge) or the whole (swap) of the panel underneath */
.dock-ghost {
  position: absolute; z-index: 50; pointer-events: none;
  border: 2px solid var(--accent); background: var(--accent-soft);
}

/* the map tile: the map and Street View share it, one big, the other small in the bottom right */
.tile-stack { position: absolute; inset: 0; }
.tile-view { position: absolute; min-width: 0; min-height: 0; overflow: hidden; background: var(--bg-1); }
.tile-view--big { inset: 0; z-index: 1; }
.tile-view--small {
  right: var(--space-3); bottom: var(--space-3); z-index: 4;
  width: 38%; height: 44%; min-width: min(260px, 100%); min-height: min(190px, 100%);
  border: 1px solid var(--rule-strong);
}
.tile-view__body { position: absolute; inset: 0; }
.tile-view__body > * { width: 100%; height: 100%; }
.tile-view--small:has(> .tile-pip__bar) .tile-view__body { top: 28px; }
.tile-pip__bar {
  position: relative; z-index: 2;
  display: flex; align-items: center; gap: var(--space-2);
  height: 28px; padding: 0 var(--space-2) 0 var(--space-3);
  background: var(--bg-1); border-bottom: 1px solid var(--rule);
}
.tile-pip__title { flex: 1; font: 600 var(--t-13)/1 var(--font-ui); color: var(--text-dim); }
.tile-pip__btn {
  display: grid; place-items: center; width: 22px; height: 22px; padding: 0;
  border: 0; border-radius: 0; background: transparent; color: var(--text-dim); cursor: pointer;
}
.tile-pip__btn:hover { background: var(--hover); color: var(--text); }
.tile-view .sv { --sv-w: 100%; width: 100%; height: 100%; max-height: 100%; border: 0; }

/* a toggle in a title bar (Street View on the map tile): inverted ink while on, like every selected state */
.dock-act {
  flex: none; display: inline-flex; align-items: center; gap: var(--space-2);
  height: 18px; margin-right: var(--space-2); padding: 0 var(--space-2);
  border: 1px solid var(--rule-strong); border-radius: 0;
  background: transparent; color: var(--text-dim);
  font: 600 var(--t-11)/1 var(--font-ui); cursor: pointer;
}
.dock-act:hover { background: var(--hover); color: var(--text); }
.dock-act[aria-pressed="true"] { background: var(--text); color: var(--bg-1); border-color: var(--text); }
.dock-act__sq { width: 6px; height: 6px; border: 1.5px solid currentColor; }
.dock-act[aria-pressed="true"] .dock-act__sq { background: currentColor; }

/* the virtual gearbox */
.shell-gear { display: flex; align-items: center; gap: var(--space-3); width: 100%; padding: 0 var(--space-3); }
.shell-gear__btn { --btn-h: 28px; padding: 0 var(--space-2); }
.shell-gear__num { font: 600 var(--t-28)/1 var(--font-num); font-variant-numeric: tabular-nums; white-space: nowrap; }
.shell-gear__num small { margin-left: 3px; font: 500 var(--t-11)/1 var(--font-num); color: var(--text-dim); }
.shell-gear__info { display: flex; flex-direction: column; gap: 2px; min-width: 0; margin-left: var(--space-2); }
.shell-gear__ratio { font: 600 var(--t-13)/1.2 var(--font-num); white-space: nowrap; }
.shell-gear__sub { font: 400 var(--t-11)/1.2 var(--font-ui); color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

/* where Firefox's native window sits */
.shell-browser { min-width: 0; min-height: 0; background: var(--bg-2); }
.shell-browser .dock-note { margin: 0; }

/* what each panel's content needs from its frame */
.dock-panel[data-panel="controls"] .dock-body { display: flex; align-items: center; }
.dock-body .hud-metrics, .dock-body .hud-stats, .dock-body .hud-workout { border-bottom: 0; }
.dock-body .hud-climb { width: 100%; }
/* the frame already says what the panel is */
.dock-body .hud-climb__head > .hud-label, .dock-body .hud-workout__head > .hud-label { display: none; }
.dock-body .hud-climb__head { justify-content: flex-end; }
.dock-body .dlg-vr { width: 100%; max-width: none; }
.dock-body .dlg-vr.panel { border: 0; background: transparent; }
.dock-body .dlg-vr .panel__head { display: none; }

/* a short panel steps the live numbers down one size so the grid and the first rows fit */
@container dock-body (max-height: 360px) {
  .num--hero { font-size: var(--t-56); letter-spacing: 0; }
  .num--hero .unit { font-size: var(--t-15); }
  .hud-cell .cell__value { font-size: var(--t-28); }
  .hud-cell .cell__unit { font-size: var(--t-13); }
}

/* full-window catcher while a drag runs: the panels below never see the pointer */
.shell-dragcatcher { position: fixed; inset: 0; z-index: 60; background: transparent; }
.shell-dragcatcher[data-op="carry"] { cursor: grabbing; }
.shell-dragcatcher[data-op="ns"] { cursor: ns-resize; }
.shell-dragcatcher[data-op="ew"] { cursor: ew-resize; }

/* ── file drop ── */
.shell-drop {
  position: fixed; inset: var(--space-3); z-index: 80;
  display: grid; place-items: center;
  border: 2px dashed var(--accent);
  background: var(--scrim);
  pointer-events: none;
  animation: shell-fade var(--dur) var(--ease);
}
.shell-drop__hint {
  display: flex; flex-direction: column; align-items: center; gap: var(--space-2); text-align: center;
  padding: var(--space-5) var(--space-6);
  background: var(--bg-1); border: 1px solid var(--rule-strong);
}
.shell-drop__title { font: 600 var(--t-28)/1.1 var(--font-display); }
.shell-drop__sub { color: var(--text-dim); font-size: var(--t-13); }
/* ── Strava upload: the native view fills the window below this strip ── */
.shell-strava {
  position: fixed; inset: 0; z-index: 1200;
  background: var(--bg-1);
}
.shell-strava__bar {
  display: flex; align-items: center; gap: var(--space-3);
  height: 52px; padding: 0 var(--space-4);
  border-bottom: 1px solid var(--rule-strong);
}
.shell-strava__title { font: 600 var(--t-15)/1 var(--font-display); white-space: nowrap; }
.shell-strava__status {
  flex: 1; min-width: 0; color: var(--text-dim); font-size: var(--t-13);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
@keyframes shell-fade { from { opacity: 0; } }

/* ── compact layout (phone, portrait tablet): tabs instead of the tiling ── */
.compact {
  grid-row: 2;
  display: grid; grid-template-rows: minmax(0, 1fr) auto auto;
  min-width: 0; min-height: 0; overflow: hidden; background: var(--bg);
}
.compact-stage {
  min-height: 0; overflow-y: auto; overscroll-behavior: contain;
  display: grid; grid-template-columns: minmax(0, 1fr); align-content: start; gap: 8px; padding: 8px;
}
.compact[data-tab="map"] .compact-stage { padding: 0; gap: 0; grid-template-rows: auto minmax(0, 1fr) 176px; align-content: stretch; overflow: hidden; }
.compact-extra { border: 0; }
.compact-extra--power { order: -1; border-bottom: 1px solid var(--rule-strong); }
.compact-extra--profile { order: 1; border-top: 1px solid var(--rule-strong); }
/* the ride tab: the head unit at its own height, the profile takes the rest of the screen */
.compact[data-tab="ride"] .compact-stage { grid-template-rows: auto minmax(200px, 1fr); align-content: stretch; }
.compact-panel {
  display: flex; flex-direction: column; min-width: 0;
  background: var(--bg-1); border: 1px solid var(--rule-strong); overflow: hidden;
}
.compact-panel.is-hidden { display: none; }
.compact-panel__body { position: relative; flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; }
.compact-panel__body > * { flex: 1 1 auto; min-height: 0; }
.compact-panel[data-panel="map"] { border: 0; min-height: 0; height: 100%; }
/* the panels were made to fill a tile; here each is as tall as its content (a fixed height left
   empty grey boxes under the short ones) */
.compact-panel[data-panel="metrics"],
.compact-panel[data-panel="climb"],
.compact-panel[data-panel="stats"],
.compact-panel[data-panel="gears"],
.compact-panel[data-panel="rider"] { height: auto; }
.compact-panel[data-panel="profile"] { height: auto; min-height: 200px; }
/* the charts tab holds only the charts: they take the whole height (at least 600 px) */
.compact[data-tab="charts"] .compact-stage { grid-template-rows: minmax(600px, 1fr); align-content: stretch; }
.compact-panel[data-panel="charts"] { height: auto; min-height: 600px; }
.compact-panel[data-panel="charts"] .profile-chart { flex: 1 1 0; }
.compact-panel[data-panel="charts"] .profile-chart-plot { flex: 1 1 auto; height: auto; min-height: 64px; }
.shell--compact .hud-controls__kbd { display: none; }
/* the transport bar of the phone: full-width, thumb-sized buttons (pause twice as wide as
   finish), the gear buttons beside them while the virtual gearbox is on */
.compact-controls {
  display: flex; align-items: stretch; gap: 8px;
  border-top: 1px solid var(--rule-strong); background: var(--bg-1); padding: 8px;
}
.compact-controls .hud-controls { flex: 1 1 auto; height: auto; padding: 0; min-width: 0; }
.compact-controls .btn-group { display: flex; gap: 8px; width: 100%; background: transparent; }
.compact-controls .hud-controls__btn {
  --btn-h: 48px; flex: 1 1 0; min-width: 0; padding: 0 12px;
  border-radius: var(--radius-md); font: 600 var(--t-15)/1 var(--font-ui);
}
.compact-controls .hud-controls__btn.btn--primary { flex-grow: 2; }
.compact-gears { flex: none; display: flex; align-items: center; gap: 4px; }
.compact-gears__btn { --btn-h: 48px; width: 44px; padding: 0; }
.compact-gears__num { min-width: 2ch; text-align: center; font: 600 var(--t-20)/1 var(--font-num); }
.compact-gears--readout { flex-direction: column; justify-content: center; gap: 2px; padding: 0 8px; border: 1px solid var(--rule-strong); }
.compact-gears__label { font: 500 var(--t-11)/1 var(--font-ui); color: var(--text-dim); }
.compact-tabs {
  display: grid; grid-template-columns: repeat(4, minmax(0, 1fr));
  border-top: 1px solid var(--rule-strong); background: var(--bg-1);
  padding-bottom: env(safe-area-inset-bottom, 0px);
}
.compact-tab {
  display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px;
  min-height: 56px; border: 0; border-top: 2px solid transparent; border-radius: 0; background: transparent; color: var(--text-dim);
  font: 500 var(--t-11)/1.1 var(--font-ui);
}
.compact-tab.is-active { color: var(--accent); border-top-color: var(--accent); }
.shell--compact .shell-tray,
.shell--compact .shell-logo > span:last-child,
.shell--compact .shell-tb .shell-label { display: none; }
.shell--compact .shell-route { max-width: none; flex: 1 1 auto; min-width: 0; }
.shell--compact .shell-tb { min-width: 44px; }
/* portrait tablet: one column, never wider than a table can be read (two columns left a lot of
   empty boxes; the tabs already split the panels by purpose) */
@media (min-width: 700px) {
  .compact[data-tab="data"] .compact-stage { grid-template-columns: minmax(0, min(100%, 720px)); justify-content: center; }
}
/* compact layout: notifications (coach messages among them) drop in under the top bar, clear of
   the transport buttons and the tabs at the bottom (same width limit as `COMPACT_BELOW_PX`) */
@media (max-width: 999.98px) {
  :root .toasts { top: calc(48px + env(safe-area-inset-top, 0px)); bottom: auto; left: 8px; right: 8px; max-width: none; }
}
/* phones: the top bar keeps only the route length (no logo: with it the length did not fit) */
@media (max-width: 699px) {
  .shell--compact .shell-logo { display: none; }
  .shell--compact .shell-route__name { display: none; }
  .shell--compact .shell-route { flex: 0 0 auto; padding: 0 12px; }
  .shell--compact .shell-tb { padding: 0 8px; min-width: 38px; justify-content: center; }
  .shell--compact .shell-tb[aria-label^="Teljes képernyő"] { display: none; }
}

/* ── narrow windows: the labels give way to the icons ── */
@media (max-width: 859px) {
  .shell-route { max-width: 18vw; }
  .shell-tb .shell-label { display: none; }
}

@media (prefers-reduced-motion: reduce) {
  .shell-dot, .shell-drop { animation: none; transition: none; }
}
"##;

/// Everything the page needs in its stylesheet, in cascade order: the design system first,
/// then each crate's own CSS, then the shell layout.
pub fn css_bundle() -> String {
    [
        ride_ui_kit::CSS,
        ride_ui_map::CSS,
        ride_ui_profile::CSS,
        ride_ui_hud::CSS,
        ride_ui_dialogs::CSS,
        ride_ui_streetview::CSS,
        CSS,
    ]
    .join("\n")
}

/// `<html data-text>`: `large` from this text scale on, where layouts with several numbers in a
/// row give each number more room (see `hud.css`).
pub const LARGE_TEXT_FROM: f64 = 1.15;

pub fn text_size_attr(scale: f64) -> &'static str {
    if scale >= LARGE_TEXT_FROM {
        "large"
    } else {
        "normal"
    }
}

pub fn theme_attr(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

/// Background colour of the native webview before the first paint (`--bg` of the theme in
/// `ride-ui-kit/assets/kit.css`; a test keeps them in step).
pub fn background_rgba(theme: Theme) -> (u8, u8, u8, u8) {
    match theme {
        Theme::Dark => (0x12, 0x11, 0x0f, 0xff),
        Theme::Light => (0xec, 0xe7, 0xda, 0xff),
    }
}

/// Content injected into `<head>`: the theme attribute and the text scale (set before first
/// paint, no flash) and all CSS.
pub fn head_html(theme: Theme, text_scale: f64) -> String {
    let size = text_size_attr(text_scale);
    let css = css_bundle().replace("</style", "<\\/style");
    format!(
        "<meta name=\"color-scheme\" content=\"dark light\">\
         <script>document.documentElement.setAttribute('data-theme','{}');         document.documentElement.style.setProperty('--ui-scale','{text_scale:.2}');\
         document.documentElement.setAttribute('data-text','{size}');</script>\
         <style id=\"ride-css\">{css}</style>",
        theme_attr(theme)
    )
}

// ───────────────────────────── keyboard bridge ─────────────────────────────

/// Page script: forwards the shortcut keys to Rust (`dioxus.send`). Keys typed into form
/// fields are left alone (except Esc / F11 / Ctrl+O). Never resolves, so the channel stays open.
const KEY_BRIDGE_JS: &str = r#"
const isField = (el) => !!el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName));
const names = { ' ': 'space', 'ArrowUp': 'up', 'ArrowDown': 'down', 'ArrowLeft': 'left', 'ArrowRight': 'right', 'Escape': 'esc', 'F11': 'f11' };
const known = ['space', 'up', 'down', 'left', 'right', 'esc', 'f11', 'ctrl+o', 'm', 'p', 't', 'v', 'x', 'w'];
const repeats = ['up', 'down', 'left', 'right'];
window.addEventListener('keydown', (e) => {
  if (e.altKey || e.metaKey || e.isComposing) return;
  let key = names[e.key] || (e.key && e.key.length === 1 ? e.key.toLowerCase() : null);
  if (!key) return;
  if (e.ctrlKey) {
    if (key !== 'o') return;
    key = 'ctrl+o';
  }
  if (!known.includes(key)) return;
  const field = isField(e.target);
  if (field && !['esc', 'f11', 'ctrl+o'].includes(key)) return;
  if (e.repeat && !repeats.includes(key)) { e.preventDefault(); return; }
  if (!repeats.includes(key)) e.preventDefault();
  dioxus.send(key);
}, true);
// A focused button would otherwise be "clicked" by Space on key release.
window.addEventListener('keyup', (e) => {
  if (e.key === ' ' && !isField(e.target)) e.preventDefault();
}, true);
await new Promise(() => {});
"#;

fn use_key_bridge(ctl: Controller) {
    use_future(move || async move {
        let mut bridge = document::eval(KEY_BRIDGE_JS);
        while let Ok(key) = bridge.recv::<String>().await {
            ctl.on_key(&key);
        }
    });
}

/// `RIDE_TILE_PROBE=1`: loads one tile through the `tiles` protocol exactly like the map's
/// `<img>` elements do and reports the outcome on stderr (end-to-end check of the proxy).
fn use_tile_probe() {
    use_future(move || async move {
        if !crate::state::boot().hooks.tile_probe {
            return;
        }
        let url = ride_ui_map::tile_url(ride_model::MapStyle::Dark, 5, 17, 11);
        let script = format!(
            "const img = new Image();             const done = new Promise((resolve) => {{               img.onload = () => resolve({{ ok: true, width: img.naturalWidth, height: img.naturalHeight }});               img.onerror = () => resolve({{ ok: false }});             }});             img.src = '{url}';             dioxus.send(await done);"
        );
        let mut probe = document::eval(&script);
        match probe.recv::<serde_json::Value>().await {
            Ok(result) => eprintln!("[ride-app] tile probe {url}: {result}"),
            Err(err) => eprintln!("[ride-app] tile probe {url}: no answer ({err})"),
        }
    });
}

/// Mirrors the theme setting into `<html data-theme>` and the text size setting into the
/// `--ui-scale` property every type size of the design is multiplied by.
fn use_theme_sync(st: AppState) {
    use_effect(move || {
        let theme = st.settings.read().ui.theme;
        document::eval(&format!(
            "document.documentElement.setAttribute('data-theme', '{}');",
            theme_attr(theme)
        ));
    });
    use_effect(move || {
        let scale = st.settings.read().ui.text_scale();
        document::eval(&format!(
            "document.documentElement.style.setProperty('--ui-scale', '{scale:.2}');\
             document.documentElement.setAttribute('data-text', '{}');",
            text_size_attr(scale)
        ));
    });
}

// ───────────────────────────── root ─────────────────────────────

#[component]
pub fn App() -> Element {
    let win = use_window();
    let st = use_hook(AppState::new);
    let ctl = use_hook(move || {
        let ctl = Controller::new(st, win);
        ctl.boot();
        ctl
    });
    use_context_provider(move || st);
    use_context_provider(move || ctl);
    use_theme_sync(st);
    use_key_bridge(ctl);
    use_tile_probe();

    let mut drag = st.drag_active;
    let active_drag = *st.drag.read();
    let mut viewport_w = use_signal(|| 0.0_f64);
    let compact = is_compact(*viewport_w.read());
    // The window width decides between the tiling (desktop, landscape tablet) and the tabbed
    // compact layout (phone, portrait tablet); the script reports every resize / rotation.
    let on_mount = move |_| {
        let Some(document) = try_consume_context::<Rc<dyn document::Document>>() else {
            return;
        };
        let mut eval = document.eval(VIEWPORT_SCRIPT.to_string());
        spawn(async move {
            while let Ok(width) = eval.recv::<f64>().await {
                if (*viewport_w.peek() - width).abs() > 0.5 {
                    viewport_w.set(width);
                }
            }
        });
    };
    rsx! {
        div {
            class: if compact { "shell shell--compact" } else { "shell" },
            id: "shell",
            onmounted: on_mount,
            ondragover: move |e| {
                e.prevent_default();
                if !*drag.peek() {
                    drag.set(true);
                }
            },
            ondragleave: move |_| {
                if *drag.peek() {
                    drag.set(false);
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                drag.set(false);
                if let Some(file) = e.files().into_iter().next() {
                    ctl.open_path(file.path());
                }
            },
            TopBar {}
            if compact {
                CompactArea {}
            } else {
                WorkArea {}
            }
            if let Some(op) = active_drag {
                DragCatcher { op }
            }
            Dialogs {}
            StravaBar {}
            DropOverlay {}
        }
    }
}

/// The cursor a drag shows while the full-window catcher is up.
fn drag_cursor(op: DragOp) -> &'static str {
    match op {
        DragOp::Carry { .. } => "carry",
        DragOp::Divider { divider } => match divider.axis {
            dock::Axis::Cols => "ew",
            dock::Axis::Rows => "ns",
        },
    }
}

/// Full-window mouse catcher that turns a press on a title bar, a divider or a parked chip into
/// a drag. Rendered only while a drag is active, so the panels below never see the move events.
/// Every position is turned into grid cells before it touches the layout, so dividers move in
/// whole cells and a carried panel lands on a panel edge or middle.
#[component]
fn DragCatcher(op: DragOp) -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let mut drag = st.drag;
    let mut dock_sig = st.dock;
    let workspace = st.workspace;
    // the tiling as it was when the drag started: every move is applied to it afresh
    let start_dock = use_hook(|| dock_sig.peek().clone());

    let mut apply = move |x: f64, y: f64| {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let work = *workspace.peek();
        match op {
            DragOp::Divider { divider } => {
                let Some((cx, cy)) = work.cell_pos_unbounded(x, y) else {
                    return;
                };
                let boundary = match divider.axis {
                    dock::Axis::Cols => cx,
                    dock::Axis::Rows => cy,
                };
                let ratio = divider.ratio_for(boundary);
                let layout = start_dock.resized(divider.path, ratio, st.availability());
                if layout != *dock_sig.peek() {
                    dock_sig.set(layout);
                }
            }
            DragOp::Carry {
                id,
                parked,
                start_x,
                start_y,
                travelled,
                target,
            } => {
                let travelled = travelled || (x - start_x).hypot(y - start_y) > CLICK_SLOP;
                let target_now = if travelled {
                    let layout = dock_sig.peek().layout(st.availability());
                    work.cell_pos(x, y)
                        .and_then(|(px, py)| dock::drop_target(&layout, px, py, id, !parked))
                } else {
                    None
                };
                if travelled != op_travelled(op) || target_now != target {
                    drag.set(Some(DragOp::Carry {
                        id,
                        parked,
                        start_x,
                        start_y,
                        travelled,
                        target: target_now,
                    }));
                }
            }
        }
    };
    let mut finish = move |commit: bool| {
        let current = *drag.peek();
        drag.set(None);
        match current {
            Some(DragOp::Divider { .. }) => ctl.save_dock(dock_sig.peek().clone(), true),
            Some(DragOp::Carry {
                id,
                parked,
                travelled,
                target,
                ..
            }) if commit => match (travelled, target) {
                // dropped on a panel: next to it, or in place of it
                (true, Some(target)) => {
                    if parked {
                        ctl.show_panel(id, Some(target));
                    } else {
                        ctl.move_panel(id, target);
                    }
                }
                // released somewhere that is no panel: nothing happens
                (true, None) => {}
                // a click on a chip brings the panel back where it was
                (false, _) if parked => ctl.show_panel(id, None),
                (false, _) => {}
            },
            _ => {}
        }
    };
    let cursor = drag_cursor(op);
    rsx! {
        div {
            class: "shell-dragcatcher",
            "data-op": "{cursor}",
            role: "presentation",
            onmousemove: move |e: Event<MouseData>| {
                let p = e.client_coordinates();
                apply(p.x, p.y);
            },
            onmouseup: move |_| finish(true),
            // leaving the window ends a divider drag where it is, but a carried panel is not
            // dropped anywhere
            onmouseleave: move |_| finish(false),
        }
    }
}

fn op_travelled(op: DragOp) -> bool {
    matches!(
        op,
        DragOp::Carry {
            travelled: true,
            ..
        }
    )
}

// ───────────────────────────── top bar ─────────────────────────────

/// One status dot next to "Eszközök".
#[derive(Debug, Clone, PartialEq)]
pub struct Dot {
    /// `ok` | `busy` | `bad` | `off` (styled via `data-state`).
    pub state: &'static str,
    pub title: String,
}

/// Aggregates the slots of `kinds` into one dot: connected beats connecting beats failed.
pub fn device_dot(slots: &[DeviceSlot], kinds: &[DeviceKind], label: &str) -> Dot {
    let relevant: Vec<&DeviceSlot> = slots.iter().filter(|s| kinds.contains(&s.kind)).collect();
    let pick = |wanted: fn(&ConnState) -> bool| relevant.iter().find(|s| wanted(&s.state)).copied();
    if let Some(slot) = pick(|s| *s == ConnState::Connected) {
        let name = slot
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        return Dot {
            state: "ok",
            title: format!("{label}: csatlakoztatva{name}"),
        };
    }
    if pick(|s| *s == ConnState::Connecting).is_some() {
        return Dot {
            state: "busy",
            title: format!("{label}: csatlakozás…"),
        };
    }
    if let Some(slot) = pick(|s| matches!(s, ConnState::Failed(_))) {
        let reason = match &slot.state {
            ConnState::Failed(r) if !r.trim().is_empty() => format!(" ({r})"),
            _ => String::new(),
        };
        return Dot {
            state: "bad",
            title: format!("{label}: a kapcsolódás nem sikerült{reason}"),
        };
    }
    Dot {
        state: "off",
        title: format!("{label}: nincs csatlakoztatva"),
    }
}

/// The three dots: trainer, heart-rate strap, other sensors (power meter / cadence).
pub fn device_dots(slots: &[DeviceSlot]) -> [Dot; 3] {
    [
        device_dot(
            slots,
            &[DeviceKind::Trainer],
            kind_label(DeviceKind::Trainer),
        ),
        device_dot(
            slots,
            &[DeviceKind::HeartRate],
            kind_label(DeviceKind::HeartRate),
        ),
        device_dot(
            slots,
            &[DeviceKind::PowerMeter, DeviceKind::Cadence],
            "Teljesítmény- és kadenciaszenzor",
        ),
    ]
}

#[component]
fn TopBar() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let route = st.route.read().clone();
    let theme = st.settings.read().ui.theme;
    let av = st.availability_live();
    let dots = device_dots(&st.slots.read());
    let (theme_icon, theme_label) = match theme {
        Theme::Dark => (IconName::Sun, "Váltás világos témára (T)"),
        Theme::Light => (IconName::Moon, "Váltás sötét témára (T)"),
    };
    let dragged_chip = match *st.drag.read() {
        Some(DragOp::Carry {
            id, parked: true, ..
        }) => Some(id),
        _ => None,
    };
    let parked: Vec<PanelId> = {
        let tiled = st.dock.read();
        PanelId::ALL
            .into_iter()
            .filter(|id| !tiled.contains(*id) && av.has(*id))
            .collect()
    };
    let mut drag = st.drag;
    let update = st.update.read().clone();
    let updating = *st.updating.read();

    rsx! {
        header { class: "shell-top", role: "banner",
            div { class: "shell-logo",
                span { class: "shell-logo__mark", Icon { name: IconName::Bike, size: 20 } }
                span { "bike-osm-ride" }
            }
            if let Some(route) = route {
                div { class: "shell-route", title: "{route.name}",
                    span { class: "shell-route__name", "{route.name}" }
                    span { class: "shell-route__meta",
                        "{fmt_km(route.total_dist)}"
                        small { "km" }
                    }
                }
            }
            div { class: "shell-tray", role: "group", aria_label: "Bezárt panelek",
                for id in parked {
                    button {
                        key: "{id.key()}",
                        class: "shell-chip",
                        r#type: "button",
                        "data-dragging": if dragged_chip == Some(id) { "true" } else { "false" },
                        title: "{id.title()}: kattints a megjelenítéshez, vagy húzd a felületre",
                        // a press starts the drag; the release decides between click and drop
                        onmousedown: move |e: Event<MouseData>| {
                            e.prevent_default();
                            let p = e.client_coordinates();
                            drag.set(Some(DragOp::Carry {
                                id,
                                parked: true,
                                start_x: p.x,
                                start_y: p.y,
                                travelled: false,
                                target: None,
                            }));
                        },
                        onkeydown: move |e: Event<KeyboardData>| {
                            if matches!(e.key(), Key::Enter) || e.key() == Key::Character(" ".into()) {
                                e.prevent_default();
                                ctl.show_panel(id, None);
                            }
                        },
                        span { class: "shell-grip", aria_hidden: "true",
                            i {}
                            i {}
                            i {}
                            i {}
                            i {}
                            i {}
                        }
                        span { "{id.title()}" }
                    }
                }
            }
            nav { class: "shell-nav", aria_label: "Fő műveletek",
                if let Some(update) = update {
                    button {
                        class: "shell-tb shell-tb--update",
                        r#type: "button",
                        disabled: updating,
                        aria_label: "Frissítés: {update.version}",
                        title: "Új verzió: {update.version}",
                        onclick: move |_| ctl.install_update(),
                        Icon { name: IconName::Download, size: 16 }
                        span { class: "shell-label",
                            if updating { "Frissítés…" } else { "Frissítés" }
                        }
                    }
                }
                button {
                    class: "shell-tb",
                    r#type: "button",
                    aria_label: "Edzés",
                    title: "Edzés kiválasztása (W)",
                    onclick: move |_| ctl.open_dialog(DialogKind::Workouts),
                    Icon { name: IconName::Trophy, size: 16 }
                    span { class: "shell-label", "Edzés" }
                }
                button {
                    class: "shell-tb",
                    r#type: "button",
                    aria_label: "Útvonal",
                    title: "Útvonal kiválasztása (Ctrl+O megnyitja a fájlválasztót)",
                    onclick: move |_| ctl.open_dialog(DialogKind::Route),
                    Icon { name: IconName::Folder, size: 16 }
                    span { class: "shell-label", "Útvonal" }
                }
                button {
                    class: "shell-tb",
                    r#type: "button",
                    aria_label: "Eszközök",
                    onclick: move |_| ctl.open_dialog(DialogKind::Devices),
                    Icon { name: IconName::Bluetooth, size: 16 }
                    span { class: "shell-label", "Eszközök" }
                    span { class: "shell-dots", aria_hidden: "true",
                        for dot in dots {
                            span { class: "shell-dot", "data-state": dot.state, title: "{dot.title}" }
                        }
                    }
                }
                button {
                    class: "shell-tb",
                    r#type: "button",
                    aria_label: "Beállítások",
                    onclick: move |_| ctl.open_dialog(DialogKind::Settings),
                    Icon { name: IconName::Settings, size: 16 }
                    span { class: "shell-label", "Beállítások" }
                }
                button {
                    class: "shell-tb shell-tb--icon",
                    r#type: "button",
                    aria_label: "{theme_label}",
                    title: "{theme_label}",
                    onclick: move |_| ctl.toggle_theme(),
                    Icon { name: theme_icon, size: 16 }
                }
                button {
                    class: "shell-tb shell-tb--icon",
                    r#type: "button",
                    aria_label: "Teljes képernyő (F11)",
                    title: "Teljes képernyő (F11)",
                    onclick: move |_| ctl.toggle_fullscreen(),
                    Icon { name: IconName::Fullscreen, size: 16 }
                }
                button {
                    class: "shell-tb shell-tb--icon",
                    r#type: "button",
                    aria_label: "Névjegy és licencek",
                    title: "Névjegy és licencek",
                    onclick: move |_| ctl.open_dialog(DialogKind::About),
                    Icon { name: IconName::Info, size: 16 }
                }
            }
        }
    }
}

// ───────────────────────────── workspace ─────────────────────────────

/// Page script that reports where the workspace is and how big it is whenever that changes, so
/// a pointer position can be turned into a grid cell.
const WORKSPACE_SCRIPT: &str = r#"(function () {
  var el = document.getElementById("shell-stage");
  if (!el) return;
  var send = function () {
    var r = el.getBoundingClientRect();
    dioxus.send([r.left, r.top, r.width, r.height]);
  };
  if (typeof ResizeObserver === "function") {
    try { new ResizeObserver(send).observe(el); } catch (e) {}
  }
  window.addEventListener("resize", send);
  send();
  return new Promise(function () {});
})();"#;

#[component]
fn WorkArea() -> Element {
    let st = use_context::<AppState>();
    let av = st.availability_live();
    let layout = st.dock.read().layout(av);
    let (carried, ghost, dragged_divider) = match *st.drag.read() {
        Some(DragOp::Carry {
            id,
            travelled,
            target,
            ..
        }) if travelled => (
            Some(id),
            target.and_then(|target| dock::drop_ghost(&layout, target)),
            None,
        ),
        Some(DragOp::Divider { divider }) => (None, None, Some(divider.path)),
        _ => (None, None, None),
    };
    let mut workspace = st.workspace;
    let mut drag = st.drag;

    // Measure the workspace once it is in the page; the observer keeps it current while the
    // window or the browser zoom change it.
    let on_mount = move |_| {
        let Some(document) = try_consume_context::<Rc<dyn document::Document>>() else {
            return;
        };
        let mut eval = document.eval(WORKSPACE_SCRIPT.to_string());
        spawn(async move {
            while let Ok(values) = eval.recv::<Vec<f64>>().await {
                if let [left, top, width, height] = values[..] {
                    let next = Workspace {
                        left,
                        top,
                        width,
                        height,
                    };
                    if *workspace.peek() != next {
                        workspace.set(next);
                    }
                }
            }
        });
    };

    // Always in the order of PanelId::ALL, so the DOM nodes (and the Street View frame inside
    // one) are never moved when the tiling changes.
    let tiles: Vec<dock::Tile> = PanelId::ALL
        .iter()
        .filter_map(|id| layout.tile(*id))
        .collect();

    rsx! {
        main {
            id: "shell-stage",
            class: "dock",
            aria_label: "Munkaterület",
            onmounted: on_mount,
            for tile in tiles {
                PanelFrame {
                    key: "{tile.id.key()}",
                    id: tile.id,
                    rect: tile.rect,
                    carried: carried == Some(tile.id),
                    {panel_body(tile.id)}
                }
            }
            for divider in layout.dividers.iter().copied() {
                div {
                    key: "{divider.path:?}",
                    class: if divider.axis == dock::Axis::Cols { "dock-divider dock-divider--cols" } else { "dock-divider dock-divider--rows" },
                    "data-dragging": if dragged_divider == Some(divider.path) { "true" } else { "false" },
                    role: "separator",
                    "aria-orientation": if divider.axis == dock::Axis::Cols { "vertical" } else { "horizontal" },
                    style: "{divider_style(divider)}",
                    onmousedown: move |e: Event<MouseData>| {
                        e.prevent_default();
                        e.stop_propagation();
                        drag.set(Some(DragOp::Divider { divider }));
                    },
                }
            }
            if let Some(rect) = ghost {
                div { class: "dock-ghost", style: "{rect.style()}" }
            }
        }
    }
}

/// Where a divider's hit area sits: on the line, over the full extent of the split.
fn divider_style(divider: dock::Divider) -> String {
    let line = divider.line();
    match divider.axis {
        dock::Axis::Cols => format!(
            "left:{:.4}%;top:{:.4}%;height:{:.4}%",
            line.x / dock::COLS * 100.0,
            line.y / dock::ROWS * 100.0,
            line.h / dock::ROWS * 100.0
        ),
        dock::Axis::Rows => format!(
            "left:{:.4}%;top:{:.4}%;width:{:.4}%",
            line.x / dock::COLS * 100.0,
            line.y / dock::ROWS * 100.0,
            line.w / dock::COLS * 100.0
        ),
    }
}

// ───────────────────────────── compact layout ─────────────────────────────

/// Below this window width the panels are shown in tabs instead of the tiling.
pub const COMPACT_BELOW_PX: f64 = 1000.0;

/// `true` for a phone or a portrait tablet (an unmeasured window, width 0, is a desktop).
pub fn is_compact(width: f64) -> bool {
    width > 0.0 && width < COMPACT_BELOW_PX
}

/// Page script that reports the window width on load and on every resize / rotation.
const VIEWPORT_SCRIPT: &str = r#"(function () {
  var send = function () { dioxus.send(window.innerWidth); };
  window.addEventListener("resize", send);
  send();
  return new Promise(function () {});
})();"#;

/// The tabs of the compact layout. "Tekerés" is the screen to ride with: everything needed
/// mid-effort (workout block, power against its target, cadence / heart rate / speed / grade and
/// the profile or the workout blocks) is on it, so no tab switch is needed while pedalling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactTab {
    Ride,
    Map,
    Charts,
    Data,
}

impl CompactTab {
    pub const ALL: [CompactTab; 4] = [
        CompactTab::Ride,
        CompactTab::Map,
        CompactTab::Charts,
        CompactTab::Data,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CompactTab::Ride => "Tekerés",
            CompactTab::Map => "Térkép",
            CompactTab::Charts => "Grafikonok",
            CompactTab::Data => "Adatok",
        }
    }

    fn key(self) -> &'static str {
        match self {
            CompactTab::Ride => "ride",
            CompactTab::Map => "map",
            CompactTab::Charts => "charts",
            CompactTab::Data => "data",
        }
    }

    fn icon(self) -> IconName {
        match self {
            CompactTab::Ride => IconName::Gauge,
            CompactTab::Map => IconName::Route,
            CompactTab::Charts => IconName::Bolt,
            CompactTab::Data => IconName::Clock,
        }
    }
}

/// Which tab shows a panel; `None` for the controls (always on screen, with the gear buttons),
/// the workout card (the ride tab's head unit carries the workout) and the docked browser (a
/// desktop-only feature). On the ride tab the "metrics" panel is the phone head unit
/// ([`RideDash`]) and the profile follows the Elevation / Workout switch.
pub fn compact_tab_of(id: PanelId) -> Option<CompactTab> {
    match id {
        PanelId::Metrics | PanelId::Profile => Some(CompactTab::Ride),
        PanelId::Map => Some(CompactTab::Map),
        PanelId::Charts => Some(CompactTab::Charts),
        PanelId::Climb | PanelId::Stats | PanelId::Gears | PanelId::Rider => Some(CompactTab::Data),
        PanelId::Workout | PanelId::Controls | PanelId::Browser => None,
    }
}

/// Order of the panels inside a compact tab (stable, so the keyed DOM nodes never move): the
/// head unit above the profile, the next climb before the statistics.
fn compact_rank(id: PanelId) -> u8 {
    match id {
        PanelId::Metrics => 0,
        PanelId::Profile => 1,
        PanelId::Climb => 2,
        PanelId::Stats => 3,
        PanelId::Gears => 4,
        PanelId::Rider => 5,
        _ => 6,
    }
}

/// The content of a panel in the compact layout: like the tiling's, except the live numbers,
/// which become the phone head unit, and the profile, whose views are turned by swiping.
fn compact_panel_body(id: PanelId) -> Element {
    match id {
        PanelId::Metrics => rsx! { DashPanel {} },
        PanelId::Profile => rsx! { ProfilePanel { phone: true } },
        _ => panel_body(id),
    }
}

/// Phone / portrait-tablet layout: one tab of panels at a time, the ride controls and the tab
/// bar pinned to the bottom. Every panel stays mounted (hidden ones only get `is-hidden`), so
/// the map and the Street View frame are never reloaded by switching tabs.
#[component]
fn CompactArea() -> Element {
    let st = use_context::<AppState>();
    let av = st.availability_live();
    let mut tab = use_signal(|| CompactTab::Ride);
    let active = *tab.read();
    let mut panels: Vec<(PanelId, CompactTab)> = PanelId::ALL
        .into_iter()
        .filter(|id| av.has(*id))
        .filter_map(|id| compact_tab_of(id).map(|t| (id, t)))
        .collect();
    panels.sort_by_key(|(id, _)| compact_rank(*id));

    rsx! {
        main {
            id: "shell-stage",
            class: "compact",
            "data-tab": active.key(),
            aria_label: "Munkaterület",
            div { class: "compact-stage",
                for (id, t) in panels {
                    section {
                        key: "{id.key()}",
                        class: if t == active { "compact-panel" } else { "compact-panel is-hidden" },
                        "data-panel": id.key(),
                        aria_label: "{id.title()}",
                        div { class: "compact-panel__body", {compact_panel_body(id)} }
                    }
                }
                // the map tab also carries the power line above the map and the elevation under it,
                // so the route can be followed without losing sight of the effort
                section {
                    key: "map-power",
                    class: if active == CompactTab::Map { "compact-panel compact-extra compact-extra--power" } else { "compact-panel compact-extra compact-extra--power is-hidden" },
                    aria_label: "Teljesítmény",
                    StripPanel {}
                }
                section {
                    key: "map-profile",
                    class: if active == CompactTab::Map { "compact-panel compact-extra compact-extra--profile" } else { "compact-panel compact-extra compact-extra--profile is-hidden" },
                    aria_label: "Magassági profil",
                    div { class: "compact-panel__body", MapProfile {} }
                }
            }
            div { class: "compact-controls",
                ControlsPanel {}
                CompactGears {}
            }
            nav { class: "compact-tabs", role: "tablist", aria_label: "Nézetek",
                for t in CompactTab::ALL {
                    button {
                        key: "{t.key()}",
                        class: if t == active { "compact-tab is-active" } else { "compact-tab" },
                        r#type: "button",
                        role: "tab",
                        "aria-selected": if t == active { "true" } else { "false" },
                        onclick: move |_| tab.set(t),
                        Icon { name: t.icon(), size: 20 }
                        span { "{t.label()}" }
                    }
                }
            }
        }
    }
}

/// The content of one panel.
fn panel_body(id: PanelId) -> Element {
    match id {
        PanelId::Map => rsx! { MapPanel {} },
        PanelId::Profile => rsx! { ProfilePanel {} },
        PanelId::Controls => rsx! { ControlsPanel {} },
        PanelId::Metrics => rsx! { MetricsPanel {} },
        PanelId::Climb => rsx! { ClimbPanel {} },
        PanelId::Stats => rsx! { StatsBody {} },
        PanelId::Charts => rsx! { ChartsPanel {} },
        PanelId::Workout => rsx! { WorkoutPanel {} },
        PanelId::Rider => rsx! { RiderPanel {} },
        PanelId::Gears => rsx! { GearsPanel {} },
        PanelId::Browser => rsx! { BrowserPanel {} },
    }
}

/// The frame every panel shares: a title bar to carry it by and a close button. The panel
/// is placed by the tiling, there is nothing to resize on it; the dividers do that.
#[component]
fn PanelFrame(id: PanelId, rect: dock::Rect, carried: bool, children: Element) -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let mut drag = st.drag;
    let title = id.title();
    let sv_on = st.settings.read().ui.streetview_enabled;

    let start_carry = move |e: Event<MouseData>| {
        e.prevent_default();
        let p = e.client_coordinates();
        drag.set(Some(DragOp::Carry {
            id,
            parked: false,
            start_x: p.x,
            start_y: p.y,
            travelled: false,
            target: None,
        }));
    };

    rsx! {
        section {
            class: "dock-panel",
            "data-panel": id.key(),
            "data-dragging": if carried { "true" } else { "false" },
            aria_label: "{title}",
            style: "{rect.style()}",
            header { class: "dock-bar", onmousedown: start_carry,
                span { class: "shell-grip", aria_hidden: "true",
                    i {}
                    i {}
                    i {}
                    i {}
                    i {}
                    i {}
                }
                span { class: "dock-title", "{title}" }
                if id == PanelId::Map {
                    button {
                        class: "dock-act",
                        r#type: "button",
                        "aria-pressed": "{sv_on}",
                        title: "Street View a térképen: kis ablak jobb alul, cserélhető (V, X)",
                        onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                        onclick: move |_| ctl.toggle_streetview(),
                        span { class: "dock-act__sq", aria_hidden: "true" }
                        "Street View"
                    }
                }
                button {
                    class: "dock-close",
                    r#type: "button",
                    aria_label: "{title} bezárása",
                    title: "Bezárás (a felső sávba kerül)",
                    // the button must not start a drag
                    onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                    onclick: move |_| ctl.hide_panel(id),
                    Icon { name: IconName::Close, size: 12 }
                }
            }
            div { class: if id.fills() { "dock-body dock-body--fill" } else { "dock-body" },
                if id.fills() {
                    div { class: "dock-fill", {children} }
                } else {
                    {children}
                }
            }
        }
    }
}

// ───────────────────────────── panel contents ─────────────────────────────

#[component]
fn MapPanel() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let settings = st.settings;
    let map_style = use_memo(move || settings.read().ui.map_style);
    let follow = use_memo(move || settings.read().ui.follow_rider);
    let ui = use_memo(move || settings.read().ui.clone());
    let (sv_on, sv_big) = {
        let s = settings.read();
        (s.ui.streetview_enabled, s.ui.streetview_large)
    };
    // Street View needs a route (and its session, with the id prefetcher, lives exactly as long
    // as it is shown here).
    let sv = sv_on && st.route.read().is_some();
    let sv_focus = sv && sv_big;
    rsx! {
        div { class: "tile-stack", "data-focus": if sv_focus { "street-view" } else { "map" },
            // The map is the same element whether it is big or small, so its pan and zoom
            // survive a swap.
            div {
                class: if sv_focus { "tile-view tile-view--small" } else { "tile-view tile-view--big" },
                if sv_focus {
                    div { class: "tile-pip__bar",
                        span { class: "tile-pip__title", "Térkép" }
                        button {
                            class: "tile-pip__btn",
                            r#type: "button",
                            aria_label: "Térkép és Street View cseréje",
                            title: "Csere: a térkép lesz nagy (X)",
                            onclick: move |_| ctl.toggle_streetview_size(),
                            Icon { name: IconName::Fullscreen, size: 12 }
                        }
                    }
                }
                div { class: "tile-view__body",
                    MapView {
                        route: st.route,
                        snapshot: st.snapshot,
                        style: map_style,
                        follow,
                        on_style: move |style| ctl.set_map_style(style),
                        on_follow: move |on| ctl.set_follow(on),
                        compact: sv_focus,
                    }
                }
            }
            if sv {
                div { class: if sv_focus { "tile-view tile-view--big" } else { "tile-view tile-view--small" },
                    StreetViewPanel {
                        route: st.route,
                        snapshot: st.snapshot,
                        settings: ui,
                        embedded: true,
                        on_close: move |_| ctl.set_streetview(false),
                        on_toggle_size: move |_| ctl.toggle_streetview_size(),
                        on_open_settings: move |_| ctl.open_dialog(DialogKind::Settings),
                    }
                }
            }
        }
    }
}

/// The profile: the terrain, or the workout's blocks while one is selected and the rider chose
/// them. `phone` turns the header buttons into swipe pages.
#[component]
fn ProfilePanel(#[props(default)] phone: bool) -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let settings = st.settings;
    let imported = st.imported_workouts;
    let theme = use_memo(move || settings.read().ui.theme);
    let mode = use_memo(move || settings.read().ui.active_profile_mode());
    let lookahead = use_memo(move || settings.read().ui.lookahead_m);
    let ftp = use_memo(move || settings.read().rider.ftp_w);
    let workout_id = use_memo(move || settings.read().ui.workout_id.clone());
    // the selected workout itself (its steps are drawn), resolved like the controller does
    let workout = use_memo(move || {
        let id = workout_id.read().clone()?;
        WorkoutLibrary::with_imported(imported.read().clone())
            .get(&id)
            .cloned()
    });
    let has_workout = workout.read().is_some();
    let view = settings.read().ui.profile_view;
    let pages = phone.then(|| phone_pages(st, ctl, has_workout));
    if has_workout && view == ProfileView::Workout {
        return rsx! {
            WorkoutProfile {
                workout,
                snapshot: st.snapshot,
                ftp_w: ftp,
                mode,
                on_mode: move |mode: ProfileMode| ctl.set_profile_mode(mode),
                on_view: move |view| ctl.set_profile_view(view),
                pages,
            }
        };
    }
    rsx! {
        ElevationProfile {
            route: st.route,
            snapshot: st.snapshot,
            mode,
            lookahead_m: lookahead,
            theme,
            on_seek: move |dist| ctl.seek(dist),
            on_mode: move |mode: ProfileMode| ctl.set_profile_mode(mode),
            on_view: has_workout.then_some(EventHandler::new(move |view| ctl.set_profile_view(view))),
            pages,
        }
    }
}

#[component]
fn ControlsPanel() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    rsx! {
        RideControls {
            snapshot: st.snapshot,
            on_start: move |_| ctl.start(),
            on_pause: move |_| ctl.pause(),
            on_resume: move |_| ctl.start(),
            on_finish: move |_| ctl.finish(),
            on_reset: move |_| ctl.reset(),
        }
    }
}

#[component]
fn MetricsPanel() -> Element {
    let st = use_context::<AppState>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    let rider = use_memo(move || settings.read().rider.clone());
    rsx! { MetricTiles { snapshot: st.snapshot, rider, theme } }
}

#[component]
fn ClimbPanel() -> Element {
    let st = use_context::<AppState>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    rsx! { ClimbCard { route: st.route, snapshot: st.snapshot, theme, prs: st.segment_prs } }
}

#[component]
fn StatsBody() -> Element {
    let st = use_context::<AppState>();
    rsx! { StatsPanel { snapshot: st.snapshot } }
}

#[component]
fn ChartsPanel() -> Element {
    let st = use_context::<AppState>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    let rider = use_memo(move || settings.read().rider.clone());
    rsx! {
        LiveChart { history: st.history, metric: ChartMetric::Power, rider, theme }
        LiveChart { history: st.history, metric: ChartMetric::HeartRate, rider, theme }
        LiveChart { history: st.history, metric: ChartMetric::Cadence, rider, theme }
    }
}

/// The head unit above the phone's map: the same numbers as on the ride tab, the workout in one
/// line.
#[component]
fn StripPanel() -> Element {
    let st = use_context::<AppState>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    let rider = use_memo(move || settings.read().rider.clone());
    rsx! { RideDash { snapshot: st.snapshot, rider, theme, slim: true } }
}

/// One page of the phone's profile: what it draws and how far it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilePage {
    pub view: ProfileView,
    pub mode: ProfileMode,
}

impl ProfilePage {
    pub fn label(self) -> &'static str {
        match (self.view, self.mode) {
            (ProfileView::Workout, ProfileMode::Lookahead) => "Edzés · előre",
            (ProfileView::Workout, ProfileMode::Full) => "Edzés · teljes",
            (ProfileView::Elevation, ProfileMode::Lookahead) => "Szint · előre",
            (ProfileView::Elevation, ProfileMode::Full) => "Szint · teljes",
        }
    }
}

/// The pages a swipe turns through: the workout's two (when one is selected and `workout` is
/// allowed), then the terrain's two, the look-ahead first.
pub fn profile_pages(workout: bool) -> Vec<ProfilePage> {
    let views: &[ProfileView] = if workout {
        &[ProfileView::Workout, ProfileView::Elevation]
    } else {
        &[ProfileView::Elevation]
    };
    views
        .iter()
        .flat_map(|&view| {
            [ProfileMode::Lookahead, ProfileMode::Full]
                .into_iter()
                .map(move |mode| ProfilePage { view, mode })
        })
        .collect()
}

/// The swipe pages of a phone profile, at the page the settings describe.
fn phone_pages(st: AppState, ctl: Controller, workout: bool) -> Pages {
    let ui = st.settings.read().ui.clone();
    let list = profile_pages(workout);
    let now = ProfilePage {
        view: if workout {
            ui.profile_view
        } else {
            ProfileView::Elevation
        },
        mode: ui.active_profile_mode(),
    };
    let current = list.iter().position(|p| *p == now).unwrap_or(0);
    Pages {
        labels: list.iter().map(|p| p.label().to_string()).collect(),
        current,
        hint: !ui.profile_swipe_learned,
        on_page: EventHandler::new(move |i: usize| {
            if let Some(page) = list.get(i) {
                ctl.set_profile_page(page.view, page.mode);
            }
        }),
    }
}

/// The elevation under the phone's map: always the terrain (the workout blocks are on the ride
/// tab); a swipe switches between the look-ahead and the whole route.
#[component]
fn MapProfile() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    let mode = use_memo(move || settings.read().ui.active_profile_mode());
    let lookahead = use_memo(move || settings.read().ui.lookahead_m);
    let pages = phone_pages(st, ctl, false);
    rsx! {
        ElevationProfile {
            route: st.route,
            snapshot: st.snapshot,
            mode,
            lookahead_m: lookahead,
            theme,
            on_seek: move |dist| ctl.seek(dist),
            on_mode: move |mode: ProfileMode| ctl.set_profile_mode(mode),
            grid_steps: 3,
            pages,
        }
    }
}

/// The phone head unit of the ride tab.
#[component]
fn DashPanel() -> Element {
    let st = use_context::<AppState>();
    let settings = st.settings;
    let theme = use_memo(move || settings.read().ui.theme);
    let rider = use_memo(move || settings.read().rider.clone());
    rsx! { RideDash { snapshot: st.snapshot, rider, theme } }
}

/// The virtual gear next to the transport buttons of the phone layout. Shown only when the rider
/// switched the virtual gearbox on and no workout holds the trainer in ERG (there the gear does
/// nothing). With a Zwift Click / Play the hardware shifts, so only the gear is shown; otherwise
/// the shift buttons are there too, so a gear change never needs a tab switch.
#[component]
fn CompactGears() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    if !st.availability_live().gears {
        return rsx! {};
    }
    let index = gearbox::index_from_saved(st.settings.read().ui.gear);
    let count = gearbox::gears().len();
    if st.shifter_connected() {
        return rsx! {
            div { class: "compact-gears compact-gears--readout", role: "status", aria_label: "Fokozat: {index + 1}",
                span { class: "compact-gears__label", "Fokozat" }
                span { class: "compact-gears__num num", "{index + 1}" }
            }
        };
    }
    rsx! {
        div { class: "compact-gears", role: "group", aria_label: "Virtuális váltó",
            button {
                class: "btn compact-gears__btn",
                r#type: "button",
                disabled: index == 0,
                aria_label: "Könnyebb fokozat",
                onclick: move |_| ctl.shift_gear(-1),
                Icon { name: IconName::Minus, size: 18 }
            }
            span { class: "compact-gears__num num", "{index + 1}" }
            button {
                class: "btn compact-gears__btn",
                r#type: "button",
                disabled: index + 1 == count,
                aria_label: "Nehezebb fokozat",
                onclick: move |_| ctl.shift_gear(1),
                Icon { name: IconName::Plus, size: 18 }
            }
        }
    }
}

#[component]
fn WorkoutPanel() -> Element {
    let st = use_context::<AppState>();
    rsx! { WorkoutCard { snapshot: st.snapshot } }
}

#[component]
fn RiderPanel() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let virtual_power = *st.virtual_power.read();
    rsx! {
        VirtualRiderPanel {
            power_w: virtual_power,
            on_power: move |watts| ctl.set_virtual_power(watts),
        }
    }
}

/// The virtual gearbox: shift down / up, the gear, what it does to the trainer and what speed
/// it gives at the current cadence.
#[component]
fn GearsPanel() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let saved = st.settings.read().ui.gear;
    let index = gearbox::index_from_saved(saved);
    let gears = gearbox::gears();
    let gear = gears[index];
    let offset = gearbox::grade_offset(index);
    let cadence = st.snapshot.read().cadence_rpm;
    let speed = cadence
        .filter(|c| *c > 1.0)
        .map(|c| format!("{:.1} km/h", gearbox::speed_kmh(index, c)))
        .unwrap_or_else(|| "—".to_string());
    rsx! {
        div { class: "shell-gear",
            button {
                class: "btn shell-gear__btn",
                r#type: "button",
                disabled: index == 0,
                aria_label: "Könnyebb fokozat",
                title: "Könnyebb fokozat (←)",
                onclick: move |_| ctl.shift_gear(-1),
                Icon { name: IconName::Minus, size: 16 }
            }
            div { class: "shell-gear__num num",
                "{index + 1}"
                small { "/ {gears.len()}" }
            }
            button {
                class: "btn shell-gear__btn",
                r#type: "button",
                disabled: index + 1 == gears.len(),
                aria_label: "Nehezebb fokozat",
                title: "Nehezebb fokozat (→)",
                onclick: move |_| ctl.shift_gear(1),
                Icon { name: IconName::Plus, size: 16 }
            }
            div { class: "shell-gear__info",
                span { class: "shell-gear__ratio", "{gear.label()} · {gear.ratio():.2}" }
                span { class: "shell-gear__sub", "{speed} · ellenállás {offset:+.1} %" }
            }
        }
    }
}

/// Page script that reports where the browser panel's body is whenever that changes (position,
/// size, scale); it ends by itself when the body is gone.
const BROWSER_WATCH_JS: &str = r#"(function () {
  var last = "";
  var timer = null;
  var seen = false;
  var misses = 0;
  var tick = function () {
    var el = document.getElementById("browser-slot");
    var msg = [];
    if (el) {
      seen = true;
      var r = el.getBoundingClientRect();
      msg = [r.left, r.top, r.width, r.height, window.devicePixelRatio || 1];
    }
    var key = msg.join(",");
    if (key !== last) { last = key; dioxus.send(msg); }
    // the panel is not in the page yet when the script starts; give up only when it was there and
    // is gone, or never shows up
    if (!el) { misses += 1; }
    if ((!el && seen) || misses > 100) { clearInterval(timer); }
  };
  timer = setInterval(tick, 120);
  tick();
  return new Promise(function () {});
})();"#;

/// The browser panel: Firefox lives in a native window that the shell keeps over this body
/// (see `browser.rs`). The panel itself only says what is going on.
#[component]
fn BrowserPanel() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    use_hook(move || ctl.browser_start());
    use_drop(move || ctl.browser_stop());
    use_future(move || async move {
        let mut watch = document::eval(BROWSER_WATCH_JS);
        while let Ok(rect) = watch.recv::<Vec<f64>>().await {
            let slot = match rect[..] {
                [x, y, w, h, dpr] => browser::Slot::from_css(x, y, w, h, dpr, 5.0),
                _ => None,
            };
            ctl.browser_place(slot);
        }
    });
    // Firefox moves / maximises itself after it starts: keep putting it back over the panel.
    use_future(move || async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(400));
        loop {
            tick.tick().await;
            browser::enforce();
        }
    });
    // The page cannot draw over a native window, so Firefox steps aside for dialogs and drags.
    use_effect(move || {
        let covered = st.dialog.read().is_some() || st.drag.read().is_some();
        ctl.browser_visible(!covered);
    });
    let note = st.browser_note.read().clone();
    rsx! {
        div { id: "browser-slot", class: "shell-browser",
            if let Some(note) = note {
                p { class: "dock-note", "{note}" }
            }
        }
    }
}

// ───────────────────────────── dialogs, toasts, drop overlay ─────────────────────────────

#[component]
fn Dialogs() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let open = *st.dialog.read();
    let scanning = *st.scanning.read();

    rsx! {
        RouteDialog {
            open: open == Some(DialogKind::Route),
            on_close: move |_| ctl.close_dialog(),
            recents: st.recents,
            busy: st.busy,
            error: st.welcome_error,
            on_pick_file: move |_| ctl.pick_file(),
            on_import_link: move |link| ctl.load_link(link),
            on_open_recent: move |id| ctl.open_recent(id),
            on_delete_recent: move |id| ctl.delete_recent(id),
            on_demo: move |_| ctl.load_demo(),
        }
        DevicesDialog {
            open: open == Some(DialogKind::Devices),
            discovered: st.discovered,
            slots: st.slots,
            scanning,
            on_command: move |cmd| ctl.command(cmd),
            on_close: move |_| ctl.close_dialog(),
        }
        SettingsDialog {
            open: open == Some(DialogKind::Settings),
            settings: st.settings,
            ghost_status: st.ghost_status,
            on_change: move |settings| ctl.apply_settings(settings),
            on_close: move |_| ctl.close_dialog(),
        }
        WorkoutsDialog {
            open: open == Some(DialogKind::Workouts),
            selected: st.settings.read().ui.workout_id.clone(),
            imported: st.imported_workouts,
            ftp_w: st.settings.read().rider.ftp_w,
            on_import: move |_| ctl.import_workout(),
            on_select: move |id| ctl.select_workout(id),
            on_close: move |_| ctl.close_dialog(),
        }
        SummaryDialog {
            open: open == Some(DialogKind::Summary),
            summary: st.summary,
            route: st.route,
            comparison: st.comparison,
            details: st.ride_details,
            on_details: move |details| ctl.set_ride_details(details),
            on_save_tcx: move |_| ctl.save_tcx(),
            uploading: *st.strava_busy.read(),
            on_upload_strava: move |_| ctl.upload_strava(),
            on_close: move |_| ctl.close_dialog(),
            on_new_ride: move |_| ctl.new_ride(),
        }
        AboutDialog { open: open == Some(DialogKind::About), on_close: move |_| ctl.close_dialog() }
        ToastHost { toasts: st.toasts, on_dismiss: move |id| st.dismiss_toast(id) }
    }
}

/// The strip above the Strava upload view (a native child WebView fills the rest of the
/// window): what to do next, and the way back to the summary.
#[component]
fn StravaBar() -> Element {
    let st = use_context::<AppState>();
    let ctl = use_context::<Controller>();
    let Some(status) = st.strava_status.read().clone() else {
        return rsx! {};
    };
    rsx! {
        div { class: "shell-strava", role: "dialog", "aria-label": "Feltöltés Stravára",
            div { class: "shell-strava__bar",
                div { class: "shell-strava__title", "Feltöltés Stravára" }
                div { class: "shell-strava__status", "{status}" }
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(not(target_os = "android"))]
                        ctl.close_strava();
                        #[cfg(target_os = "android")]
                        let _ = ctl;
                    },
                    "Kész"
                }
            }
        }
    }
}

#[component]
fn DropOverlay() -> Element {
    let st = use_context::<AppState>();
    if !*st.drag_active.read() {
        return rsx! {};
    }
    rsx! {
        div { class: "shell-drop", role: "presentation",
            div { class: "shell-drop__hint",
                Icon { name: IconName::Upload, size: 32 }
                div { class: "shell-drop__title", "Engedd el a GPX-fájlt az útvonal megnyitásához" }
                div { class: "shell-drop__sub", "bikerouter.de-ről exportált vagy bármilyen más GPX" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(kind: DeviceKind, state: ConnState, name: Option<&str>) -> DeviceSlot {
        DeviceSlot {
            kind,
            state,
            name: name.map(String::from),
        }
    }

    #[test]
    fn dots_aggregate_slots() {
        let slots = vec![
            slot(DeviceKind::Trainer, ConnState::Connected, Some("KICKR")),
            slot(DeviceKind::HeartRate, ConnState::Connecting, None),
            slot(
                DeviceKind::PowerMeter,
                ConnState::Failed("időtúllépés".into()),
                None,
            ),
            slot(DeviceKind::Cadence, ConnState::Disconnected, None),
        ];
        let [trainer, hr, other] = device_dots(&slots);
        assert_eq!(
            trainer,
            Dot {
                state: "ok",
                title: "Trainer: csatlakoztatva (KICKR)".into()
            }
        );
        assert_eq!(hr.state, "busy");
        assert_eq!(other.state, "bad");
        assert!(other.title.contains("időtúllépés"));
    }

    #[test]
    fn connected_beats_failed_within_a_dot() {
        let slots = vec![
            slot(DeviceKind::PowerMeter, ConnState::Failed("x".into()), None),
            slot(DeviceKind::Cadence, ConnState::Connected, Some("CSC")),
        ];
        let [_, _, other] = device_dots(&slots);
        assert_eq!(other.state, "ok");
    }

    #[test]
    fn nothing_connected_is_off() {
        let [a, b, c] = device_dots(&crate::state::default_slots());
        assert!([&a, &b, &c].iter().all(|d| d.state == "off"));
        assert_eq!(a.title, "Trainer: nincs csatlakoztatva");
        assert_eq!(device_dots(&[])[1].state, "off");
    }

    #[test]
    fn the_conditional_panels_depend_on_the_ride_setup() {
        let nothing = dock::Availability::default();
        for id in PanelId::ALL {
            // the browser panel exists only where it can be implemented (Windows)
            let always = !matches!(
                id,
                PanelId::Workout | PanelId::Rider | PanelId::Gears | PanelId::Browser
            ) || (id == PanelId::Browser && cfg!(windows));
            assert_eq!(nothing.has(id), always, "{id:?}");
        }
        let workout = dock::Availability {
            workout: true,
            rider: false,
            gears: false,
        };
        assert!(workout.has(PanelId::Workout) && !workout.has(PanelId::Rider));
    }

    #[test]
    fn every_drag_has_a_cursor_the_stylesheet_knows() {
        let layout = dock::Dock::default().layout(dock::Availability::default());
        let mut ops = vec![DragOp::Carry {
            id: PanelId::Map,
            parked: false,
            start_x: 0.0,
            start_y: 0.0,
            travelled: false,
            target: None,
        }];
        ops.extend(
            layout
                .dividers
                .iter()
                .map(|&divider| DragOp::Divider { divider }),
        );
        assert!(ops.len() > 2, "both divider directions are covered");
        for op in ops {
            let cursor = drag_cursor(op);
            assert!(
                CSS.contains(&format!("data-op=\"{cursor}\"")),
                "no rule for {cursor}"
            );
        }
        for axis in ["cols", "rows"] {
            assert!(
                CSS.contains(&format!(".dock-divider--{axis} ")),
                "no rule for {axis}"
            );
        }
    }

    /// Every `var(--x)` of the shell stylesheet must be defined by the design kit (or locally).
    #[test]
    fn shell_css_uses_only_defined_tokens() {
        let kit = ride_ui_kit::CSS;
        let mut rest = CSS;
        let mut seen = 0;
        while let Some(at) = rest.find("var(--") {
            let after = &rest[at + 6..];
            let end = after.find([')', ',']).expect("closed var()");
            let name = &after[..end];
            let def = format!("--{name}:");
            assert!(
                kit.contains(&def) || CSS.contains(&def),
                "--{name} is neither a kit token nor defined by the shell"
            );
            seen += 1;
            rest = &after[end..];
        }
        assert!(seen > 60, "found only {seen} token uses");
    }

    #[test]
    fn shell_css_has_no_hard_coded_colours() {
        assert!(
            !CSS.contains('#'),
            "hex colours are not allowed, use tokens"
        );
        assert!(
            !CSS.contains("rgb"),
            "rgb()/rgba() colours are not allowed, use tokens"
        );
    }

    /// The house rules of docs/DESIGN.md: no blur, no gradient, no soft shadow, radius <= 4 px.
    #[test]
    fn shell_css_follows_the_house_rules() {
        for banned in [
            "blur(",
            "gradient",
            "box-shadow",
            "text-shadow",
            "drop-shadow",
            "backdrop",
        ] {
            assert!(!CSS.contains(banned), "shell CSS must not use {banned}");
        }
        for line in CSS.lines().filter(|l| l.contains("border-radius")) {
            assert!(
                line.contains("50%")
                    || line.contains("border-radius: 0;")
                    || line.contains("var(--radius-"),
                "unexpected radius: {line}"
            );
        }
    }

    #[test]
    fn head_contains_theme_and_all_css() {
        let head = head_html(Theme::Light, 1.25);
        assert!(head.contains("setAttribute('data-theme','light')"));
        assert!(
            head.contains("setProperty('--ui-scale','1.25')"),
            "the text scale is set"
        );
        assert!(head.contains("setAttribute('data-text','large')"));
        assert_eq!(text_size_attr(1.0), "normal");
        assert_eq!(text_size_attr(1.15), "large");
        assert!(head.contains(".shell-top"), "shell CSS present");
        assert!(head.contains(".sv__stage"), "street view CSS present");
        assert!(head.contains("--bg"), "kit tokens present");
        assert!(
            head.find("--bg").unwrap_or(usize::MAX) < head.find(".shell-top").unwrap_or(0),
            "kit comes first"
        );
        assert_eq!(head.matches("</style>").count(), 1);
    }

    #[test]
    fn style_end_tags_inside_css_cannot_break_out() {
        assert!(!"a</style>b"
            .replace("</style", "<\\/style")
            .contains("</style"));
    }

    /// `--bg` of both themes in the kit stylesheet, as the native window background.
    #[test]
    fn window_background_matches_the_kit_tokens() {
        let kit = ride_ui_kit::CSS;
        let hex = |theme_block: &str| -> String {
            let at = kit.find(theme_block).expect("theme block");
            let tail = &kit[at..];
            let bg = tail.find("--bg:").expect("--bg");
            tail[bg + 5..]
                .trim_start()
                .chars()
                .take(7)
                .collect::<String>()
        };
        let rgb = |c: (u8, u8, u8, u8)| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
        assert_eq!(hex(":root {"), rgb(background_rgba(Theme::Dark)));
        assert_eq!(
            hex(":root[data-theme=\"light\"] {"),
            rgb(background_rgba(Theme::Light))
        );
    }

    #[test]
    fn the_ride_tab_carries_what_is_needed_while_riding() {
        assert_eq!(
            CompactTab::ALL[0],
            CompactTab::Ride,
            "the phone opens on the ride tab"
        );
        assert_eq!(compact_tab_of(PanelId::Metrics), Some(CompactTab::Ride));
        assert_eq!(compact_tab_of(PanelId::Profile), Some(CompactTab::Ride));
        assert_eq!(
            compact_tab_of(PanelId::Workout),
            None,
            "the head unit carries the workout"
        );
        for id in PanelId::ALL {
            let placed = compact_tab_of(id).is_some();
            let expected = !matches!(id, PanelId::Workout | PanelId::Controls | PanelId::Browser);
            assert_eq!(placed, expected, "{id:?}");
        }
        assert!(compact_rank(PanelId::Metrics) < compact_rank(PanelId::Profile));
    }

    #[test]
    fn the_phone_profile_pages_cover_every_view_once() {
        let with = profile_pages(true);
        assert_eq!(with.len(), 4);
        assert_eq!(
            with[0].view,
            ProfileView::Workout,
            "the workout comes first"
        );
        let mut labels: Vec<_> = with.iter().map(|p| p.label()).collect();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), 4);
        let without = profile_pages(false);
        assert_eq!(without.len(), 2);
        assert!(without.iter().all(|p| p.view == ProfileView::Elevation));
    }
}
