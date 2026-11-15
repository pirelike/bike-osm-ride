//! The "Teljesítmény" view of the review page: fitness and form, the FTP the last 90 days
//! suggest, the power curve of a chosen period, the time in zones and the weekly / monthly load.
//!
//! The numbers come from `ride_sim::power_curve` and `ride_sim::fitness` (pure, tested); this
//! file only lays them out. Only measured watts reach the curve and the FTP, which the page says
//! out loud: an FTP derived from a physics model's own watts would confirm nothing.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{HrZoneModel, RideRecord, RiderSettings};
use ride_sim::fitness::FtpMethod;
use ride_sim::power_curve::{
    self, Analysis, Bucket, Period, Window, DURATIONS, FTP_LOOKBACK_DAYS, FTP_WINDOW, TABLE_ROWS,
};
use ride_ui_kit::format::{fmt_duration, fmt_km};
use ride_ui_kit::Segmented;

use crate::plot::{self, Bars, Line, PlotFrame};
use crate::{chart, letter, var_of, DASH};
use ride_i18n::{decimal, t, tf};

/// "5 mp", "1 perc", ...
pub fn duration_label(seconds: u32) -> String {
    if seconds < 60 {
        tf(
            "ui_review.power.duration_seconds",
            &[("n", &seconds.to_string())],
        )
    } else {
        tf(
            "ui_review.power.duration_minutes",
            &[("n", &(seconds / 60).to_string())],
        )
    }
}

/// `11.13.` for the week or month starting at `unix_s`; a month is `2026-11`.
pub fn period_label(unix_s: f64, period: Period) -> String {
    if !unix_s.is_finite() {
        return DASH.to_string();
    }
    let (year, month, day) = power_curve::civil_from_days((unix_s / 86_400.0).floor() as i64);
    match period {
        Period::Week => format!("{month}.{day:02}."),
        Period::Month => format!("{year}-{month:02}"),
    }
}

/// The label of a period choice.
pub fn window_label(window: Window) -> &'static str {
    match window {
        Window::Days(28) => t("ui_review.power.window_28"),
        Window::Days(42) => t("ui_review.power.window_42"),
        Window::Days(90) => t("ui_review.power.window_90"),
        Window::Days(365) => t("ui_review.power.window_365"),
        Window::Days(_) => t("ui_review.power.window_90"),
        Window::All => t("ui_review.power.window_all"),
    }
}

pub(crate) fn watts_text(value: Option<f64>) -> String {
    match value {
        Some(w) if w.is_finite() => format!("{w:.0}"),
        _ => DASH.to_string(),
    }
}

/// W/kg with two decimals, "—" without a usable mass.
pub(crate) fn wkg_text(watts: Option<f64>, kg: f64) -> String {
    match watts {
        Some(w) if w.is_finite() && kg.is_finite() && kg > 0.0 => decimal(w / kg, 2),
        _ => DASH.to_string(),
    }
}

/// Height of a bar, percent of the tallest one (at least a sliver when there is something).
pub(crate) fn bar_pct(value: f64, max: f64) -> f64 {
    if !(value.is_finite() && max.is_finite()) || max <= 0.0 || value <= 0.0 {
        return 0.0;
    }
    (value / max * 100.0).clamp(2.0, 100.0)
}

/// The name of a heart-rate zone model.
pub fn hr_model_label(model: HrZoneModel) -> &'static str {
    match model {
        HrZoneModel::MaxHr => t("ui_review.power.hr_model_max"),
        HrZoneModel::Reserve => t("ui_review.power.hr_model_reserve"),
        HrZoneModel::Lthr => t("ui_review.power.hr_model_lthr"),
    }
}

/// Seconds per zone as horizontal bars with their share (`zone_var(i)` colours zone `i + 1`).
#[component]
pub fn ZoneBars(secs: Vec<f64>, prefix: String, aria: String) -> Element {
    let total: f64 = secs.iter().filter(|s| s.is_finite()).sum();
    let max = secs.iter().copied().fold(0.0, f64::max);
    rsx! {
        div { class: "rv-zones", role: "img", "aria-label": "{aria}",
            for (i , s) in secs.iter().enumerate() {
                {
                    let share = if total > 0.0 { s / total * 100.0 } else { 0.0 };
                    let width = bar_pct(*s, max);
                    rsx! {
                        div { key: "{prefix}{i}", class: "rv-zone",
                            span { class: "rv-zone-name", "{prefix}{i + 1}" }
                            span { class: "rv-zone-bar",
                                i { style: "width: {width:.1}%; background: var(--z{i + 1})" }
                            }
                            span { class: "rv-zone-time num", "{fmt_duration(*s)}" }
                            span { class: "rv-zone-pct num", "{share:.0} %" }
                        }
                    }
                }
            }
        }
    }
}

/// The curve on a logarithmic time axis: the period's best, the best ever (dashed) and the
/// ticked rides.
fn curve_plot(analysis: &Analysis, ticked: &[(usize, Vec<Option<f64>>)]) -> (String, f64, f64) {
    let x = |seconds: u32| f64::from(seconds).ln();
    let series = |values: Vec<Option<f64>>, color: String, dash: &'static str, width: f64| Line {
        color,
        dash,
        width,
        points: DURATIONS
            .iter()
            .zip(values)
            .map(|(t, v)| (x(*t), v))
            .collect(),
    };
    let mut lines = vec![
        series(
            analysis
                .best_ever
                .iter()
                .map(|b| b.as_ref().map(|b| b.watts))
                .collect(),
            "var(--text-faint)".into(),
            "4 4",
            1.5,
        ),
        series(
            analysis
                .best
                .iter()
                .map(|b| b.as_ref().map(|b| b.watts))
                .collect(),
            "var(--accent)".into(),
            "",
            2.5,
        ),
    ];
    for (slot, values) in ticked {
        lines.push(series(
            values.clone(),
            var_of(*slot),
            chart::dash(*slot),
            1.5,
        ));
    }
    let (y0, y1) = plot::y_range(&lines, &[], true, 100.0);
    (plot::markup(&lines, &[]), y0, y1)
}

/// The performance management chart: fitness and fatigue as lines, form as bars.
fn pmc_plot(analysis: &Analysis) -> (String, f64, f64, f64, f64) {
    let first = analysis.pmc.first().map_or(0, |d| d.day);
    let x = |day: i64| (day - first) as f64;
    let ctl = Line {
        color: "var(--accent)".into(),
        dash: "",
        width: 2.5,
        points: analysis
            .pmc
            .iter()
            .map(|d| (x(d.day) + 0.5, Some(d.ctl)))
            .collect(),
    };
    let atl = Line {
        color: "var(--series-3)".into(),
        dash: "",
        width: 1.5,
        points: analysis
            .pmc
            .iter()
            .map(|d| (x(d.day) + 0.5, Some(d.atl)))
            .collect(),
    };
    let tsb = Bars {
        color: "var(--fill-3)".into(),
        bars: analysis
            .pmc
            .iter()
            .map(|d| (x(d.day) + 0.1, 0.8, d.tsb))
            .collect(),
    };
    let lines = [ctl, atl];
    let bars = [tsb];
    let (y0, y1) = plot::y_range(&lines, &bars, true, 20.0);
    let x1 = analysis.pmc.len().max(1) as f64;
    (plot::markup(&lines, &bars), 0.0, x1, y0, y1)
}

/// Everything the view shows besides its own period and trend choice.
#[component]
pub fn PowerView(
    analysis: ReadSignal<Arc<Analysis>>,
    /// The period of the curve, the zones and the load chart.
    window: ReadSignal<Window>,
    on_window: EventHandler<Window>,
    rider: ReadSignal<RiderSettings>,
    /// The ticked rides of the comparison view (their curves are shown next to the best ones).
    loaded: ReadSignal<Arc<Vec<Arc<RideRecord>>>>,
    on_apply_ftp: EventHandler<f64>,
    on_apply_max_hr: EventHandler<f64>,
    /// Opens a ride: `(route id, stem)`.
    on_open_ride: EventHandler<(String, String)>,
) -> Element {
    let mut period = use_signal(|| Period::Week);

    let analysis_now = analysis.read().clone();
    let rider_now = rider.read().clone();
    let current_ftp = rider_now.ftp_w;
    let kg = rider_now.rider_kg;
    let period_now = *period.read();
    let window_now = *window.read();

    // the curve of every ticked ride; the watts of an estimated one are marked with a tilde
    let ticked = use_memo(move || -> Vec<(usize, String, bool, Vec<Option<f64>>)> {
        loaded
            .read()
            .iter()
            .enumerate()
            .map(|(slot, record)| {
                (
                    slot,
                    chart::fmt_stamp(record.summary.started_unix_s),
                    record.power_estimated || record.simulated,
                    power_curve::mean_max(&record.points, &DURATIONS),
                )
            })
            .collect()
    });
    let ticked = ticked.read();

    let best_max = analysis_now
        .best
        .iter()
        .flatten()
        .map(|b| b.watts)
        .reduce(f64::max)
        .unwrap_or(0.0);
    let nothing_yet = analysis_now.best_ever.iter().all(Option::is_none);
    let nothing_in_window = analysis_now.best.iter().all(Option::is_none);

    let buckets: &[Bucket] = match period_now {
        Period::Week => &analysis_now.weeks,
        Period::Month => &analysis_now.months,
    };
    let load_max = buckets.iter().map(Bucket::load).fold(0.0, f64::max);
    let total_rides: usize = buckets.iter().map(|b| b.rides).sum();
    let total_time: f64 = buckets.iter().map(|b| b.time_s).sum();
    let total_load: f64 = buckets.iter().map(Bucket::load).sum();
    let total_dist: f64 = buckets.iter().map(|b| b.dist_m).sum();
    let period_name = match period_now {
        Period::Week => t("ui_review.power.het"),
        Period::Month => t("ui_review.power.honap"),
    };

    let windows: Vec<(String, String)> = Window::CHOICES
        .iter()
        .map(|w| (w.key(), window_label(*w).to_string()))
        .collect();

    // fitness and form
    let today = analysis_now.pmc.last().copied();
    let has_load = analysis_now.pmc.iter().any(|d| d.load > 0.0);
    let (pmc_markup, pmc_x0, pmc_x1, pmc_y0, pmc_y1) = pmc_plot(&analysis_now);
    let pmc_ticks: Vec<(f64, String)> = {
        let n = analysis_now.pmc.len();
        let step = (n / 5).max(1);
        analysis_now
            .pmc
            .iter()
            .enumerate()
            .filter(|(i, _)| i % step == 0 || *i + 1 == n)
            .map(|(i, d)| {
                (
                    i as f64 + 0.5,
                    period_label(d.day as f64 * 86_400.0, Period::Week),
                )
            })
            .collect()
    };
    let counts = analysis_now.load_counts;

    // the curve
    let curve_ticked: Vec<(usize, Vec<Option<f64>>)> = ticked
        .iter()
        .map(|(slot, _, _, c)| (*slot, c.clone()))
        .collect();
    let (curve_markup, curve_y0, curve_y1) = curve_plot(&analysis_now, &curve_ticked);
    let curve_x0 = f64::from(DURATIONS[0]).ln();
    let curve_x1 = f64::from(DURATIONS[DURATIONS.len() - 1]).ln();
    let curve_ticks: Vec<(f64, String)> = [5u32, 60, 300, 1_200, 3_600]
        .iter()
        .map(|s| (f64::from(*s).ln(), duration_label(*s)))
        .collect();

    // zones of the period
    let p = &analysis_now.period;
    let power_total: f64 = p.power_zone_secs.iter().sum();
    let polarised = (power_total > 0.0).then(|| {
        let easy = (p.power_zone_secs[0] + p.power_zone_secs[1]) / power_total * 100.0;
        let mid = p.power_zone_secs[2] / power_total * 100.0;
        (easy, mid, 100.0 - easy - mid)
    });
    let hr_total: f64 = p.hr_zone_secs.iter().sum();

    // the FTP of the last weeks
    let ftp_weeks_max = analysis_now
        .ftp_weeks
        .iter()
        .filter_map(|w| w.ftp_w)
        .fold(0.0, f64::max);
    let ftp_changes: Vec<_> = analysis_now
        .ftp_history
        .iter()
        .rev()
        .take(6)
        .copied()
        .collect();

    rsx! {
        div { class: "rv-power", role: "region", "aria-label": t("ui_review.chart.teljesitmeny"),
            div { class: "rv-power-bar",
                span { class: "t-caption", {t("ui_review.power.idoszak")} }
                Segmented {
                    options: windows,
                    value: window_now.key(),
                    on_change: move |value: String| {
                        if let Some(w) = Window::from_key(&value) {
                            on_window.call(w);
                        }
                    },
                    aria_label: t("ui_review.power.idoszak").to_string(),
                    small: true,
                }
            }
            if analysis_now.rides_pending > 0 {
                div { class: "rv-note", role: "status",
                    {tf(
                        "ui_review.power.analysis_now_rides_pending_regebbi_edzes_szamai_meg_",
                        &[("n", &analysis_now.rides_pending.to_string())],
                    )}
                }
            }

            // ── fitness, fatigue, form ──
            section { class: "rv-card", "aria-label": t("ui_review.power.forma_es_terheles"),
                h2 { class: "rv-card-title", {t("ui_review.power.forma_es_terheles")} }
                if has_load {
                    div { class: "rv-kpis",
                        div { class: "rv-kpi",
                            span { class: "rv-kpi-label", {t("ui_review.power.fitnesz_ctl")} }
                            span { class: "rv-kpi-value num rv-kpi--ctl", {today.map_or(DASH.to_string(), |d| format!("{:.0}", d.ctl))} }
                        }
                        div { class: "rv-kpi",
                            span { class: "rv-kpi-label", {t("ui_review.power.faradtsag_atl")} }
                            span { class: "rv-kpi-value num rv-kpi--atl", {today.map_or(DASH.to_string(), |d| format!("{:.0}", d.atl))} }
                        }
                        div { class: "rv-kpi",
                            span { class: "rv-kpi-label", {t("ui_review.power.forma_tsb")} }
                            span { class: "rv-kpi-value num", {today.map_or(DASH.to_string(), |d| format!("{:+.0}", d.tsb))} }
                        }
                        div { class: "rv-kpi",
                            span { class: "rv-kpi-label", {t("ui_review.power.heti_valtozas")} }
                            span { class: "rv-kpi-value num", {analysis_now.ramp_7d.map_or(DASH.to_string(), |r| format!("{}{}", if r >= 0.0 { "+" } else { "−" }, decimal(r.abs(), 1)))} }
                        }
                    }
                    PlotFrame {
                        markup: pmc_markup,
                        x0: pmc_x0,
                        x1: pmc_x1,
                        y0: pmc_y0,
                        y1: pmc_y1,
                        unit: "TSS".to_string(),
                        y_text: move |v: f64| format!("{v:.0}"),
                        x_ticks: pmc_ticks,
                        empty: None,
                        class: "rv-pmc".to_string(),
                    }
                    div { class: "rv-legend",
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--ctl" } {t("ui_review.power.fitnesz_ctl")} }
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--atl" } {t("ui_review.power.faradtsag_atl")} }
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--tsb" } {t("ui_review.power.forma_tsb")} }
                    }
                    p { class: "rv-card-note", {t("ui_review.power.pmc_magyarazat")} }
                    p { class: "rv-footnote",
                        {tf(
                            "ui_review.power.terheles_forrasai",
                            &[
                                ("power", &counts.power.to_string()),
                                ("hr", &counts.heart_rate.to_string()),
                                ("est", &counts.estimated.to_string()),
                                ("none", &counts.none.to_string()),
                            ],
                        )}
                    }
                } else {
                    div { class: "rv-note", {t("ui_review.power.meg_nincs_terheles")} }
                }
            }

            // ── FTP ──
            section { class: "rv-card rv-ftp", "aria-label": "FTP",
                h2 { class: "rv-card-title", "FTP" }
                if let Some(estimate) = analysis_now.ftp_estimate.clone() {
                    div { class: "rv-ftp-row",
                        span { class: "rv-ftp-value num", "{estimate.ftp_w:.0}" }
                        span { class: "rv-ftp-unit", "W" }
                        span { class: "rv-ftp-unit", "· {wkg_text(Some(estimate.ftp_w), kg)} W/kg" }
                        span { class: "rv-ftp-now", {tf("ui_review.power.jelenleg_beallitva_current_ftp_0_w", &[("ftp", &format!("{:.0}", current_ftp))])} }
                    }
                    p { class: "rv-card-note",
                        {tf("ui_review.power.ftp_alap", &[("days", &FTP_LOOKBACK_DAYS.to_string())])}
                        " "
                        {match (estimate.method, estimate.model) {
                            (FtpMethod::Model, Some(model)) => tf(
                                "ui_review.power.ftp_modellbol",
                                &[("p20", &format!("{:.0}", model.power_at(1_200.0)))],
                            ),
                            _ => tf(
                                "ui_review.power.a_legjobb_20_perced_estimate_best_20min_w_0_w_volt_c",
                                &[
                                    ("w", &watts_text(estimate.best_20min_w)),
                                    ("when", &estimate.best_20min_unix_s.map_or(DASH.to_string(), chart::fmt_stamp)),
                                ],
                            ),
                        }}
                    }
                    if let Some(model) = estimate.model {
                        p { class: "rv-card-note",
                            {tf(
                                "ui_review.power.cp_modell",
                                &[
                                    ("cp", &format!("{:.0}", model.cp_w)),
                                    ("wp", &decimal(model.w_prime_j / 1000.0, 1)),
                                    ("n", &model.points.to_string()),
                                ],
                            )}
                        }
                    }
                    if estimate.ftp_w + 0.5 < current_ftp {
                        div { class: "rv-note", {t("ui_review.power.ftp_csokkent")} }
                    }
                    button {
                        class: "btn rv-ftp-action",
                        r#type: "button",
                        disabled: (estimate.ftp_w - current_ftp).abs() < 0.5,
                        onclick: {
                            let target = estimate.ftp_w;
                            move |_| on_apply_ftp.call(target)
                        },
                        if (estimate.ftp_w - current_ftp).abs() < 0.5 {
                            {t("ui_review.power.az_ftp_mar_ennyi")}
                        } else {
                            {tf(
                                "ui_review.power.ftp_beallitasa_estimate_ftp_w_0_w_ra",
                                &[("ftp", &format!("{:.0}", estimate.ftp_w))],
                            )}
                        }
                    }
                } else {
                    div { class: "rv-note",
                        {t("ui_review.power.meg_nincs_20_perces_mert_teljesitmeny_az_edzesek_koz")}
                    }
                }
                if ftp_weeks_max > 0.0 {
                    h3 { class: "rv-card-sub", {t("ui_review.power.becsult_ftp_hetente")} }
                    div { class: "rv-trend rv-trend--small", role: "img", "aria-label": t("ui_review.power.becsult_ftp_hetente"),
                        for week in analysis_now.ftp_weeks.iter() {
                            {
                                let value = week.ftp_w.unwrap_or(0.0);
                                // the bars start at 60 % of the highest week, so a change of a few watts shows
                                let floor = ftp_weeks_max * 0.6;
                                let height = bar_pct(value - floor, ftp_weeks_max - floor);
                                let label = period_label(week.start_unix_s, Period::Week);
                                rsx! {
                                    div { key: "ftp-{week.start_unix_s}", class: "rv-trend-col",
                                        span { class: "rv-trend-value num", {week.ftp_w.map(|w| format!("{w:.0}")).unwrap_or_default()} }
                                        span { class: "rv-trend-bar", i { style: "height: {height:.1}%" } }
                                        span { class: "rv-trend-label", "{label}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if !ftp_changes.is_empty() {
                    h3 { class: "rv-card-sub", {t("ui_review.power.ftp_valtozasai")} }
                    ul { class: "rv-history",
                        for change in ftp_changes.iter() {
                            li { key: "{change.unix_s}",
                                span { class: "rv-dim", "{chart::fmt_stamp(change.unix_s)}" }
                                span { class: "num", "{change.ftp_w:.0} W" }
                            }
                        }
                    }
                }
                p { class: "rv-footnote",
                    {t("ui_review.power.csak_mert_watt_szamit_a_trainer_sajat_wattja_nem_tel")}
                }
            }

            // ── heart rate ──
            section { class: "rv-card", "aria-label": t("ui_review.power.pulzus"),
                h2 { class: "rv-card-title", {t("ui_review.power.pulzus")} }
                p { class: "rv-card-note",
                    {tf(
                        "ui_review.power.pulzus_beallitasok",
                        &[
                            ("max", &format!("{:.0}", rider_now.max_hr)),
                            ("lthr", &format!("{:.0}", rider_now.effective_lthr())),
                            ("rest", &format!("{:.0}", rider_now.effective_rest_hr())),
                            ("model", hr_model_label(rider_now.hr_zone_model)),
                        ],
                    )}
                }
                if let Some((seen, when)) = analysis_now.max_hr_seen {
                    p { class: "rv-card-note",
                        {tf(
                            "ui_review.power.legmagasabb_mert_pulzus",
                            &[("hr", &format!("{seen:.0}")), ("when", &chart::fmt_stamp(when))],
                        )}
                    }
                    if seen > rider_now.max_hr + 0.5 {
                        button {
                            class: "btn rv-ftp-action",
                            r#type: "button",
                            onclick: move |_| on_apply_max_hr.call(seen.round()),
                            {tf("ui_review.power.maxpulzus_beallitasa", &[("hr", &format!("{seen:.0}"))])}
                        }
                    }
                }
            }

            // ── the power curve ──
            section { class: "rv-card", "aria-label": t("ui_review.power.csucsteljesitmeny"),
                h2 { class: "rv-card-title", {t("ui_review.power.csucsteljesitmeny")} }
                p { class: "rv-card-note",
                    {tf("ui_review.power.gorbe_magyarazat", &[("period", window_label(window_now))])}
                }
                if nothing_yet {
                    div { class: "rv-note",
                        {t("ui_review.power.meg_nincs_mert_teljesitmenyu_edzes_a_szimulalt_teker")}
                    }
                } else {
                    PlotFrame {
                        markup: curve_markup,
                        x0: curve_x0,
                        x1: curve_x1,
                        y0: curve_y0,
                        y1: curve_y1,
                        unit: "W".to_string(),
                        y_text: move |v: f64| format!("{v:.0}"),
                        x_ticks: curve_ticks,
                        empty: nothing_in_window.then(|| t("ui_review.power.ebben_az_idoszakban_nincs_mert_watt").to_string()),
                        class: "rv-curve-plot".to_string(),
                    }
                    div { class: "rv-legend",
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--best" } {window_label(window_now)} }
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--ever" } {t("ui_review.power.mindenkori")} }
                        for (slot , stamp , _estimated , _curve) in ticked.iter() {
                            span { key: "lg-{slot}", class: "rv-legend-item",
                                span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(*slot)}", "{letter(*slot)}" }
                                span { class: "rv-legend-date", "{stamp}" }
                            }
                        }
                    }
                }
                div { class: "rv-table-wrap",
                    table { class: "rv-table rv-curve",
                        thead {
                            tr {
                                th { {t("ui_review.ido")} }
                                th { {t("ui_review.power.legjobb_w")} }
                                th { "W/kg" }
                                th { {t("ui_review.power.mikor")} }
                                th { class: "rv-bar-head", "" }
                                th { {t("ui_review.power.mindenkori")} }
                                for (slot , stamp , _estimated , _curve) in ticked.iter() {
                                    th { key: "head-{slot}", title: "{stamp}",
                                        span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(*slot)}", "{letter(*slot)}" }
                                    }
                                }
                            }
                        }
                        tbody {
                            for index in TABLE_ROWS.iter().copied() {
                                {
                                    let seconds = DURATIONS[index];
                                    let best = analysis_now.best.get(index).and_then(|b| b.as_ref()).cloned();
                                    let ever = analysis_now.best_ever.get(index).and_then(|b| b.as_ref()).map(|b| b.watts);
                                    let pct = bar_pct(best.as_ref().map_or(0.0, |b| b.watts), best_max);
                                    let best_kg = best.as_ref().and_then(|b| b.rider_kg).unwrap_or(kg);
                                    rsx! {
                                        tr { key: "dur-{seconds}", class: if index == FTP_WINDOW { "is-key" } else { "" },
                                            th { scope: "row", "{duration_label(seconds)}" }
                                            td { class: "num", "{watts_text(best.as_ref().map(|b| b.watts))}" }
                                            td { class: "num", "{wkg_text(best.as_ref().map(|b| b.watts), best_kg)}" }
                                            td { class: "rv-dim rv-when",
                                                if let Some(b) = best.clone() {
                                                    button {
                                                        class: "rv-link",
                                                        r#type: "button",
                                                        title: t("ui_review.reszletek").to_string(),
                                                        onclick: move |_| on_open_ride.call((b.route_id.clone(), b.stem.clone())),
                                                        span { class: "rv-when-date", "{chart::fmt_stamp(b.started_unix_s)}" }
                                                        span { class: "rv-when-route", "{b.route_name}" }
                                                    }
                                                } else {
                                                    "{DASH}"
                                                }
                                            }
                                            td { class: "rv-bar-cell",
                                                span { class: "rv-bar",
                                                    i { style: "width: {pct:.1}%" }
                                                }
                                            }
                                            td { class: "num rv-dim", "{watts_text(ever)}" }
                                            for (slot , _stamp , estimated , curve) in ticked.iter() {
                                                td { key: "cell-{slot}", class: "num",
                                                    if *estimated && curve.get(index).copied().flatten().is_some() {
                                                        "~"
                                                    }
                                                    "{watts_text(curve.get(index).copied().flatten())}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if ticked.iter().any(|t| t.2) {
                    p { class: "rv-footnote",
                        {t("ui_review.power.a_jelu_ertekek_becsult_vagy_szimulalt_wattbol_szamol")}
                    }
                }
            }

            // ── time in zones ──
            section { class: "rv-card", "aria-label": t("ui_review.power.zonak"),
                h2 { class: "rv-card-title", {t("ui_review.power.zonak")} }
                p { class: "rv-card-note",
                    {tf(
                        "ui_review.power.zonak_magyarazat",
                        &[
                            ("period", window_label(window_now)),
                            ("rides", &p.rides.to_string()),
                            ("time", &fmt_duration(p.time_s)),
                            ("km", &fmt_km(p.dist_m)),
                        ],
                    )}
                }
                div { class: "rv-zone-grid",
                    div { class: "rv-zone-col",
                        h3 { class: "rv-card-sub", {t("ui_review.power.teljesitmenyzonak")} }
                        if power_total > 0.0 {
                            ZoneBars { secs: p.power_zone_secs.to_vec(), prefix: "Z".to_string(), aria: t("ui_review.power.teljesitmenyzonak").to_string() }
                            if let Some((easy, mid, hard)) = polarised {
                                p { class: "rv-footnote",
                                    {tf(
                                        "ui_review.power.eloszlas",
                                        &[
                                            ("easy", &format!("{easy:.0}")),
                                            ("mid", &format!("{mid:.0}")),
                                            ("hard", &format!("{hard:.0}")),
                                        ],
                                    )}
                                }
                            }
                        } else {
                            div { class: "rv-note", {t("ui_review.power.nincs_mert_watt_az_idoszakban")} }
                        }
                    }
                    div { class: "rv-zone-col",
                        h3 { class: "rv-card-sub",
                            {tf("ui_review.power.pulzuszonak_modell", &[("model", hr_model_label(rider_now.hr_zone_model))])}
                        }
                        if hr_total > 0.0 {
                            ZoneBars { secs: p.hr_zone_secs.to_vec(), prefix: "P".to_string(), aria: t("ui_review.power.pulzus").to_string() }
                        } else {
                            div { class: "rv-note", {t("ui_review.power.nincs_pulzus_az_idoszakban")} }
                        }
                    }
                }
            }

            // ── load per week / month ──
            section { class: "rv-card", "aria-label": t("ui_review.power.terheles"),
                div { class: "rv-card-head",
                    h2 { class: "rv-card-title", {t("ui_review.power.terheles")} }
                    Segmented {
                        options: vec![
                            ("week".to_string(), t("ui_review.power.heti").to_string()),
                            ("month".to_string(), t("ui_review.power.havi").to_string()),
                        ],
                        value: if period_now == Period::Week { "week".to_string() } else { "month".to_string() },
                        on_change: move |value: String| {
                            period.set(if value == "month" { Period::Month } else { Period::Week });
                        },
                        aria_label: t("ui_review.power.idoszak").to_string(),
                        small: true,
                    }
                }
                p { class: "rv-card-note",
                    {tf(
                        "ui_review.power.terheles_osszesen",
                        &[
                            ("n", &buckets.len().to_string()),
                            ("period", period_name),
                            ("rides", &total_rides.to_string()),
                            ("time", &fmt_duration(total_time)),
                            ("km", &fmt_km(total_dist)),
                            ("load", &format!("{total_load:.0}")),
                        ],
                    )}
                }
                div { class: "rv-trend", role: "img", "aria-label": t("ui_review.power.terheles_idoszakonkent"),
                    for bucket in buckets.iter() {
                        {
                            let load = bucket.load();
                            let height = bar_pct(load, load_max);
                            let label = period_label(bucket.start_unix_s, period_now);
                            let title = tf(
                                "ui_review.power.oszlop_sugo",
                                &[
                                    ("label", &label),
                                    ("rides", &bucket.rides.to_string()),
                                    ("time", &fmt_duration(bucket.time_s)),
                                    ("km", &fmt_km(bucket.dist_m)),
                                    ("tss", &format!("{:.0}", bucket.tss)),
                                    ("hr", &format!("{:.0}", bucket.hr_tss)),
                                    ("est", &format!("{:.0}", bucket.estimated_tss)),
                                ],
                            );
                            rsx! {
                                div { key: "{bucket.start_unix_s}", class: "rv-trend-col", title: "{title}",
                                    span { class: "rv-trend-value num",
                                        if load > 0.0 {
                                            "{load:.0}"
                                        }
                                    }
                                    span { class: "rv-trend-bar",
                                        i { style: "height: {height:.1}%" }
                                    }
                                    span { class: "rv-trend-label", "{label}" }
                                }
                            }
                        }
                    }
                }
                p { class: "rv-footnote", {t("ui_review.power.terheles_labjegyzet")} }
            }
        }
    }
}
