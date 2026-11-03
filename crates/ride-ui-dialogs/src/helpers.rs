//! Pure helpers behind the dialogs: link validation, sorting, date formatting, Hungarian
//! labels, settings patching and zone-bar percentages. Everything here is side-effect free
//! and unit-tested; the components only render what these functions return.

use ride_model::{
    ConnState, DeviceInfo, DeviceKind, GhostMode, GhostStatus, MapStyle, ProfileMode, RecentRoute,
    RideComparison, RideSummary, RiderSettings, Route, RouteSource, Settings, Theme, UiSettings,
};
use ride_ui_kit::format;
use ride_ui_kit::IconName;

use crate::DeviceSlot;

/// Id under which the sensor hub offers the simulated rider as a normal device.
pub const SIM_DEVICE_ID: &str = "sim:rider";

/// Slot order of the devices dialog (trainer, heart rate, power, cadence, shifter).
pub const SLOT_ORDER: [DeviceKind; 5] = [
    DeviceKind::Trainer,
    DeviceKind::HeartRate,
    DeviceKind::PowerMeter,
    DeviceKind::Cadence,
    DeviceKind::Shifter,
];

// ───────────────────────────── welcome: bikerouter link ─────────────────────────────

/// What a pasted bikerouter link is missing, as a Hungarian hint. `None` = the text looks
/// like a valid bikerouter.de share link.
///
/// A share link keeps its state in the URL fragment
/// (`#map=…&lonlats=47.5,19.0;47.6,19.1&profile=trekking`), so at minimum the host and the
/// `lonlats=` parameter must be present for the route loader to have anything to work with.
pub fn bikerouter_hint(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some("Illessz be egy bikerouter.de linket, vagy nyiss meg egy GPX-fájlt.".into());
    }
    let lower = trimmed.to_lowercase();
    if !lower.contains("bikerouter.de") {
        return Some("Ez nem bikerouter.de-link. A bikerouter.de „Megosztás” gombjával másolt linket illeszd be.".into());
    }
    if !lower.contains("lonlats=") {
        return Some("A linkből hiányzik az útvonal (lonlats=…). Nyisd meg a bikerouter.de-n az útvonalat, és onnan másold ki a linket.".into());
    }
    None
}

/// `true` when [`bikerouter_hint`] finds nothing to complain about.
pub fn is_valid_bikerouter_link(text: &str) -> bool {
    bikerouter_hint(text).is_none()
}

/// Trims the pasted text and drops a surrounding `<>` (some chats wrap links like that).
pub fn normalize_link_input(text: &str) -> String {
    let trimmed = text.trim();
    let unwrapped = trimmed
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(trimmed);
    unwrapped.trim().to_string()
}

// ───────────────────────────── welcome: recents ─────────────────────────────

/// Recents, most recently opened first (a missing/NaN timestamp sorts last).
pub fn sorted_recents(list: &[RecentRoute]) -> Vec<RecentRoute> {
    let mut out = list.to_vec();
    out.sort_by(|a, b| {
        rank_desc(b.last_opened_unix)
            .partial_cmp(&rank_desc(a.last_opened_unix))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Sortable rank for a timestamp: non-finite values become the oldest possible instant.
fn rank_desc(unix_s: f64) -> f64 {
    if unix_s.is_finite() {
        unix_s
    } else {
        f64::NEG_INFINITY
    }
}

/// Hungarian short date (`2026.10.31.`) from Unix seconds; `—` for a missing value.
pub fn fmt_date(unix_s: f64) -> String {
    if !unix_s.is_finite() {
        return "—".into();
    }
    let days = (unix_s / 86_400.0).floor();
    // Keep the day number inside a range where the civil-calendar arithmetic cannot overflow.
    if !(-2.0e9..=2.0e9).contains(&days) {
        return "—".into();
    }
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}.{month:02}.{day:02}.")
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (Howard Hinnant's
/// `civil_from_days`, public domain algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

/// Where a recent route came from, for the little source chip.
pub fn route_source_label(source: RouteSource) -> &'static str {
    match source {
        RouteSource::GpxFile => "GPX",
        RouteSource::Bikerouter => "Bikerouter",
        RouteSource::Demo => "Bemutató",
    }
}

// ───────────────────────────── devices ─────────────────────────────

/// Dialog label of a sensor kind (Hungarian, mirrors the shell's vocabulary).
pub fn kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Trainer => "Trainer",
        DeviceKind::HeartRate => "Pulzus",
        DeviceKind::PowerMeter => "Teljesítménymérő",
        DeviceKind::Cadence => "Cadence-szenzor",
        DeviceKind::Shifter => "Váltó",
    }
}

/// One line below the slot title: which BLE profile the slot speaks.
pub fn kind_hint(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Trainer => "Okos trainer – FTMS",
        DeviceKind::HeartRate => "Pulzusmérő – BLE",
        DeviceKind::PowerMeter => "Teljesítménymérő – CPS",
        DeviceKind::Cadence => "Sebesség- és kadenciaszenzor – CSC",
        DeviceKind::Shifter => "Zwift Click / Play – virtuális váltó",
    }
}

/// Icon of a sensor kind.
pub fn kind_icon(kind: DeviceKind) -> IconName {
    match kind {
        DeviceKind::Trainer => IconName::Gauge,
        DeviceKind::HeartRate => IconName::Heart,
        DeviceKind::PowerMeter => IconName::Bolt,
        DeviceKind::Cadence => IconName::Cadence,
        DeviceKind::Shifter => IconName::Sliders,
    }
}

/// Badge text of a connection state.
pub fn conn_label(state: &ConnState) -> &'static str {
    match state {
        ConnState::Disconnected => "Nincs kapcsolat",
        ConnState::Connecting => "Kapcsolódás…",
        ConnState::Connected => "Csatlakoztatva",
        ConnState::Failed(_) => "Sikertelen",
    }
}

/// Semantic tone of a connection state: `off` / `busy` / `ok` / `bad`.
pub fn conn_tone(state: &ConnState) -> &'static str {
    match state {
        ConnState::Disconnected => "off",
        ConnState::Connecting => "busy",
        ConnState::Connected => "ok",
        ConnState::Failed(_) => "bad",
    }
}

/// Kit chip classes for the state badge.
pub fn conn_chip_class(state: &ConnState) -> &'static str {
    match state {
        ConnState::Disconnected => "chip",
        ConnState::Connecting => "chip chip--warn",
        ConnState::Connected => "chip chip--ok",
        ConnState::Failed(_) => "chip chip--crit",
    }
}

/// Badge icon (`None` = no icon, the text carries the meaning).
pub fn conn_icon(state: &ConnState) -> Option<IconName> {
    match state {
        ConnState::Connected => Some(IconName::Check),
        ConnState::Failed(_) => Some(IconName::Warning),
        ConnState::Disconnected | ConnState::Connecting => None,
    }
}

/// The failure reason of a slot, if it carries a non-empty one.
pub fn conn_failure(state: &ConnState) -> Option<&str> {
    match state {
        ConnState::Failed(reason) if !reason.trim().is_empty() => Some(reason.as_str()),
        _ => None,
    }
}

/// `true` for the simulated rider (`sim:rider`).
pub fn is_simulated(dev: &DeviceInfo) -> bool {
    dev.id == SIM_DEVICE_ID || dev.id.starts_with("sim:")
}

/// Device name for display; falls back to the id when the device advertises no name.
pub fn device_title(dev: &DeviceInfo) -> String {
    let name = dev.name.trim();
    if name.is_empty() {
        dev.id.clone()
    } else {
        name.to_string()
    }
}

/// Kind the devices dialog offers first: trainer > heart rate > power meter > cadence > shifter.
pub fn suggested_kind(dev: &DeviceInfo) -> Option<DeviceKind> {
    SLOT_ORDER
        .iter()
        .copied()
        .find(|kind| dev.kinds.contains(kind))
}

/// Discovered devices: the simulated rider first, then strongest signal first, then name.
pub fn sorted_discovered(list: &[DeviceInfo]) -> Vec<DeviceInfo> {
    let mut out = list.to_vec();
    out.sort_by(|a, b| {
        is_simulated(b)
            .cmp(&is_simulated(a))
            .then_with(|| rssi_rank(b).cmp(&rssi_rank(a)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// Sort key of a signal strength: unknown RSSI is treated as the weakest possible signal.
fn rssi_rank(dev: &DeviceInfo) -> i16 {
    dev.rssi.unwrap_or(i16::MIN)
}

/// Signal-quality label; every colour-coded value also has this text.
pub fn rssi_label(rssi: Option<i16>) -> &'static str {
    match rssi {
        Some(r) if r >= -60 => "Erős jel",
        Some(r) if r >= -75 => "Közepes jel",
        Some(_) => "Gyenge jel",
        None => "Nincs jel",
    }
}

/// Semantic tone of the signal quality: `ok` / `warn` / `bad` / `off`.
pub fn rssi_tone(rssi: Option<i16>) -> &'static str {
    match rssi {
        Some(r) if r >= -60 => "ok",
        Some(r) if r >= -75 => "warn",
        Some(_) => "bad",
        None => "off",
    }
}

/// `"−62 dBm"` (em dash when the backend reports nothing).
pub fn rssi_text(rssi: Option<i16>) -> String {
    match rssi {
        Some(r) => format!("{} dBm", format::fmt_opt0(Some(f64::from(r)))),
        None => "— dBm".into(),
    }
}

/// The slot of `kind`, if the app tracks one.
pub fn slot_of(slots: &[DeviceSlot], kind: DeviceKind) -> Option<DeviceSlot> {
    slots.iter().find(|s| s.kind == kind).cloned()
}

/// Name of the device currently connected in the slot of `kind`.
pub fn slot_name(slots: &[DeviceSlot], kind: DeviceKind) -> Option<String> {
    slot_of(slots, kind)
        .and_then(|s| s.name)
        .filter(|name| !name.trim().is_empty())
}

/// State of the slot of `kind`; a slot the app does not track counts as disconnected.
pub fn slot_state(slots: &[DeviceSlot], kind: DeviceKind) -> ConnState {
    slot_of(slots, kind)
        .map(|s| s.state)
        .unwrap_or(ConnState::Disconnected)
}

/// `true` when the slot of `kind` holds the device with this display name.
pub fn slot_matches(slots: &[DeviceSlot], kind: DeviceKind, name: &str) -> bool {
    slot_name(slots, kind).is_some_and(|connected| connected == name)
}

/// Names held by more than one live slot: a physical device serving several roles at once.
pub fn shared_slot_names(slots: &[DeviceSlot]) -> Vec<String> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for slot in slots
        .iter()
        .filter(|s| matches!(s.state, ConnState::Connected | ConnState::Connecting))
    {
        if let Some(name) = slot
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            *counts.entry(name.to_string()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(name, _)| name)
        .collect()
}

// ───────────────────────────── virtual rider ─────────────────────────────

/// Highest power the virtual rider panel can set.
pub const VIRTUAL_RIDER_MAX_W: f64 = 1000.0;
/// Slider / keyboard grid of the virtual rider panel.
pub const VIRTUAL_RIDER_STEP_W: f64 = 5.0;

/// Quick power presets of the floating panel: `(label, watts, hint)`.
pub const VIRTUAL_QUICK: [(&str, f64, &str); 4] = [
    ("Z2", 150.0, "Z2 – állóképesség, 150 W"),
    ("Z4", 250.0, "Z4 – küszöb, 250 W"),
    ("Sprint", 700.0, "Sprint, 700 W"),
    ("Szabad", 0.0, "Gurulás, 0 W"),
];

/// Clamps to `0..=1000 W` and snaps to the 5 W grid; NaN becomes 0.
pub fn clamp_virtual_power(watts: f64) -> f64 {
    format::snap_to_step(watts, 0.0, VIRTUAL_RIDER_MAX_W, VIRTUAL_RIDER_STEP_W)
}

// ───────────────────────────── settings ─────────────────────────────

/// Copy of `settings` with only the rider section patched. The dialog always emits a whole
/// [`Settings`] value, so unknown/new fields survive a change.
pub fn with_rider(settings: &Settings, patch: impl FnOnce(&mut RiderSettings)) -> Settings {
    let mut next = settings.clone();
    patch(&mut next.rider);
    next
}

/// Copy of `settings` with only the UI section patched.
pub fn with_ui(settings: &Settings, patch: impl FnOnce(&mut UiSettings)) -> Settings {
    let mut next = settings.clone();
    patch(&mut next.ui);
    next
}

/// `trainer_grade_scale` (0…1.5) as the percentage the slider shows (0…150).
pub fn trainer_scale_percent(scale: f64) -> f64 {
    let scale = if scale.is_finite() { scale } else { 1.0 };
    format::snap_to_step(scale * 100.0, 0.0, 150.0, 5.0)
}

/// Percentage from the slider back to `trainer_grade_scale` (0…1.5).
pub fn trainer_scale_from_percent(percent: f64) -> f64 {
    let percent = if percent.is_finite() { percent } else { 100.0 };
    format::snap_to_step(percent / 100.0, 0.0, 1.5, 0.05)
}

/// `text_scale` (0.8…1.6) as the percentage the slider shows (80…160), in steps of 5.
pub fn text_scale_percent(scale: f64) -> f64 {
    let scale = if scale.is_finite() { scale } else { 1.0 };
    format::snap_to_step(scale * 100.0, 80.0, 160.0, 5.0)
}

/// Percentage from the slider back to `text_scale`.
pub fn text_scale_from_percent(percent: f64) -> f64 {
    let percent = if percent.is_finite() { percent } else { 100.0 };
    format::snap_to_step(percent / 100.0, 0.8, 1.6, 0.05)
}

/// `Segmented` value of a theme.
pub fn theme_value(theme: Theme) -> String {
    match theme {
        Theme::Dark => "dark".into(),
        Theme::Light => "light".into(),
    }
}

/// Inverse of [`theme_value`]; anything else stays on the current (`dark` default).
pub fn theme_from_value(value: &str) -> Theme {
    match value {
        "light" => Theme::Light,
        _ => Theme::Dark,
    }
}

/// `Segmented` value of a map style.
pub fn map_style_value(style: MapStyle) -> String {
    match style {
        MapStyle::Dark => "dark".into(),
        MapStyle::Light => "light".into(),
        MapStyle::Osm => "osm".into(),
    }
}

/// Inverse of [`map_style_value`].
pub fn map_style_from_value(value: &str) -> MapStyle {
    match value {
        "light" => MapStyle::Light,
        "osm" => MapStyle::Osm,
        _ => MapStyle::Dark,
    }
}

/// `Segmented` value of a profile mode.
pub fn profile_mode_value(mode: ProfileMode) -> String {
    match mode {
        ProfileMode::Full => "full".into(),
        ProfileMode::Lookahead => "ahead".into(),
    }
}

/// Inverse of [`profile_mode_value`].
pub fn profile_mode_from_value(value: &str) -> ProfileMode {
    match value {
        "full" => ProfileMode::Full,
        _ => ProfileMode::Lookahead,
    }
}

/// `Segmented` value of the ghost mode.
pub fn ghost_mode_value(mode: GhostMode) -> String {
    match mode {
        GhostMode::Off => "off".into(),
        GhostMode::Route => "route".into(),
        GhostMode::Last => "last".into(),
        GhostMode::Best => "best".into(),
    }
}

/// Inverse of [`ghost_mode_value`].
pub fn ghost_mode_from_value(value: &str) -> GhostMode {
    match value {
        "off" => GhostMode::Off,
        "route" => GhostMode::Route,
        "last" => GhostMode::Last,
        _ => GhostMode::Best,
    }
}

/// One sentence under the ghost selector: what the ride log of the loaded route can replay.
///
/// The ghost is invisible until there is something to replay, so this is the place that says why
/// it is missing: no route, no ride yet, only simulated rides, no ride that qualifies, or a GPX
/// without `<time>` stamps.
pub fn ghost_hint(status: GhostStatus, mode: GhostMode) -> String {
    let route_time = status
        .route_time_s
        .filter(|t| t.is_finite() && *t > 0.0)
        .map(format::fmt_duration);
    if mode == GhostMode::Off {
        return "A szellem ki van kapcsolva. A szegmensek PR-idői ettől még látszanak.".into();
    }
    if !status.loaded {
        return "Tölts be egy útvonalat: a szellem az adott útvonal korábbi meneteiből jön.".into();
    }
    if mode == GhostMode::Route {
        return match route_time {
            Some(time) => format!(
                "A GPX-be írt menet ideje {time}. Ez a szellem a fájl időbélyegeiből jön, saját mentett menet nélkül is."
            ),
            None => "Ebben a GPX-ben nincsenek időbélyegek (nincs <time>), ezért az útvonal saját ideje nem játszható vissza.".into(),
        };
    }
    if status.rides == 0 {
        return match route_time {
            Some(time) => format!(
                "Ehhez az útvonalhoz még nincs mentett menet. A GPX saját ideje {time}: válaszd az „Az útvonal ideje” módot, és már most van szellem."
            ),
            None => "Ehhez az útvonalhoz még nincs mentett menet. Az első befejezett menet (legalább 1 perc és 200 m) után a szellem már a rajtvonalon vár.".into(),
        };
    }
    let rides = format!("{} korábbi menet", status.rides);
    if status.replayable {
        if status.partial {
            return format!(
                "{rides}, de egyik sem ért célba: a szellem a legtovább jutott menetet játssza vissza, és annak a végénél megáll."
            );
        }
        if status.simulated > 0 {
            return format!(
                "{rides} (ebből {} szimulált). A szellem a következő tekerésnél a rajtvonalon vár.",
                status.simulated
            );
        }
        return format!("{rides}. A szellem a következő tekerésnél a rajtvonalon vár.");
    }
    if status.simulated == status.rides {
        return format!(
            "{rides}, de mind szimulált. A „Legjobb” szellem csak valódi menet lehet: válaszd a „Legutóbbi menet” módot."
        );
    }
    format!(
        "{rides}, de egyik sem indult a rajtból, ezért nem lehet belőle szellem. Indíts egy menetet a rajtvonaltól, vagy válaszd az „Az útvonal ideje” módot."
    )
}

// ───────────────────────────── summary ─────────────────────────────

/// One row of the segment table of the summary.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentRow {
    /// Index into `Route::climbs` (0-based; the display name is 1-based).
    pub climb: usize,
    /// `"2. emelkedő"`, or just `"2."` when the route is not available any more.
    pub name: String,
    /// `"12.40–16.80 km"`; empty when the climb cannot be resolved.
    pub range: String,
    /// The time of this ride, `"8:12"`.
    pub time: String,
    /// The PR before this ride, `"—"` when there was none.
    pub previous: String,
    pub is_pr: bool,
}

/// Table rows for the comparison, in climb order. Segments whose climb is not in `route` are
/// still shown (the ride is real even if the route changed), only without a distance range.
pub fn segment_rows(comparison: &RideComparison, route: Option<&Route>) -> Vec<SegmentRow> {
    let mut rows: Vec<SegmentRow> = comparison
        .segments
        .iter()
        .map(|segment| {
            let climb = route.and_then(|route| route.climbs.get(segment.climb));
            SegmentRow {
                climb: segment.climb,
                name: format!("{}. emelkedő", segment.climb + 1),
                range: climb.map_or_else(String::new, |climb| {
                    format!(
                        "{}–{} km",
                        format::fmt_km(climb.start_dist),
                        format::fmt_km(climb.end_dist)
                    )
                }),
                time: format::fmt_duration(segment.time_s),
                previous: segment
                    .previous_pr_s
                    .map_or_else(|| format::fmt_opt0(None), format::fmt_duration),
                is_pr: segment.is_pr,
            }
        })
        .collect();
    rows.sort_by_key(|row| row.climb);
    rows
}

/// One zone row of the summary: zone number (1-based), seconds and percentage of the ride.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneBar {
    pub zone: u8,
    pub secs: f64,
    pub pct: f64,
}

/// Share of each entry in percent, ignoring missing/negative values. When there is any time
/// at all the percentages add up to exactly 100 (the rounding residual goes to the longest
/// zone); when everything is zero every share is zero.
pub fn zone_shares(secs: &[f64]) -> Vec<f64> {
    let clean: Vec<f64> = secs
        .iter()
        .map(|s| if s.is_finite() && *s > 0.0 { *s } else { 0.0 })
        .collect();
    let total: f64 = clean.iter().sum();
    if total <= 0.0 {
        return vec![0.0; clean.len()];
    }
    let mut shares: Vec<f64> = clean.iter().map(|s| s / total * 100.0).collect();
    let sum: f64 = shares.iter().sum();
    let longest = shares
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(index, _)| index);
    if let Some(index) = longest {
        if let Some(share) = shares.get_mut(index) {
            *share += 100.0 - sum;
        }
    }
    shares
}

/// Zone bars of the power distribution (`power_zone_secs`, Z1..Z7).
pub fn power_zone_bars(secs: &[f64; 7]) -> Vec<ZoneBar> {
    zone_bars(secs.iter().copied())
}

/// Zone bars of the heart-rate distribution (`hr_zone_secs`, H1..H5).
pub fn hr_zone_bars(secs: &[f64; 5]) -> Vec<ZoneBar> {
    zone_bars(secs.iter().copied())
}

fn zone_bars(secs: impl Iterator<Item = f64>) -> Vec<ZoneBar> {
    let values: Vec<f64> = secs
        .map(|s| if s.is_finite() && s > 0.0 { s } else { 0.0 })
        .collect();
    let shares = zone_shares(&values);
    values
        .iter()
        .zip(shares.iter())
        .enumerate()
        .filter_map(|(index, (secs, pct))| {
            let zone = u8::try_from(index + 1).ok()?;
            Some(ZoneBar {
                zone,
                secs: *secs,
                pct: *pct,
            })
        })
        .collect()
}

/// Summary date + time of day line: `2026.10.31. · 1:42:09` (elapsed handled separately).
pub fn summary_headline(summary: &RideSummary) -> String {
    format!(
        "{} · {}",
        fmt_date(summary.started_unix_s),
        format::fmt_duration(summary.elapsed_s)
    )
}

// ───────────────────────────── street view settings ─────────────────────────────

/// Longest Google Maps API key the settings field keeps (real keys have 39 characters).
pub const API_KEY_MAX_LEN: usize = 128;
/// Length of a real Google API key: `AIza` + 35 characters.
pub const API_KEY_TYPICAL_LEN: usize = 39;

/// What the key field says about the typed key. It is advice only: Google decides whether a
/// key works; we never block saving, so a typo can be corrected in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiKeyCheck {
    /// Nothing typed yet.
    Empty,
    /// Looks like a Google API key.
    Plausible,
    /// Allowed characters, but not the usual shape (`AIza…`, 39 characters).
    Unusual,
    /// Contains characters a Google API key never has.
    Invalid,
}

/// Cleans a pasted key: drops every whitespace and control character (a key copied from a web
/// page often carries a trailing newline or a wrapped line) and caps the length.
pub fn normalize_api_key(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .take(API_KEY_MAX_LEN)
        .collect()
}

/// Classifies a (normalised) key for the hint line below the field.
pub fn check_api_key(key: &str) -> ApiKeyCheck {
    if key.is_empty() {
        return ApiKeyCheck::Empty;
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return ApiKeyCheck::Invalid;
    }
    if key.starts_with("AIza") && key.chars().count() == API_KEY_TYPICAL_LEN {
        ApiKeyCheck::Plausible
    } else {
        ApiKeyCheck::Unusual
    }
}

/// The Hungarian hint line for a key check.
pub fn api_key_hint(check: ApiKeyCheck) -> &'static str {
    match check {
        ApiKeyCheck::Empty => "Illeszd be a saját Google Maps API-kulcsodat. Kulcs nélkül nem jelenik meg utcakép.",
        ApiKeyCheck::Plausible => "A kulcs alakja rendben van. Hogy valóban működik-e, az az első kérésnél derül ki.",
        ApiKeyCheck::Unusual => "Szokatlan kulcs: a Google API-kulcsok általában „AIza” kezdetűek és 39 karakter hosszúak.",
        ApiKeyCheck::Invalid => "A kulcs csak betűt, számot, kötőjelet és aláhúzást tartalmazhat.",
    }
}

/// Intensity factor with the two decimals the value needs; `—` when unavailable.
pub fn fmt_if(value: Option<f64>) -> String {
    match value {
        Some(v) if v.is_finite() => format::fmt_step_value(v, 0.01),
        _ => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(id: &str, opened: f64, name: &str) -> RecentRoute {
        RecentRoute {
            id: id.into(),
            name: name.into(),
            total_dist: 24_100.0,
            ascent: 410.0,
            source: RouteSource::GpxFile,
            last_opened_unix: opened,
        }
    }

    fn device(id: &str, name: &str, kinds: &[DeviceKind], rssi: Option<i16>) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: name.into(),
            kinds: kinds.to_vec(),
            rssi,
        }
    }

    fn summary() -> RideSummary {
        RideSummary {
            route_name: "Budai kör".into(),
            route_id: "demo-route".into(),
            started_unix_s: 1_790_000_000.0,
            elapsed_s: 6_129.0,
            dist: 24_100.0,
            ascent: 412.0,
            avg_speed_ms: 6.9,
            max_speed_ms: 13.2,
            avg_power_w: Some(212.0),
            max_power_w: Some(540.0),
            normalized_power_w: Some(238.0),
            intensity_factor: Some(0.9),
            tss: Some(138.0),
            work_kj: 1_300.0,
            kcal: 1_240.0,
            avg_hr: Some(148.0),
            max_hr: Some(176.0),
            avg_cadence: Some(88.0),
            power_zone_secs: [600.0, 1_800.0, 2_400.0, 900.0, 300.0, 100.0, 29.0],
            hr_zone_secs: [120.0, 1_800.0, 3_000.0, 1_100.0, 109.0],
        }
    }

    // ── bikerouter link validation ──

    #[test]
    fn valid_bikerouter_link_is_accepted() {
        let link = "https://bikerouter.de/#map=13/47.50/19.05/standard&lonlats=19.0,47.5;19.1,47.6&profile=trekking";
        assert_eq!(bikerouter_hint(link), None);
        assert!(is_valid_bikerouter_link(link));
        // host case does not matter
        assert!(is_valid_bikerouter_link(
            "HTTPS://BIKEROUTER.DE/#lonlats=19.0,47.5"
        ));
    }

    #[test]
    fn link_without_lonlats_is_rejected() {
        let link = "https://bikerouter.de/#map=13/47.50/19.05";
        let hint = bikerouter_hint(link).unwrap_or_default();
        assert!(hint.contains("lonlats="), "{hint}");
        assert!(!is_valid_bikerouter_link(link));
    }

    #[test]
    fn foreign_link_is_rejected() {
        let hint =
            bikerouter_hint("https://example.com/route?lonlats=19.0,47.5").unwrap_or_default();
        assert!(hint.contains("bikerouter.de"), "{hint}");
        assert!(!is_valid_bikerouter_link("https://example.com/x"));
    }

    #[test]
    fn empty_and_whitespace_links_are_rejected() {
        for empty in ["", "   ", "\n\t "] {
            assert!(
                bikerouter_hint(empty).is_some(),
                "{empty:?} must be rejected"
            );
            assert!(!is_valid_bikerouter_link(empty));
        }
    }

    #[test]
    fn link_input_is_normalised() {
        assert_eq!(
            normalize_link_input("  https://bikerouter.de/x  "),
            "https://bikerouter.de/x"
        );
        assert_eq!(
            normalize_link_input(" <https://bikerouter.de/#lonlats=1,2> "),
            "https://bikerouter.de/#lonlats=1,2"
        );
        assert_eq!(normalize_link_input(""), "");
        // a lone '<' is not a wrapper
        assert_eq!(normalize_link_input("<x"), "<x");
    }

    // ── recents ──

    #[test]
    fn recents_sort_most_recent_first() {
        let list = vec![
            recent("a", 1_700_000_000.0, "Régi"),
            recent("b", 1_790_000_000.0, "Legújabb"),
            recent("c", 1_750_000_000.0, "Középső"),
        ];
        let sorted = sorted_recents(&list);
        let ids: Vec<&str> = sorted.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
        // the input is not modified
        assert_eq!(list[0].id, "a");
    }

    #[test]
    fn recents_with_missing_timestamps_sort_last() {
        let list = vec![
            recent("bad", f64::NAN, "Hibás dátum"),
            recent("ok", 1_700_000_000.0, "Jó dátum"),
        ];
        let ids: Vec<String> = sorted_recents(&list).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["ok", "bad"]);
        assert_eq!(sorted_recents(&[]).len(), 0);
    }

    #[test]
    fn dates_are_formatted_hungarian_style() {
        assert_eq!(fmt_date(0.0), "1970.01.01.");
        assert_eq!(fmt_date(1_700_000_000.0), "2023.11.14.");
        assert_eq!(fmt_date(86_399.0), "1970.01.01.");
        assert_eq!(fmt_date(86_400.0), "1970.01.02.");
        // leap day
        assert_eq!(fmt_date(1_709_164_800.0), "2024.02.29.");
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e18] {
            assert_eq!(fmt_date(bad), "—", "{bad}");
        }
    }

    #[test]
    fn source_labels_are_hungarian() {
        assert_eq!(route_source_label(RouteSource::GpxFile), "GPX");
        assert_eq!(route_source_label(RouteSource::Bikerouter), "Bikerouter");
        assert_eq!(route_source_label(RouteSource::Demo), "Bemutató");
    }

    // ── devices ──

    #[test]
    fn simulated_rider_comes_first_then_signal_strength() {
        let list = vec![
            device("aa:bb", "KICKR", &[DeviceKind::Trainer], Some(-70)),
            device(
                SIM_DEVICE_ID,
                "Szimulált lovas",
                &[DeviceKind::Trainer],
                None,
            ),
            device("cc:dd", "Polar H10", &[DeviceKind::HeartRate], Some(-52)),
            device("ee:ff", "Nincs jel", &[DeviceKind::Cadence], None),
        ];
        let ids: Vec<String> = sorted_discovered(&list).into_iter().map(|d| d.id).collect();
        assert_eq!(ids, [SIM_DEVICE_ID, "cc:dd", "aa:bb", "ee:ff"]);
    }

    #[test]
    fn suggested_kind_prefers_the_trainer() {
        let multi = device(
            "x",
            "Combo",
            &[DeviceKind::Cadence, DeviceKind::PowerMeter],
            Some(-60),
        );
        assert_eq!(suggested_kind(&multi), Some(DeviceKind::PowerMeter));
        let trainer = device(
            "y",
            "KICKR",
            &[DeviceKind::Trainer, DeviceKind::PowerMeter],
            Some(-60),
        );
        assert_eq!(suggested_kind(&trainer), Some(DeviceKind::Trainer));
        let nothing = device("z", "Ismeretlen", &[], Some(-60));
        assert_eq!(suggested_kind(&nothing), None);
        let click = device("c", "Zwift Click 7F3A", &[DeviceKind::Shifter], Some(-60));
        assert_eq!(suggested_kind(&click), Some(DeviceKind::Shifter));
        assert_eq!(kind_label(DeviceKind::Shifter), "Váltó");
    }

    #[test]
    fn signal_quality_has_text_for_every_colour() {
        assert_eq!(rssi_label(Some(-40)), "Erős jel");
        assert_eq!(rssi_label(Some(-70)), "Közepes jel");
        assert_eq!(rssi_label(Some(-90)), "Gyenge jel");
        assert_eq!(rssi_label(None), "Nincs jel");
        assert_eq!(rssi_tone(Some(-40)), "ok");
        assert_eq!(rssi_tone(Some(-70)), "warn");
        assert_eq!(rssi_tone(Some(-90)), "bad");
        assert_eq!(rssi_tone(None), "off");
        assert!(rssi_text(Some(-62)).starts_with("-62"));
        assert!(rssi_text(Some(-62)).ends_with("dBm"));
        assert_eq!(rssi_text(None), "— dBm");
    }

    #[test]
    fn connection_states_map_to_hungarian_badges() {
        assert_eq!(conn_label(&ConnState::Disconnected), "Nincs kapcsolat");
        assert_eq!(conn_label(&ConnState::Connecting), "Kapcsolódás…");
        assert_eq!(conn_label(&ConnState::Connected), "Csatlakoztatva");
        assert_eq!(conn_label(&ConnState::Failed(String::new())), "Sikertelen");
        assert_eq!(conn_chip_class(&ConnState::Connected), "chip chip--ok");
        assert_eq!(
            conn_chip_class(&ConnState::Failed("x".into())),
            "chip chip--crit"
        );
        assert_eq!(conn_tone(&ConnState::Connecting), "busy");
        assert_eq!(conn_icon(&ConnState::Connected), Some(IconName::Check));
        assert_eq!(conn_icon(&ConnState::Connecting), None);
        assert_eq!(conn_failure(&ConnState::Failed("  ".into())), None);
        assert_eq!(
            conn_failure(&ConnState::Failed("időtúllépés".into())),
            Some("időtúllépés")
        );
        assert_eq!(conn_failure(&ConnState::Connected), None);
    }

    #[test]
    fn device_title_falls_back_to_the_id() {
        assert_eq!(device_title(&device("aa", " KICKR ", &[], None)), "KICKR");
        assert_eq!(device_title(&device("aa", "  ", &[], None)), "aa");
        assert!(is_simulated(&device(SIM_DEVICE_ID, "", &[], None)));
        assert!(!is_simulated(&device("aa", "", &[], None)));
    }

    #[test]
    fn slots_are_looked_up_by_kind() {
        let slots = vec![
            DeviceSlot {
                kind: DeviceKind::Trainer,
                state: ConnState::Connected,
                name: Some("KICKR".into()),
            },
            DeviceSlot {
                kind: DeviceKind::HeartRate,
                state: ConnState::Disconnected,
                name: None,
            },
        ];
        assert_eq!(
            slot_name(&slots, DeviceKind::Trainer).as_deref(),
            Some("KICKR")
        );
        assert_eq!(slot_name(&slots, DeviceKind::HeartRate), None);
        assert!(slot_of(&slots, DeviceKind::Cadence).is_none());
        assert_eq!(
            slot_state(&slots, DeviceKind::Trainer),
            ConnState::Connected
        );
        assert_eq!(
            slot_state(&slots, DeviceKind::Cadence),
            ConnState::Disconnected
        );
        assert!(slot_matches(&slots, DeviceKind::Trainer, "KICKR"));
        assert!(!slot_matches(&slots, DeviceKind::Trainer, "NEO"));
        assert!(!slot_matches(&slots, DeviceKind::HeartRate, "Polar"));
    }

    // ── virtual rider ──

    #[test]
    fn virtual_power_is_snapped_and_bounded() {
        assert_eq!(clamp_virtual_power(252.0), 250.0);
        assert_eq!(clamp_virtual_power(253.0), 255.0);
        assert_eq!(clamp_virtual_power(-10.0), 0.0);
        assert_eq!(clamp_virtual_power(5_000.0), VIRTUAL_RIDER_MAX_W);
        assert_eq!(clamp_virtual_power(f64::NAN), 0.0);
        for (label, watts, hint) in VIRTUAL_QUICK {
            assert!(!label.is_empty() && !hint.is_empty());
            assert_eq!(clamp_virtual_power(watts), watts, "{label}");
        }
    }

    // ── settings ──

    #[test]
    fn rider_patch_keeps_every_other_field() {
        let base = Settings::default();
        let next = with_rider(&base, |rider| rider.ftp_w = 268.0);
        assert_eq!(next.rider.ftp_w, 268.0);
        assert_eq!(next.rider.rider_kg, base.rider.rider_kg);
        assert_eq!(next.rider.cda, base.rider.cda);
        assert_eq!(
            next.rider.trainer_grade_scale,
            base.rider.trainer_grade_scale
        );
        assert_eq!(next.ui, base.ui, "the UI section must be untouched");
    }

    #[test]
    fn ui_patch_keeps_the_rider_section() {
        let base = Settings::default();
        let next = with_ui(&base, |ui| {
            ui.theme = Theme::Light;
            ui.follow_rider = false;
        });
        assert_eq!(next.ui.theme, Theme::Light);
        assert!(!next.ui.follow_rider);
        assert_eq!(next.ui.map_style, base.ui.map_style);
        assert_eq!(
            next.rider, base.rider,
            "the rider section must be untouched"
        );
    }

    #[test]
    fn settings_round_trip_through_the_change_handler() {
        // Simulates the dialog: every control emits the whole (patched) Settings value and the
        // app stores it; a sequence of changes must accumulate without losing earlier ones.
        let mut stored = Settings::default();
        // A settings control emits the whole patched value; the app stores it verbatim.
        type Change = Box<dyn Fn(&Settings) -> Settings>;
        let changes: Vec<Change> = vec![
            Box::new(|s| with_rider(s, |r| r.rider_kg = 82.5)),
            Box::new(|s| with_rider(s, |r| r.ftp_w = 275.0)),
            Box::new(|s| with_ui(s, |u| u.theme = Theme::Light)),
            Box::new(|s| with_ui(s, |u| u.lookahead_m = 5_000.0)),
            Box::new(|s| {
                with_rider(s, |r| {
                    r.trainer_grade_scale = trainer_scale_from_percent(85.0)
                })
            }),
        ];
        for change in &changes {
            stored = change(&stored);
        }
        assert_eq!(stored.rider.rider_kg, 82.5);
        assert_eq!(stored.rider.ftp_w, 275.0);
        assert_eq!(stored.rider.trainer_grade_scale, 0.85);
        assert_eq!(stored.ui.theme, Theme::Light);
        assert_eq!(stored.ui.lookahead_m, 5_000.0);
        // untouched fields keep their defaults
        assert_eq!(stored.rider.bike_kg, Settings::default().rider.bike_kg);
        assert_eq!(stored.ui.map_style, Settings::default().ui.map_style);

        // An identity patch is a true round trip.
        let identity = with_rider(&stored, |_| {});
        assert_eq!(identity, stored);
    }

    #[test]
    fn the_text_scale_slider_round_trips_and_stays_in_range() {
        for percent in (80..=160).step_by(5) {
            let scale = text_scale_from_percent(f64::from(percent));
            assert_eq!(text_scale_percent(scale), f64::from(percent), "{percent}");
        }
        assert_eq!(text_scale_percent(f64::NAN), 100.0);
        assert_eq!(text_scale_percent(9.0), 160.0);
        assert_eq!(text_scale_from_percent(10.0), 0.8);
        assert_eq!(text_scale_from_percent(f64::NAN), 1.0);
    }

    #[test]
    fn trainer_scale_slider_round_trips() {
        for scale in [0.0, 0.5, 0.85, 1.0, 1.5] {
            let percent = trainer_scale_percent(scale);
            assert!((0.0..=150.0).contains(&percent));
            let back = trainer_scale_from_percent(percent);
            assert!(
                (back - scale).abs() < 1e-9,
                "{scale} -> {percent} -> {back}"
            );
        }
        assert_eq!(trainer_scale_percent(f64::NAN), 100.0);
        assert_eq!(trainer_scale_from_percent(f64::NAN), 1.0);
        assert_eq!(trainer_scale_percent(9.0), 150.0, "clamped");
    }

    #[test]
    fn segmented_values_round_trip() {
        for theme in [Theme::Dark, Theme::Light] {
            assert_eq!(theme_from_value(&theme_value(theme)), theme);
        }
        for style in [MapStyle::Dark, MapStyle::Light, MapStyle::Osm] {
            assert_eq!(map_style_from_value(&map_style_value(style)), style);
        }
        for mode in [ProfileMode::Full, ProfileMode::Lookahead] {
            assert_eq!(profile_mode_from_value(&profile_mode_value(mode)), mode);
        }
        // unknown values fall back to the defaults
        assert_eq!(theme_from_value("nope"), Theme::Dark);
        assert_eq!(map_style_from_value(""), MapStyle::Dark);
        assert_eq!(profile_mode_from_value(""), ProfileMode::Lookahead);
    }

    // ── summary ──

    #[test]
    fn zone_percentages_sum_to_one_hundred() {
        let shares = zone_shares(&[600.0, 300.0, 100.0, 0.0]);
        assert_eq!(shares.len(), 4);
        assert!((shares[0] - 60.0).abs() < 1e-9);
        assert!((shares[1] - 30.0).abs() < 1e-9);
        assert!((shares[2] - 10.0).abs() < 1e-9);
        assert!((shares[3] - 0.0).abs() < 1e-9);
        let sum: f64 = shares.iter().sum();
        assert!((sum - 100.0).abs() < 1e-9, "sum={sum}");

        // awkward thirds still add up to exactly 100
        let thirds = zone_shares(&[1.0, 1.0, 1.0]);
        let sum: f64 = thirds.iter().sum();
        assert!((sum - 100.0).abs() < 1e-9, "sum={sum}");
        assert!(
            thirds.iter().all(|p| (p - 33.333_333).abs() < 0.001),
            "{thirds:?}"
        );
    }

    #[test]
    fn zero_seconds_give_zero_percentages() {
        let zero = zone_shares(&[0.0; 7]);
        assert_eq!(zero, vec![0.0; 7]);
        assert!(zero.iter().sum::<f64>() == 0.0);
        let bars = power_zone_bars(&[0.0; 7]);
        assert_eq!(bars.len(), 7);
        assert!(bars.iter().all(|b| b.pct == 0.0 && b.secs == 0.0));
        // missing / negative / non-finite time is ignored, not propagated
        let bars = hr_zone_bars(&[f64::NAN, -5.0, 0.0, 30.0, 10.0]);
        assert!((bars[3].pct - 75.0).abs() < 1e-9);
        assert!((bars[4].pct - 25.0).abs() < 1e-9);
        assert_eq!(bars[0].pct, 0.0);
        assert_eq!(bars[1].pct, 0.0);
    }

    #[test]
    fn zone_bars_are_numbered_from_one() {
        let bars = power_zone_bars(&summary().power_zone_secs);
        let zones: Vec<u8> = bars.iter().map(|b| b.zone).collect();
        assert_eq!(zones, [1, 2, 3, 4, 5, 6, 7]);
        let hr = hr_zone_bars(&summary().hr_zone_secs);
        assert_eq!(hr.len(), 5);
        let sum: f64 = hr.iter().map(|b| b.pct).sum();
        assert!((sum - 100.0).abs() < 1e-9);
    }

    #[test]
    fn summary_helpers_format_values() {
        let s = summary();
        assert_eq!(fmt_if(s.intensity_factor), "0.90");
        assert_eq!(fmt_if(None), "—");
        assert_eq!(fmt_if(Some(f64::NAN)), "—");
        assert!(
            summary_headline(&s).contains(" · 1:42:09"),
            "{}",
            summary_headline(&s)
        );
        assert!(summary_headline(&s).starts_with("2026."));
    }

    // ── street view key ──

    #[test]
    fn api_key_is_cleaned_of_whitespace_and_capped() {
        assert_eq!(normalize_api_key("  AIzaSy-abc_123 \r\n"), "AIzaSy-abc_123");
        assert_eq!(normalize_api_key("AIza Sy\tab\ncd"), "AIzaSyabcd");
        assert_eq!(normalize_api_key("\u{a0}x\u{7}"), "x");
        assert_eq!(normalize_api_key(""), "");
        let long = "a".repeat(500);
        assert_eq!(normalize_api_key(&long).len(), API_KEY_MAX_LEN);
    }

    #[test]
    fn api_key_check_classifies_the_shape() {
        let real = format!("AIza{}", "x".repeat(35));
        assert_eq!(real.len(), API_KEY_TYPICAL_LEN);
        assert_eq!(check_api_key(""), ApiKeyCheck::Empty);
        assert_eq!(check_api_key(&real), ApiKeyCheck::Plausible);
        assert_eq!(check_api_key("AIzaSy-_09"), ApiKeyCheck::Unusual);
        assert_eq!(check_api_key(&"b".repeat(39)), ApiKeyCheck::Unusual);
        assert_eq!(check_api_key("AIza.bad"), ApiKeyCheck::Invalid);
        assert_eq!(check_api_key("kulcs ő"), ApiKeyCheck::Invalid);
        assert_eq!(check_api_key("a/b"), ApiKeyCheck::Invalid);
        // every state has its own, non-empty Hungarian hint
        let hints: Vec<&str> = [
            ApiKeyCheck::Empty,
            ApiKeyCheck::Plausible,
            ApiKeyCheck::Unusual,
            ApiKeyCheck::Invalid,
        ]
        .into_iter()
        .map(api_key_hint)
        .collect();
        for (i, a) in hints.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &hints[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn street_view_fields_survive_a_ui_patch() {
        let mut base = Settings::default();
        base.ui.google_maps_key = "AIzaTest".into();
        let next = with_ui(&base, |ui| ui.streetview_cache_panos = true);
        assert!(next.ui.streetview_cache_panos);
        assert_eq!(next.ui.google_maps_key, "AIzaTest");
        // a rider patch must not touch the street view block either
        let rider = with_rider(&next, |r| r.ftp_w = 280.0);
        assert_eq!(rider.ui, next.ui);
    }
}
