//! Shared data model. Every crate (core, ble, app) compiles against these types.
//!
//! This file is the *contract* between the parallel workstreams. Adding fields or
//! helper methods is fine; renaming/removing/retyping anything is not without
//! updating every user (grep the workspace first).
//!
//! Units everywhere: metres, seconds, metres/second, watts, percent for grades
//! (5.0 == 5 %), degrees for lat/lon/heading. Display conversion (km/h, h:mm:ss)
//! belongs to the UI layer only.

use serde::{Deserialize, Serialize};

mod extras;
pub use extras::*;

mod history;
pub use history::*;

mod workout;
pub use workout::*;

// ───────────────────────────── zones ─────────────────────────────

/// Coggan power zone (1..=7) of an intensity in percent of FTP; `0` when the value is unusable.
///
/// The thresholds live here so the engine (`ride_sim::zones::power_zone`), the charts and the
/// workout picker all colour the same watts the same way.
pub fn power_zone_of_pct(pct: f64) -> u8 {
    if !pct.is_finite() {
        return 0;
    }
    let pct = pct.max(0.0);
    if pct < 55.0 {
        1
    } else if pct < 76.0 {
        2
    } else if pct < 91.0 {
        3
    } else if pct < 106.0 {
        4
    } else if pct < 121.0 {
        5
    } else if pct < 151.0 {
        6
    } else {
        7
    }
}

// ───────────────────────────── Route ─────────────────────────────

/// One point of the *processed* route: resampled to a roughly uniform spacing
/// (~10 m), elevation smoothed, grade derived from the smoothed elevation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoutePoint {
    pub lat: f64,
    pub lon: f64,
    /// Smoothed elevation above sea level, metres.
    pub ele: f64,
    /// Cumulative distance from the route start, metres.
    pub dist: f64,
    /// Smoothed grade at this point in percent (positive = uphill).
    pub grade: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClimbCategory {
    /// Hors catégorie
    Hc,
    Cat1,
    Cat2,
    Cat3,
    Cat4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Climb {
    pub start_dist: f64,
    pub end_dist: f64,
    pub length: f64,
    /// Total elevation gain inside the climb, metres.
    pub gain: f64,
    pub avg_grade: f64,
    pub max_grade: f64,
    pub start_ele: f64,
    pub end_ele: f64,
    /// `None` = too small to categorise but still worth showing.
    pub category: Option<ClimbCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// Distance along the route of the nearest route point, metres.
    pub dist_on_route: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteSource {
    GpxFile,
    Bikerouter,
    Demo,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// Stable id (hash of the geometry) used for the recent-routes store.
    pub id: String,
    pub name: String,
    pub source: RouteSource,
    pub points: Vec<RoutePoint>,
    /// Elapsed seconds from the route start at every point: the `<time>` stamps of the source GPX,
    /// resampled with the geometry. Empty when the file carries no complete, monotone timeline
    /// (BRouter routes have none). It makes the route its own ghost, before any ride is stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<f64>,
    pub total_dist: f64,
    /// Hysteresis-filtered total ascent / descent, metres (descent is positive).
    pub ascent: f64,
    pub descent: f64,
    pub min_ele: f64,
    pub max_ele: f64,
    pub bounds: Bounds,
    pub climbs: Vec<Climb>,
    pub waypoints: Vec<Waypoint>,
    /// `false` when the source had no usable elevation (profile is flat).
    pub has_elevation: bool,
    /// Named sections the GPX file brought along (e.g. Strava segments added to the file as
    /// tracks of their own), in route order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<RouteSegment>,
}

/// A named section of the route that came with the GPX file: a track of the file that lies on
/// the route (a Strava segment exported and added to the route file, a KOM, a sprint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteSegment {
    pub name: String,
    /// Where it starts / ends along the route, metres.
    pub start_dist: f64,
    pub end_dist: f64,
}

/// Interpolated state of the route at some distance (see `Route::locate`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutePosition {
    pub dist: f64,
    pub lat: f64,
    pub lon: f64,
    pub ele: f64,
    pub grade: f64,
    /// Direction of travel, degrees clockwise from north, 0..360.
    pub heading_deg: f64,
}

// ───────────────────────────── Settings ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiderSettings {
    pub rider_kg: f64,
    pub bike_kg: f64,
    pub ftp_w: f64,
    pub max_hr: f64,
    /// Drag area, m².
    pub cda: f64,
    /// Rolling resistance coefficient.
    pub crr: f64,
    /// Drivetrain efficiency 0..1.
    pub drivetrain_eff: f64,
    /// Air density, kg/m³.
    pub air_density: f64,
    /// Fraction (0..=1.5) of the real grade sent to the trainer ("difficulty").
    pub trainer_grade_scale: f64,
    /// Use the trainer-reported speed instead of the physics model.
    pub use_trainer_speed: bool,
}

impl Default for RiderSettings {
    fn default() -> Self {
        Self {
            rider_kg: 75.0,
            bike_kg: 9.0,
            ftp_w: 200.0,
            max_hr: 190.0,
            cda: 0.32,
            crr: 0.004,
            drivetrain_eff: 0.975,
            air_density: 1.225,
            trainer_grade_scale: 1.0,
            use_trainer_speed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapStyle {
    /// CARTO dark matter (default with the dark theme)
    Dark,
    /// CARTO positron
    Light,
    /// Standard OpenStreetMap
    Osm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileMode {
    /// Whole route.
    Full,
    /// Window ahead of the rider (trainer-app style).
    Lookahead,
}

/// What the profile panel draws while a workout is selected (without one it is always the
/// elevation).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileView {
    /// The elevation profile of the route.
    Elevation,
    /// The blocks of the workout, with the rider's place in them: in ERG mode the grade does not
    /// change the effort, the next block does.
    #[default]
    Workout,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: Theme,
    pub map_style: MapStyle,
    pub profile_mode: ProfileMode,
    /// Elevation or workout blocks in the profile panel while a workout is selected.
    pub profile_view: ProfileView,
    /// The rider has turned the profile's pages by swiping on a phone: the hint is not shown
    /// any more.
    pub profile_swipe_learned: bool,
    pub follow_rider: bool,
    /// Lookahead window length in metres.
    pub lookahead_m: f64,
    /// Show Google Street View on the map tile (needs `google_maps_key`).
    pub streetview_enabled: bool,
    /// Which of the two shares the map tile's big area: `false` = the map is big and Street View
    /// the small picture in the bottom right corner, `true` = the other way round.
    pub streetview_large: bool,
    /// The virtual gearbox (shift keys + the gear panel) is on; it changes the resistance the
    /// trainer is asked for.
    pub virtual_gears: bool,
    /// The selected virtual gear, `0` = the easiest (clamped to the gearbox size by the app).
    pub gear: u8,
    /// The user's own Google Maps Platform API key (Maps Embed API + Street View Static API
    /// enabled). Stored in the local settings file only; never sent anywhere except to Google.
    pub google_maps_key: String,
    /// Remember Street View panorama IDs per route on disk (and resolve them ahead of the
    /// rider) so repeat rides need no lookups. Only IDs are stored, never imagery.
    pub streetview_cache_panos: bool,
    /// Download the Street View images of the route (one per 75–250 m) and keep them on disk,
    /// so repeat rides of the same route need no Google request and no API key at all. The
    /// files are shown for at most 30 days.
    pub streetview_offline: bool,
    /// Id of the selected workout (see [`WorkoutLibrary`]); `None` = free ride.
    pub workout_id: Option<String>,
    /// Which earlier ride of the same route is replayed as a ghost during the ride.
    pub ghost: GhostMode,
    /// How the ride screen is tiled: which panels are shown and how the space is divided between
    /// them. `None` = the default layout; panels that are not in the tree are parked in the top bar.
    pub tiles: Option<TileNode>,
}

/// The tiling of the modular ride screen as a binary tree: a leaf is one panel (by its stable key),
/// a split divides its area between two subtrees. `ratio` is the first subtree's share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TileNode {
    /// Every panel is closed.
    Empty,
    Leaf {
        panel: String,
    },
    Split {
        /// `false`: the subtrees sit side by side (first = left); `true`: stacked (first = top).
        rows: bool,
        ratio: f64,
        a: Box<TileNode>,
        b: Box<TileNode>,
    },
}

/// `Debug` never prints the API key (it must not reach logs, panics or test output).
impl std::fmt::Debug for UiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiSettings")
            .field("theme", &self.theme)
            .field("map_style", &self.map_style)
            .field("profile_mode", &self.profile_mode)
            .field("profile_view", &self.profile_view)
            .field("profile_swipe_learned", &self.profile_swipe_learned)
            .field("follow_rider", &self.follow_rider)
            .field("lookahead_m", &self.lookahead_m)
            .field("streetview_enabled", &self.streetview_enabled)
            .field("streetview_large", &self.streetview_large)
            .field("virtual_gears", &self.virtual_gears)
            .field("gear", &self.gear)
            .field(
                "google_maps_key",
                &if self.google_maps_key.is_empty() {
                    "<üres>"
                } else {
                    "<elrejtve>"
                },
            )
            .field("streetview_cache_panos", &self.streetview_cache_panos)
            .field("streetview_offline", &self.streetview_offline)
            .field("workout_id", &self.workout_id)
            .field("ghost", &self.ghost)
            .field("tiles", &self.tiles.is_some())
            .finish()
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            map_style: MapStyle::Dark,
            profile_mode: ProfileMode::Lookahead,
            profile_view: ProfileView::Workout,
            profile_swipe_learned: false,
            follow_rider: true,
            lookahead_m: 3000.0,
            streetview_enabled: false,
            streetview_large: false,
            virtual_gears: false,
            gear: 255,
            google_maps_key: String::new(),
            streetview_cache_panos: false,
            streetview_offline: true,
            workout_id: None,
            ghost: GhostMode::default(),
            tiles: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub rider: RiderSettings,
    pub ui: UiSettings,
}

// ───────────────────────────── Sensors ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceKind {
    /// Smart trainer (FTMS) – power, cadence, speed and resistance control.
    Trainer,
    HeartRate,
    /// Standalone power meter (Cycling Power Service).
    PowerMeter,
    /// Speed/cadence sensor (CSC).
    Cadence,
    /// Zwift Click / Play virtual-shifting controller
    /// (it steps the virtual gearbox; it is not a sensor).
    Shifter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

/// A device found while scanning (BLE or simulated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Opaque, backend specific id (BLE address / peripheral id string).
    pub id: String,
    pub name: String,
    /// Which service(s) it advertises, so the UI can offer sensible kinds.
    pub kinds: Vec<DeviceKind>,
    pub rssi: Option<i16>,
}

/// Latest known values from all connected sensors (merged). `None` = not
/// available / stale (the hub clears values that have not been refreshed for ~3 s).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SensorSample {
    pub power_w: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub hr_bpm: Option<f64>,
    /// Speed reported by the trainer / speed sensor, m/s.
    pub speed_ms: Option<f64>,
    /// Trainer resistance level as reported (unit-less, device specific).
    pub resistance: Option<f64>,
    /// Left pedal share of power in percent (power meters that report it).
    pub power_balance_left_pct: Option<f64>,
    /// Energy expended reported by the trainer, kcal.
    pub energy_kcal: Option<f64>,
}

/// What the trainer should simulate (FTMS "Set Indoor Bike Simulation Parameters").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrainerSim {
    /// Percent, already scaled by `trainer_grade_scale` and smoothed.
    pub grade_pct: f64,
    pub crr: f64,
    /// Wind resistance coefficient kg/m (0.5 * rho * CdA).
    pub cw: f64,
    pub wind_ms: f64,
}

/// UI -> sensor backend.
#[derive(Debug, Clone, PartialEq)]
pub enum SensorCommand {
    StartScan,
    StopScan,
    Connect {
        id: String,
        kind: DeviceKind,
    },
    Disconnect(DeviceKind),
    SetSimulation(TrainerSim),
    SetTargetPower(f64),
    /// Tell the trainer a session starts / pauses (FTMS Start/Stop control points).
    TrainerStart,
    TrainerPause,
    /// Only meaningful for the simulated backend: virtual rider power in watts.
    SetVirtualPower(f64),
}

/// Sensor backend -> UI.
#[derive(Debug, Clone, PartialEq)]
pub enum SensorEvent {
    Discovered(Vec<DeviceInfo>),
    State {
        kind: DeviceKind,
        state: ConnState,
        name: Option<String>,
    },
    /// Merged latest sample, emitted whenever anything changes (rate-limited to ~10 Hz).
    Sample(SensorSample),
    /// One virtual gear step from a shifting controller: `+1` harder, `-1` easier.
    Shift(i32),
    /// Human readable, Hungarian, safe to show in a toast.
    Notice(String),
}

// ───────────────────────────── Ride ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RideState {
    /// No route loaded.
    Empty,
    /// Route loaded, not started.
    Ready,
    Riding,
    Paused,
    Finished,
}

/// Everything the HUD needs, produced by the engine on every step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RideSnapshot {
    pub state: RideState,
    pub pos: RoutePosition,
    pub speed_ms: f64,
    /// Instantaneous / smoothed power etc. (`None` if no source).
    pub power_w: Option<f64>,
    pub power_3s: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub hr_bpm: Option<f64>,
    pub resistance: Option<f64>,
    /// Timer time (excludes pauses), seconds.
    pub elapsed_s: f64,
    pub dist: f64,
    pub remaining_dist: f64,
    pub eta_s: Option<f64>,
    pub ascent_done: f64,
    pub descent_done: f64,
    pub ascent_left: f64,
    pub avg_speed_ms: f64,
    pub max_speed_ms: f64,
    pub avg_power_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub work_kj: f64,
    pub kcal: f64,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    /// 1..=7 (Coggan) relative to FTP, from the 3 s power.
    pub power_zone: Option<u8>,
    /// 1..=5 relative to max HR.
    pub hr_zone: Option<u8>,
    /// Index into `Route::climbs` of the climb we are in or the next one ahead.
    pub next_climb: Option<usize>,
    /// Distance to the start of `next_climb` (0 when inside it), metres.
    pub next_climb_in: Option<f64>,
    /// Grade last sent to the trainer (after scaling/smoothing), percent.
    pub trainer_grade: f64,
    /// The running workout (ERG target, current step, remaining time), if one is selected.
    pub workout: Option<WorkoutProgress>,
    /// The ghost of an earlier ride on the same route, if one is being replayed.
    #[serde(default)]
    pub ghost: Option<GhostLive>,
    /// The climb the rider is inside right now, with its PR comparison.
    #[serde(default)]
    pub segment: Option<SegmentLive>,
}

/// One recorded 1 Hz sample (for the history charts and the TCX export).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecordPoint {
    /// Seconds since ride start (timer time).
    pub t: f64,
    /// Wall clock, seconds since the Unix epoch.
    pub unix_s: f64,
    pub dist: f64,
    pub lat: f64,
    pub lon: f64,
    pub ele: f64,
    pub speed_ms: f64,
    pub power_w: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub hr_bpm: Option<f64>,
    pub grade: f64,
}

/// Ride summary shown when the ride ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RideSummary {
    pub route_name: String,
    /// Id of the route the ride was made on (the key of the ride log). Older files have none.
    #[serde(default)]
    pub route_id: String,
    pub started_unix_s: f64,
    pub elapsed_s: f64,
    pub dist: f64,
    pub ascent: f64,
    pub avg_speed_ms: f64,
    pub max_speed_ms: f64,
    pub avg_power_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub intensity_factor: Option<f64>,
    pub tss: Option<f64>,
    pub work_kj: f64,
    pub kcal: f64,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    /// Seconds spent in power zones Z1..Z7.
    pub power_zone_secs: [f64; 7],
    /// Seconds spent in HR zones Z1..Z5.
    pub hr_zone_secs: [f64; 5],
}

#[cfg(test)]
mod ui_settings_debug_tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_the_api_key() {
        let mut s = Settings::default();
        s.ui.google_maps_key = "AIzaSECRETKEY123".into();
        let text = format!("{s:?} {s:#?} {:?}", s.ui);
        assert!(!text.contains("AIzaSECRETKEY123"), "{text}");
        assert!(text.contains("elrejtve"));
    }

    #[test]
    fn a_settings_file_without_the_ghost_field_uses_the_default() {
        let s: Settings = serde_json::from_str(r#"{"ui":{"theme":"Light"}}"#).unwrap();
        assert_eq!(s.ui.ghost, GhostMode::Best);
        assert_eq!(s.ui.theme, Theme::Light);
        // and the field is written from now on
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"ghost\":\"Best\""), "{json}");
    }

    #[test]
    fn a_route_without_a_timeline_still_loads() {
        let route = crate::testing::straight_route(&[(100.0, 0.0)], 100.0);
        // a route cache written before the timeline existed has no such field
        let mut json = serde_json::to_value(&route).unwrap();
        json.as_object_mut().unwrap().remove("timeline");
        let loaded: Route = serde_json::from_value(json).unwrap();
        assert!(loaded.timeline.is_empty());
        // an empty timeline is not written out at all
        assert!(!serde_json::to_string(&route).unwrap().contains("timeline"));
        // a real timeline round-trips
        let mut timed = route;
        timed.timeline = timed.points.iter().map(|p| p.dist / 5.0).collect();
        let text = serde_json::to_string(&timed).unwrap();
        assert!(text.contains("timeline"), "{text}");
        let back: Route = serde_json::from_str(&text).unwrap();
        assert_eq!(back.timeline, timed.timeline);
    }

    #[test]
    fn an_old_summary_without_a_route_id_still_loads() {
        let summary = RideSummary {
            route_name: "Régi túra".into(),
            route_id: "abc".into(),
            started_unix_s: 1.0,
            elapsed_s: 2.0,
            dist: 3.0,
            ascent: 4.0,
            avg_speed_ms: 5.0,
            max_speed_ms: 6.0,
            avg_power_w: None,
            max_power_w: None,
            normalized_power_w: None,
            intensity_factor: None,
            tss: None,
            work_kj: 0.0,
            kcal: 0.0,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            power_zone_secs: [0.0; 7],
            hr_zone_secs: [0.0; 5],
        };
        let mut value = serde_json::to_value(&summary).unwrap();
        value.as_object_mut().unwrap().remove("route_id");
        let old: RideSummary = serde_json::from_value(value).unwrap();
        assert_eq!(old.route_id, "");
        assert_eq!(old.route_name, "Régi túra");
    }
}
