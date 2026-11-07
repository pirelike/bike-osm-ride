//! Elevation profile and live history charts — the bottom panel and the rail charts of the
//! ride screen, drawn like a printed race sheet.
//!
//! * [`ElevationProfile`] draws the whole route (or a look-ahead window) as one SVG whose
//!   geometry lives in *route space* (metres, metres): a flat fill in the 15 grade buckets (hard
//!   colour steps, no gradient), a crisp 2 px outline, a padded elevation axis with hairlines, a
//!   km ruler along the bottom (tick per step, numeral per major step), climbs as flags with the
//!   category numeral plus a bar on the ruler, and the rider as a solid disc on a vertical
//!   hairline. The ridden part is covered by the surface colour. Only the group transform and
//!   the small HTML overlays move while riding, so the component stays smooth at 10 Hz.
//! * [`LiveChart`] draws the last `window_s` seconds of the 1 Hz recording as one flat area with
//!   a 2 px ink line, the dashed FTP rule (power) and the zone colours in an 8 px strip along
//!   the bottom edge, with a value read-out and a hover read-out.
//!
//! Empty inputs never panic: a route without points or an empty history renders a neutral
//! Hungarian placeholder. Colours come from the design-kit tokens (`var(--…)`) or from
//! [`ride_ui_kit::palette`]; every visible number is formatted by [`ride_ui_kit::format`].

use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{
    ClimbCategory, ProfileMode, ProfileView, RecordPoint, RideSnapshot, RideState, RiderSettings,
    Route, Theme,
};
use ride_ui_kit::color::ink_on;
use ride_ui_kit::format::{fmt_delta_s, fmt_grade, fmt_km, fmt_m};

use ride_i18n::{t, tf};
use ride_ui_kit::palette;
use ride_ui_kit::{Icon, IconName, Segmented};

pub mod geom;
pub mod series;
pub mod swipe;
pub mod workout;

pub use swipe::Pages;

pub use workout::WorkoutProfile;

/// Component stylesheet (tokens only; every class is prefixed `profile-`). The shell injects
/// it after the design kit.
pub const CSS: &str = include_str!("../assets/profile.css");

/// Placeholder for a value that is not available ("—", U+2014).
pub const DASH: &str = "—";

/// Above this pointer fraction the hover tooltip is anchored on its right edge.
const TIP_FLIP_FRAC: f64 = 0.68;

/// A climb band at least this wide (percent of the plot) gets a text label next to its flag.
const FLAG_LABEL_MIN_PCT: f64 = 14.0;

// ───────────────────────────── small helpers ─────────────────────────────

/// A percentage that can never be `NaN%` or leave the plot.
fn pct(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

/// The percentage formatted for a CSS `left` / `top` / `width`.
fn pct_text(value: f64) -> String {
    format!("{:.3}%", pct(value))
}

/// Measures the element behind `mounted` and stores its width (CSS pixels) in `width`.
/// Called on mount and whenever the pointer enters the plot, so hover and seek stay correct
/// after the window was resized. A no-op when the renderer has no layout (SSR previews) or
/// the measurement is unusable.
fn measure_width(mounted: Signal<Option<Rc<MountedData>>>, mut width: Signal<f64>) {
    let handle = match &*mounted.peek() {
        Some(handle) => handle.clone(),
        None => return,
    };
    spawn(async move {
        if let Ok(rect) = handle.get_client_rect().await {
            let w = rect.size.width;
            if w.is_finite() && w > 1.0 {
                width.set(w);
            }
        }
    });
}

/// Pointer fraction inside the plot (`0..=1`) from an element-local x coordinate and the
/// measured width; `None` while the width is still unknown.
fn pointer_frac(x: f64, width: f64) -> Option<f64> {
    if width.is_finite() && width > 1.0 && x.is_finite() {
        Some((x / width).clamp(0.0, 1.0))
    } else {
        None
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

/// One climb of the profile: a flag on a pole at the climb start (numeral = category, colour =
/// category colour) and a bar of the same colour on the km ruler, positioned in percent of the
/// plot width.
#[derive(Clone, Debug, PartialEq)]
struct Flag {
    /// Left edge of the climb inside the window, percent.
    left: f64,
    /// Width of the climb inside the window, percent.
    width: f64,
    /// Terrain elevation under the pole, metres.
    ele: f64,
    numeral: &'static str,
    /// Category colour (hex of the theme).
    color: String,
    /// Black or white, whichever reads better on `color`.
    ink: &'static str,
    /// Category, length and grade for climbs that are wide enough to carry text.
    label: Option<String>,
}

/// Flags of every climb that intersects the window. The pole stands at the climb start (or at
/// the window edge when the climb began before it); wide climbs also get a text label.
fn climb_flags(route: &Route, scale: geom::XScale, theme: Theme) -> Vec<Flag> {
    let mut out = Vec::new();
    for climb in &route.climbs {
        let start = climb.start_dist.max(scale.start());
        let end = climb.end_dist.min(scale.end());
        if !(start.is_finite() && end.is_finite()) || end <= start {
            continue;
        }
        let left = scale.pct(start);
        let width = (scale.pct(end) - left).max(0.3);
        let color = palette::climb_category_color(climb.category, theme);
        let label = (width >= FLAG_LABEL_MIN_PCT).then(|| {
            format!(
                "{} · {} km · {} %",
                palette::climb_category_label(climb.category),
                fmt_km(climb.length),
                fmt_grade(climb.avg_grade)
            )
        });
        out.push(Flag {
            left,
            width,
            ele: route.locate(start).ele,
            numeral: category_numeral(climb.category),
            ink: ink_on(&color),
            color,
            label,
        });
    }
    out
}

/// One named segment of the file inside the window, in percent of the plot width.
#[derive(Clone, Debug, PartialEq)]
struct SegmentMark {
    left: f64,
    width: f64,
    name: String,
}

/// The file's named segments (Strava segments added to the GPX) that intersect the window.
fn segment_marks(route: &Route, scale: geom::XScale) -> Vec<SegmentMark> {
    route
        .segments
        .iter()
        .filter_map(|seg| {
            let start = seg.start_dist.max(scale.start());
            let end = seg.end_dist.min(scale.end());
            if !(start.is_finite() && end.is_finite()) || end <= start {
                return None;
            }
            let left = scale.pct(start);
            Some(SegmentMark {
                left,
                width: (scale.pct(end) - left).max(0.3),
                name: seg.name.clone(),
            })
        })
        .collect()
}

// ───────────────────────────── ElevationProfile ─────────────────────────────

/// Elevation / Workout switch of the profile panel header (only offered while a workout is
/// selected).
#[component]
fn ViewSwitch(view: ProfileView, on_view: EventHandler<ProfileView>) -> Element {
    rsx! {
        Segmented {
            options: vec![
                ("elevation".to_string(), t("ui_profile.magassag").to_string()),
                ("workout".to_string(), t("ui_profile.edzes").to_string()),
            ],
            value: if view == ProfileView::Workout { "workout".to_string() } else { "elevation".to_string() },
            on_change: move |value: String| {
                on_view.call(if value == "workout" { ProfileView::Workout } else { ProfileView::Elevation });
            },
            aria_label: t("ui_profile.a_panel_tartalma").to_string(),
            small: true,
        }
    }
}

/// One label / value pair of the profile header ("Táv 9.82 / 24.10 km").
///
/// `tone` colours a signed difference: `""` (neutral), `"ahead"` (green, the rider is in front
/// of the ghost) or `"behind"` (red).
#[component]
fn Pair(
    label: String,
    value: String,
    #[props(default = String::new())] unit: String,
    #[props(default = String::new())] tone: String,
) -> Element {
    let class = if tone.is_empty() {
        "profile-pair".to_string()
    } else {
        format!("profile-pair profile-pair--{tone}")
    };
    rsx! {
        span { class: "{class}",
            span { class: "profile-pair-label", "{label}" }
            span { class: "profile-pair-value", "{value}" }
            if !unit.is_empty() {
                span { class: "profile-pair-unit", "{unit}" }
            }
        }
    }
}

/// Read-out tone of a ghost difference (positive = behind).
fn delta_tone(delta: f64) -> &'static str {
    if !delta.is_finite() || delta.abs() < 0.5 {
        ""
    } else if delta < 0.0 {
        "ahead"
    } else {
        "behind"
    }
}

/// Flat grade-coloured elevation profile with the rider marker, climb flags (category
/// numeral; length and grade on wide climbs), a km ruler, hover read-out (distance / elevation
/// / grade), click to seek (emits the distance in metres; the app ignores it while riding) and
/// a Full / Look-ahead toggle (`on_mode`). In look-ahead mode the window starts slightly behind
/// the rider and spans `lookahead_m`. With `on_view` the header also offers the switch to the
/// workout view.
#[component]
pub fn ElevationProfile(
    route: ReadSignal<Option<Arc<Route>>>,
    snapshot: ReadSignal<RideSnapshot>,
    mode: ReadSignal<ProfileMode>,
    lookahead_m: ReadSignal<f64>,
    theme: ReadSignal<Theme>,
    on_seek: EventHandler<f64>,
    on_mode: EventHandler<ProfileMode>,
    #[props(default)] on_view: Option<EventHandler<ProfileView>>,
    /// At most this many elevation hairlines (a short profile, like the one under the phone's
    /// map, reads better with three than with seven).
    #[props(default = 7)]
    grid_steps: usize,
    /// Phone: the views are pages turned by swiping, instead of the header buttons.
    #[props(default)]
    pages: Option<Pages>,
) -> Element {
    let mut hover = use_signal(|| None::<f64>);
    let width = use_signal(|| 0.0_f64);
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);

    // phone: a sideways swipe over the plot turns the page (see `swipe.rs`)
    let mut swipe_from = use_signal(|| None::<(f64, f64)>);
    let mut swiped = use_signal(|| false);
    // The y axis drawn last time, with what it was drawn for (route id, mode): read and written
    // during render, so it is plain memory, not a signal.
    let last_axis = use_hook(|| {
        Rc::new(std::cell::RefCell::new(
            None::<(String, ProfileMode, (f64, f64))>,
        ))
    });

    // The static geometry only depends on the route, the theme and the 250 m bucket the
    // rider is in — never on the 10 Hz snapshot itself. `bucket` is cheap to recompute and
    // only invalidates the geometry memo when its value actually changes.
    let bucket = use_memo(move || match *mode.read() {
        ProfileMode::Full => 0u64,
        ProfileMode::Lookahead => geom::bucket_index(snapshot.read().dist),
    });
    let geometry = use_memo(move || -> Option<geom::ProfileGeometry> {
        let route = route.read().clone()?;
        if route.points.len() < 2 {
            return None;
        }
        let theme = *theme.read();
        let mode = *mode.read();
        let look = *lookahead_m.read();
        let bucket = bucket();
        // the window that will be shown inside this bucket, taken at the bucket start so the
        // memo never reads the live snapshot
        let bucket_start = bucket as f64 * geom::GEOMETRY_BUCKET_M;
        let live = geom::window_bounds(route.total_dist, mode, bucket_start, look);
        let (from, to) = geom::geometry_bounds(route.total_dist, mode, look, bucket, live);
        Some(geom::build_geometry(&route, from, to, theme))
    });

    let mode_v = *mode.read();
    let look_v = *lookahead_m.read();
    let theme_v = *theme.read();
    let route_v = route.read().clone();
    let snap = snapshot.read().clone();

    let Some(route_ref) = route_v else {
        return profile_placeholder(
            t("ui_profile.nincs_betoltott_utvonal"),
            t("ui_profile.nyiss_meg_egy_gpx_fajlt_vagy_illessz_be_egy_bikerout"),
        );
    };
    if route_ref.points.len() < 2 {
        return profile_placeholder(
            t("ui_profile.nincs_mit_kirajzolni"),
            t("ui_profile.az_utvonal_nem_tartalmaz_eleg_pontot_a_magassagi_pro"),
        );
    }

    let total = if route_ref.total_dist.is_finite() {
        route_ref.total_dist.max(0.0)
    } else {
        0.0
    };
    let dist = if snap.dist.is_finite() {
        snap.dist.clamp(0.0, total)
    } else {
        0.0
    };
    let (d0, d1) = geom::window_bounds(total, mode_v, dist, look_v);
    let scale = geom::XScale::new(d0, d1);
    let (y_lo, y_hi) = match mode_v {
        ProfileMode::Full => geom::elevation_range(&route_ref, d0, d1)
            .map(|(lo, hi)| geom::y_range(lo, hi))
            .unwrap_or_else(|| geom::y_range(0.0, 100.0)),
        ProfileMode::Lookahead => {
            // the terrain of the window and of one more window beyond it, so a climb coming up
            // widens the axis before it reaches the edge, and the axis only changes when it must
            let ahead = (d1 + (d1 - d0)).min(total);
            let (lo, hi) = geom::elevation_range(&route_ref, d0, ahead).unwrap_or((0.0, 100.0));
            let mut memory = last_axis.borrow_mut();
            let prev = memory
                .as_ref()
                .filter(|(id, m, _)| *id == route_ref.id && *m == mode_v)
                .map(|(_, _, axis)| *axis);
            let axis = geom::stable_y_range(prev, lo, hi);
            *memory = Some((route_ref.id.clone(), mode_v, axis));
            axis
        }
    };
    let transform = geom::view_transform(d0, d1, y_lo, y_hi);
    let markup = geometry
        .read()
        .as_ref()
        .map(|geometry| geometry.markup.clone())
        .unwrap_or_default();

    let y_pct = |ele: f64| (1.0 - geom::elevation_frac(ele, y_lo, y_hi)) * 100.0;

    // rider marker
    let show_rider = snap.state != RideState::Empty;
    let rider_x = scale.pct(dist);
    let rider_y = y_pct(route_ref.locate(dist).ele);
    // The disc never leaves the plot: at the start/end of the route, and on the flattest or
    // steepest part of the window, half of it would hang over the edge without the clamp.
    let rider_left = format!("clamp(8px, {}, calc(100% - 8px))", pct_text(rider_x));
    let rider_top = format!("clamp(8px, {}, calc(100% - 8px))", pct_text(rider_y));

    // ghost marker: only while it is inside the shown window (a look-ahead window would
    // otherwise pin a ghost that is kilometres behind to the left edge)
    let ghost = snap.ghost.as_ref().filter(|ghost| {
        show_rider
            && ghost.dist.is_finite()
            && ghost.dist >= d0
            && ghost.dist <= d1
            && ghost.dist <= total
    });
    let ghost_delta = snap.ghost.as_ref().and_then(|ghost| ghost.delta_s);

    // km ruler: the first numeral carries the unit
    let mut ticks: Vec<(geom::RulerTick, f64)> = geom::ruler_ticks(d0, d1)
        .into_iter()
        .map(|tick| {
            let left = scale.pct(tick.dist);
            (tick, left)
        })
        .collect();
    if let Some((tick, _)) = ticks.iter_mut().find(|(tick, _)| tick.label.is_some()) {
        tick.label = tick.label.take().map(|label| format!("{label} km"));
    }
    // a hairline right under the header would put its label on the header's text
    let grid: Vec<(f64, String)> = geom::grid_values_n(y_lo, y_hi, grid_steps.max(1))
        .into_iter()
        .map(|value| (y_pct(value), format!("{} m", fmt_m(value))))
        .filter(|(top, _)| *top >= 8.0)
        .collect();
    let segment_bands = segment_marks(&route_ref, scale);
    // the segment the rider is on, for the read-out
    let live_segment = route_ref
        .segments
        .iter()
        .find(|seg| show_rider && dist >= seg.start_dist && dist < seg.end_dist)
        .map(|seg| (seg.name.clone(), seg.end_dist - dist));
    let flags: Vec<(Flag, f64)> = climb_flags(&route_ref, scale, theme_v)
        .into_iter()
        .map(|flag| {
            let top = y_pct(flag.ele);
            (flag, top)
        })
        .collect();

    // hover read-out
    let hover_frac = *hover.read();
    let hover_info = hover_frac.map(|frac| {
        let d = scale.dist_at_frac(frac);
        let pos = route_ref.locate(d);
        (frac, d, pos.ele, pos.grade)
    });
    let hover_segment = hover_info.and_then(|(_, d, _, _)| {
        route_ref
            .segments
            .iter()
            .find(|seg| d >= seg.start_dist && d <= seg.end_dist)
            .map(|seg| seg.name.clone())
    });

    let summary = tf(
        "ui_profile.magassagi_profil_km_m_szintemelkedes_emelkedo",
        &[
            ("km", &fmt_km(total)),
            ("gain", &fmt_m(route_ref.ascent)),
            ("climbs", &route_ref.climbs.len().to_string()),
        ],
    );
    let no_elevation = !route_ref.has_elevation;

    let segments = geometry
        .read()
        .as_ref()
        .map(|geometry| geometry.segments)
        .unwrap_or(0);

    rsx! {
        div { class: if pages.as_ref().is_some_and(|p| p.hint) { "profile profile--nudge" } else { "profile" },
            div { class: if pages.is_some() { "profile-head profile-head--pages" } else { "profile-head" },
                div { class: "profile-readout",
                    if let Some((_, d, ele, grade)) = hover_info {
                        Pair { label: t("ui_profile.tav").to_string(), value: fmt_km(d), unit: "km".to_string() }
                        Pair { label: t("ui_profile.magassag").to_string(), value: fmt_m(ele), unit: "m".to_string() }
                        Pair { label: t("ui_profile.meredekseg").to_string(), value: fmt_grade(grade), unit: "%".to_string() }
                    } else {
                        Pair {
                            label: t("ui_profile.tav").to_string(),
                            value: format!("{} / {}", fmt_km(dist), fmt_km(total)),
                            unit: "km".to_string(),
                        }
                        Pair {
                            label: t("ui_profile.szint").to_string(),
                            value: if show_rider {
                                format!("+{} / {}", fmt_m(snap.ascent_done.max(0.0)), fmt_m(route_ref.ascent))
                            } else {
                                format!("+{}", fmt_m(route_ref.ascent))
                            },
                            unit: "m".to_string(),
                        }
                        if mode_v == ProfileMode::Lookahead {
                            Pair { label: t("ui_profile.ablak").to_string(), value: fmt_km(d1 - d0), unit: "km".to_string() }
                        }
                        if let Some((name, left)) = live_segment.clone() {
                            Pair {
                                label: format!("Szegmens: {name}"),
                                value: fmt_km(left.max(0.0)),
                                unit: t("ui_profile.km_hatra").to_string(),
                            }
                        }
                        if let Some(delta) = ghost_delta {
                            Pair {
                                label: t("ui_profile.szellem").to_string(),
                                value: fmt_delta_s(delta),
                                unit: "s".to_string(),
                                tone: delta_tone(delta).to_string(),
                            }
                        }
                        if no_elevation {
                            span { class: "tag profile-readout-note", {t("ui_profile.nincs_magassagi_adat")} }
                        }
                    }
                }
                if let Some(pages) = pages.clone() {
                    swipe::PageDots { pages }
                } else {
                    div { class: "profile-switches",
                        Segmented {
                            options: vec![
                                ("full".to_string(), t("ui_profile.teljes").to_string()),
                                ("lookahead".to_string(), t("ui_profile.eloretekintes").to_string()),
                            ],
                            value: if mode_v == ProfileMode::Full { "full".to_string() } else { "lookahead".to_string() },
                            on_change: move |value: String| {
                                on_mode.call(if value == "full" { ProfileMode::Full } else { ProfileMode::Lookahead });
                            },
                            aria_label: t("ui_profile.profil_nezet").to_string(),
                            small: true,
                        }
                        if let Some(on_view) = on_view {
                            ViewSwitch { view: ProfileView::Elevation, on_view }
                        }
                    }
                }
            }
            div { class: "profile-plot", role: "img", "aria-label": "{summary}",
                if segments == 0 {
                    div { class: "profile-plot-note", {t("ui_profile.nincs_kirajzolhato_szakasz")} }
                }
                svg {
                    class: "profile-svg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    g {
                        transform: "{transform}",
                        dangerous_inner_html: markup,
                    }
                }
                for (top , _text) in grid.iter() {
                    div { key: "grid-{top:.1}", class: "profile-grid-h", style: "top: {pct_text(*top)}" }
                }
                if show_rider {
                    div { class: "profile-done", style: "width: {pct_text(rider_x)}" }
                }
                for (top , text) in grid.iter() {
                    div { key: "y-{top:.1}", class: "profile-ytick", style: "top: {pct_text(*top)}", "{text}" }
                }
                for seg in segment_bands.iter() {
                    div {
                        key: "seg-{seg.left:.2}",
                        class: "profile-seg",
                        style: "left: {pct_text(seg.left)}; width: {pct_text(seg.width)}",
                        title: "Szegmens: {seg.name}",
                        span { class: "profile-seg-label", "{seg.name}" }
                    }
                }
                for (flag , top) in flags.iter() {
                    div {
                        key: "flag-{flag.left:.2}",
                        class: "profile-flag",
                        style: "left: {pct_text(flag.left)}; bottom: {pct_text(100.0 - *top)}; height: clamp(18px, {pct_text(*top)}, 28px); --c: {flag.color}; --ink: {flag.ink}",
                        div { class: "profile-flag-top",
                            span { class: "profile-flag-box", "{flag.numeral}" }
                            if let Some(label) = &flag.label {
                                span { class: "profile-flag-label", "{label}" }
                            }
                        }
                    }
                }
                if show_rider {
                    div { class: "profile-guide", style: "left: {rider_left}" }
                    svg {
                        class: "profile-dot",
                        style: "left: {rider_left}; top: {rider_top}",
                        width: "16",
                        height: "16",
                        view_box: "0 0 16 16",
                        "aria-hidden": "true",
                        circle {
                            cx: "8",
                            cy: "8",
                            r: "6.5",
                            fill: "var(--accent)",
                            stroke: "var(--marker-ring)",
                            stroke_width: "3",
                        }
                    }
                }
                if let Some(ghost) = ghost {
                    div {
                        class: "profile-guide profile-guide--ghost",
                        style: "left: {pct_text(scale.pct(ghost.dist))}",
                    }
                    div {
                        class: "profile-ghost",
                        style: "left: clamp(5px, {pct_text(scale.pct(ghost.dist))}, calc(100% - 5px)); top: clamp(5px, {pct_text(y_pct(route_ref.locate(ghost.dist).ele))}, calc(100% - 5px))",
                        title: t("ui_profile.szellem").to_string(),
                        role: "img",
                        "aria-label": t("ui_profile.szellem"),
                    }
                }
                if let Some((frac, d, ele, grade)) = hover_info {
                    div {
                        class: "profile-guide profile-guide--hover",
                        style: "left: {pct_text(frac * 100.0)}",
                    }
                    div {
                        class: if frac > TIP_FLIP_FRAC { "tip profile-tip profile-tip--flip" } else { "tip profile-tip" },
                        style: "left: {pct_text(frac * 100.0)}",
                        div { class: "profile-tip-row",
                            span { class: "profile-tip-label", {t("ui_profile.tav")} }
                            span { class: "profile-tip-value", "{fmt_km(d)} km" }
                        }
                        div { class: "profile-tip-row",
                            span { class: "profile-tip-label", {t("ui_profile.magassag")} }
                            span { class: "profile-tip-value", "{fmt_m(ele)} m" }
                        }
                        div { class: "profile-tip-row",
                            span { class: "profile-tip-label", {t("ui_profile.meredekseg")} }
                            span { class: "profile-tip-value", "{fmt_grade(grade)} %" }
                        }
                        if let Some(name) = &hover_segment {
                            div { class: "profile-tip-row profile-tip-row--seg",
                                span { class: "profile-tip-label", {t("ui_profile.szegmens")} }
                                span { class: "profile-tip-seg", "{name}" }
                            }
                        }
                    }
                }
                div {
                    class: if pages.is_some() { "profile-hit profile-hit--swipe" } else { "profile-hit" },
                    onpointerdown: move |event: PointerEvent| {
                        let p = event.client_coordinates();
                        swipe_from.set(Some((p.x, p.y)));
                        swiped.set(false);
                    },
                    onpointercancel: move |_| swipe_from.set(None),
                    onpointerup: {
                        let pages = pages.clone();
                        move |event: PointerEvent| {
                            let Some((x0, y0)) = swipe_from.take() else {
                                return;
                            };
                            let p = event.client_coordinates();
                            let turned = pages.as_ref().and_then(|pages| {
                                let to = pages.step(swipe::swipe_step(p.x - x0, p.y - y0)?)?;
                                pages.on_page.call(to);
                                Some(())
                            });
                            if turned.is_some() {
                                swiped.set(true);
                                hover.set(None);
                            }
                        }
                    },
                    onmounted: move |event: MountedEvent| {
                        mounted.set(Some(event.data()));
                        measure_width(mounted, width);
                    },
                    onmouseenter: move |_| measure_width(mounted, width),
                    onmousemove: move |event: MouseEvent| {
                        let w = *width.peek();
                        if let Some(frac) = pointer_frac(event.element_coordinates().x, w) {
                            hover.set(Some(frac));
                        }
                    },
                    onmouseleave: move |_| hover.set(None),
                    onclick: move |event: MouseEvent| {
                        // the end of a swipe is not a tap on the profile
                        if swiped.take() {
                            return;
                        }
                        let w = *width.peek();
                        if let Some(frac) = pointer_frac(event.element_coordinates().x, w) {
                            on_seek.call(scale.dist_at_frac(frac));
                        }
                    },
                }
            }
            div { class: "profile-ruler", "aria-hidden": "true",
                for (flag , _top) in flags.iter() {
                    div {
                        key: "bar-{flag.left:.2}",
                        class: "profile-climbbar",
                        style: "left: {pct_text(flag.left)}; width: {pct_text(flag.width)}; --c: {flag.color}",
                    }
                }
                for (tick , left) in ticks.iter() {
                    div {
                        key: "tick-{tick.dist:.0}",
                        class: if tick.major { "profile-tick profile-tick--major" } else { "profile-tick" },
                        style: "left: {pct_text(*left)}",
                        if let Some(label) = &tick.label {
                            span {
                                class: if *left < 1.0 {
                                    "profile-tick-label profile-tick-label--start"
                                } else if *left > 97.0 {
                                    "profile-tick-label profile-tick-label--end"
                                } else {
                                    "profile-tick-label"
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Neutral placeholder of an unavailable profile.
fn profile_placeholder(title: &'static str, hint: &'static str) -> Element {
    rsx! {
        div { class: "profile profile--empty", "aria-label": t("ui_profile.magassagi_profil"),
            div { class: "profile-empty",
                Icon { name: IconName::Mountain, size: 24 }
                span { class: "profile-empty-title", "{title}" }
                span { class: "profile-empty-hint", "{hint}" }
            }
        }
    }
}

// ───────────────────────────── ChartMetric ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartMetric {
    Power,
    HeartRate,
    Cadence,
    Speed,
}

// ───────────────────────────── LiveChart ─────────────────────────────

/// Rolling history chart of the last `window_s` seconds: one flat area with a 2 px ink line, the
/// dashed FTP rule for power and the zone colours (power vs FTP, heart rate vs max HR) in a strip
/// along the bottom edge, with hover read-out. `history` is the engine's 1 Hz recording.
#[component]
pub fn LiveChart(
    history: ReadSignal<Arc<Vec<RecordPoint>>>,
    metric: ChartMetric,
    #[props(default = 300.0)] window_s: f64,
    rider: ReadSignal<RiderSettings>,
    theme: ReadSignal<Theme>,
) -> Element {
    let mut hover = use_signal(|| None::<f64>);
    let width = use_signal(|| 0.0_f64);
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let window = if window_s.is_finite() {
        window_s.clamp(15.0, 3_600.0)
    } else {
        300.0
    };

    // The markup is rebuilt once per recorded second (the history signal changes then) and
    // never on the 10 Hz ride ticks.
    let geometry = use_memo(move || {
        let history = history.read().clone();
        let rider = rider.read().clone();
        let theme = *theme.read();
        let series = series::series_for(&history, metric, window);
        series::chart_geometry(&series, metric, &rider, theme, window)
    });

    let rider_v = rider.read().clone();
    let geo = geometry.read();
    if geo.series.t.is_empty() {
        return rsx! {
            div { class: "profile-chart profile-chart--empty", "aria-label": "{metric.title()}",
                div { class: "profile-chart-head",
                    span { class: "profile-chart-title", "{metric.title()}" }
                    span { class: "profile-chart-now",
                        span { class: "profile-chart-value is-dim", "{DASH}" }
                        span { class: "profile-chart-unit", "{metric.unit()}" }
                    }
                }
                div { class: "profile-chart-empty", {t("ui_profile.meg_nincs_adat_a_grafikonon")} }
            }
        };
    }

    let transform = geom::view_transform(geo.t0, geo.t1, geo.y0, geo.y1);
    let markup = geo.markup.clone();
    let strip = geo.strip.clone();
    let strip_box = format!("{:.2} 0 {:.2} 1", geo.t0, (geo.t1 - geo.t0).max(1.0));
    let last = geo.series.last_value();
    let badge = series::zone_badge(metric, last, &rider_v);
    let y_pct = |value: f64| (1.0 - geom::elevation_frac(value, geo.y0, geo.y1)) * 100.0;
    let grid: Vec<(f64, String)> = geom::grid_values_n(geo.y0, geo.y1, 3)
        .into_iter()
        .map(|value| (y_pct(value), metric.axis_text(value)))
        .collect();
    // the FTP label sits on the side of the rule that the live line is not on
    let guide = geo.guide.map(|g| {
        let below = last.is_some_and(|v| v >= g);
        (y_pct(g), format!("FTP {}", metric.axis_text(g)), below)
    });
    // a scale label that would sit on the FTP rule is left out (the rule has its own label)
    let grid: Vec<(f64, String)> = match &guide {
        Some((top, _, _)) => grid
            .into_iter()
            .filter(|(t, _)| (t - top).abs() > 14.0)
            .collect(),
        None => grid,
    };
    let labels = series::window_labels(window);

    let hover_frac = *hover.read();
    let hover_info = hover_frac.map(|frac| {
        let t = geo.t0 + frac * (geo.t1 - geo.t0);
        let offset = (geo.t1 - t).max(0.0);
        let text = series::hover_text(metric, offset, geo.series.value_at(t));
        (frac, text)
    });

    let now_text = if geo.is_empty() {
        t("ui_profile.meg_nincs_eleg_adat").to_string()
    } else {
        tf(
            "ui_profile.jelenleg",
            &[("value", &metric.value_text(last)), ("unit", metric.unit())],
        )
    };
    let summary = tf(
        "ui_profile.grafikon_az_utolso",
        &[
            ("metric", metric.title()),
            ("window", &series::window_caption(window)),
            ("now", &now_text),
        ],
    );

    rsx! {
        div { class: "profile-chart", "aria-label": "{summary}",
            div { class: "profile-chart-head",
                span { class: "profile-chart-title", "{metric.title()}" }
                span { class: "profile-chart-caption",
                    if let Some((_, text)) = &hover_info {
                        "{text}"
                    } else {
                        "{series::window_caption(window)}"
                    }
                }
                if let Some((zone, name, var)) = badge {
                    span { class: "profile-chart-zone",
                        span { class: "profile-chart-sw", style: "--c: {var}" }
                        "{metric.zone_prefix()}{zone} {name}"
                    }
                }
                span { class: "profile-chart-now",
                    span { class: if last.is_none() { "profile-chart-value is-dim" } else { "profile-chart-value" },
                        "{metric.value_text(last)}"
                    }
                    span { class: "profile-chart-unit", "{metric.unit()}" }
                }
            }
            div { class: "profile-chart-plot",
                svg {
                    class: "profile-chart-svg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    g {
                        transform: "{transform}",
                        dangerous_inner_html: markup,
                    }
                }
                for (top , _text) in grid.iter() {
                    div {
                        key: "grid-{top:.1}",
                        class: "profile-chart-grid",
                        style: "top: {pct_text(*top)}",
                    }
                }
                for (top , text) in grid.iter() {
                    span {
                        key: "y-{top:.1}",
                        class: "profile-chart-ytick",
                        style: "top: {pct_text(*top)}",
                        "{text}"
                    }
                }
                if let Some((top, text, below)) = &guide {
                    span {
                        class: if *below { "profile-chart-ftp profile-chart-ftp--below" } else { "profile-chart-ftp" },
                        style: "top: {pct_text(*top)}",
                        "{text}"
                    }
                }
                if geo.is_empty() {
                    div { class: "profile-chart-note", {t("ui_profile.gyulnek_az_adatok")} }
                }
                if let Some((frac, _)) = &hover_info {
                    div {
                        class: "profile-chart-guide",
                        style: "left: {pct_text(*frac * 100.0)}",
                    }
                }
                div {
                    class: "profile-chart-hit",
                    onmounted: move |event: MountedEvent| {
                        mounted.set(Some(event.data()));
                        measure_width(mounted, width);
                    },
                    onmouseenter: move |_| measure_width(mounted, width),
                    onmousemove: move |event: MouseEvent| {
                        let w = *width.peek();
                        if let Some(frac) = pointer_frac(event.element_coordinates().x, w) {
                            hover.set(Some(frac));
                        }
                    },
                    onmouseleave: move |_| hover.set(None),
                }
            }
            if !strip.is_empty() {
                svg {
                    class: "profile-chart-strip",
                    view_box: "{strip_box}",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    dangerous_inner_html: strip,
                }
            }
            div { class: "profile-chart-times", "aria-hidden": "true",
                for (i , label) in labels.iter().enumerate() {
                    span { key: "{i}", "{label}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::ClimbCategory;
    use std::collections::HashSet;

    // ── fixtures ────────────────────────────────────────────────────────────────

    fn rider() -> RiderSettings {
        RiderSettings {
            ftp_w: 235.0,
            max_hr: 190.0,
            ..RiderSettings::default()
        }
    }

    /// A ~24 km hilly route with two climbs and a plausible Budapest-area shape.
    fn fake_route() -> Route {
        let mut route = straight_route(
            &[
                (4_000.0, 1.5),
                (7_000.0, 6.8),
                (3_000.0, -3.2),
                (4_000.0, 4.1),
                (3_000.0, -2.4),
                (3_000.0, 0.8),
            ],
            118.0,
        );
        route.name = "Budai-hegység kör".into();
        route.climbs = vec![
            ride_model::Climb {
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
            ride_model::Climb {
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

    fn history() -> Vec<RecordPoint> {
        (0..300)
            .map(|i| {
                let t = i as f64;
                let wave = (t / 11.0).sin();
                RecordPoint {
                    t,
                    unix_s: 1_700_000_000.0 + t,
                    dist: t * 7.0,
                    lat: 47.5,
                    lon: 19.0 + t / 75_000.0,
                    ele: 120.0 + t * 0.3,
                    speed_ms: 7.0 + wave,
                    power_w: Some(205.0 + wave * 60.0),
                    cadence_rpm: Some(84.0 + wave * 4.0),
                    hr_bpm: Some(142.0 + (t / 30.0).sin() * 10.0),
                    grade: 2.0,
                }
            })
            .collect()
    }

    fn snapshot_at(dist: f64) -> RideSnapshot {
        let mut snapshot = RideSnapshot::empty();
        snapshot.state = RideState::Riding;
        snapshot.dist = dist;
        snapshot.pos.dist = dist;
        snapshot
    }

    // ── bands ──────────────────────────────────────────────────────────────────

    #[test]
    fn climb_flags_are_clipped_to_the_window_and_carry_the_category_numeral() {
        let route = fake_route();
        // whole route
        let flags = climb_flags(
            &route,
            geom::XScale::new(0.0, route.total_dist),
            Theme::Dark,
        );
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0].numeral, "2");
        assert_eq!(flags[1].numeral, "4");
        assert!(flags[0].left > 0.0 && flags[0].width > 0.0);
        assert!(flags[0].left < flags[1].left, "in route order");
        assert!(flags[0]
            .label
            .as_deref()
            .unwrap_or("")
            .starts_with("2. kat."));
        assert!(flags[1]
            .label
            .as_deref()
            .unwrap_or("")
            .starts_with("4. kat."));
        // the colour is the category colour of the theme and the numeral ink reads on it
        assert_eq!(
            flags[0].color,
            palette::climb_category_color(Some(ClimbCategory::Cat2), Theme::Dark)
        );
        assert_eq!(flags[0].ink, ink_on(&flags[0].color));
        let light = climb_flags(
            &route,
            geom::XScale::new(0.0, route.total_dist),
            Theme::Light,
        );
        assert_ne!(
            light[0].color, flags[0].color,
            "the light theme has its own ramp"
        );
        // the pole stands on the terrain at the climb start
        assert!((flags[0].ele - route.locate(4_300.0).ele).abs() < 1e-9);
        // a window that only shows the second climb
        let flags = climb_flags(&route, geom::XScale::new(14_000.0, 20_000.0), Theme::Dark);
        assert_eq!(flags.len(), 1);
        assert!(flags[0].left >= 0.0 && flags[0].left + flags[0].width <= 100.0 + 1e-9);
        // a window that starts inside a climb pins the flag to the left edge
        let flags = climb_flags(&route, geom::XScale::new(6_000.0, 9_000.0), Theme::Dark);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].left, 0.0);
        assert!((flags[0].ele - route.locate(6_000.0).ele).abs() < 1e-9);
        // a window between the climbs shows none
        assert!(climb_flags(&route, geom::XScale::new(12_000.0, 14_000.0), Theme::Dark).is_empty());
        // a narrow climb keeps only its flag: no text label
        let flags = climb_flags(&route, geom::XScale::new(0.0, 120_000.0), Theme::Dark);
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0].label, None);
        assert_eq!(flags[0].numeral, "2");
    }

    #[test]
    fn category_numerals_cover_every_category() {
        assert_eq!(category_numeral(Some(ClimbCategory::Cat4)), "4");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat3)), "3");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat2)), "2");
        assert_eq!(category_numeral(Some(ClimbCategory::Cat1)), "1");
        assert_eq!(category_numeral(Some(ClimbCategory::Hc)), "HC");
        assert_eq!(category_numeral(None), "–");
    }

    // ── css ────────────────────────────────────────────────────────────────────

    /// Custom properties the components define inline (not part of the token set).
    const LOCAL_VARS: [&str; 6] = ["--c", "--p", "--fill", "--ink", "--profile-dock", "--wpad"];

    /// Every `--name:` declaration of a stylesheet (comments included declarations the plain
    /// `split(';')` approach would swallow, so this scans for the declaration colon).
    fn defined_tokens(css: &str) -> HashSet<String> {
        let bytes = css.as_bytes();
        let mut out = HashSet::new();
        let mut i = 0usize;
        while i + 2 < bytes.len() {
            if bytes[i] == b'-' && bytes[i + 1] == b'-' {
                let start = i + 2;
                let mut j = start;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
                    j += 1;
                }
                if j > start && j < bytes.len() && bytes[j] == b':' {
                    out.insert(css[i..j].to_string());
                    i = j;
                    continue;
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
        out
    }

    #[test]
    fn profile_css_only_uses_tokens_the_kit_defines() {
        let kit = include_str!("../../ride-ui-kit/assets/kit.css");
        let defined = defined_tokens(kit);
        assert!(defined.contains("--font-ui"), "the token parser works");
        assert!(defined.contains("--z4"));
        let mut rest = CSS;
        let mut seen = 0usize;
        while let Some(at) = rest.find("var(--") {
            let after = &rest[at + 6..];
            let end = after.find([')', ',']).expect("var() is closed");
            let name = after[..end].trim();
            assert!(
                defined.contains(&format!("--{name}"))
                    || LOCAL_VARS.contains(&format!("--{name}").as_str()),
                "--{name} is not defined by the design kit"
            );
            seen += 1;
            rest = &after[end..];
        }
        assert!(seen > 30, "found only {seen} token uses");
    }

    #[test]
    fn profile_css_has_no_hard_coded_colours() {
        assert!(
            !CSS.contains('#'),
            "hex colours are not allowed, use tokens"
        );
        assert!(
            !CSS.contains("rgb"),
            "rgb()/rgba() colours are not allowed, use tokens"
        );
    }

    #[test]
    fn every_class_of_this_crate_is_prefixed() {
        let mut rest = CSS;
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
                assert!(
                    class == "profile" || class.starts_with("profile-") || class.starts_with("is-"),
                    ".{class} must be prefixed `profile-`"
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
        route: Option<Arc<Route>>,
        history: Arc<Vec<RecordPoint>>,
        theme: Theme,
        mode: ProfileMode,
        dist: f64,
        /// `(distance, delta in seconds)` of a ghost to put in the snapshot.
        ghost: Option<(f64, Option<f64>)>,
    }

    #[component]
    fn Harness(props: HarnessProps) -> Element {
        let route0 = props.route.clone();
        let route = use_signal(move || route0);
        let dist0 = props.dist;
        let ghost0 = props.ghost;
        let snapshot = use_signal(move || {
            let mut snapshot = RideSnapshot::empty();
            snapshot.state = RideState::Riding;
            snapshot.dist = dist0;
            snapshot.pos.dist = dist0;
            if let Some((ghost_dist, delta_s)) = ghost0 {
                snapshot.ghost = Some(ride_model::GhostLive {
                    dist: ghost_dist,
                    pos: Default::default(),
                    delta_s,
                });
            }
            snapshot
        });
        let mode0 = props.mode;
        let mode = use_signal(move || mode0);
        let lookahead = use_signal(|| 3_000.0_f64);
        let theme0 = props.theme;
        let theme = use_signal(move || theme0);
        let history0 = props.history.clone();
        let history = use_signal(move || history0);
        let rider = use_signal(rider);
        rsx! {
            ElevationProfile {
                route,
                snapshot,
                mode,
                lookahead_m: lookahead,
                theme,
                on_seek: move |_| {},
                on_mode: move |_| {},
            }
            LiveChart { history, metric: ChartMetric::Power, rider, theme }
            LiveChart { history, metric: ChartMetric::HeartRate, rider, theme }
            LiveChart { history, metric: ChartMetric::Cadence, rider, theme }
            LiveChart { history, metric: ChartMetric::Speed, rider, theme }
        }
    }

    fn render(props: HarnessProps) -> String {
        let mut vdom = VirtualDom::new_with_props(Harness, props);
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    fn harness(dist: f64, mode: ProfileMode) -> HarnessProps {
        HarnessProps {
            route: Some(Arc::new(fake_route())),
            history: Arc::new(history()),
            theme: Theme::Dark,
            mode,
            dist,
            ghost: None,
        }
    }

    #[test]
    fn renders_the_profile_and_the_charts() {
        // 6 km: inside the first climb, so its flag is visible
        let html = render(harness(6_000.0, ProfileMode::Lookahead));
        assert!(html.contains("profile-plot"), "plot rendered");
        assert!(html.contains("profile-svg"), "svg rendered");
        assert!(html.contains("<polygon"), "flat fills");
        assert!(html.contains("vector-effect"), "crisp outline");
        assert!(html.contains("profile-guide"), "rider hairline");
        assert!(html.contains("profile-dot"), "rider disc");
        // the disc is clamped on both axes, so it never hangs over the plot edge
        let dot = html
            .split(r#"class="profile-dot""#)
            .nth(1)
            .expect("the rider disc");
        assert!(
            dot.contains("left: clamp(8px,") && dot.contains("top: clamp(8px,"),
            "the disc is clamped left and top: {dot:.160}"
        );
        assert!(html.contains("profile-done"), "ridden part covered");
        assert!(html.contains("profile-flag-box"), "climb flag");
        assert!(html.contains("profile-climbbar"), "climb bar on the ruler");
        assert!(html.contains("Előretekintés"), "mode toggle");
        assert!(html.contains("profile-ruler"), "km ruler");
        assert!(html.contains("profile-tick--major"), "ruler numerals");
        assert!(html.contains("profile-ytick"), "elevation ticks");
        assert!(
            !html.contains("profile-band"),
            "the old tinted bands are gone"
        );
        assert!(!html.contains("profile-ahead"), "the old shading is gone");
        assert!(html.contains("profile-chart-strip"), "zone strip");
        assert!(html.contains("FTP 235"), "FTP rule label");
        assert!(html.contains("Teljesítmény"), "power chart");
        assert!(html.contains("Pulzus"), "heart-rate chart");
        assert!(html.contains("Kadencia"), "cadence chart");
        assert!(html.contains("Sebesség"), "speed chart");
        assert!(html.contains("profile-chart-svg"), "chart svg");
        assert!(
            html.contains("Z4 Küszöb") || html.contains("Z5"),
            "power zone"
        );
        assert!(!html.contains("NaN"), "no NaN reaches the markup");
        assert!(!html.contains("Infinity"), "no infinity reaches the markup");
    }

    #[test]
    fn renders_the_full_mode_without_a_lookahead_window() {
        let html = render(harness(11_400.0, ProfileMode::Full));
        assert!(html.contains("profile-plot"));
        assert!(html.contains("Teljes"));
        assert!(!html.contains("Ablak"), "no window pair in full mode");
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn the_ghost_gets_a_marker_and_a_signed_read_out() {
        // 6 km, ghost 500 m ahead (positive = the rider is behind): red, +25 s
        let mut props = harness(6_000.0, ProfileMode::Full);
        props.ghost = Some((6_500.0, Some(25.0)));
        let html = render(props);
        assert!(html.contains("profile-ghost"), "ghost diamond missing");
        assert!(
            html.contains("profile-guide--ghost"),
            "ghost hairline missing"
        );
        assert!(html.contains("Szellem"), "ghost label missing");
        assert!(
            html.contains("profile-pair profile-pair--behind"),
            "behind must be coloured: {html}"
        );
        assert!(html.contains("+25"), "signed difference missing");
        assert!(
            html.matches("profile-dot").count() >= 1,
            "the rider keeps its disc"
        );

        // ahead: green, negative
        let mut props = harness(6_500.0, ProfileMode::Full);
        props.ghost = Some((6_000.0, Some(-25.0)));
        let html = render(props);
        assert!(html.contains("profile-pair profile-pair--ahead"), "{html}");
        assert!(html.contains("-25"));

        // a ghost outside the shown window is not pinned to the edge
        let mut props = harness(20_000.0, ProfileMode::Lookahead);
        props.ghost = Some((1_000.0, Some(400.0)));
        let html = render(props);
        assert!(!html.contains("profile-ghost"), "no ghost far behind");
        // ... but its difference is still reported
        assert!(html.contains("+400"));

        // no ghost at all: no marker, no pair
        let html = render(harness(6_000.0, ProfileMode::Full));
        assert!(!html.contains("profile-ghost"));
        assert!(!html.contains("Szellem"));
    }

    #[test]
    fn the_delta_tone_is_symmetric() {
        assert_eq!(delta_tone(-12.0), "ahead");
        assert_eq!(delta_tone(12.0), "behind");
        assert_eq!(delta_tone(0.2), "");
        assert_eq!(delta_tone(f64::NAN), "");
    }

    #[test]
    fn the_full_profile_has_a_km_ruler_with_a_numeral_per_5_km_and_two_flags() {
        let html = render(harness(11_400.0, ProfileMode::Full));
        // fake_route is 24 km: ticks 0..=24, numerals 0 5 10 15 20 (the first one with the unit)
        assert_eq!(
            html.matches("class=\"profile-tick\"").count(),
            20,
            "20 minor ticks"
        );
        assert_eq!(html.matches("profile-tick profile-tick--major").count(), 5);
        assert_eq!(
            html.matches("class=\"profile-tick-label").count(),
            5,
            "5 numerals"
        );
        assert!(html.contains(">0 km<"), "first numeral carries the unit");
        assert!(html.contains(">20<"));
        assert_eq!(
            html.matches("class=\"profile-flag\"").count(),
            2,
            "two climbs, two flags"
        );
        assert_eq!(html.matches("profile-climbbar").count(), 2);
        assert!(html.contains("2. kat. · "), "wide climb label");
        assert!(html.contains("--ink: "), "numeral ink is set per flag");
        // flat: nothing is translucent or a gradient
        assert!(!html.contains("fill-opacity"));
        assert!(!html.contains("Gradient"));
    }

    #[test]
    fn profile_css_keeps_to_the_house_rules() {
        for banned in [
            "blur(",
            "backdrop-filter",
            "text-shadow",
            "drop-shadow",
            "text-transform",
            "radial-gradient",
            "conic-gradient",
            "linear-gradient",
            "letter-spacing",
        ] {
            assert!(
                !CSS.contains(banned),
                "{banned} is not allowed in the profile CSS"
            );
        }
        let mut rest = CSS;
        while let Some(at) = rest.find("box-shadow:") {
            let after = &rest[at + "box-shadow:".len()..];
            let end = after.find(';').unwrap_or(after.len());
            let value = after[..end].trim();
            assert!(
                value == "none",
                "box-shadow `{value}` must be none (the tip uses the kit class)"
            );
            rest = &after[end..];
        }
        let mut rest = CSS;
        while let Some(at) = rest.find("border-radius:") {
            let after = &rest[at + "border-radius:".len()..];
            let end = after.find(';').unwrap_or(after.len());
            for word in after[..end].split_whitespace() {
                if let Some(px) = word.strip_suffix("px") {
                    let v: f64 = px.parse().expect("a px radius");
                    assert!(v <= 4.0, "radius {word} is larger than 4px");
                }
                assert!(
                    !word.contains('%'),
                    "no round shapes in the profile CSS: {word}"
                );
            }
            rest = &after[end..];
        }
    }

    #[test]
    fn empty_inputs_render_placeholders() {
        let html = render(HarnessProps {
            route: None,
            history: Arc::new(Vec::new()),
            theme: Theme::Light,
            mode: ProfileMode::Lookahead,
            dist: 0.0,
            ghost: None,
        });
        assert!(
            html.contains("profile--empty"),
            "neutral profile placeholder"
        );
        assert!(html.contains("Nincs betöltött útvonal"));
        assert!(html.contains("Még nincs adat a grafikonon."));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("<polygon"));
    }

    #[test]
    fn a_single_point_route_does_not_break_the_profile() {
        let mut route = fake_route();
        route.points.truncate(1);
        route.total_dist = 0.0;
        let html = render(HarnessProps {
            route: Some(Arc::new(route)),
            history: Arc::new(history()),
            theme: Theme::Dark,
            mode: ProfileMode::Lookahead,
            dist: 0.0,
            ghost: None,
        });
        assert!(html.contains("Nincs mit kirajzolni"));
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn a_route_without_elevation_is_flagged() {
        let mut route = fake_route();
        route.has_elevation = false;
        let html = render(HarnessProps {
            route: Some(Arc::new(route)),
            history: Arc::new(history()),
            theme: Theme::Dark,
            mode: ProfileMode::Full,
            dist: 0.0,
            ghost: None,
        });
        assert!(html.contains("Nincs magassági adat"));
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn a_history_with_only_missing_values_still_renders() {
        let mut broken = history();
        for point in &mut broken {
            point.power_w = None;
            point.hr_bpm = None;
            point.cadence_rpm = None;
        }
        let html = render(HarnessProps {
            route: Some(Arc::new(fake_route())),
            history: Arc::new(broken),
            theme: Theme::Dark,
            mode: ProfileMode::Lookahead,
            dist: 1_000.0,
            ghost: None,
        });
        assert!(html.contains("profile-chart"));
        assert!(html.contains("jelenleg —") || html.contains("—"));
        assert!(!html.contains("NaN"));
    }

    // ── geometry helpers used by the component ─────────────────────────────────

    #[test]
    fn pointer_fractions_are_clamped_and_need_a_width() {
        assert_eq!(pointer_frac(50.0, 200.0), Some(0.25));
        assert_eq!(pointer_frac(-5.0, 200.0), Some(0.0));
        assert_eq!(pointer_frac(500.0, 200.0), Some(1.0));
        assert_eq!(pointer_frac(50.0, 0.0), None);
        assert_eq!(pointer_frac(f64::NAN, 200.0), None);
        assert_eq!(pct(f64::NAN), 0.0);
        assert_eq!(pct(120.0), 100.0);
        assert_eq!(pct_text(12.3456), "12.346%");
        assert_eq!(pct_text(f64::NAN), "0.000%");
    }

    #[test]
    fn the_harness_snapshot_matches_the_component_expectations() {
        let snapshot = snapshot_at(11_400.0);
        assert_eq!(snapshot.state, RideState::Riding);
        assert_eq!(snapshot.dist, 11_400.0);
        assert_eq!(
            ride_model::Route::locate(&fake_route(), snapshot.dist).dist,
            11_400.0
        );
    }
}
