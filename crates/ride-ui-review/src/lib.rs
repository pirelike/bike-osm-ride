//! The review page: the stored rides of one route side by side.
//!
//! * a **list** of the route's rides (up to four are ticked at once, each gets a colour and a dash
//!   pattern), with imports, deletion and — for a ride whose watts were estimated — the estimate
//!   setup;
//! * the **map** with the zoomed section highlighted and a dot at the cursor;
//! * an **overview** (the whole elevation profile) on which a drag selects the section to zoom
//!   into, with the route's climbs as quick picks;
//! * the **chart** of power, heart rate, cadence, speed or time difference over distance, for the
//!   ticked rides, zoomed to the same section, with a shared cursor;
//! * the **section table**: time, speeds, watts, heart rate, cadence and climbing per ride.
//!
//! The page owns only view state (metric, section, cursor, open forms). What is stored, which
//! rides are ticked and the file I/O belong to the app; it passes the loaded records in and gets
//! events back. The numbers come from [`chart`] and `ride_sim::compare`, pure and tested.

use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{
    EstimateParams, MapStyle, RideMeta, RideRecord, RideSnapshot, RideSource, RiderSettings, Route,
    RouteRides, Theme,
};
use ride_sim::compare::{DistSeries, SectionStats, GRID_M};
use ride_sim::estimate::{custom_params, profile_params};
use ride_sim::power_curve::Analysis;
use ride_ui_kit::format::{fmt_delta_duration, fmt_duration, fmt_km, fmt_opt0, fmt_speed_kmh};
use ride_ui_kit::{Icon, IconName, NumberField, Segmented};
use ride_ui_map::{MapOverlay, MapView, OverlayMarker};
use ride_ui_profile::geom;

pub mod chart;
pub mod power;

use chart::{Metric, SLOTS};
use power::PowerView;
use ride_i18n::{t, tf};

/// Component stylesheet (tokens only; every class is prefixed `rv-`). The shell injects it after
/// the design kit.
pub const CSS: &str = include_str!("../assets/review.css");

/// Placeholder for a value that is not available ("—", U+2014).
pub const DASH: &str = "—";

/// A drag shorter than this (metres) is a click, not a section.
const MIN_SELECT_M: f64 = 100.0;

/// Quick-pick chips for the route's climbs.
const MAX_CLIMB_CHIPS: usize = 4;

/// The stem of a stored record: its start time in whole seconds (see `ride_route::rides`).
pub fn stem_of(record: &RideRecord) -> String {
    format!("{:.0}", record.summary.started_unix_s.round())
}

// ───────────────────────────── small helpers ─────────────────────────────

fn pct_text(value: f64) -> String {
    let v = if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    };
    format!("{v:.3}%")
}

/// Measures the element behind `mounted` and stores its width (CSS pixels) in `width`.
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

/// Class of an axis numeral at `pct` percent of the width: centred on its tick, or pulled
/// inside when it sits at an edge of the plot.
fn edge_class(base: &str, pct: f64) -> String {
    if pct < 4.0 {
        format!("{base} is-start")
    } else if pct > 96.0 {
        format!("{base} is-end")
    } else {
        base.to_string()
    }
}

fn pointer_frac(x: f64, width: f64) -> Option<f64> {
    (width.is_finite() && width > 1.0 && x.is_finite()).then(|| (x / width).clamp(0.0, 1.0))
}

fn var_of(slot: usize) -> String {
    format!("var(--series-{})", slot % SLOTS + 1)
}

/// `A`, `B`, `C`, `D`: the letter a ride goes by on the page and on the map.
fn letter(slot: usize) -> &'static str {
    ["A", "B", "C", "D"][slot % SLOTS]
}

fn km_text(dist_m: f64) -> String {
    format!("{} km", fmt_km(dist_m))
}

fn stat_opt(value: Option<f64>) -> String {
    match value {
        Some(v) if v.is_finite() => fmt_opt0(Some(v)),
        _ => DASH.to_string(),
    }
}

fn kind_tags(meta: &RideMeta) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if meta.source == RideSource::Imported {
        tags.push(t("ui_review.importalt"));
    }
    if meta.simulated {
        tags.push(t("ui_review.szimulalt"));
    }
    if meta.power_estimated {
        tags.push(t("ui_review.becsult_watt"));
    }
    tags
}

// ───────────────────────────── selection layer ─────────────────────────────

/// Transparent layer over a plot: reports the distance under the pointer and, after a drag, the
/// section the user swept.
///
/// A mouse shows the cursor while it hovers and drags a section. A finger must still be able to
/// scroll the page, because on a phone these plots fill most of the screen: the layer only claims
/// sideways movement (`touch-action: pan-y`), so a vertical swipe scrolls (the browser then
/// cancels the pointer), a sideways drag selects, and a tap puts the cursor down and leaves it
/// there, since a finger has no hover to show the values with.
#[component]
fn HitLayer(
    x0: f64,
    x1: f64,
    on_cursor: EventHandler<Option<f64>>,
    on_select: EventHandler<(f64, f64)>,
) -> Element {
    let width = use_signal(|| 0.0_f64);
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut press = use_signal(|| None::<f64>);
    let mut live = use_signal(|| None::<f64>);
    let mut touch = use_signal(|| false);
    let dist_at = move |frac: f64| x0 + frac * (x1 - x0);

    let finish = move |end: Option<f64>| {
        let mut press = press;
        let mut live = live;
        let Some(start) = press.take() else {
            return;
        };
        live.set(None);
        let Some(end) = end else {
            return;
        };
        let span = (x1 - x0).max(1.0);
        let (a, b) = (start.min(end), start.max(end));
        if (b - a) * span >= MIN_SELECT_M.max(span * 0.01) {
            on_select.call((x0 + a * span, x0 + b * span));
        }
    };

    let drag = match (*press.read(), *live.read()) {
        (Some(a), Some(b)) => Some((a.min(b) * 100.0, (a - b).abs() * 100.0)),
        _ => None,
    };

    rsx! {
        div {
            class: "rv-hit",
            onmounted: move |event: MountedEvent| {
                mounted.set(Some(event.data()));
                measure_width(mounted, width);
            },
            onpointerdown: move |event: PointerEvent| {
                if !event.is_primary() {
                    return;
                }
                touch.set(event.pointer_type() == "touch");
                measure_width(mounted, width);
                if let Some(frac) = pointer_frac(event.element_coordinates().x, *width.peek()) {
                    press.set(Some(frac));
                    live.set(Some(frac));
                }
            },
            onpointermove: move |event: PointerEvent| {
                if !event.is_primary() {
                    return;
                }
                if let Some(frac) = pointer_frac(event.element_coordinates().x, *width.peek()) {
                    on_cursor.call(Some(dist_at(frac)));
                    if press.peek().is_some() {
                        live.set(Some(frac));
                    }
                }
            },
            onpointerup: move |event: PointerEvent| {
                if !event.is_primary() {
                    return;
                }
                let end = pointer_frac(event.element_coordinates().x, *width.peek());
                if *touch.peek() {
                    // a tap (or the end of a drag) leaves the cursor where the finger was
                    if let Some(frac) = end {
                        on_cursor.call(Some(dist_at(frac)));
                    }
                }
                finish(end);
            },
            onpointercancel: move |_| {
                // the browser took the gesture over (a vertical swipe scrolls the page)
                press.set(None);
                live.set(None);
            },
            onpointerleave: move |_| {
                // a finger leaves after every lift: keep its cursor, only a mouse clears it
                if !*touch.peek() {
                    on_cursor.call(None);
                }
                let end = *live.peek();
                finish(end);
            },
            if let Some((left, wide)) = drag {
                div {
                    class: "rv-drag",
                    style: "left: {pct_text(left)}; width: {pct_text(wide)}",
                }
            }
        }
    }
}

// ───────────────────────────── overview ─────────────────────────────

/// The whole route as an elevation profile; the selected section is a band over it.
#[component]
fn Overview(
    route: Arc<Route>,
    theme: Theme,
    range: Option<(f64, f64)>,
    cursor: Option<f64>,
    on_cursor: EventHandler<Option<f64>>,
    on_select: EventHandler<(f64, f64)>,
) -> Element {
    let total = route.total_dist.max(1.0);
    let markup = use_memo({
        let route = route.clone();
        move || geom::build_geometry(&route, 0.0, total, theme).markup
    });
    let (lo, hi) = geom::elevation_range(&route, 0.0, total)
        .map(|(lo, hi)| geom::y_range(lo, hi))
        .unwrap_or((0.0, 100.0));
    let transform = geom::view_transform(0.0, total, lo, hi);
    let mut ticks = geom::ruler_ticks(0.0, total);
    // the first numeral carries the unit, like the ride screen's profile
    if let Some(tick) = ticks.iter_mut().find(|tick| tick.label.is_some()) {
        tick.label = tick.label.take().map(|label| format!("{label} km"));
    }
    let scale = geom::XScale::new(0.0, total);

    rsx! {
        div { class: "rv-overview", "aria-label": t("ui_review.a_teljes_utvonal_magassagi_profilja"),
            div { class: "rv-overview-plot",
                svg {
                    class: "rv-svg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    g { transform: "{transform}", dangerous_inner_html: "{markup}" }
                }
                if let Some((a, b)) = range {
                    div {
                        class: "rv-band",
                        style: "left: {pct_text(scale.pct(a))}; width: {pct_text(scale.pct(b) - scale.pct(a))}",
                    }
                }
                if let Some(d) = cursor {
                    div { class: "rv-cursor", style: "left: {pct_text(scale.pct(d))}" }
                }
                HitLayer { x0: 0.0, x1: total, on_cursor, on_select }
            }
            div { class: "rv-ruler", "aria-hidden": "true",
                for tick in ticks.iter() {
                    div {
                        key: "tick-{tick.dist:.0}",
                        class: if tick.major { "rv-tick rv-tick--major" } else { "rv-tick" },
                        style: "left: {pct_text(scale.pct(tick.dist))}",
                        if let Some(label) = &tick.label {
                            span { class: edge_class("rv-tick-label", scale.pct(tick.dist)), "{label}" }
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────────── chart ─────────────────────────────

#[component]
fn DistanceChart(
    metric: Metric,
    geometry: chart::ChartGeometry,
    cursor: Option<f64>,
    on_cursor: EventHandler<Option<f64>>,
    on_select: EventHandler<(f64, f64)>,
) -> Element {
    let g = &geometry;
    let transform = geom::view_transform(g.x0, g.x1, g.y0, g.y1);
    let y_pct = |v: f64| (1.0 - geom::elevation_frac(v, g.y0, g.y1)) * 100.0;
    let scale = geom::XScale::new(g.x0, g.x1);
    let x_ticks = geom::km_ticks(g.x0, g.x1, 6);
    let step = geom::km_tick_step(g.x1 - g.x0, 6);
    let empty = g.drawn == 0;

    rsx! {
        div { class: "rv-chart",
            div { class: "rv-plot",
                svg {
                    class: "rv-svg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    g { transform: "{transform}", dangerous_inner_html: "{g.markup}" }
                }
                for (i , v) in g.ticks.iter().enumerate() {
                    div {
                        key: "grid-{v:.1}",
                        class: "rv-grid",
                        style: "top: {pct_text(y_pct(*v))}",
                        span { class: "rv-ytick",
                            // the top rule names the unit
                            if i + 1 == g.ticks.len() {
                                "{metric.value_text(*v)} {metric.unit()}"
                            } else {
                                "{metric.value_text(*v)}"
                            }
                        }
                    }
                }
                if empty {
                    div { class: "rv-note", {t("ui_review.ehhez_a_metrikahoz_nincs_adat_a_kijelolt_edzesekben")} }
                }
                if let Some(d) = cursor.filter(|d| *d >= g.x0 && *d <= g.x1) {
                    div { class: "rv-cursor", style: "left: {pct_text(scale.pct(d))}" }
                }
                HitLayer { x0: g.x0, x1: g.x1, on_cursor, on_select }
            }
            div { class: "rv-xaxis", "aria-hidden": "true",
                for (i , d) in x_ticks.iter().enumerate() {
                    span {
                        key: "x-{d:.0}",
                        // a numeral at an edge is pulled inside, not cut in half
                        class: edge_class("rv-xtick", scale.pct(*d)),
                        style: "left: {pct_text(scale.pct(*d))}",
                        if i == 0 {
                            "{geom::tick_label(*d, step)} km"
                        } else {
                            "{geom::tick_label(*d, step)}"
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────────── table ─────────────────────────────

fn stats_cells(stats: &SectionStats, estimated: bool) -> Vec<String> {
    let watts = |w: Option<f64>| match w {
        Some(v) if estimated => format!("~{}", fmt_opt0(Some(v))),
        other => stat_opt(other),
    };
    vec![
        fmt_duration(stats.time_s),
        fmt_speed_kmh(stats.avg_speed_ms),
        watts(stats.avg_power_w),
        watts(stats.normalized_power_w),
        stat_opt(stats.avg_hr),
        stat_opt(stats.max_hr),
        stat_opt(stats.avg_cadence),
        format!("{} m", fmt_opt0(Some(stats.ascent_m))),
        stat_opt(stats.vam),
    ]
}

fn table_head() -> [&'static str; 9] {
    [
        t("ui_review.ido"),
        t("ui_review.atl_seb_km_h"),
        t("ui_review.atl_watt"),
        "NP (W)",
        t("ui_review.atl_pulzus"),
        t("ui_review.max_pulzus"),
        t("ui_review.chart.kadencia"),
        t("ui_review.emelkedes"),
        "VAM (m/h)",
    ]
}

// ───────────────────────────── panels ─────────────────────────────

/// The title strip of a section, the same 24 px bar the ride screen's panels have, so the page
/// reads as part of the same instrument (without their grip: these panels do not move). The
/// phone has none: its tab already names the section.
fn panel_bar(title: &str, compact: bool) -> Element {
    if compact {
        return rsx! {};
    }
    rsx! {
        div { class: "rv-panel-bar",
            h2 { class: "rv-panel-title", "{title}" }
        }
    }
}

// ───────────────────────────── the page ─────────────────────────────

#[component]
pub fn ReviewPage(
    routes: ReadSignal<Vec<RouteRides>>,
    route_id: ReadSignal<Option<String>>,
    route: ReadSignal<Option<Arc<Route>>>,
    /// The ticked rides, in colour order (the first is the reference of the time difference).
    selected: ReadSignal<Vec<String>>,
    /// The records of `selected`, in the same order.
    loaded: ReadSignal<Arc<Vec<Arc<RideRecord>>>>,
    busy: ReadSignal<bool>,
    /// Messages from the last import (partial coverage, estimated watts, ...).
    notes: ReadSignal<Vec<String>>,
    rider: ReadSignal<RiderSettings>,
    theme: ReadSignal<Theme>,
    map_style: ReadSignal<MapStyle>,
    /// The power curve, the FTP suggestion and the trend of every ride (the "Teljesítmény" view).
    analysis: ReadSignal<Arc<Analysis>>,
    /// The FTP in the settings, watts.
    ftp_w: ReadSignal<f64>,
    compact: bool,
    on_close: EventHandler<()>,
    on_route: EventHandler<String>,
    on_toggle: EventHandler<String>,
    on_import: EventHandler<()>,
    on_delete: EventHandler<String>,
    on_reestimate: EventHandler<(String, EstimateParams)>,
    on_map_style: EventHandler<MapStyle>,
    on_dismiss_notes: EventHandler<()>,
    /// "FTP beállítása N W-ra": the app changes the setting.
    on_apply_ftp: EventHandler<f64>,
) -> Element {
    let mut view = use_signal(|| "compare".to_string());
    let mut metric = use_signal(|| Metric::Power);
    let mut range = use_signal(|| None::<(f64, f64)>);
    let mut cursor = use_signal(|| None::<f64>);
    // Starts one ahead of the map, so the first fit is still owed when the map first has a real
    // size: on a phone it is hidden behind a tab until then, and a fit made at another size left
    // the start of the route under the scale bar.
    let fit_seq = use_signal(|| 1u64);
    let mut tab = use_signal(|| "rides".to_string());
    let mut confirm_delete = use_signal(|| None::<String>);
    let mut estimating = use_signal(|| None::<String>);
    let mut est_custom = use_signal(|| false);
    let mut est_rider_kg = use_signal(|| 75.0_f64);
    let mut est_bike_kg = use_signal(|| 9.0_f64);
    let mut est_crr = use_signal(|| 0.004_f64);
    let follow = use_signal(|| false);
    let snapshot = use_signal(RideSnapshot::empty);

    // A different route or a different set of rides starts from the whole route again.
    use_effect(move || {
        let _ = route_id.read();
        range.set(None);
        cursor.set(None);
        confirm_delete.set(None);
        estimating.set(None);
    });

    // Every loaded ride on the 10 m grid; a ride that cannot be put on it is left out.
    let pairs = use_memo(move || -> Vec<(Arc<RideRecord>, Arc<DistSeries>)> {
        loaded
            .read()
            .iter()
            .take(SLOTS)
            .filter_map(|record| {
                DistSeries::from_points(&record.points, GRID_M)
                    .map(|series| (record.clone(), Arc::new(series)))
            })
            .collect()
    });

    // The default x window: everything the ticked rides cover.
    let full_span = use_memo(move || -> (f64, f64) {
        let pairs = pairs.read();
        let from = pairs
            .iter()
            .map(|(_, s)| s.start_dist)
            .reduce(f64::min)
            .unwrap_or(0.0);
        let to = pairs.iter().map(|(_, s)| s.end_dist()).reduce(f64::max);
        let total = route.read().as_ref().map_or(0.0, |r| r.total_dist);
        (from, to.unwrap_or(total).max(from + 1.0))
    });
    let window = range.read().unwrap_or(*full_span.read());

    // Wide windows are smoothed more; the series are rebuilt only when that choice changes.
    let smooth_m = ((window.1 - window.0) / 120.0 / GRID_M)
        .round()
        .clamp(1.0, 30.0)
        * GRID_M;
    let smoothed = use_memo(move || -> Vec<Arc<DistSeries>> {
        pairs
            .read()
            .iter()
            .map(|(_, s)| Arc::new(s.smoothed(smooth_m)))
            .collect()
    });

    let metric_now = *metric.read();
    let geometry = {
        let rides = smoothed.read();
        let lines = chart::lines(&rides, metric_now, window.0, window.1);
        chart::chart_geometry(metric_now, &lines, window.0, window.1)
    };

    let set_range = move |next: Option<(f64, f64)>| {
        let mut range = range;
        let mut fit_seq = fit_seq;
        range.set(next);
        fit_seq.with_mut(|n| *n += 1);
    };

    // map: the section in the accent colour, a dot at the cursor, the camera follows the section
    let overlay = use_memo(move || -> MapOverlay {
        let range = *range.read();
        let route_now = route.read().clone();
        let marker = (*cursor.read()).zip(route_now.as_ref()).map(|(d, r)| {
            let at = r.locate(d);
            OverlayMarker {
                lat: at.lat,
                lon: at.lon,
                color: "--accent".to_string(),
                label: km_text(d),
            }
        });
        MapOverlay {
            highlight: range,
            markers: marker.into_iter().collect(),
            fit: Some(
                range.unwrap_or_else(|| (0.0, route_now.as_ref().map_or(0.0, |r| r.total_dist))),
            ),
            fit_seq: *fit_seq.read(),
        }
    });

    // ── data for the markup ──
    let theme_now = *theme.read();
    let route_now = route.read().clone();
    let routes_now = routes.read().clone();
    let current_id = route_id.read().clone();
    let current_rides: Vec<RideMeta> = routes_now
        .iter()
        .find(|r| Some(&r.route_id) == current_id.as_ref())
        .map(|r| r.rides.clone())
        .unwrap_or_default();
    let selected_now = selected.read().clone();
    let records: Vec<Arc<RideRecord>> = pairs.read().iter().map(|(r, _)| r.clone()).collect();
    let slot_of = |stem: &str| records.iter().position(|r| stem_of(r) == stem);
    let cursor_now = *cursor.read();
    let busy_now = *busy.read();
    let notes_now = notes.read().clone();
    let tab_now = tab.read().clone();
    let view_now = view.read().clone();
    let power_view = view_now == "power";
    let shared = chart::common_span(&smoothed.read());
    let table_span = range.read().or(shared);
    let rows = table_span
        .map(|(a, b)| chart::stats_rows(&records, a, b))
        .unwrap_or_default();
    let climbs = route_now
        .as_ref()
        .map(|r| chart::climb_ranges(r, MAX_CLIMB_CHIPS))
        .unwrap_or_default();
    let rider_now = rider.read().clone();
    let reference_is_estimated = records.iter().any(|r| r.power_estimated);

    // legend: one chip per ride with the value under the cursor; without a cursor the mean over
    // the window ("Ø", the time difference at its end), so the legend is never a row of dashes
    let legend: Vec<(usize, String, String)> = records
        .iter()
        .enumerate()
        .map(|(slot, record)| {
            let with_unit = |v: f64| format!("{} {}", metric_now.value_text(v), metric_now.unit());
            let value = match cursor_now {
                Some(d) => chart::value_at(&smoothed.read(), slot, metric_now, d).map(with_unit),
                None => chart::window_value(&smoothed.read(), slot, metric_now, window.0, window.1)
                    .map(|v| {
                        if metric_now == Metric::Delta {
                            with_unit(v)
                        } else {
                            format!("Ø {}", with_unit(v))
                        }
                    }),
            }
            .unwrap_or_else(|| DASH.to_string());
            let estimated = record.power_estimated && metric_now == Metric::Power;
            (
                slot,
                chart::fmt_stamp(record.summary.started_unix_s),
                if estimated {
                    format!("~{value}")
                } else {
                    value
                },
            )
        })
        .collect();

    // the section table turned on its side for the phone: (row head, one cell per ride)
    let by_metric: Vec<(String, Vec<String>)> = {
        let mut heads: Vec<String> = table_head().iter().map(|h| h.to_string()).collect();
        heads.push(t("ui_review.kulonbseg").to_string());
        let columns: Vec<Vec<String>> = rows
            .iter()
            .map(|row| match row.stats {
                Some(stats) => {
                    let estimated = records.get(row.slot).is_some_and(|r| r.power_estimated);
                    let mut cells = stats_cells(&stats, estimated);
                    cells.push(match row.time_delta_s {
                        Some(delta) if row.slot != 0 => fmt_delta_duration(delta),
                        _ => DASH.to_string(),
                    });
                    cells
                }
                None => vec![DASH.to_string(); heads.len()],
            })
            .collect();
        heads
            .into_iter()
            .enumerate()
            .map(|(i, head)| {
                let cells = columns
                    .iter()
                    .map(|c| c.get(i).cloned().unwrap_or_else(|| DASH.to_string()))
                    .collect();
                (head, cells)
            })
            .collect()
    };

    let page_class = if compact {
        format!("rv-page rv-page--compact rv-tab-{tab_now}")
    } else {
        "rv-page".to_string()
    };
    let no_rides = routes_now.is_empty();
    let range_text = (*range.read()).map(|(a, b)| {
        tf(
            "ui_review.kijelolt_szakasz",
            &[
                ("from", &km_text(a)),
                ("to", &km_text(b)),
                ("length", &km_text(b - a)),
            ],
        )
    });
    let editing: Option<(String, Option<EstimateParams>)> = estimating.read().clone().map(|stem| {
        let current = records
            .iter()
            .find(|r| stem_of(r) == stem)
            .and_then(|r| r.estimate);
        (stem, current)
    });

    let views = vec![
        (
            "compare".to_string(),
            t("ui_review.osszehasonlitas").to_string(),
        ),
        (
            "power".to_string(),
            t("ui_review.chart.teljesitmeny").to_string(),
        ),
    ];
    // Phone: the metric buttons carry the unit (language-neutral and short; five full names do
    // not fit one row in every language), and the chart head names the metric in full.
    let metric_options: Vec<(String, String)> = Metric::ALL
        .iter()
        .map(|m| {
            let label = if compact {
                m.short_label().to_string()
            } else {
                m.title().to_string()
            };
            (m.key().to_string(), label)
        })
        .collect();

    rsx! {
        div { class: "{page_class}", role: "region", "aria-label": t("ui_review.edzesek_attekintese"),
            // Phone: one row holds the way back, the two views and the import (icons only, the
            // top bar already names the page); the desktop keeps the title and a row of its own.
            header { class: "rv-head",
                button {
                    class: if compact { "btn btn--ghost btn--icon" } else { "btn btn--ghost btn--sm" },
                    "aria-label": t("ui_review.vissza_az_edzeshez"),
                    title: t("ui_review.vissza_az_edzeshez").to_string(),
                    onclick: move |_| on_close.call(()),
                    Icon { name: IconName::ChevronLeft, size: if compact { 20 } else { 16 } }
                    if !compact {
                        {t("ui_review.vissza")}
                    }
                }
                if compact {
                    div { class: "rv-head-views",
                        Segmented {
                            options: views.clone(),
                            value: view_now.clone(),
                            on_change: move |value: String| view.set(value),
                            aria_label: t("ui_review.oldal").to_string(),
                            block: true,
                        }
                    }
                } else {
                    h1 { class: "rv-title", {t("ui_review.edzesek_attekintese")} }
                }
                button {
                    class: if compact { "btn btn--icon" } else { "btn btn--sm" },
                    "aria-label": t("ui_review.edzes_importalasa"),
                    title: t("ui_review.edzes_importalasa").to_string(),
                    disabled: busy_now || route_now.is_none() && !no_rides && current_id.is_none(),
                    onclick: move |_| on_import.call(()),
                    Icon { name: IconName::Upload, size: if compact { 20 } else { 16 } }
                    if !compact {
                        {t("ui_review.edzes_importalasa")}
                    }
                }
            }
            if !compact {
                div { class: "rv-views",
                    Segmented {
                        options: views.clone(),
                        value: view_now.clone(),
                        on_change: move |value: String| view.set(value),
                        aria_label: t("ui_review.oldal").to_string(),
                        small: true,
                    }
                }
            }
            if compact && !power_view {
                div { class: "rv-tabs",
                    Segmented {
                        options: vec![
                            ("rides".to_string(), t("ui_review.edzesek").to_string()),
                            ("map".to_string(), t("ui_review.terkep").to_string()),
                            ("chart".to_string(), t("ui_review.grafikon").to_string()),
                            ("table".to_string(), t("ui_review.szakasz").to_string()),
                        ],
                        value: tab_now.clone(),
                        on_change: move |value: String| tab.set(value),
                        aria_label: t("ui_review.nezet").to_string(),
                        small: true,
                        block: true,
                    }
                }
            }
            if !notes_now.is_empty() {
                div { class: "rv-notes", role: "status",
                    ul {
                        for (i , note) in notes_now.iter().enumerate() {
                            li { key: "{i}", "{note}" }
                        }
                    }
                    button {
                        class: "btn btn--ghost btn--icon btn--sm",
                        "aria-label": t("ui_review.uzenetek_bezarasa"),
                        onclick: move |_| on_dismiss_notes.call(()),
                        Icon { name: IconName::Close, size: 14 }
                    }
                }
            }
            if busy_now {
                div { class: "rv-busy", role: "status", {t("ui_review.betoltes")} }
            }
            if power_view {
                PowerView { analysis, ftp_w, loaded, on_apply_ftp }
            } else if no_rides {
                div { class: "rv-empty",
                    Icon { name: IconName::Bike, size: 28 }
                    div { class: "rv-empty-title", {t("ui_review.meg_nincs_mentett_edzes")} }
                    div { class: "rv-empty-hint",
                        {t("ui_review.az_edzesek_a_befejezesuk_utan_itt_jelennek_meg_mas_a")}
                    }
                }
            } else {
                div { class: "rv-body",
                    aside { class: "rv-side",
                      section { class: "rv-panel", "aria-label": t("ui_review.edzesek"),
                        {panel_bar(t("ui_review.edzesek"), compact)}
                        div { class: "rv-panel-body rv-side-list",
                        div { class: "rv-side-routes",
                            label { class: "t-caption", r#for: "rv-route", {t("ui_review.utvonal")} }
                            select {
                                id: "rv-route",
                                class: "rv-select",
                                value: current_id.clone().unwrap_or_default(),
                                onchange: move |event: FormEvent| on_route.call(event.value()),
                                for r in routes_now.iter() {
                                    option {
                                        key: "{r.route_id}",
                                        value: "{r.route_id}",
                                        selected: Some(&r.route_id) == current_id.as_ref(),
                                        {tf(
                                            "ui_review.r_route_name_r_rides_len_edzes",
                                            &[
                                                ("name", &r.route_name),
                                                ("n", &r.rides.len().to_string()),
                                            ],
                                        )}
                                    }
                                }
                            }
                        }
                        ul { class: "rv-rides",
                            for meta in current_rides.iter() {
                                {
                                    let stem = meta.stem.clone();
                                    let ticked = selected_now.contains(&stem);
                                    let slot = slot_of(&stem);
                                    let can_tick = ticked || selected_now.len() < SLOTS;
                                    let tags = kind_tags(meta);
                                    let asking = confirm_delete.read().as_deref() == Some(stem.as_str());
                                    let swatch = slot.map(var_of).unwrap_or_default();
                                    rsx! {
                                        li { key: "{stem}", class: if ticked { "rv-ride is-on" } else { "rv-ride" },
                                            button {
                                                class: "rv-ride-main",
                                                disabled: !can_tick,
                                                "aria-pressed": "{ticked}",
                                                title: if can_tick { "" } else { {t("ui_review.legfeljebb_negy_edzest_lehet_egyszerre_osszehasonlit")} },
                                                onclick: {
                                                    let stem = stem.clone();
                                                    move |_| on_toggle.call(stem.clone())
                                                },
                                                span { class: "rv-swatch", style: "--c: {swatch}",
                                                    if let Some(slot) = slot {
                                                        "{letter(slot)}"
                                                    }
                                                }
                                                span { class: "rv-ride-text",
                                                    span { class: "rv-ride-date", "{chart::fmt_stamp(meta.started_unix_s)}" }
                                                    span { class: "rv-ride-meta",
                                                        "{fmt_duration(meta.elapsed_s)} · {stat_opt(meta.avg_power_w)} W · {stat_opt(meta.avg_hr)} bpm"
                                                        if !meta.complete {
                                                            {t("ui_review.reszleges")}
                                                        }
                                                    }
                                                    if !tags.is_empty() {
                                                        span { class: "rv-tags",
                                                            for tag in tags.iter() {
                                                                span { key: "{tag}", class: "tag", "{tag}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "rv-ride-actions",
                                                if meta.power_estimated && ticked {
                                                    button {
                                                        class: "btn btn--ghost btn--icon btn--sm",
                                                        title: t("ui_review.a_becsles_beallitasai").to_string(),
                                                        "aria-label": t("ui_review.a_becsles_beallitasai"),
                                                        onclick: {
                                                            let stem = stem.clone();
                                                            let rider = rider_now.clone();
                                                            move |_| {
                                                                est_rider_kg.set(rider.rider_kg);
                                                                est_bike_kg.set(rider.bike_kg);
                                                                est_crr.set(rider.crr);
                                                                est_custom.set(false);
                                                                estimating.set(Some(stem.clone()));
                                                            }
                                                        },
                                                        Icon { name: IconName::Sliders, size: 16 }
                                                    }
                                                }
                                                if asking {
                                                    button {
                                                        class: "btn btn--danger btn--sm",
                                                        onclick: {
                                                            let stem = stem.clone();
                                                            move |_| {
                                                                confirm_delete.set(None);
                                                                on_delete.call(stem.clone());
                                                            }
                                                        },
                                                        {t("ui_review.torles")}
                                                    }
                                                    button {
                                                        class: "btn btn--ghost btn--sm",
                                                        onclick: move |_| confirm_delete.set(None),
                                                        {t("ui_review.megse")}
                                                    }
                                                } else {
                                                    button {
                                                        class: "btn btn--ghost btn--icon btn--sm",
                                                        title: t("ui_review.edzes_torlese").to_string(),
                                                        "aria-label": t("ui_review.edzes_torlese"),
                                                        onclick: {
                                                            let stem = stem.clone();
                                                            move |_| confirm_delete.set(Some(stem.clone()))
                                                        },
                                                        Icon { name: IconName::Trash, size: 16 }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        }
                      }
                        if let Some((stem, current)) = editing.clone() {
                            div { class: "rv-panel rv-estimate", role: "group", "aria-label": t("ui_review.a_becsult_watt_beallitasai"),
                                div { class: "rv-panel-bar rv-estimate-head",
                                    h2 { class: "rv-panel-title", {t("ui_review.becsult_watt")} }
                                    button {
                                        class: "rv-panel-close",
                                        "aria-label": t("ui_review.bezaras"),
                                        title: t("ui_review.bezaras").to_string(),
                                        onclick: move |_| estimating.set(None),
                                        Icon { name: IconName::Close, size: 14 }
                                    }
                                }
                                div { class: "rv-panel-body rv-estimate-body",
                                p { class: "rv-estimate-lede",
                                    {t("ui_review.a_fajlban_nem_volt_teljesitmenyadat_ezert_a_szimulac")}
                                }
                                if let Some(current) = current {
                                    p { class: "rv-estimate-now",
                                        {tf(
                                            "ui_review.most_chart_hu_current_rider_kg_1_kg_versenyzo_chart_",
                                            &[
                                                ("rider", &chart::hu(current.rider_kg, 1)),
                                                ("bike", &chart::hu(current.bike_kg, 1)),
                                                ("crr", &chart::hu(current.crr, 4)),
                                            ],
                                        )}
                                        if current.from_profile {
                                            {t("ui_review.a_sajat_adataid")}
                                        } else {
                                            {t("ui_review.egyeni_ertekek")}
                                        }
                                    }
                                }
                                Segmented {
                                    options: vec![
                                        ("own".to_string(), t("ui_review.sajat_adataim").to_string()),
                                        ("custom".to_string(), t("ui_review.egyeni_ertekek_2").to_string()),
                                    ],
                                    value: if *est_custom.read() { "custom".to_string() } else { "own".to_string() },
                                    on_change: move |value: String| est_custom.set(value == "custom"),
                                    aria_label: t("ui_review.kinek_az_adataival_szamoljunk").to_string(),
                                    small: true,
                                    block: true,
                                }
                                if *est_custom.read() {
                                    div { class: "rv-estimate-fields",
                                        NumberField {
                                            label: t("ui_review.testsuly").to_string(),
                                            value: *est_rider_kg.read(),
                                            min: 30.0,
                                            max: 200.0,
                                            step: 0.5,
                                            unit: "kg".to_string(),
                                            on_change: move |v: f64| est_rider_kg.set(v),
                                        }
                                        NumberField {
                                            label: t("ui_review.kerekpar").to_string(),
                                            value: *est_bike_kg.read(),
                                            min: 3.0,
                                            max: 30.0,
                                            step: 0.1,
                                            unit: "kg".to_string(),
                                            on_change: move |v: f64| est_bike_kg.set(v),
                                        }
                                        NumberField {
                                            label: t("ui_review.gordulesi_ellenallas_crr").to_string(),
                                            value: *est_crr.read(),
                                            min: 0.002,
                                            max: 0.02,
                                            step: 0.001,
                                            hint: t("ui_review.jo_aszfalton_kb_0_004_kavicsos_utra_nincs_kulon_mode").to_string(),
                                            on_change: move |v: f64| est_crr.set(v),
                                        }
                                    }
                                } else {
                                    p { class: "rv-estimate-now",
                                        {tf(
                                            "ui_review.chart_hu_rider_now_rider_kg_1_kg_chart_hu_rider_now_",
                                            &[
                                                ("rider", &chart::hu(rider_now.rider_kg, 1)),
                                                ("bike", &chart::hu(rider_now.bike_kg, 1)),
                                                ("crr", &chart::hu(rider_now.crr, 4)),
                                            ],
                                        )}
                                    }
                                }
                                p { class: "rv-estimate-now", {t("ui_review.a_cda_t_a_hajtaslanc_hatasfokat_es_a_levego_surusege")} }
                                button {
                                    class: "btn btn--primary btn--sm",
                                    disabled: busy_now,
                                    onclick: {
                                        let rider = rider_now.clone();
                                        move |_| {
                                            let params = if *est_custom.read() {
                                                custom_params(&rider, *est_rider_kg.read(), *est_bike_kg.read(), *est_crr.read())
                                            } else {
                                                profile_params(&rider)
                                            };
                                            on_reestimate.call((stem.clone(), params));
                                        }
                                    },
                                    Icon { name: IconName::Refresh, size: 16 }
                                    {t("ui_review.ujraszamolas")}
                                }
                                }
                            }
                        }
                    }
                    main { class: "rv-main",
                        section { class: "rv-panel rv-map-panel", "aria-label": t("ui_review.terkep"),
                            {panel_bar(t("ui_review.terkep"), compact)}
                            div { class: "rv-map",
                                MapView {
                                    route,
                                    snapshot,
                                    style: map_style,
                                    follow,
                                    on_style: move |style| on_map_style.call(style),
                                    on_follow: move |_| {},
                                    overlay,
                                }
                            }
                        }
                        section { class: "rv-panel rv-charts", "aria-label": t("ui_review.grafikon"),
                            {panel_bar(t("ui_review.szakasz"), compact)}
                            div { class: "rv-panel-body rv-charts-body",
                            div { class: "rv-section-bar",
                                button {
                                    class: if range.read().is_none() { "rv-chip is-on" } else { "rv-chip" },
                                    onclick: move |_| set_range(None),
                                    {t("ui_review.teljes_utvonal")}
                                }
                                for (index , from , to) in climbs.iter().copied() {
                                    button {
                                        key: "climb-{index}",
                                        class: if *range.read() == Some((from, to)) { "rv-chip is-on" } else { "rv-chip" },
                                        title: t("ui_review.rakozelites_erre_az_emelkedore").to_string(),
                                        onclick: move |_| set_range(Some((from, to))),
                                        Icon { name: IconName::Mountain, size: 14 }
                                        {tf(
                                            "ui_review.index_1_emelkedo_km_text_to_from",
                                            &[
                                                ("n", &(index + 1).to_string()),
                                                ("km", &km_text(to - from)),
                                            ],
                                        )}
                                    }
                                }
                                if let Some(text) = range_text {
                                    span { class: "rv-range-text", "{text}" }
                                } else {
                                    span { class: "rv-range-text", {t("ui_review.huzd_vegig_az_egeret_vagy_az_ujjad_a_profilon_vagy_a")} }
                                }
                            }
                            if let Some(route_ref) = route_now.clone() {
                                Overview {
                                    route: route_ref,
                                    theme: theme_now,
                                    range: *range.read(),
                                    cursor: cursor_now,
                                    on_cursor: move |d| cursor.set(d),
                                    on_select: move |(a, b)| set_range(Some((a, b))),
                                }
                            } else {
                                div { class: "rv-note", {t("ui_review.ennek_az_utvonalnak_a_fajlja_mar_nem_erheto_el_ezert")} }
                            }
                            }
                        }
                        section { class: "rv-panel rv-charts", "aria-label": t("ui_review.mit_mutasson_a_grafikon"),
                            {panel_bar(t("ui_review.grafikon"), compact)}
                            div { class: "rv-panel-body rv-charts-body",
                            div { class: "rv-chart-head",
                                Segmented {
                                    options: metric_options.clone(),
                                    value: metric_now.key().to_string(),
                                    on_change: move |value: String| {
                                        if let Some(m) = Metric::from_key(&value) {
                                            metric.set(m);
                                        }
                                    },
                                    aria_label: t("ui_review.mit_mutasson_a_grafikon").to_string(),
                                    small: !compact,
                                    block: compact,
                                }
                                span { class: "rv-unit",
                                    if compact {
                                        "{metric_now.title()}"
                                    } else {
                                        "{metric_now.unit()}"
                                    }
                                }
                            }
                            div { class: "rv-legend",
                                for (slot , stamp , value) in legend.iter() {
                                    span { key: "legend-{slot}", class: "rv-legend-item",
                                        span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(*slot)}", "{letter(*slot)}" }
                                        span { class: "rv-legend-date", "{stamp}" }
                                        span { class: "rv-legend-value num", "{value}" }
                                    }
                                }
                                if records.is_empty() {
                                    span { class: "rv-legend-hint", {t("ui_review.jelolj_ki_legalabb_egy_edzest_a_listaban")} }
                                }
                            }
                            DistanceChart {
                                metric: metric_now,
                                geometry: geometry.clone(),
                                cursor: cursor_now,
                                on_cursor: move |d| cursor.set(d),
                                on_select: move |(a, b)| set_range(Some((a, b))),
                            }
                            }
                        }
                        section { class: "rv-panel rv-stats", "aria-label": t("ui_review.a_szakasz_adatai"),
                            {panel_bar(t("ui_review.a_szakasz_adatai"), compact)}
                            div { class: "rv-panel-body rv-stats-body",
                            if rows.is_empty() || rows.iter().all(|r| r.stats.is_none()) {
                                div { class: "rv-note",
                                    if records.is_empty() {
                                        {t("ui_review.jelolj_ki_egy_vagy_tobb_edzest_a_listaban")}
                                    } else if table_span.is_none() {
                                        {t("ui_review.a_kijelolt_edzeseknek_nincs_kozos_szakasza_jelolj_ki")}
                                    } else {
                                        {t("ui_review.a_kijelolt_szakaszt_nem_erintette_vegig_egyik_edzes_")}
                                    }
                                }
                            } else if compact {
                                // phone: one row per number, one column per ride, so up to four
                                // rides fit the width without scrolling sideways
                                div { class: "rv-table-wrap",
                                    table { class: "rv-table rv-table--by-metric",
                                        thead {
                                            tr {
                                                th { scope: "col", class: "rv-dim", {t("ui_review.edzes")} }
                                                for row in rows.iter() {
                                                    th { key: "col-{row.slot}", scope: "col",
                                                        title: "{legend.get(row.slot).map(|l| l.1.clone()).unwrap_or_default()}",
                                                        span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(row.slot)}", "{letter(row.slot)}" }
                                                    }
                                                }
                                            }
                                        }
                                        tbody {
                                            for (i , (head , cells)) in by_metric.iter().enumerate() {
                                                tr { key: "m-{i}",
                                                    th { scope: "row", "{head}" }
                                                    for (j , cell) in cells.iter().enumerate() {
                                                        td { key: "m-{i}-{j}", class: "num", "{cell}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if reference_is_estimated {
                                    p { class: "rv-footnote",
                                        {t("ui_review.a_jelu_watt_ertekek_becsultek_a_sebessegbol_es_a_lej")}
                                    }
                                }
                            } else {
                                div { class: "rv-table-wrap",
                                    table { class: "rv-table",
                                        thead {
                                            tr {
                                                th { {t("ui_review.edzes")} }
                                                for head in table_head().iter() {
                                                    th { key: "{head}", "{head}" }
                                                }
                                                th { {t("ui_review.kulonbseg")} }
                                            }
                                        }
                                        tbody {
                                            for row in rows.iter() {
                                                tr { key: "row-{row.slot}",
                                                    th { scope: "row",
                                                        span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(row.slot)}", "{letter(row.slot)}" }
                                                        "{legend.get(row.slot).map(|l| l.1.clone()).unwrap_or_default()}"
                                                    }
                                                    if let Some(stats) = row.stats {
                                                        for (i , cell) in stats_cells(&stats, records.get(row.slot).is_some_and(|r| r.power_estimated)).into_iter().enumerate() {
                                                            td { key: "c-{i}", class: "num", "{cell}" }
                                                        }
                                                        td { class: "num",
                                                            if row.slot == 0 {
                                                                "{DASH}"
                                                            } else if let Some(delta) = row.time_delta_s {
                                                                "{fmt_delta_duration(delta)}"
                                                            } else {
                                                                "{DASH}"
                                                            }
                                                        }
                                                    } else {
                                                        td { colspan: "{table_head().len() + 1}", class: "rv-dim", {t("ui_review.ezt_a_szakaszt_az_edzes_nem_erintette_vegig")} }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if reference_is_estimated {
                                    p { class: "rv-footnote",
                                        {t("ui_review.a_jelu_watt_ertekek_becsultek_a_sebessegbol_es_a_lej")}
                                    }
                                }
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
