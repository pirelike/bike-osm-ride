//! HUD: metric tiles, stats panel, next-climb card and the transport controls.
//!
//! The four components are the head unit of the ride screen (see `docs/DESIGN.md`): a power hero
//! (huge condensed numeral, a 7-step zone bar with the FTP tick), a ruled 2 x 2 data grid
//! (cadence, heart rate, speed, grade), the stats as a ruled table and the next climb as a flush
//! panel. Every visible number is produced by [`ride_ui_kit::format`]; the pure helpers in this
//! module ([`MetricView`], [`stat_rows`], [`stat_groups`], [`watts_per_kg`], [`power_zone_of`],
//! [`hr_zone_of`], [`ftp_tick_pct`], [`eta_text`], [`select_climb`], [`climb_progress`]) are
//! unit-tested and the components only lay the resulting strings out. A value that is not
//! available is shown as an em dash ([`DASH`]), never as `NaN` or an empty box.
//!
//! Colours come from the design-kit tokens (`var(--z4)`, `var(--cat-2)`, ...) or from
//! [`ride_ui_kit::palette`]; the stylesheet is [`CSS`] and every class is prefixed `hud-`.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{Climb, ClimbCategory, RideSnapshot, RideState, RiderSettings, Route, Theme};
use ride_ui_kit::color::ink_on;
use ride_ui_kit::format::{
    fmt_delta_s, fmt_duration, fmt_grade, fmt_hm_text, fmt_km, fmt_m, fmt_opt0, fmt_speed_kmh,
    fmt_step_value,
};
use ride_ui_kit::palette;
use ride_ui_kit::{Icon, IconName};

/// Component stylesheet (tokens only). The shell injects it after the design kit.
pub const CSS: &str = include_str!("../assets/hud.css");

/// Placeholder for a value that is not available ("—", U+2014).
pub const DASH: &str = "—";

mod dash;
pub use dash::{target_state, RideDash};

// ───────────────────────────── pure helpers ─────────────────────────────

/// Clamps a fraction to `0..=1`, mapping NaN and infinities to `0` so that a CSS percentage
/// can never become `NaN%`.
pub fn clamp01(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Hungarian name of a ride state (also used for the toolbar's accessible name).
pub fn state_label(state: RideState) -> &'static str {
    match state {
        RideState::Empty => "Nincs útvonal",
        RideState::Ready => "Indulásra kész",
        RideState::Riding => "Tekerés",
        RideState::Paused => "Szüneteltetve",
        RideState::Finished => "Befejezve",
    }
}

/// Class list of the state badge (a pill is semantically right for a state): green while
/// riding, amber while paused, neutral otherwise.
pub fn state_badge_class(state: RideState) -> &'static str {
    match state {
        RideState::Riding => "badge badge--ok",
        RideState::Paused => "badge badge--warn",
        RideState::Empty | RideState::Ready | RideState::Finished => "badge",
    }
}

/// Power-to-weight ratio in W/kg; `None` when there is no power or the weight is unusable
/// (`0`, negative, NaN).
pub fn watts_per_kg(power_w: Option<f64>, rider_kg: f64) -> Option<f64> {
    match power_w {
        Some(p) if p.is_finite() && rider_kg.is_finite() && rider_kg > 0.0 => Some(p / rider_kg),
        _ => None,
    }
}

/// Lower edges of the seven Coggan zones in percent of FTP, plus the end of the scale: the
/// zone bar maps every zone to one seventh of its width.
pub const ZONE_EDGES_PCT: [f64; 8] = [0.0, 55.0, 76.0, 91.0, 106.0, 121.0, 151.0, 200.0];

/// Coggan power zone `1..=7` of an absolute power relative to `ftp_w` (`<55, 55-75, 76-90,
/// 91-105, 106-120, 121-150, >150 %`). The thresholds mirror `ride_sim::zones::power_zone`
/// (the engine normally supplies the zone; this is the fallback for a snapshot without one).
/// `None` without power or with an unusable FTP (0, negative, NaN) — a rider without an FTP
/// simply has no zone badges.
pub fn power_zone_of(power_w: Option<f64>, ftp_w: f64) -> Option<u8> {
    let power = power_w.filter(|p| p.is_finite())?.max(0.0);
    if !ftp_w.is_finite() || ftp_w <= 0.0 {
        return None;
    }
    let pct = power / ftp_w * 100.0;
    Some(if pct < 55.0 {
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
    })
}

/// Heart-rate zone `1..=5` relative to `max_hr` (`<60, 60-70, 70-80, 80-90, >90 %`), with the
/// same thresholds as `ride_sim::zones::hr_zone`; `None` without a heart rate or with an
/// unusable maximum.
pub fn hr_zone_of(hr_bpm: Option<f64>, max_hr: f64) -> Option<u8> {
    let hr = hr_bpm.filter(|v| v.is_finite())?.max(0.0);
    if !max_hr.is_finite() || max_hr <= 0.0 {
        return None;
    }
    let pct = hr / max_hr * 100.0;
    Some(if pct < 60.0 {
        1
    } else if pct < 70.0 {
        2
    } else if pct < 80.0 {
        3
    } else if pct < 90.0 {
        4
    } else {
        5
    })
}

/// Position of a power inside the 7-step zone bar in percent of its width (`0..=100`): inside a
/// zone the position is linear between the zone's edges, so the FTP tick (100 %) and the live
/// power live on the same scale. `None` without power or with an unusable FTP.
pub fn zone_bar_pct(power_w: Option<f64>, ftp_w: f64) -> Option<f64> {
    let power = power_w.filter(|p| p.is_finite())?.max(0.0);
    if !ftp_w.is_finite() || ftp_w <= 0.0 {
        return None;
    }
    Some(zone_bar_pct_of_ftp_pct(power / ftp_w * 100.0))
}

/// [`zone_bar_pct`] for a power already expressed in percent of FTP.
pub fn zone_bar_pct_of_ftp_pct(ftp_pct: f64) -> f64 {
    if !ftp_pct.is_finite() || ftp_pct <= 0.0 {
        return 0.0;
    }
    for zone in 0..7 {
        let (lo, hi) = (ZONE_EDGES_PCT[zone], ZONE_EDGES_PCT[zone + 1]);
        if ftp_pct < hi {
            return (zone as f64 + (ftp_pct - lo) / (hi - lo)) / 7.0 * 100.0;
        }
    }
    100.0
}

/// Where the FTP tick sits on the zone bar: 100 % of FTP is 60 % of the way through zone 4
/// (91-106 %), i.e. at 51.4 % of the bar.
pub fn ftp_tick_pct() -> f64 {
    zone_bar_pct_of_ftp_pct(100.0)
}

/// Estimated time to the finish (`eta_s` seconds), `—` when the engine has no estimate.
pub fn eta_text(eta_s: Option<f64>) -> String {
    match eta_s {
        Some(s) if s.is_finite() && s >= 0.0 => fmt_hm_text(s),
        _ => DASH.to_string(),
    }
}

/// A speed value that is worth showing: `None` before a route is loaded (`Empty`) and for
/// non-finite values.
pub fn live_speed(state: RideState, speed_ms: f64) -> Option<f64> {
    if state == RideState::Empty || !speed_ms.is_finite() {
        None
    } else {
        Some(speed_ms)
    }
}

/// A grade value that is worth showing: `None` before a route is loaded and for non-finite
/// values.
pub fn live_grade(state: RideState, grade: f64) -> Option<f64> {
    if state == RideState::Empty || !grade.is_finite() {
        None
    } else {
        Some(grade)
    }
}

/// Text of a grade's direction; the grade cell carries both the colour and this label.
pub fn grade_label(grade: f64) -> &'static str {
    if grade > 0.5 {
        "Emelkedő"
    } else if grade < -0.5 {
        "Lejtő"
    } else {
        "Sík"
    }
}

/// The numeral printed on a climb flag: `4 3 2 1 HC`, `–` for an uncategorised climb.
pub fn category_numeral(category: Option<ClimbCategory>) -> &'static str {
    match category {
        Some(ClimbCategory::Cat4) => "4",
        Some(ClimbCategory::Cat3) => "3",
        Some(ClimbCategory::Cat2) => "2",
        Some(ClimbCategory::Cat1) => "1",
        Some(ClimbCategory::Hc) => "HC",
        None => "–",
    }
}

// ───────────────────────────── metric tiles view model ─────────────────────────────

/// Everything the metric tiles show, already formatted. Built once per render from the
/// snapshot, so the component itself contains no number formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricView {
    /// Hero number: the 3 s average power.
    pub power_text: String,
    /// Instantaneous power, the small figure beside the hero numeral.
    pub power_instant_text: String,
    pub wkg_text: String,
    /// FTP of the rider, whole watts (`—` without a usable FTP).
    pub ftp_text: String,
    /// Power zone 1..=7 (from the engine, or derived from FTP when the engine left it empty).
    pub zone: Option<u8>,
    pub zone_name: &'static str,
    pub cadence_text: String,
    pub hr_text: String,
    pub hr_zone: Option<u8>,
    pub hr_zone_name: &'static str,
    pub speed_text: String,
    pub avg_speed: Option<f64>,
    pub grade_text: String,
    pub grade: Option<f64>,
    pub grade_label: &'static str,
    /// `false` when no power, cadence or heart rate is available at all.
    pub has_sensor_data: bool,
    pub state: RideState,
}

impl MetricView {
    /// Formats one snapshot for the tiles. `rider` supplies FTP (zone fallback), max HR (zone
    /// fallback) and the weight (W/kg).
    pub fn new(snapshot: &RideSnapshot, rider: &RiderSettings) -> Self {
        let power = snapshot.power_3s.filter(|v| v.is_finite());
        let zone = snapshot
            .power_zone
            .or_else(|| power_zone_of(power, rider.ftp_w));
        let hr_zone = snapshot
            .hr_zone
            .or_else(|| hr_zone_of(snapshot.hr_bpm, rider.max_hr));
        let wkg = watts_per_kg(power, rider.rider_kg);
        let speed = live_speed(snapshot.state, snapshot.speed_ms);
        let grade = live_grade(snapshot.state, snapshot.pos.grade);
        Self {
            power_text: fmt_opt0(power),
            power_instant_text: fmt_opt0(snapshot.power_w),
            wkg_text: match wkg {
                Some(v) => fmt_step_value(v, 0.01),
                None => DASH.to_string(),
            },
            ftp_text: if rider.ftp_w.is_finite() && rider.ftp_w > 0.0 {
                fmt_opt0(Some(rider.ftp_w))
            } else {
                DASH.to_string()
            },
            zone,
            zone_name: zone.map_or("", palette::power_zone_name),
            cadence_text: fmt_opt0(snapshot.cadence_rpm),
            hr_text: fmt_opt0(snapshot.hr_bpm),
            hr_zone,
            hr_zone_name: hr_zone.map_or("", palette::hr_zone_name),
            speed_text: speed.map_or_else(|| DASH.to_string(), fmt_speed_kmh),
            avg_speed: live_speed(snapshot.state, snapshot.avg_speed_ms),
            grade_text: grade.map_or_else(|| DASH.to_string(), fmt_grade),
            grade,
            grade_label: grade.map_or("", grade_label),
            has_sensor_data: power.is_some()
                || snapshot.cadence_rpm.is_some()
                || snapshot.hr_bpm.is_some(),
            state: snapshot.state,
        }
    }
}

// ───────────────────────────── stats panel view model ─────────────────────────────

/// One label / value / unit row of the flat stats list (the value is already formatted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatRow {
    pub label: &'static str,
    pub value: String,
    pub unit: &'static str,
}

/// The flat stats list: distance, remaining, time, ETA, ascent done/left, average and
/// maximum speed, average/normalised/maximum power, work, energy, average/maximum heart rate.
/// (The panel itself shows the same numbers as the ruled table of [`stat_groups`].)
pub fn stat_rows(snapshot: &RideSnapshot) -> Vec<StatRow> {
    vec![
        StatRow {
            label: "Megtett táv",
            value: fmt_km(snapshot.dist),
            unit: "km",
        },
        StatRow {
            label: "Hátralévő táv",
            value: fmt_km(snapshot.remaining_dist),
            unit: "km",
        },
        StatRow {
            label: "Eltelt idő",
            value: fmt_duration(snapshot.elapsed_s),
            unit: "",
        },
        StatRow {
            label: "Hátralévő idő",
            value: eta_text(snapshot.eta_s),
            unit: "",
        },
        StatRow {
            label: "Megtett szint",
            value: fmt_m(snapshot.ascent_done),
            unit: "m",
        },
        StatRow {
            label: "Hátralévő szintemelkedés",
            value: fmt_m(snapshot.ascent_left),
            unit: "m",
        },
        StatRow {
            label: "Átlagsebesség",
            value: fmt_speed_kmh(snapshot.avg_speed_ms),
            unit: "km/h",
        },
        StatRow {
            label: "Csúcssebesség",
            value: fmt_speed_kmh(snapshot.max_speed_ms),
            unit: "km/h",
        },
        StatRow {
            label: "Átlagteljesítmény",
            value: fmt_opt0(snapshot.avg_power_w),
            unit: "W",
        },
        StatRow {
            label: "Normalizált teljesítmény",
            value: fmt_opt0(snapshot.normalized_power_w),
            unit: "W",
        },
        StatRow {
            label: "Csúcsteljesítmény",
            value: fmt_opt0(snapshot.max_power_w),
            unit: "W",
        },
        StatRow {
            label: "Munka",
            value: fmt_opt0(Some(snapshot.work_kj)),
            unit: "kJ",
        },
        StatRow {
            label: "Energia",
            value: fmt_opt0(Some(snapshot.kcal)),
            unit: "kcal",
        },
        StatRow {
            label: "Átlagpulzus",
            value: fmt_opt0(snapshot.avg_hr),
            unit: "bpm",
        },
        StatRow {
            label: "Csúcspulzus",
            value: fmt_opt0(snapshot.max_hr),
            unit: "bpm",
        },
    ]
}

/// One row of the ruled stats table: a label with its unit and three value columns (an empty
/// string is an empty cell, `—` a missing sensor value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatTableRow {
    pub label: &'static str,
    pub unit: &'static str,
    pub cells: [String; 3],
}

/// A block of the stats table under its own column heads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatGroup {
    pub heads: [&'static str; 3],
    pub rows: Vec<StatTableRow>,
}

/// Sum of two optional-free metre / second figures that tolerates a non-finite part.
fn finite_sum(a: f64, b: f64) -> Option<f64> {
    (a.is_finite() && b.is_finite()).then_some(a + b)
}

/// The stats as three ruled blocks: *done / left / total* for distance, ascent and time,
/// *average / normalised / maximum* for speed, power and heart rate, and the energy spent.
/// All fifteen numbers of [`stat_rows`] appear in it.
pub fn stat_groups(snapshot: &RideSnapshot) -> Vec<StatGroup> {
    let total_time = match (snapshot.eta_s, snapshot.elapsed_s) {
        (Some(eta), elapsed) if eta.is_finite() && eta >= 0.0 => {
            finite_sum(eta, elapsed).map_or_else(|| DASH.to_string(), fmt_hm_text)
        }
        _ => DASH.to_string(),
    };
    let total_dist =
        finite_sum(snapshot.dist, snapshot.remaining_dist).map_or_else(|| DASH.to_string(), fmt_km);
    let total_ascent = finite_sum(snapshot.ascent_done, snapshot.ascent_left)
        .map_or_else(|| DASH.to_string(), fmt_m);
    vec![
        StatGroup {
            heads: ["Megtett", "Hátralévő", "Összesen"],
            rows: vec![
                StatTableRow {
                    label: "Táv",
                    unit: "km",
                    cells: [
                        fmt_km(snapshot.dist),
                        fmt_km(snapshot.remaining_dist),
                        total_dist,
                    ],
                },
                StatTableRow {
                    label: "Szint",
                    unit: "m",
                    cells: [
                        fmt_m(snapshot.ascent_done),
                        fmt_m(snapshot.ascent_left),
                        total_ascent,
                    ],
                },
                StatTableRow {
                    label: "Idő",
                    unit: "",
                    cells: [
                        fmt_duration(snapshot.elapsed_s),
                        eta_text(snapshot.eta_s),
                        total_time,
                    ],
                },
            ],
        },
        StatGroup {
            heads: ["Átlag", "NP", "Csúcs"],
            rows: vec![
                StatTableRow {
                    label: "Sebesség",
                    unit: "km/h",
                    cells: [
                        fmt_speed_kmh(snapshot.avg_speed_ms),
                        String::new(),
                        fmt_speed_kmh(snapshot.max_speed_ms),
                    ],
                },
                StatTableRow {
                    label: "Teljesítmény",
                    unit: "W",
                    cells: [
                        fmt_opt0(snapshot.avg_power_w),
                        fmt_opt0(snapshot.normalized_power_w),
                        fmt_opt0(snapshot.max_power_w),
                    ],
                },
                StatTableRow {
                    label: "Pulzus",
                    unit: "bpm",
                    cells: [
                        fmt_opt0(snapshot.avg_hr),
                        String::new(),
                        fmt_opt0(snapshot.max_hr),
                    ],
                },
            ],
        },
        StatGroup {
            heads: ["Munka (kJ)", "Energia (kcal)", ""],
            rows: vec![StatTableRow {
                label: "Leadott",
                unit: "",
                cells: [
                    fmt_opt0(Some(snapshot.work_kj)),
                    fmt_opt0(Some(snapshot.kcal)),
                    String::new(),
                ],
            }],
        },
    ]
}

// ───────────────────────────── climb card view model ─────────────────────────────

/// One climb as the card shows it at the rider's distance.
#[derive(Clone, Debug, PartialEq)]
pub struct ClimbView {
    /// Index into [`Route::climbs`].
    pub index: usize,
    pub climb: Climb,
    /// `true` while the rider is between `start_dist` and `end_dist`.
    pub inside: bool,
    /// Distance to the climb start in metres (`0` while inside).
    pub ahead_m: f64,
    /// Progress inside the climb, `0..=1` (`0` before it, `1` at its end).
    pub progress: f64,
}

/// Progress of `dist` inside `climb`, clamped to `0..=1`. NaN or an empty clamp yields `0`.
pub fn climb_progress(climb: &Climb, dist: f64) -> f64 {
    let span = climb.end_dist - climb.start_dist;
    if !(dist.is_finite() && span.is_finite()) || span <= 0.0 {
        return 0.0;
    }
    clamp01((dist - climb.start_dist) / span)
}

/// Card content for climb `index` of `route` at `dist`; `None` when the index is stale
/// (the route changed under the snapshot).
pub fn climb_view(route: &Route, index: usize, dist: f64) -> Option<ClimbView> {
    let climb = route.climbs.get(index)?.clone();
    let inside = dist >= climb.start_dist && dist < climb.end_dist;
    Some(ClimbView {
        index,
        ahead_m: if inside {
            0.0
        } else {
            (climb.start_dist - dist).max(0.0)
        },
        progress: climb_progress(&climb, dist),
        inside,
        climb,
    })
}

/// The climb the card shows: the engine's `next_climb` when it still resolves, otherwise the
/// first climb ahead of the rider (e.g. a snapshot from before the route was swapped).
/// `next_climb_in` stays authoritative for the countdown while the rider is still before it.
pub fn select_climb(route: &Route, snapshot: &RideSnapshot) -> Option<ClimbView> {
    let index = snapshot
        .next_climb
        .filter(|i| *i < route.climbs.len())
        .or_else(|| route.climb_ahead(snapshot.dist))?;
    let mut view = climb_view(route, index, snapshot.dist)?;
    if !view.inside {
        if let Some(ahead) = snapshot.next_climb_in {
            if ahead.is_finite() && ahead >= 0.0 {
                view.ahead_m = ahead;
            }
        }
    }
    Some(view)
}

// ───────────────────────────── MetricTiles ─────────────────────────────

/// One cell of the 2 x 2 data grid: label (and an auxiliary figure) on top, the numeral below.
/// `width_ch` reserves the digits of the numeral so the unit never moves when the value changes.
#[component]
fn DataCell(
    label: String,
    value: String,
    unit: String,
    width_ch: f64,
    #[props(default)] children: Element,
) -> Element {
    let dim = value == DASH;
    rsx! {
        div { class: "cell hud-cell",
            div { class: "cell__label",
                span { "{label}" }
                span { class: "hud-aux", {children} }
            }
            div { class: "cell__value hud-cell__value",
                span {
                    class: if dim { "hud-n is-dim" } else { "hud-n" },
                    style: "--w: {width_ch}ch",
                    "{value}"
                }
                if !unit.is_empty() {
                    span { class: "cell__unit", "{unit}" }
                }
            }
        }
    }
}

/// The big live numbers: power (W, 3 s average, a 7-step zone bar with the FTP tick, W/kg and
/// the zone name), then cadence, heart rate (zone), speed and grade as a ruled 2 x 2 grid.
/// Each cell shows "—" when its sensor is absent.
#[component]
pub fn MetricTiles(
    snapshot: ReadSignal<RideSnapshot>,
    rider: ReadSignal<RiderSettings>,
    theme: ReadSignal<Theme>,
) -> Element {
    let view = MetricView::new(&snapshot.read(), &rider.read());
    let theme = *theme.read();
    let grade_color = view.grade.map(|g| palette::grade_color(g, theme));
    let hr_color = view
        .hr_zone
        .map_or("var(--text-faint)", palette::hr_zone_var);
    let ftp_tick = format!("{:.2}", ftp_tick_pct());
    let power_dim = view.power_text == DASH;

    rsx! {
        section { class: "hud-metrics", aria_label: "Élő mérőszámok",
            div { class: "hud-hero",
                div { class: "hud-hero__head",
                    span { class: "hud-label", "Teljesítmény" }
                    span { class: "hud-label hud-label--faint", "3 mp átlag" }
                    span { class: "hud-hero__state {state_badge_class(view.state)}", "{state_label(view.state)}" }
                }
                div { class: "hud-hero__body",
                    div { class: if power_dim { "num num--hero num--trim hud-hero__num is-dim" } else { "num num--hero num--trim hud-hero__num" },
                        span { class: "hud-n", style: "--w: 3ch", "{view.power_text}" }
                        span { class: "unit", "W" }
                    }
                    div { class: "hud-hero__side",
                        div { class: "hud-kv",
                            span { class: "hud-label", "Azonnali" }
                            span { class: "num num--md",
                                "{view.power_instant_text}"
                                span { class: "unit", "W" }
                            }
                        }
                        div { class: "hud-kv",
                            span { class: "hud-label", "Fajlagos" }
                            span { class: "num num--md",
                                "{view.wkg_text}"
                                span { class: "unit", "W/kg" }
                            }
                        }
                    }
                }
                div { class: "zonebar hud-zonebar", style: "--ftp: {ftp_tick}", aria_hidden: "true",
                    for z in 1..=7u8 {
                        span {
                            key: "{z}",
                            class: if view.zone.is_some_and(|v| v == z) {
                                "zonebar__seg is-current"
                            } else {
                                "zonebar__seg"
                            },
                            style: if view.zone.is_some() {
                                "--c: {palette::power_zone_var(z)}"
                            } else {
                                "--c: var(--bg-3)"
                            },
                        }
                    }
                    span { class: "zonebar__ftp" }
                }
                div { class: "hud-zlabels", aria_hidden: "true",
                    for z in 1..=7u8 {
                        span {
                            key: "{z}",
                            class: if view.zone.is_some_and(|v| v == z) { "is-current" } else { "" },
                            "Z{z}"
                        }
                    }
                }
                div { class: "hud-hero__foot",
                    if let Some(z) = view.zone {
                        span { class: "hud-zone",
                            span { class: "hud-sw", style: "--c: {palette::power_zone_var(z)}" }
                            span { class: "hud-zone__name", "Z{z} {view.zone_name}" }
                        }
                    } else {
                        span { class: "hud-zone hud-zone--none", "Nincs teljesítmény" }
                    }
                    span { class: "hud-label", "FTP {view.ftp_text} W" }
                }
            }
            div { class: "cells cells--2 cells--flush hud-grid",
                DataCell {
                    label: "Kadencia".to_string(),
                    value: view.cadence_text.clone(),
                    unit: "rpm".to_string(),
                    width_ch: 3.0,
                }
                DataCell {
                    label: "Pulzus".to_string(),
                    value: view.hr_text.clone(),
                    unit: "bpm".to_string(),
                    width_ch: 3.0,
                    if let Some(z) = view.hr_zone {
                        span { class: "hud-sw", style: "--c: {hr_color}" }
                        "H{z} {view.hr_zone_name}"
                    }
                }
                DataCell {
                    label: "Sebesség".to_string(),
                    value: view.speed_text.clone(),
                    unit: "km/h".to_string(),
                    width_ch: 3.5,
                    if let Some(avg) = view.avg_speed {
                        "átl. {fmt_speed_kmh(avg)}"
                    }
                }
                DataCell {
                    label: "Meredekség".to_string(),
                    value: view.grade_text.clone(),
                    unit: "%".to_string(),
                    width_ch: 3.5,
                    if let Some(color) = grade_color.clone() {
                        span { class: "hud-sw", style: "--c: {color}" }
                        "{view.grade_label}"
                    }
                }
            }
            if !view.has_sensor_data && view.state != RideState::Empty {
                div { class: "hud-note", role: "status",
                    span { class: "hud-note__title", "Nincs szenzoradat" }
                    span { "Csatlakoztass eszközt az Eszközök menüben." }
                }
            }
        }
    }
}

// ───────────────────────────── StatsPanel ─────────────────────────────

/// Secondary stats as a ruled table: done / left / total for distance, ascent and time;
/// average / normalised / maximum for speed, power and heart rate; work and energy.
#[component]
pub fn StatsPanel(snapshot: ReadSignal<RideSnapshot>) -> Element {
    let groups = stat_groups(&snapshot.read());
    rsx! {
        section { class: "hud-stats", aria_label: "Menetadatok",
            table { class: "hud-tbl",
                for group in groups {
                    tbody { key: "{group.heads[0]}",
                        tr { class: "hud-tbl__head",
                            th { class: "hud-tbl__corner", scope: "col" }
                            for head in group.heads {
                                th { scope: "col", "{head}" }
                            }
                        }
                        for row in group.rows {
                            tr { key: "{row.label}",
                                th { scope: "row",
                                    span { "{row.label}" }
                                    if !row.unit.is_empty() {
                                        span { class: "hud-tbl__unit", "{row.unit}" }
                                    }
                                }
                                for (i , cell) in row.cells.iter().enumerate() {
                                    td { key: "{i}", class: if cell == DASH { "is-dim" } else { "" }, "{cell}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────────── ClimbCard ─────────────────────────────

/// "Következő emelkedő 1.2 km múlva · 3.4 km @ 6.1 % · +208 m" — the next climb with a distance
/// countdown, length / average grade / gain, the category flag and, while inside it, a
/// progress bar with the remaining distance and gain. A flush panel; with no route or no
/// climb ahead it keeps its place and says so (the card lives in a dock panel of its own).
/// The card without a climb: same header, one quiet line.
fn climb_placeholder(text: &str) -> Element {
    rsx! {
        section { class: "hud-climb hud-climb--empty", aria_label: "Következő emelkedő: {text}",
            div { class: "hud-climb__head",
                span { class: "hud-label", "Következő emelkedő" }
            }
            p { class: "hud-climb__none", "{text}" }
        }
    }
}

#[component]
pub fn ClimbCard(
    route: ReadSignal<Option<Arc<Route>>>,
    snapshot: ReadSignal<RideSnapshot>,
    theme: ReadSignal<Theme>,
    /// The PR time of every climb of the route (indexed like `Route::climbs`): known before the
    /// ride from the ride log, so the card can show the target even before the first pedal stroke.
    prs: ReadSignal<Vec<Option<f64>>>,
) -> Element {
    let route = route.read().clone();
    let snapshot = snapshot.read().clone();
    let theme = *theme.read();
    let Some(route) = route else {
        return climb_placeholder("Nincs betöltött útvonal.");
    };
    let Some(view) = select_climb(&route, &snapshot) else {
        return climb_placeholder("Nincs több emelkedő az útvonalon.");
    };
    let known_pr = prs.read().get(view.index).copied().flatten();

    let climb = &view.climb;
    let cat = climb.category;
    let cat_color = palette::climb_category_color(cat, theme);
    let cat_ink = ink_on(&cat_color);
    let numeral = category_numeral(cat);
    let cat_word = if cat.is_some() {
        "kategória"
    } else {
        "besorolatlan"
    };
    let inside = view.inside;
    let title = if inside {
        "Emelkedő"
    } else {
        "Következő emelkedő"
    };

    let progress = clamp01(view.progress);
    let pct_text = format!("{:.1}", progress * 100.0);
    let pct_whole = format!("{:.0}", progress * 100.0);
    let remaining_m = (climb.end_dist - snapshot.dist).max(0.0);
    let hero_value = if inside {
        fmt_km(remaining_m)
    } else {
        fmt_km(view.ahead_m)
    };
    let hero_unit = if inside { "km hátra" } else { "km múlva" };
    let length_text = fmt_km(climb.length);
    let grade_text = fmt_grade(climb.avg_grade);
    let gain_text = fmt_m(climb.gain);
    let remaining_gain = fmt_m((climb.gain * (1.0 - progress)).max(0.0));
    let cat_label = palette::climb_category_label(cat);
    let summary = if inside {
        format!(
            "{title}, {cat_label}: {length_text} km hosszú, {grade_text} % átlagos meredekség, {hero_value} km és +{remaining_gain} m hátra, {pct_whole} % kész"
        )
    } else {
        format!(
            "{title}, {cat_label}: {hero_value} {hero_unit}, {length_text} km hosszú, {grade_text} % átlagos meredekség, +{gain_text} m szint"
        )
    };

    // The live segment (while inside this climb) brings the PR time and the difference measured
    // at the same distance; before the ride only the stored PR time is known.
    let live = snapshot
        .segment
        .filter(|segment| segment.climb == view.index);
    let pr_s = live.and_then(|segment| segment.pr_s).or(known_pr);
    let pr_delta = live.and_then(|segment| segment.delta_s);

    rsx! {
        section { class: "hud-climb", aria_label: "{summary}",
            div { class: "hud-climb__head",
                span { class: "hud-label", "{title}" }
                span { class: "hud-cat",
                    span {
                        class: "hud-flag",
                        style: "--c: {cat_color}; color: {cat_ink}",
                        "{numeral}"
                    }
                    span { class: "hud-label", "{cat_word}" }
                }
            }
            div { class: "num num--xl hud-climb__hero",
                "{hero_value}"
                span { class: "unit", "{hero_unit}" }
            }
            if let Some(pr_s) = pr_s {
                div { class: "hud-climb__pr",
                    span { class: "hud-label", "PR" }
                    span { class: "hud-climb__pr-time", "{fmt_duration(pr_s)}" }
                    if let Some(delta) = pr_delta {
                        span {
                            class: if delta < 0.0 { "hud-climb__pr-delta is-ahead" } else { "hud-climb__pr-delta" },
                            "· {fmt_delta_s(delta)} s"
                        }
                    }
                }
            }
            div { class: "cells cells--3 cells--flush hud-climb__cells",
                div { class: "cell cell--sm",
                    div { class: "cell__label", "Hossz" }
                    div { class: "cell__value",
                        "{length_text}"
                        span { class: "cell__unit", "km" }
                    }
                }
                div { class: "cell cell--sm",
                    div { class: "cell__label", "Átlag" }
                    div { class: "cell__value",
                        "{grade_text}"
                        span { class: "cell__unit", "%" }
                    }
                }
                div { class: "cell cell--sm",
                    div { class: "cell__label", "Szint" }
                    div { class: "cell__value",
                        "+{gain_text}"
                        span { class: "cell__unit", "m" }
                    }
                }
            }
            if inside {
                div { class: "progress hud-climb__bar", style: "--p: {pct_text}; --c: {cat_color}", i {} }
                div { class: "hud-climb__foot",
                    span { "Hátra {fmt_km(remaining_m)} km · +{remaining_gain} m" }
                    span { class: "hud-climb__pct", "{pct_whole} %" }
                }
            }
        }
    }
}

// ───────────────────────────── WorkoutCard ─────────────────────────────

/// The running workout: current block, ERG target, the two countdowns and overall progress.
/// Hidden when no workout is selected. After the last block it turns into a "done" state.
#[component]
pub fn WorkoutCard(snapshot: ReadSignal<RideSnapshot>) -> Element {
    let snapshot = snapshot.read().clone();
    let Some(w) = snapshot.workout.clone() else {
        return rsx! {
            section { class: "hud-workout hud-workout--empty", aria_label: "Edzés: nincs aktív edzés",
                p { class: "hud-climb__none", "Nincs aktív edzés. Az útvonal betöltése után indul a kiválasztott edzés." }
            }
        };
    };
    let finished = w.finished;
    let (target_text, unit_text) = match (w.target_w, finished) {
        (Some(t), _) => (format!("{}", t.round() as i64), "W"),
        (None, true) => ("Kész".to_string(), ""),
        (None, false) => ("Szabad".to_string(), ""),
    };
    let pct_text = w
        .ftp_pct
        .map(|p| format!("{p:.0} % FTP"))
        .unwrap_or_else(|| "nincs előírt watt".to_string());
    let cadence_text = w.cadence.map(|c| format!("{c} rpm"));
    let remaining = fmt_duration(w.step_remaining_s);
    let total_left = fmt_hm_text(w.total_remaining_s);
    let block_pct = format!("{:.0}", w.step_progress * 100.0);
    let overall = format!("{:.1}", w.progress * 100.0);
    let overall_whole = format!("{:.0}", w.progress * 100.0);
    let step_number = format!("{}/{}", w.step_index + 1, w.step_count);
    let next_text = if finished {
        "Minden blokk kész".to_string()
    } else {
        match &w.next_step_name {
            Some(name) => format!("Következik: {name}"),
            None => "Utolsó blokk".to_string(),
        }
    };
    let summary = if finished {
        format!("Edzés kész: {}", w.workout_name)
    } else {
        format!(
            "Edzés: {}, {} – {} W, {} hátra, {} kész",
            w.workout_name, w.step_name, target_text, remaining, overall_whole
        )
    };

    rsx! {
        section { class: if finished { "hud-workout hud-workout--done" } else { "hud-workout" },
            aria_label: "{summary}",
            div { class: "hud-workout__head",
                span { class: "hud-label", "Edzés" }
                span { class: "hud-workout__name", "{w.workout_name}" }
                span { class: "hud-workout__count", "{step_number}" }
            }
            div { class: "hud-workout__body",
                div { class: "hud-workout__step",
                    span { class: "hud-workout__stepname", "{w.step_name}" }
                    span { class: "hud-workout__kind", "{w.step_kind.label()}" }
                }
                div { class: "hud-workout__target",
                    div { class: "num num--lg hud-workout__w",
                        "{target_text}"
                        if !unit_text.is_empty() {
                            span { class: "unit", "{unit_text}" }
                        }
                    }
                    div { class: "hud-workout__sub",
                        span { "{pct_text}" }
                        if let Some(c) = cadence_text {
                            span { "· {c}" }
                        }
                    }
                }
                div { class: "cells cells--3 cells--flush hud-workout__cells",
                    div { class: "cell cell--sm",
                        div { class: "cell__label", "Blokk vége" }
                        div { class: "cell__value", "{remaining}" }
                    }
                    div { class: "cell cell--sm",
                        div { class: "cell__label", "Edzés vége" }
                        div { class: "cell__value", "{total_left}" }
                    }
                    div { class: "cell cell--sm",
                        div { class: "cell__label", "Blokk" }
                        div { class: "cell__value",
                            "{block_pct}"
                            span { class: "cell__unit", "%" }
                        }
                    }
                }
                div { class: "progress hud-workout__bar", style: "--p: {overall}", i {} }
                div { class: "hud-workout__foot",
                    span { class: "hud-workout__pct", "{overall_whole} %" }
                    span { "{next_text}" }
                }
                // the coach's message of the moment, in the card (never a separate notification)
                if let Some(message) = &w.message {
                    p { key: "{message.id}", class: "hud-dash__coach", role: "status", "{message.text}" }
                }
            }
        }
    }
}

// ───────────────────────────── RideControls ─────────────────────────────

/// One transport button: icon + Hungarian label, with an explicit accessible name.
#[component]
fn TransportButton(
    label: String,
    icon: IconName,
    #[props(default = false)] primary: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = String::new())] shortcut: String,
    on_click: EventHandler<()>,
) -> Element {
    let class = if primary {
        "btn btn--primary hud-controls__btn"
    } else {
        "btn hud-controls__btn"
    };
    let title = if shortcut.is_empty() {
        label.clone()
    } else {
        format!("{label} ({shortcut})")
    };
    rsx! {
        button {
            class,
            r#type: "button",
            "aria-label": "{label}",
            title: "{title}",
            disabled,
            onclick: move |_| on_click.call(()),
            Icon { name: icon, size: 16 }
            span { "{label}" }
        }
    }
}

/// Start / pause / resume / finish / restart depending on `snapshot.state`, as a ruled button
/// group with the keyboard hint of the app (Space = start / pause / resume). A 32 px bar that
/// the shell docks at the top-left corner of the profile panel.
#[component]
pub fn RideControls(
    snapshot: ReadSignal<RideSnapshot>,
    on_start: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_finish: EventHandler<()>,
    on_reset: EventHandler<()>,
) -> Element {
    let state = snapshot.read().state;
    let toolbar_label = format!("Menet vezérlése — {}", state_label(state));

    rsx! {
        div { class: "hud-controls", role: "toolbar", aria_label: "{toolbar_label}",
            div { class: "btn-group",
                match state {
                    RideState::Empty => rsx! {
                        TransportButton {
                            label: "Kezdés".to_string(),
                            icon: IconName::Play,
                            primary: true,
                            disabled: true,
                            on_click: move |_| {},
                        }
                    },
                    RideState::Ready => rsx! {
                        TransportButton {
                            label: "Kezdés".to_string(),
                            icon: IconName::Play,
                            primary: true,
                            shortcut: "Szóköz".to_string(),
                            on_click: move |_| on_start.call(()),
                        }
                    },
                    RideState::Riding => rsx! {
                        TransportButton {
                            label: "Szünet".to_string(),
                            icon: IconName::Pause,
                            primary: true,
                            shortcut: "Szóköz".to_string(),
                            on_click: move |_| on_pause.call(()),
                        }
                        TransportButton {
                            label: "Befejezés".to_string(),
                            icon: IconName::Stop,
                            on_click: move |_| on_finish.call(()),
                        }
                    },
                    RideState::Paused => rsx! {
                        TransportButton {
                            label: "Folytatás".to_string(),
                            icon: IconName::Play,
                            primary: true,
                            shortcut: "Szóköz".to_string(),
                            on_click: move |_| on_resume.call(()),
                        }
                        TransportButton {
                            label: "Befejezés".to_string(),
                            icon: IconName::Stop,
                            on_click: move |_| on_finish.call(()),
                        }
                    },
                    RideState::Finished => rsx! {
                        TransportButton {
                            label: "Újrakezdés".to_string(),
                            icon: IconName::Reset,
                            primary: true,
                            on_click: move |_| on_reset.call(()),
                        }
                    },
                }
            }
            if matches!(state, RideState::Ready | RideState::Riding | RideState::Paused) {
                span { class: "kbd hud-controls__kbd", title: "Szóköz: indítás, szünet, folytatás", "Szóköz" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::{ClimbCategory, RoutePosition};
    use std::collections::HashSet;

    // ── fixtures ────────────────────────────────────────────────────────────────

    fn rider() -> RiderSettings {
        RiderSettings {
            ftp_w: 235.0,
            max_hr: 190.0,
            rider_kg: 75.0,
            ..RiderSettings::default()
        }
    }

    fn mid_climb_snapshot() -> RideSnapshot {
        RideSnapshot {
            state: RideState::Riding,
            pos: RoutePosition {
                dist: 6_200.0,
                lat: 47.5,
                lon: 19.05,
                ele: 420.0,
                grade: 6.8,
                heading_deg: 90.0,
            },
            speed_ms: 7.2,
            power_w: Some(252.0),
            power_3s: Some(238.0),
            cadence_rpm: Some(84.0),
            hr_bpm: Some(148.0),
            resistance: Some(42.0),
            elapsed_s: 1_264.0,
            dist: 6_200.0,
            remaining_dist: 15_800.0,
            eta_s: Some(4_200.0),
            ascent_done: 312.0,
            descent_done: 148.0,
            ascent_left: 268.0,
            avg_speed_ms: 7.1,
            max_speed_ms: 12.4,
            avg_power_w: Some(214.0),
            max_power_w: Some(402.0),
            normalized_power_w: Some(236.0),
            work_kj: 264.0,
            kcal: 264.0,
            avg_hr: Some(139.0),
            max_hr: Some(168.0),
            power_zone: Some(4),
            hr_zone: Some(3),
            next_climb: Some(0),
            next_climb_in: Some(0.0),
            trainer_grade: 6.6,
            workout: None,
            ghost: None,
            segment: None,
        }
    }

    fn two_climb_route() -> Route {
        let mut route = straight_route(
            &[
                (6000.0, 2.0),
                (4000.0, 7.0),
                (3000.0, -2.0),
                (5000.0, 4.0),
                (4000.0, -1.0),
            ],
            118.0,
        );
        route.name = "Budai-hegység kör".into();
        route.climbs = vec![
            Climb {
                start_dist: 4_300.0,
                end_dist: 11_100.0,
                length: 6_800.0,
                gain: 462.0,
                avg_grade: 6.8,
                max_grade: 11.4,
                start_ele: 118.0,
                end_ele: 580.0,
                category: Some(ClimbCategory::Cat2),
            },
            Climb {
                start_dist: 15_500.0,
                end_dist: 18_900.0,
                length: 3_400.0,
                gain: 208.0,
                avg_grade: 6.1,
                max_grade: 9.2,
                start_ele: 96.0,
                end_ele: 304.0,
                category: Some(ClimbCategory::Cat4),
            },
        ];
        route
    }

    fn snapshot_at(dist: f64) -> RideSnapshot {
        let mut s = RideSnapshot::empty();
        s.state = RideState::Ready;
        s.dist = dist;
        s.pos.dist = dist;
        s
    }

    // ── helpers ────────────────────────────────────────────────────────────────

    #[test]
    fn watts_per_kg_needs_power_and_a_positive_weight() {
        assert_eq!(watts_per_kg(Some(238.0), 75.0), Some(238.0 / 75.0));
        assert_eq!(watts_per_kg(None, 75.0), None);
        assert_eq!(watts_per_kg(Some(238.0), 0.0), None);
        assert_eq!(watts_per_kg(Some(238.0), -70.0), None);
        assert_eq!(watts_per_kg(Some(238.0), f64::NAN), None);
        assert_eq!(watts_per_kg(Some(f64::NAN), 75.0), None);
        assert_eq!(watts_per_kg(Some(f64::INFINITY), 75.0), None);
    }

    #[test]
    fn power_zones_follow_coggan_and_need_an_ftp() {
        let ftp = 200.0;
        // same edges as ride_sim::zones: <55, <76, <91, <106, <121, <151, otherwise
        let cases = [
            (0.0, 1),
            (109.9, 1), // 54.95 %
            (110.0, 2), // 55 %
            (151.9, 2), // 75.95 %
            (152.0, 3), // 76 %
            (181.9, 3), // 90.95 %
            (182.0, 4), // 91 %
            (211.9, 4), // 105.95 %
            (212.0, 5), // 106 %
            (241.9, 5), // 120.95 %
            (242.0, 6), // 121 %
            (301.9, 6), // 150.95 %
            (302.0, 7), // 151 %
            (900.0, 7),
        ];
        for (watts, zone) in cases {
            assert_eq!(power_zone_of(Some(watts), ftp), Some(zone), "{watts} W");
        }
        assert_eq!(power_zone_of(None, ftp), None);
        assert_eq!(power_zone_of(Some(238.0), 0.0), None);
        assert_eq!(power_zone_of(Some(238.0), f64::NAN), None);
        assert_eq!(power_zone_of(Some(f64::NAN), ftp), None);
        // negative (trainer noise while coasting) counts as Z1, not as an error
        assert_eq!(power_zone_of(Some(-12.0), ftp), Some(1));
    }

    #[test]
    fn hr_zones_need_a_maximum() {
        let max = 190.0;
        // same edges as ride_sim::zones: <60, <70, <80, <90, otherwise
        let cases = [
            (0.0, 1),
            (113.0, 1), // 59.5 %
            (114.0, 2), // 60 %
            (132.9, 2), // 69.9 %
            (133.0, 3), // 70 %
            (151.9, 3), // 79.9 %
            (152.0, 4), // 80 %
            (170.9, 4), // 89.9 %
            (171.0, 5), // 90 %
            (220.0, 5),
        ];
        for (bpm, zone) in cases {
            assert_eq!(hr_zone_of(Some(bpm), max), Some(zone), "{bpm} bpm");
        }
        assert_eq!(hr_zone_of(None, max), None);
        assert_eq!(hr_zone_of(Some(148.0), 0.0), None);
        assert_eq!(hr_zone_of(Some(148.0), f64::NAN), None);
    }

    #[test]
    fn zone_bar_position_is_piecewise_linear_per_zone() {
        // zone starts are multiples of one seventh
        for (pct, zone_start) in [
            (0.0, 0.0),
            (55.0, 1.0),
            (76.0, 2.0),
            (91.0, 3.0),
            (106.0, 4.0),
            (121.0, 5.0),
            (151.0, 6.0),
        ] {
            let got = zone_bar_pct_of_ftp_pct(pct);
            assert!(
                (got - zone_start / 7.0 * 100.0).abs() < 1e-9,
                "{pct} % of FTP -> {got}"
            );
        }
        // the middle of zone 4 is the middle of the 4th segment
        let mid = zone_bar_pct_of_ftp_pct((91.0 + 106.0) / 2.0);
        assert!((mid - 3.5 / 7.0 * 100.0).abs() < 1e-9, "{mid}");
        // monotone, bounded
        let mut last = -1.0;
        for step in 0..=400 {
            let p = zone_bar_pct_of_ftp_pct(f64::from(step));
            assert!((0.0..=100.0).contains(&p), "{p}");
            assert!(p >= last, "monotone at {step} %");
            last = p;
        }
        assert_eq!(zone_bar_pct_of_ftp_pct(1_000.0), 100.0);
        assert_eq!(zone_bar_pct_of_ftp_pct(-5.0), 0.0);
        assert_eq!(zone_bar_pct_of_ftp_pct(f64::NAN), 0.0);
        // FTP itself is 60 % through zone 4
        assert!((ftp_tick_pct() - 3.6 / 7.0 * 100.0).abs() < 1e-9);
        assert!(ftp_tick_pct() > 50.0 && ftp_tick_pct() < 53.0);
        // the live power lands on the same scale: exactly FTP is exactly on the tick
        assert!((zone_bar_pct(Some(235.0), 235.0).unwrap_or(-1.0) - ftp_tick_pct()).abs() < 1e-9);
        assert_eq!(zone_bar_pct(None, 235.0), None);
        assert_eq!(zone_bar_pct(Some(200.0), 0.0), None);
        assert_eq!(zone_bar_pct(Some(f64::NAN), 235.0), None);
        // a power in zone N is inside segment N
        for power in [50.0, 120.0, 190.0, 235.0, 270.0, 330.0, 420.0] {
            let zone = power_zone_of(Some(power), 235.0).unwrap_or(0);
            let pos = zone_bar_pct(Some(power), 235.0).unwrap_or(-1.0);
            let lo = f64::from(zone - 1) / 7.0 * 100.0;
            let hi = f64::from(zone) / 7.0 * 100.0;
            assert!(
                pos >= lo - 1e-9 && pos < hi + 1e-9,
                "{power} W: {pos} not in {lo}..{hi}"
            );
        }
    }

    #[test]
    fn clamp_is_never_nan() {
        assert_eq!(clamp01(f64::NAN), 0.0);
        assert_eq!(clamp01(f64::INFINITY), 0.0);
        assert_eq!(clamp01(-3.0), 0.0);
        assert_eq!(clamp01(3.0), 1.0);
    }

    #[test]
    fn category_numerals_follow_the_flag_convention() {
        assert_eq!(category_numeral(Some(ClimbCategory::Cat4)), "4");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat3)), "3");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat2)), "2");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat1)), "1");
        assert_eq!(category_numeral(Some(ClimbCategory::Hc)), "HC");
        assert_eq!(category_numeral(None), "–");
    }

    #[test]
    fn state_badges_are_pills_only_for_states() {
        assert_eq!(state_badge_class(RideState::Riding), "badge badge--ok");
        assert_eq!(state_badge_class(RideState::Paused), "badge badge--warn");
        assert_eq!(state_badge_class(RideState::Ready), "badge");
        assert_eq!(state_badge_class(RideState::Empty), "badge");
        assert_eq!(state_badge_class(RideState::Finished), "badge");
    }

    #[test]
    fn eta_uses_the_duration_format_or_a_dash() {
        assert_eq!(eta_text(Some(4_200.0)), fmt_hm_text(4_200.0));
        assert_eq!(eta_text(Some(4_200.0)), "1:10 ó");
        assert_eq!(eta_text(Some(600.0)), "10 p");
        assert_eq!(eta_text(None), "—");
        assert_eq!(eta_text(Some(-1.0)), "—");
        assert_eq!(eta_text(Some(f64::NAN)), "—");
        assert_eq!(eta_text(Some(f64::INFINITY)), "—");
    }

    #[test]
    fn climb_selection_before_inside_and_after() {
        let route = two_climb_route();
        assert_eq!(route.climbs.len(), 2);

        // before the first climb: the countdown comes from `next_climb_in`
        let mut before = snapshot_at(2_800.0);
        before.next_climb = Some(0);
        before.next_climb_in = Some(1_500.0);
        let view = select_climb(&route, &before).expect("climb ahead");
        assert_eq!(view.index, 0);
        assert!(!view.inside);
        assert_eq!(view.ahead_m, 1_500.0);
        assert_eq!(view.progress, 0.0);

        // inside the first climb: no countdown, progress instead
        let inside = snapshot_at(6_200.0);
        let view = select_climb(&route, &inside).expect("climb ahead");
        assert_eq!(view.index, 0);
        assert!(view.inside);
        assert_eq!(view.ahead_m, 0.0);
        assert!((view.progress - (1_900.0 / 6_800.0)).abs() < 1e-9);

        // between the climbs: the second one is next
        let view = select_climb(&route, &snapshot_at(12_000.0)).expect("climb ahead");
        assert_eq!(view.index, 1);
        assert!(!view.inside);
        assert_eq!(view.ahead_m, 3_500.0);

        // after the last climb: nothing to show
        assert!(select_climb(&route, &snapshot_at(20_000.0)).is_none());
        assert!(select_climb(&route, &snapshot_at(18_900.0)).is_none());
    }

    #[test]
    fn climb_selection_falls_back_when_the_snapshot_index_is_stale() {
        let route = two_climb_route();
        let mut stale = snapshot_at(6_200.0);
        stale.next_climb = Some(99); // route changed under the snapshot
        let view = select_climb(&route, &stale).expect("falls back to climb_ahead");
        assert_eq!(view.index, 0);

        // a stale `next_climb_in` must not override the distance while inside a climb
        let mut inside = snapshot_at(6_200.0);
        inside.next_climb = Some(0);
        inside.next_climb_in = Some(0.0);
        let view = select_climb(&route, &inside).expect("inside");
        assert_eq!(view.ahead_m, 0.0);
    }

    #[test]
    fn climb_progress_is_clamped() {
        let route = two_climb_route();
        let climb = &route.climbs[0];
        assert_eq!(climb_progress(climb, 0.0), 0.0);
        assert_eq!(climb_progress(climb, climb.start_dist), 0.0);
        assert!((climb_progress(climb, climb.start_dist + climb.length / 2.0) - 0.5).abs() < 1e-9);
        assert_eq!(climb_progress(climb, climb.end_dist), 1.0);
        assert_eq!(climb_progress(climb, 1e9), 1.0);
        assert_eq!(climb_progress(climb, f64::NAN), 0.0);
        // a degenerate climb cannot divide by zero
        let mut flat = climb.clone();
        flat.end_dist = flat.start_dist;
        assert_eq!(climb_progress(&flat, 5_000.0), 0.0);
    }

    #[test]
    fn live_values_hide_before_a_route_is_loaded() {
        assert_eq!(live_speed(RideState::Empty, 7.2), None);
        assert_eq!(live_speed(RideState::Riding, 7.2), Some(7.2));
        assert_eq!(live_speed(RideState::Riding, f64::NAN), None);
        assert_eq!(live_grade(RideState::Empty, 6.8), None);
        assert_eq!(live_grade(RideState::Riding, 6.8), Some(6.8));
        assert_eq!(live_grade(RideState::Riding, f64::NAN), None);
        assert_eq!(grade_label(6.8), "Emelkedő");
        assert_eq!(grade_label(-4.0), "Lejtő");
        assert_eq!(grade_label(0.2), "Sík");
    }

    #[test]
    fn metric_view_formats_a_live_snapshot() {
        let view = MetricView::new(&mid_climb_snapshot(), &rider());
        assert_eq!(view.power_text, "238");
        assert_eq!(view.power_instant_text, "252");
        assert_eq!(view.wkg_text, fmt_step_value(238.0 / 75.0, 0.01)); // 238 / 75
        assert_eq!(view.ftp_text, "235");
        assert_eq!(view.zone, Some(4)); // 238 / 235 = 101 %
        assert_eq!(view.zone_name, "Küszöb");
        assert_eq!(view.cadence_text, "84");
        assert_eq!(view.hr_text, "148");
        assert_eq!(view.hr_zone, Some(3)); // 148 / 190 = 78 %
        assert_eq!(view.hr_zone_name, "Aerob");
        assert_eq!(view.speed_text, fmt_speed_kmh(7.2));
        assert_eq!(view.grade_text, fmt_grade(6.8));
        assert_eq!(view.grade_label, "Emelkedő");
        assert!(view.has_sensor_data);
        assert_eq!(view.state, RideState::Riding);
    }

    #[test]
    fn metric_view_of_the_empty_snapshot_is_all_dashes() {
        let view = MetricView::new(&RideSnapshot::empty(), &rider());
        assert_eq!(view.power_text, "—");
        assert_eq!(view.power_instant_text, "—");
        assert_eq!(view.wkg_text, "—");
        assert_eq!(view.cadence_text, "—");
        assert_eq!(view.hr_text, "—");
        assert_eq!(view.speed_text, "—");
        assert_eq!(view.grade_text, "—");
        assert_eq!(view.zone, None);
        assert_eq!(view.hr_zone, None);
        assert_eq!(view.avg_speed, None);
        assert!(!view.has_sensor_data);
    }

    #[test]
    fn metric_view_survives_a_rideless_ftp() {
        let mut settings = rider();
        settings.ftp_w = 0.0;
        let mut snapshot = mid_climb_snapshot();
        snapshot.power_zone = None; // no zone from the engine either
        let view = MetricView::new(&snapshot, &settings);
        assert_eq!(view.zone, None, "FTP 0 must not invent a zone");
        assert_eq!(view.zone_name, "");
        assert_eq!(view.ftp_text, "—");
        assert_eq!(view.power_text, "238", "the power itself is still shown");
    }

    #[test]
    fn stats_rows_cover_the_brief_and_use_the_format_helpers() {
        let rows = stat_rows(&mid_climb_snapshot());
        assert_eq!(rows.len(), 15);
        let get = |label: &str| {
            rows.iter()
                .find(|r| r.label == label)
                .unwrap_or_else(|| panic!("missing row {label}"))
        };
        assert_eq!(get("Megtett táv").value, fmt_km(6_200.0));
        assert_eq!(get("Megtett táv").unit, "km");
        assert_eq!(get("Hátralévő táv").value, fmt_km(15_800.0));
        assert_eq!(get("Eltelt idő").value, "21:04");
        assert_eq!(get("Hátralévő idő").value, "1:10 ó");
        assert_eq!(get("Megtett szint").value, "312");
        assert_eq!(get("Hátralévő szintemelkedés").value, "268");
        assert_eq!(get("Átlagsebesség").value, fmt_speed_kmh(7.1));
        assert_eq!(get("Csúcssebesség").value, fmt_speed_kmh(12.4));
        assert_eq!(get("Átlagteljesítmény").value, "214");
        assert_eq!(get("Normalizált teljesítmény").value, "236");
        assert_eq!(get("Csúcsteljesítmény").value, "402");
        assert_eq!(get("Munka").value, "264");
        assert_eq!(get("Munka").unit, "kJ");
        assert_eq!(get("Energia").value, "264");
        assert_eq!(get("Energia").unit, "kcal");
        assert_eq!(get("Átlagpulzus").value, "139");
        assert_eq!(get("Csúcspulzus").value, "168");
    }

    #[test]
    fn stats_rows_of_the_empty_snapshot_have_dashes_for_missing_sensors() {
        let rows = stat_rows(&RideSnapshot::empty());
        let avg_power = rows.iter().find(|r| r.label == "Átlagteljesítmény");
        assert_eq!(avg_power.map(|r| r.value.as_str()), Some("—"));
        let eta = rows.iter().find(|r| r.label == "Hátralévő idő");
        assert_eq!(eta.map(|r| r.value.as_str()), Some("—"));
        let dist = rows.iter().find(|r| r.label == "Megtett táv");
        assert_eq!(dist.map(|r| r.value.clone()), Some(fmt_km(0.0)));
    }

    #[test]
    fn stat_groups_carry_the_same_numbers_as_the_flat_list() {
        let snapshot = mid_climb_snapshot();
        let groups = stat_groups(&snapshot);
        assert_eq!(groups.len(), 3);
        let rows = stat_rows(&snapshot);
        let flat = |label: &str| {
            rows.iter()
                .find(|r| r.label == label)
                .map(|r| r.value.clone())
                .unwrap_or_default()
        };
        let cell = |g: usize, r: usize, c: usize| groups[g].rows[r].cells[c].clone();
        // done / left / total
        assert_eq!(groups[0].heads, ["Megtett", "Hátralévő", "Összesen"]);
        assert_eq!(cell(0, 0, 0), flat("Megtett táv"));
        assert_eq!(cell(0, 0, 1), flat("Hátralévő táv"));
        assert_eq!(cell(0, 0, 2), fmt_km(22_000.0), "6.2 + 15.8 km");
        assert_eq!(cell(0, 1, 0), flat("Megtett szint"));
        assert_eq!(cell(0, 1, 1), flat("Hátralévő szintemelkedés"));
        assert_eq!(cell(0, 1, 2), "580", "312 + 268 m");
        assert_eq!(cell(0, 2, 0), flat("Eltelt idő"));
        assert_eq!(cell(0, 2, 1), flat("Hátralévő idő"));
        assert_eq!(cell(0, 2, 2), fmt_hm_text(4_200.0 + 1_264.0));
        // average / normalised / maximum
        assert_eq!(groups[1].heads, ["Átlag", "NP", "Csúcs"]);
        assert_eq!(cell(1, 0, 0), flat("Átlagsebesség"));
        assert_eq!(cell(1, 0, 2), flat("Csúcssebesség"));
        assert_eq!(cell(1, 1, 0), flat("Átlagteljesítmény"));
        assert_eq!(cell(1, 1, 1), flat("Normalizált teljesítmény"));
        assert_eq!(cell(1, 1, 2), flat("Csúcsteljesítmény"));
        assert_eq!(cell(1, 2, 0), flat("Átlagpulzus"));
        assert_eq!(cell(1, 2, 2), flat("Csúcspulzus"));
        assert_eq!(cell(1, 0, 1), "", "no normalised speed");
        // energy
        assert_eq!(cell(2, 0, 0), flat("Munka"));
        assert_eq!(cell(2, 0, 1), flat("Energia"));
        // units ride on the row label
        assert_eq!(groups[0].rows[0].unit, "km");
        assert_eq!(groups[1].rows[1].unit, "W");
    }

    #[test]
    fn stat_groups_of_the_empty_snapshot_have_dashes_not_nan() {
        let groups = stat_groups(&RideSnapshot::empty());
        for group in &groups {
            for row in &group.rows {
                for cell in &row.cells {
                    assert!(!cell.contains("NaN") && !cell.contains("inf"), "{cell}");
                }
            }
        }
        assert_eq!(groups[0].rows[2].cells[1], "—", "no ETA");
        assert_eq!(
            groups[0].rows[2].cells[2], "—",
            "no total time without an ETA"
        );
        assert_eq!(groups[1].rows[1].cells[0], "—", "no power");
        // a broken snapshot value never reaches a cell as NaN
        let mut broken = mid_climb_snapshot();
        broken.remaining_dist = f64::NAN;
        broken.eta_s = Some(f64::NAN);
        let groups = stat_groups(&broken);
        assert_eq!(groups[0].rows[0].cells[2], "—");
        assert_eq!(groups[0].rows[2].cells[2], "—");
    }

    // ── stylesheet ─────────────────────────────────────────────────────────────

    /// Custom properties the components define inline (not part of the token set).
    const LOCAL_VARS: [&str; 3] = ["--c", "--p", "--w"];

    /// Drops `/* ... */` comments so that prose cannot be mistaken for a declaration.
    fn without_comments(css: &str) -> String {
        let mut out = String::with_capacity(css.len());
        let mut rest = css;
        while let Some(at) = rest.find("/*") {
            out.push_str(&rest[..at]);
            let after = &rest[at + 2..];
            match after.find("*/") {
                Some(end) => rest = &after[end + 2..],
                None => return out,
            }
        }
        out.push_str(rest);
        out
    }

    fn css_tokens(css: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        for decl in css.split(';') {
            let Some((key, _)) = decl.split_once(':') else {
                continue;
            };
            let name = key.trim();
            if name.starts_with("--") && name.len() > 2 {
                out.insert(name.to_string());
            }
        }
        out
    }

    #[test]
    fn hud_css_only_uses_tokens_that_the_kit_defines() {
        let kit = include_str!("../../ride-ui-kit/assets/kit.css");
        let defined = css_tokens(&without_comments(kit));
        let css = without_comments(CSS);
        let mut rest = css.as_str();
        let mut seen = 0usize;
        while let Some(at) = rest.find("var(--") {
            let after = &rest[at + 6..];
            let end = after.find([')', ',']).expect("var() is closed");
            let name = format!("--{}", after[..end].trim());
            assert!(
                defined.contains(&name) || LOCAL_VARS.contains(&name.as_str()),
                "{name} is not defined by the design kit"
            );
            seen += 1;
            rest = &after[end..];
        }
        assert!(seen > 40, "found only {seen} token uses");
    }

    #[test]
    fn hud_css_has_no_hard_coded_colours() {
        let css = without_comments(CSS);
        assert!(
            !css.contains('#'),
            "hex colours are not allowed, use tokens"
        );
        assert!(
            !css.contains("rgb"),
            "rgb()/rgba() colours are not allowed, use tokens"
        );
    }

    #[test]
    fn hud_css_keeps_to_the_house_rules() {
        let css = without_comments(CSS);
        for banned in [
            "blur(",
            "backdrop-filter",
            "text-shadow",
            "drop-shadow",
            "text-transform",
            "radial-gradient",
            "conic-gradient",
            "linear-gradient",
        ] {
            assert!(!css.contains(banned), "{banned} is not allowed in the HUD");
        }
        // no soft shadows: a box-shadow must have 0 blur (or be `none`)
        let mut rest = css.as_str();
        while let Some(at) = rest.find("box-shadow:") {
            let after = &rest[at + "box-shadow:".len()..];
            let end = after.find(';').unwrap_or(after.len());
            let value = after[..end].trim();
            assert!(
                value == "none",
                "box-shadow `{value}` must be none in the HUD"
            );
            rest = &after[end..];
        }
        // radius 4 px at most
        let mut rest = css.as_str();
        while let Some(at) = rest.find("border-radius:") {
            let after = &rest[at + "border-radius:".len()..];
            let end = after.find(';').unwrap_or(after.len());
            for word in after[..end].split_whitespace() {
                if let Some(px) = word.strip_suffix("px") {
                    let v: f64 = px.parse().expect("a px radius");
                    assert!(v <= 4.0, "radius {word} is larger than 4px");
                }
                assert!(!word.contains('%'), "no round shapes in the HUD: {word}");
            }
            rest = &after[end..];
        }
    }

    #[test]
    fn every_class_of_this_crate_is_prefixed() {
        let css = without_comments(CSS);
        let mut rest = css.as_str();
        let mut seen = 0usize;
        while let Some(at) = rest.find('.') {
            let after = &rest[at + 1..];
            let end = after
                .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(after.len());
            let class = &after[..end];
            if !class.is_empty()
                && class
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
            {
                // `is-...` is the kit's state-modifier convention (`.is-active`), the rest is ours
                assert!(
                    class.starts_with("hud-") || class.starts_with("is-"),
                    ".{class} must be prefixed `hud-`"
                );
                seen += 1;
            }
            rest = &after[end..];
        }
        assert!(seen > 20, "found only {seen} class selectors");
    }

    // ── server-side render smoke tests ─────────────────────────────────────────

    #[derive(Clone, PartialEq, Props)]
    struct HarnessProps {
        snapshot: RideSnapshot,
        route: Option<Arc<Route>>,
        /// PR times of the climbs, indexed like `Route::climbs`.
        prs: Vec<Option<f64>>,
    }

    #[component]
    fn Harness(props: HarnessProps) -> Element {
        let snapshot0 = props.snapshot.clone();
        let snapshot = use_signal(move || snapshot0);
        let route0 = props.route.clone();
        let route = use_signal(move || route0);
        let prs0 = props.prs.clone();
        let prs = use_signal(move || prs0);
        let settings = use_signal(rider);
        let theme = use_signal(|| Theme::Dark);
        rsx! {
            MetricTiles { snapshot, rider: settings, theme }
            StatsPanel { snapshot }
            ClimbCard { route, snapshot, theme, prs }
            WorkoutCard { snapshot }
            RideControls {
                snapshot,
                on_start: move |_| {},
                on_pause: move |_| {},
                on_resume: move |_| {},
                on_finish: move |_| {},
                on_reset: move |_| {},
            }
        }
    }

    fn render(snapshot: RideSnapshot, route: Option<Arc<Route>>) -> String {
        render_with_prs(snapshot, route, Vec::new())
    }

    fn render_with_prs(
        snapshot: RideSnapshot,
        route: Option<Arc<Route>>,
        prs: Vec<Option<f64>>,
    ) -> String {
        let mut vdom = VirtualDom::new_with_props(
            Harness,
            HarnessProps {
                snapshot,
                route,
                prs,
            },
        );
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    #[test]
    fn renders_a_mid_climb_dashboard() {
        let html = render(mid_climb_snapshot(), Some(Arc::new(two_climb_route())));
        assert!(html.contains("hud-metrics"), "metric tiles rendered");
        assert!(html.contains("hud-stats"), "stats rendered");
        assert!(html.contains("hud-climb"), "climb card rendered");
        assert!(html.contains("Z4 Küszöb"), "power zone name");
        assert!(html.contains("H3 Aerob"), "heart-rate zone name");
        assert!(!html.contains("hud-ring"), "the ring gauge is gone");
        assert_eq!(html.matches("zonebar__seg").count(), 7, "seven steps");
        assert_eq!(
            html.matches("zonebar__seg is-current").count(),
            1,
            "one current zone"
        );
        assert!(html.contains("zonebar__ftp"), "FTP tick");
        assert!(
            html.contains(&format!("--ftp: {:.2}", ftp_tick_pct())),
            "FTP tick position"
        );
        assert!(html.contains("FTP 235 W"));
        assert!(html.contains("hud-tbl"), "stats table");
        assert!(html.contains("Összesen"), "table totals");
        assert!(html.contains("NP"), "table heads");
        assert!(html.contains("Emelkedő"), "in-climb title");
        assert!(html.contains("hud-flag"), "category flag");
        assert!(html.contains(">2<"), "category numeral");
        assert!(html.contains("Szünet"), "riding transport");
        assert!(html.contains("badge badge--ok"), "riding state badge");
        assert!(!html.contains("NaN"), "no NaN reaches the markup");
        assert!(!html.contains("text-transform"), "no inline caps");
    }

    /// A snapshot in the middle of the first climb with a live segment running against a 4:32 PR.
    fn segment_snapshot() -> RideSnapshot {
        let mut snapshot = mid_climb_snapshot();
        snapshot.segment = Some(ride_model::SegmentLive {
            climb: 0,
            elapsed_s: 272.0,
            pr_s: Some(280.0),
            delta_s: Some(-6.0),
        });
        snapshot
    }

    #[test]
    fn the_climb_card_shows_the_pr_and_the_live_difference() {
        // before the ride only the stored PR is known
        let mut ready = RideSnapshot::empty();
        ready.state = RideState::Ready;
        ready.next_climb = Some(0);
        ready.next_climb_in = Some(1_500.0);
        let html = render_with_prs(ready, Some(Arc::new(two_climb_route())), vec![Some(272.0)]);
        assert!(html.contains("hud-climb__pr"), "PR line missing: {html}");
        assert!(html.contains("hud-climb__pr-time"));
        assert!(html.contains("4:32"), "the PR time, formatted");
        assert!(
            !html.contains("hud-climb__pr-delta"),
            "no difference before the ride"
        );

        // inside the climb: the live difference, green when the rider is ahead
        let html = render_with_prs(
            segment_snapshot(),
            Some(Arc::new(two_climb_route())),
            vec![Some(280.0)],
        );
        assert!(html.contains("hud-climb__pr-delta"), "difference missing");
        assert!(html.contains("is-ahead"), "ahead is green");
        assert!(html.contains("-6 s"), "signed seconds");

        // behind: red, no modifier class
        let mut behind = segment_snapshot();
        behind.segment = Some(ride_model::SegmentLive {
            climb: 0,
            elapsed_s: 292.0,
            pr_s: Some(280.0),
            delta_s: Some(12.0),
        });
        let html = render_with_prs(behind, Some(Arc::new(two_climb_route())), vec![]);
        assert!(html.contains("+12 s"));
        assert!(!html.contains("is-ahead"), "behind is not green");

        // no PR at all: the card stays as it was
        let html = render(mid_climb_snapshot(), Some(Arc::new(two_climb_route())));
        assert!(!html.contains("hud-climb__pr"));
    }

    #[test]
    fn renders_the_empty_dashboard_without_a_climb_card() {
        let html = render(RideSnapshot::empty(), None);
        assert!(html.contains("hud-metrics"));
        assert!(
            html.contains("hud-climb--empty"),
            "no route: the climb card says so"
        );
        assert!(html.contains("Nincs betöltött útvonal"));
        assert!(
            !html.contains("hud-climb__hero"),
            "no route, no climb numbers"
        );
        assert!(html.contains("Kezdés"), "empty transport");
        assert!(html.contains("disabled"), "start is disabled");
        assert!(html.contains("Nincs teljesítmény"), "no power text");
        assert!(!html.contains("hud-note"), "no sensor hint without a route");
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn no_sensors_on_a_ready_ride_shows_the_hint_and_idle_zone_bar() {
        let mut snapshot = RideSnapshot::empty();
        snapshot.state = RideState::Ready;
        let html = render(snapshot, None);
        assert!(html.contains("Nincs szenzoradat"));
        assert!(
            !html.contains("zonebar__seg is-current"),
            "no current zone without power"
        );
        assert!(
            html.contains("--c: var(--bg-3)"),
            "idle segments are neutral"
        );
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn transport_follows_the_ride_state() {
        for (state, expected) in [
            (RideState::Ready, "Kezdés"),
            (RideState::Riding, "Szünet"),
            (RideState::Paused, "Folytatás"),
            (RideState::Finished, "Újrakezdés"),
        ] {
            let mut snapshot = RideSnapshot::empty();
            snapshot.state = state;
            let html = render(snapshot, None);
            assert!(html.contains(expected), "{state:?} should offer {expected}");
        }
    }

    fn workout_progress() -> ride_model::WorkoutProgress {
        ride_model::WorkoutProgress {
            workout_id: "w".into(),
            workout_name: "Küszöb 2 × 20".into(),
            step_index: 1,
            step_count: 5,
            step_name: "Küszöb 1".into(),
            step_kind: ride_model::StepKind::Interval,
            target_w: Some(245.0),
            ftp_pct: Some(98.0),
            cadence: Some(90),
            step_elapsed_s: 600.0,
            step_remaining_s: 600.0,
            step_progress: 0.5,
            total_elapsed_s: 1200.0,
            total_remaining_s: 2400.0,
            progress: 0.333,
            finished: false,
            next_step_name: Some("Pihenő".into()),
            next_target_w: Some(120.0),
            message: None,
        }
    }

    #[test]
    fn the_workout_card_shows_the_block_and_the_target() {
        let mut snapshot = RideSnapshot::empty();
        snapshot.state = RideState::Riding;
        snapshot.workout = Some(workout_progress());
        let html = render(snapshot, None);
        assert!(html.contains("hud-workout"));
        assert!(html.contains("Küszöb 2 × 20"));
        assert!(html.contains("Küszöb 1"));
        assert!(html.contains("245"));
        assert!(html.contains("98 % FTP"));
        assert!(html.contains("90 rpm"));
        assert!(html.contains("10:00"), "step countdown");
        assert!(html.contains("2/5"), "step number");
        assert!(html.contains("Következik: Pihenő"));
        assert!(html.contains("--p: 33.3"));
        assert!(!html.contains("NaN"));
        // a finished workout turns into the done state
        let mut done = workout_progress();
        done.finished = true;
        done.target_w = None;
        done.ftp_pct = None;
        let mut snapshot = RideSnapshot::empty();
        snapshot.workout = Some(done);
        let html = render(snapshot, None);
        assert!(html.contains("hud-workout--done"));
        assert!(html.contains("Kész"));
        assert!(html.contains("Minden blokk kész"));
    }
}
