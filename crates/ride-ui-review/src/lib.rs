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
        tags.push("Importált");
    }
    if meta.simulated {
        tags.push("Szimulált");
    }
    if meta.power_estimated {
        tags.push("Becsült watt");
    }
    tags
}

// ───────────────────────────── selection layer ─────────────────────────────

/// Transparent layer over a plot: reports the distance under the pointer and, after a drag, the
/// section the user swept. Works with a mouse and with a finger (`touch-action: none`).
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
                measure_width(mounted, width);
                if let Some(frac) = pointer_frac(event.element_coordinates().x, *width.peek()) {
                    press.set(Some(frac));
                    live.set(Some(frac));
                }
            },
            onpointermove: move |event: PointerEvent| {
                if let Some(frac) = pointer_frac(event.element_coordinates().x, *width.peek()) {
                    on_cursor.call(Some(x0 + frac * (x1 - x0)));
                    if press.peek().is_some() {
                        live.set(Some(frac));
                    }
                }
            },
            onpointerup: move |event: PointerEvent| {
                let end = pointer_frac(event.element_coordinates().x, *width.peek());
                finish(end);
            },
            onpointercancel: move |_| {
                press.set(None);
                live.set(None);
            },
            onpointerleave: move |_| {
                on_cursor.call(None);
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
    let ticks = geom::ruler_ticks(0.0, total);
    let scale = geom::XScale::new(0.0, total);

    rsx! {
        div { class: "rv-overview", "aria-label": "A teljes útvonal magassági profilja",
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
                            span { class: "rv-tick-label", "{label}" }
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
                for v in g.ticks.iter() {
                    div {
                        key: "grid-{v:.1}",
                        class: "rv-grid",
                        style: "top: {pct_text(y_pct(*v))}",
                        span { class: "rv-ytick", "{metric.value_text(*v)}" }
                    }
                }
                if empty {
                    div { class: "rv-note", "Ehhez a metrikához nincs adat a kijelölt edzésekben." }
                }
                if let Some(d) = cursor.filter(|d| *d >= g.x0 && *d <= g.x1) {
                    div { class: "rv-cursor", style: "left: {pct_text(scale.pct(d))}" }
                }
                HitLayer { x0: g.x0, x1: g.x1, on_cursor, on_select }
            }
            div { class: "rv-xaxis", "aria-hidden": "true",
                for d in x_ticks.iter() {
                    span {
                        key: "x-{d:.0}",
                        class: "rv-xtick",
                        style: "left: {pct_text(scale.pct(*d))}",
                        "{geom::tick_label(*d, step)}"
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

const TABLE_HEAD: [&str; 9] = [
    "Idő",
    "Átl. seb. (km/h)",
    "Átl. watt",
    "NP (W)",
    "Átl. pulzus",
    "Max. pulzus",
    "Kadencia",
    "Emelkedés",
    "VAM (m/h)",
];

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
    let fit_seq = use_signal(|| 0u64);
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

    // legend: one chip per ride with the value under the cursor
    let legend: Vec<(usize, String, String)> = records
        .iter()
        .enumerate()
        .map(|(slot, record)| {
            let value = cursor_now
                .and_then(|d| chart::value_at(&smoothed.read(), slot, metric_now, d))
                .map(|v| format!("{} {}", metric_now.value_text(v), metric_now.unit()))
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

    let page_class = if compact {
        format!("rv-page rv-page--compact rv-tab-{tab_now}")
    } else {
        "rv-page".to_string()
    };
    let no_rides = routes_now.is_empty();
    let range_text = (*range.read()).map(|(a, b)| {
        format!(
            "Kijelölt szakasz: {} – {} ({})",
            km_text(a),
            km_text(b),
            km_text(b - a)
        )
    });
    let editing: Option<(String, Option<EstimateParams>)> = estimating.read().clone().map(|stem| {
        let current = records
            .iter()
            .find(|r| stem_of(r) == stem)
            .and_then(|r| r.estimate);
        (stem, current)
    });

    rsx! {
        div { class: "{page_class}", role: "region", "aria-label": "Edzések áttekintése",
            header { class: "rv-head",
                button {
                    class: "btn btn--ghost btn--sm",
                    "aria-label": "Vissza az edzéshez",
                    onclick: move |_| on_close.call(()),
                    Icon { name: IconName::ChevronLeft, size: 16 }
                    "Vissza"
                }
                h1 { class: "rv-title", "Edzések áttekintése" }
                button {
                    class: "btn btn--sm",
                    disabled: busy_now || route_now.is_none() && !no_rides && current_id.is_none(),
                    onclick: move |_| on_import.call(()),
                    Icon { name: IconName::Upload, size: 16 }
                    "Edzés importálása"
                }
            }
            div { class: "rv-views",
                Segmented {
                    options: vec![
                        ("compare".to_string(), "Összehasonlítás".to_string()),
                        ("power".to_string(), "Teljesítmény".to_string()),
                    ],
                    value: view_now.clone(),
                    on_change: move |value: String| view.set(value),
                    aria_label: "Oldal".to_string(),
                    small: true,
                }
            }
            if compact && !power_view {
                div { class: "rv-tabs",
                    Segmented {
                        options: vec![
                            ("rides".to_string(), "Edzések".to_string()),
                            ("map".to_string(), "Térkép".to_string()),
                            ("chart".to_string(), "Grafikon".to_string()),
                            ("table".to_string(), "Szakasz".to_string()),
                        ],
                        value: tab_now.clone(),
                        on_change: move |value: String| tab.set(value),
                        aria_label: "Nézet".to_string(),
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
                        "aria-label": "Üzenetek bezárása",
                        onclick: move |_| on_dismiss_notes.call(()),
                        Icon { name: IconName::Close, size: 14 }
                    }
                }
            }
            if busy_now {
                div { class: "rv-busy", role: "status", "Betöltés…" }
            }
            if power_view {
                PowerView { analysis, ftp_w, loaded, on_apply_ftp }
            } else if no_rides {
                div { class: "rv-empty",
                    Icon { name: IconName::Bike, size: 28 }
                    div { class: "rv-empty-title", "Még nincs mentett edzés" }
                    div { class: "rv-empty-hint",
                        "Az edzések a befejezésük után itt jelennek meg. Más alkalmazásból (pl. Strava) exportált GPX vagy TCX fájlt is importálhatsz."
                    }
                }
            } else {
                div { class: "rv-body",
                    aside { class: "rv-side",
                        div { class: "rv-side-routes",
                            label { class: "t-caption", r#for: "rv-route", "Útvonal" }
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
                                        "{r.route_name} · {r.rides.len()} edzés"
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
                                                title: if can_tick { "" } else { "Legfeljebb négy edzést lehet egyszerre összehasonlítani." },
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
                                                            " · részleges"
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
                                                        title: "A becslés beállításai",
                                                        "aria-label": "A becslés beállításai",
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
                                                        "Törlés"
                                                    }
                                                    button {
                                                        class: "btn btn--ghost btn--sm",
                                                        onclick: move |_| confirm_delete.set(None),
                                                        "Mégse"
                                                    }
                                                } else {
                                                    button {
                                                        class: "btn btn--ghost btn--icon btn--sm",
                                                        title: "Edzés törlése",
                                                        "aria-label": "Edzés törlése",
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
                        if let Some((stem, current)) = editing.clone() {
                            div { class: "rv-estimate", role: "group", "aria-label": "A becsült watt beállításai",
                                div { class: "rv-estimate-head",
                                    span { class: "t-title", "Becsült watt" }
                                    button {
                                        class: "btn btn--ghost btn--icon btn--sm",
                                        "aria-label": "Bezárás",
                                        onclick: move |_| estimating.set(None),
                                        Icon { name: IconName::Close, size: 14 }
                                    }
                                }
                                p { class: "rv-estimate-lede",
                                    "A fájlban nem volt teljesítményadat, ezért a szimuláció fizikáját visszafelé futtattuk: a sebességből és a lejtőből számoltuk ki, mennyi watt kellett hozzá. Szél és szélárnyék nélkül ez közelítés, a kerékpár és a versenyző adatai pedig sokat számítanak."
                                }
                                if let Some(current) = current {
                                    p { class: "rv-estimate-now",
                                        "Most: {chart::hu(current.rider_kg, 1)} kg versenyző + {chart::hu(current.bike_kg, 1)} kg kerékpár, Crr {chart::hu(current.crr, 4)}"
                                        if current.from_profile {
                                            " (a saját adataid)"
                                        } else {
                                            " (egyéni értékek)"
                                        }
                                    }
                                }
                                Segmented {
                                    options: vec![
                                        ("own".to_string(), "Saját adataim".to_string()),
                                        ("custom".to_string(), "Egyéni értékek".to_string()),
                                    ],
                                    value: if *est_custom.read() { "custom".to_string() } else { "own".to_string() },
                                    on_change: move |value: String| est_custom.set(value == "custom"),
                                    aria_label: "Kinek az adataival számoljunk".to_string(),
                                    small: true,
                                    block: true,
                                }
                                if *est_custom.read() {
                                    div { class: "rv-estimate-fields",
                                        NumberField {
                                            label: "Testsúly".to_string(),
                                            value: *est_rider_kg.read(),
                                            min: 30.0,
                                            max: 200.0,
                                            step: 0.5,
                                            unit: "kg".to_string(),
                                            on_change: move |v: f64| est_rider_kg.set(v),
                                        }
                                        NumberField {
                                            label: "Kerékpár".to_string(),
                                            value: *est_bike_kg.read(),
                                            min: 3.0,
                                            max: 30.0,
                                            step: 0.1,
                                            unit: "kg".to_string(),
                                            on_change: move |v: f64| est_bike_kg.set(v),
                                        }
                                        NumberField {
                                            label: "Gördülési ellenállás (Crr)".to_string(),
                                            value: *est_crr.read(),
                                            min: 0.002,
                                            max: 0.02,
                                            step: 0.001,
                                            hint: "Jó aszfalton kb. 0,004. Kavicsos útra nincs külön modell.".to_string(),
                                            on_change: move |v: f64| est_crr.set(v),
                                        }
                                    }
                                } else {
                                    p { class: "rv-estimate-now",
                                        "{chart::hu(rider_now.rider_kg, 1)} kg + {chart::hu(rider_now.bike_kg, 1)} kg kerékpár, Crr {chart::hu(rider_now.crr, 4)} (Beállítások)"
                                    }
                                }
                                p { class: "rv-estimate-now", "A CdA-t, a hajtáslánc hatásfokát és a levegő sűrűségét mindig a Beállításokból vesszük." }
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
                                    "Újraszámolás"
                                }
                            }
                        }
                    }
                    main { class: "rv-main",
                        section { class: "rv-map", "aria-label": "Térkép",
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
                        section { class: "rv-charts",
                            div { class: "rv-section-bar",
                                button {
                                    class: if range.read().is_none() { "rv-chip is-on" } else { "rv-chip" },
                                    onclick: move |_| set_range(None),
                                    "Teljes útvonal"
                                }
                                for (index , from , to) in climbs.iter().copied() {
                                    button {
                                        key: "climb-{index}",
                                        class: if *range.read() == Some((from, to)) { "rv-chip is-on" } else { "rv-chip" },
                                        title: "Ráközelítés erre az emelkedőre",
                                        onclick: move |_| set_range(Some((from, to))),
                                        Icon { name: IconName::Mountain, size: 14 }
                                        "{index + 1}. emelkedő · {km_text(to - from)}"
                                    }
                                }
                                if let Some(text) = range_text {
                                    span { class: "rv-range-text", "{text}" }
                                } else {
                                    span { class: "rv-range-text", "Húzd végig az egeret (vagy az ujjad) a profilon vagy a grafikonon egy szakasz kijelöléséhez." }
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
                                div { class: "rv-note", "Ennek az útvonalnak a fájlja már nem érhető el, ezért a térkép és a profil nem rajzolható. Töltsd be újra az Útvonal ablakban." }
                            }
                            div { class: "rv-chart-head",
                                Segmented {
                                    options: Metric::ALL
                                        .iter()
                                        .map(|m| (m.key().to_string(), m.title().to_string()))
                                        .collect::<Vec<_>>(),
                                    value: metric_now.key().to_string(),
                                    on_change: move |value: String| {
                                        if let Some(m) = Metric::from_key(&value) {
                                            metric.set(m);
                                        }
                                    },
                                    aria_label: "Mit mutasson a grafikon".to_string(),
                                    small: true,
                                }
                                span { class: "rv-unit", "{metric_now.unit()}" }
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
                                    span { class: "rv-legend-hint", "Jelölj ki legalább egy edzést a listában." }
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
                        section { class: "rv-stats", "aria-label": "A szakasz adatai",
                            if rows.is_empty() || rows.iter().all(|r| r.stats.is_none()) {
                                div { class: "rv-note",
                                    if records.is_empty() {
                                        "Jelölj ki egy vagy több edzést a listában."
                                    } else if table_span.is_none() {
                                        "A kijelölt edzéseknek nincs közös szakasza. Jelölj ki egy szakaszt, amelyet mindegyik érint."
                                    } else {
                                        "A kijelölt szakaszt nem érintette végig egyik edzés sem."
                                    }
                                }
                            } else {
                                div { class: "rv-table-wrap",
                                    table { class: "rv-table",
                                        thead {
                                            tr {
                                                th { "Edzés" }
                                                for head in TABLE_HEAD.iter() {
                                                    th { key: "{head}", "{head}" }
                                                }
                                                th { "Különbség" }
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
                                                        td { colspan: "{TABLE_HEAD.len() + 1}", class: "rv-dim", "Ezt a szakaszt az edzés nem érintette végig." }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if reference_is_estimated {
                                    p { class: "rv-footnote",
                                        "A ~ jelű watt-értékek becsültek: a sebességből és a lejtőből számoltuk vissza őket."
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
