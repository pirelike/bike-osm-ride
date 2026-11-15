//! The ride view of the review page: one stored ride on the time axis.
//!
//! * the numbers of the ride: the saved summary plus what `ride_sim::ride_detail` derives
//!   (VI, EF, Pw:HR decoupling, W/kg, work above FTP, the lowest W′ balance);
//! * a time chart of power with the workout target, heart rate, cadence, speed or W′ balance;
//! * the laps: the workout's steps with target, result and compliance, or the hard efforts found
//!   in a ride without a workout;
//! * time in power / heart-rate zones (one second at a time) and the cadence spread;
//! * the rider's rating (RPE 1–10) and note, and the TCX / FIT export.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{RideRecord, RiderSettings, MAX_RIDE_NOTE_CHARS};
use ride_sim::fitness::CpModel;
use ride_sim::ride_detail::{self, Dense, Lap, LapKind, RideDetail, CADENCE_BANDS};
use ride_ui_kit::format::{fmt_duration, fmt_km, fmt_speed_kmh};
use ride_ui_kit::{Icon, IconName, Segmented};

use crate::plot::{self, Line, PlotFrame};
use crate::power::{watts_text, wkg_text, ZoneBars};
use crate::{chart, DASH};
use ride_i18n::{decimal, t, tf};

/// The chart is averaged down to this many points.
const CHART_POINTS: usize = 600;

/// What the time chart shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeMetric {
    Power,
    HeartRate,
    Cadence,
    Speed,
    WPrime,
}

impl TimeMetric {
    pub const ALL: [TimeMetric; 5] = [
        TimeMetric::Power,
        TimeMetric::HeartRate,
        TimeMetric::Cadence,
        TimeMetric::Speed,
        TimeMetric::WPrime,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TimeMetric::Power => "power",
            TimeMetric::HeartRate => "hr",
            TimeMetric::Cadence => "cadence",
            TimeMetric::Speed => "speed",
            TimeMetric::WPrime => "wbal",
        }
    }

    pub fn label(self, compact: bool) -> &'static str {
        match (self, compact) {
            (TimeMetric::Power, false) => t("ui_review.chart.teljesitmeny"),
            (TimeMetric::HeartRate, false) => t("ui_review.chart.pulzus"),
            (TimeMetric::Cadence, false) => t("ui_review.chart.kadencia"),
            (TimeMetric::Speed, false) => t("ui_review.chart.sebesseg"),
            (TimeMetric::WPrime, false) => t("ui_review.detail.w_egyenleg"),
            (TimeMetric::Power, true) => "W",
            (TimeMetric::HeartRate, true) => "bpm",
            (TimeMetric::Cadence, true) => "rpm",
            (TimeMetric::Speed, true) => "km/h",
            (TimeMetric::WPrime, true) => "W′",
        }
    }

    fn unit(self) -> &'static str {
        match self {
            TimeMetric::Power => "W",
            TimeMetric::HeartRate => "bpm",
            TimeMetric::Cadence => "rpm",
            TimeMetric::Speed => "km/h",
            TimeMetric::WPrime => "kJ",
        }
    }
}

/// The lines of the time chart for `metric`.
pub fn time_lines(dense: &Dense, metric: TimeMetric, model: Option<&CpModel>) -> Vec<Line> {
    let line = |values: Vec<Option<f64>>, color: &str, dash: &'static str, width: f64| Line {
        color: color.to_string(),
        dash,
        width,
        points: plot::bucket_mean(&values, 0.0, CHART_POINTS),
    };
    match metric {
        TimeMetric::Power => {
            let mut out = vec![line(dense.power_w.clone(), "var(--accent)", "", 1.5)];
            if dense.target_w.iter().any(Option::is_some) {
                out.insert(0, line(dense.target_w.clone(), "var(--text-dim)", "", 2.5));
            }
            out
        }
        TimeMetric::HeartRate => vec![line(dense.hr_bpm.clone(), "var(--series-3)", "", 1.5)],
        TimeMetric::Cadence => vec![line(dense.cadence_rpm.clone(), "var(--series-2)", "", 1.5)],
        TimeMetric::Speed => vec![line(
            dense.speed_ms.iter().map(|v| v.map(|v| v * 3.6)).collect(),
            "var(--series-1)",
            "",
            1.5,
        )],
        TimeMetric::WPrime => match model.and_then(|m| ride_detail::w_prime_balance(dense, m)) {
            Some(balance) => vec![line(
                balance.into_iter().map(|j| Some(j / 1000.0)).collect(),
                "var(--accent)",
                "",
                2.0,
            )],
            None => Vec::new(),
        },
    }
}

/// The name of a lap in the table.
pub fn lap_name(lap: &Lap, index: usize) -> String {
    match &lap.kind {
        LapKind::Step { name, .. } if !name.trim().is_empty() => name.clone(),
        LapKind::Step { kind, .. } => kind.label().to_string(),
        LapKind::Effort => tf(
            "ui_review.detail.erofeszites_n",
            &[("n", &(index + 1).to_string())],
        ),
    }
}

fn opt(value: Option<f64>, decimals: usize) -> String {
    match value {
        Some(v) if v.is_finite() => decimal(v, decimals),
        _ => DASH.to_string(),
    }
}

/// One number of the header grid.
#[component]
fn Tile(label: String, value: String, #[props(default)] hint: String) -> Element {
    rsx! {
        div { class: "rv-kpi", title: "{hint}",
            span { class: "rv-kpi-label", "{label}" }
            span { class: "rv-kpi-value num", "{value}" }
        }
    }
}

#[component]
pub fn RideDetailView(
    record: ReadSignal<Option<Arc<RideRecord>>>,
    rider: ReadSignal<RiderSettings>,
    /// The critical-power model of the last 90 days (W′ balance), if there is one.
    model: Option<CpModel>,
    compact: bool,
    on_close: EventHandler<()>,
    /// The rider's rating and note.
    on_save_journal: EventHandler<(Option<u8>, String)>,
    #[props(default)] on_agent: EventHandler<()>,
    /// `"tcx"` or `"fit"`.
    on_export: EventHandler<String>,
) -> Element {
    let mut metric = use_signal(|| TimeMetric::Power);
    // the form starts from the open ride; a ride opened later brings its own (the effect below)
    let mut rpe = use_signal(move || {
        record
            .peek()
            .as_ref()
            .and_then(|r| ride_model::valid_rpe(r.rpe))
    });
    let mut note = use_signal(move || {
        record
            .peek()
            .as_ref()
            .map(|r| r.note.clone())
            .unwrap_or_default()
    });
    let mut dirty = use_signal(|| false);

    // a newly opened ride brings its own rating and note into the form
    use_effect(move || {
        let current = record.read().clone();
        rpe.set(current.as_ref().and_then(|r| ride_model::valid_rpe(r.rpe)));
        note.set(current.as_ref().map(|r| r.note.clone()).unwrap_or_default());
        dirty.set(false);
    });

    let dense = use_memo(move || {
        record
            .read()
            .as_ref()
            .map(|r| Arc::new(ride_detail::dense(r)))
            .unwrap_or_default()
    });
    let derived = use_memo(move || -> (RideDetail, Vec<Lap>, bool) {
        let Some(r) = record.read().clone() else {
            return (RideDetail::default(), Vec::new(), false);
        };
        let rider = rider.read();
        let d = dense.read().clone();
        let ftp = r.ftp_w.unwrap_or(rider.ftp_w);
        let kg = r.rider_kg.unwrap_or(rider.rider_kg);
        let detail = ride_detail::ride_detail(&d, ftp, kg, &rider, model.as_ref());
        let workout_laps = ride_detail::workout_laps(r.workout.as_ref(), &d);
        if workout_laps.is_empty() {
            (detail, ride_detail::auto_intervals(&d, ftp), false)
        } else {
            (detail, workout_laps, true)
        }
    });

    let Some(r) = record.read().clone() else {
        return rsx! {
            div { class: "rv-busy", role: "status", {t("ui_review.betoltes")} }
        };
    };
    let rider_now = rider.read().clone();
    let (detail, laps, from_workout) = derived.read().clone();
    let s = &r.summary;
    let ftp = r.ftp_w.unwrap_or(rider_now.ftp_w);
    let kg = r.rider_kg.unwrap_or(rider_now.rider_kg);
    let estimated = r.power_estimated || r.simulated;
    let tilde = |text: String| {
        if estimated && text != DASH {
            format!("~{text}")
        } else {
            text
        }
    };
    let hr_tss =
        ride_sim::fitness::hr_tss(&ride_sim::power_curve::hr_histogram(&r.points), &rider_now);

    let metric_now = *metric.read();
    let dense_now = dense.read().clone();
    let lines = time_lines(&dense_now, metric_now, model.as_ref());
    let (y0, y1) = plot::y_range(
        &lines,
        &[],
        metric_now != TimeMetric::HeartRate,
        match metric_now {
            TimeMetric::HeartRate | TimeMetric::Cadence => 30.0,
            TimeMetric::Speed => 10.0,
            TimeMetric::WPrime => 5.0,
            TimeMetric::Power => 100.0,
        },
    );
    let x1 = (dense_now.len() as f64).max(60.0);
    let x_ticks: Vec<(f64, String)> = plot::time_ticks(x1, if compact { 4 } else { 8 })
        .into_iter()
        .map(|x| (x, plot::clock_label(x)))
        .collect();
    let chart_empty = lines
        .iter()
        .all(|l| l.points.iter().all(|(_, v)| v.is_none()));
    let empty_text = if metric_now == TimeMetric::WPrime && model.is_none() {
        t("ui_review.detail.w_egyenleg_modell_nelkul")
    } else {
        t("ui_review.ehhez_a_metrikahoz_nincs_adat_a_kijelolt_edzesekben")
    };
    let metrics: Vec<(String, String)> = TimeMetric::ALL
        .iter()
        .map(|m| (m.key().to_string(), m.label(compact).to_string()))
        .collect();

    let cadence_labels: Vec<String> = {
        let mut out = vec![tf(
            "ui_review.detail.kadencia_alatt",
            &[("n", &format!("{:.0}", CADENCE_BANDS[0]))],
        )];
        for pair in CADENCE_BANDS.windows(2) {
            out.push(format!("{:.0}–{:.0}", pair[0], pair[1]));
        }
        out.push(tf(
            "ui_review.detail.kadencia_felett",
            &[(
                "n",
                &format!("{:.0}", CADENCE_BANDS[CADENCE_BANDS.len() - 1]),
            )],
        ));
        out
    };
    let cadence_total: f64 = detail.cadence_secs.iter().sum();
    let cadence_max = detail.cadence_secs.iter().copied().fold(0.0, f64::max);

    let rpe_now = *rpe.read();
    let note_now = note.read().clone();
    let dirty_now = *dirty.read();

    rsx! {
        div { class: "rv-power rv-detail", role: "region", "aria-label": t("ui_review.detail.menet_reszletei"),
            div { class: "rv-detail-head",
                button {
                    class: if compact { "btn btn--ghost btn--icon" } else { "btn btn--ghost btn--sm" },
                    r#type: "button",
                    "aria-label": t("ui_review.detail.vissza_a_listahoz"),
                    title: t("ui_review.detail.vissza_a_listahoz").to_string(),
                    onclick: move |_| on_close.call(()),
                    Icon { name: IconName::ChevronLeft, size: 16 }
                    if !compact {
                        {t("ui_review.detail.vissza_a_listahoz")}
                    }
                }
                div { class: "rv-detail-title",
                    h2 { class: "rv-card-title", "{chart::fmt_stamp(s.started_unix_s)}" }
                    span { class: "rv-ride-meta",
                        "{r.route_name}"
                        if let Some(w) = r.workout.as_ref() {
                            " · {w.name}"
                        }
                    }
                }
                div { class: "rv-detail-actions",
                    button {
                        class: "btn btn--sm", r#type: "button",
                        onclick: move |_| on_agent.call(()),
                        {t("app.agent.review")}
                    }
                    button {
                        class: "btn btn--sm",
                        r#type: "button",
                        title: t("ui_review.detail.mentes_tcx").to_string(),
                        onclick: move |_| on_export.call("tcx".to_string()),
                        Icon { name: IconName::Download, size: 14 }
                        "TCX"
                    }
                    button {
                        class: "btn btn--sm",
                        r#type: "button",
                        title: t("ui_review.detail.mentes_fit").to_string(),
                        onclick: move |_| on_export.call("fit".to_string()),
                        Icon { name: IconName::Download, size: 14 }
                        "FIT"
                    }
                }
            }
            if estimated {
                div { class: "rv-note", {t("ui_review.detail.becsult_figyelmeztetes")} }
            }

            section { class: "rv-card", "aria-label": t("ui_review.detail.szamok"),
                div { class: "rv-kpis rv-kpis--wide",
                    Tile { label: t("ui_review.ido").to_string(), value: fmt_duration(s.elapsed_s) }
                    Tile { label: t("ui_review.detail.tav").to_string(), value: format!("{} km", fmt_km(s.dist)) }
                    Tile { label: t("ui_review.detail.atlagsebesseg").to_string(), value: format!("{} km/h", fmt_speed_kmh(s.avg_speed_ms)) }
                    Tile { label: t("ui_review.atl_watt").to_string(), value: tilde(watts_text(detail.avg_power_w)) }
                    Tile { label: "NP".to_string(), value: tilde(watts_text(detail.np_w)), hint: t("ui_review.detail.np_sugo").to_string() }
                    Tile { label: "IF".to_string(), value: tilde(opt(detail.np_w.filter(|_| ftp > 0.0).map(|np| np / ftp), 2)) }
                    Tile {
                        label: "TSS".to_string(),
                        value: tilde(opt(s.tss, 0)),
                        hint: tf("ui_review.detail.tss_sugo", &[("ftp", &format!("{ftp:.0}"))]),
                    }
                    Tile { label: "hrTSS".to_string(), value: opt(hr_tss, 0), hint: t("ui_review.detail.hrtss_sugo").to_string() }
                    Tile { label: "VI".to_string(), value: tilde(opt(detail.variability_index, 2)), hint: t("ui_review.detail.vi_sugo").to_string() }
                    Tile { label: "EF".to_string(), value: tilde(opt(detail.efficiency_factor, 2)), hint: t("ui_review.detail.ef_sugo").to_string() }
                    Tile {
                        label: "Pw:HR".to_string(),
                        value: tilde(detail.decoupling_pct.map_or(DASH.to_string(), |d| format!("{} %", decimal(d, 1)))),
                        hint: t("ui_review.detail.decoupling_sugo").to_string(),
                    }
                    Tile { label: t("ui_review.detail.w_kg_atlag_np").to_string(), value: tilde(format!("{} / {}", wkg_text(detail.avg_power_w, kg), wkg_text(detail.np_w, kg))) }
                    Tile { label: t("ui_review.detail.munka").to_string(), value: tilde(detail.work_kj.map_or(DASH.to_string(), |w| format!("{w:.0} kJ"))) }
                    Tile {
                        label: t("ui_review.detail.ftp_felett").to_string(),
                        value: tilde(detail.work_above_ftp_kj.map_or(DASH.to_string(), |w| format!("{w:.0} kJ"))),
                    }
                    Tile {
                        label: t("ui_review.detail.w_minimum").to_string(),
                        value: detail.w_prime_min_pct.map_or(DASH.to_string(), |p| format!("{p:.0} %")),
                        hint: t("ui_review.detail.w_minimum_sugo").to_string(),
                    }
                    Tile { label: t("ui_review.detail.pulzus_atl_max").to_string(), value: format!("{} / {}", watts_text(detail.avg_hr), watts_text(detail.max_hr)) }
                    Tile { label: t("ui_review.chart.kadencia").to_string(), value: watts_text(detail.avg_cadence) }
                    Tile { label: t("ui_review.detail.ftp_aznap").to_string(), value: format!("{ftp:.0} W") }
                    Tile { label: t("ui_review.detail.testsuly").to_string(), value: format!("{} kg", decimal(kg, 1)) }
                }
            }

            section { class: "rv-card", "aria-label": t("ui_review.grafikon"),
                div { class: "rv-chart-head",
                    Segmented {
                        options: metrics,
                        value: metric_now.key().to_string(),
                        on_change: move |value: String| {
                            if let Some(m) = TimeMetric::ALL.into_iter().find(|m| m.key() == value) {
                                metric.set(m);
                            }
                        },
                        aria_label: t("ui_review.detail.grafikon_valaszto").to_string(),
                        small: true,
                    }
                    if metric_now == TimeMetric::Power && dense_now.target_w.iter().any(Option::is_some) {
                        span { class: "rv-legend-item", span { class: "rv-key rv-key--target" } {t("ui_review.detail.cel")} }
                    }
                }
                PlotFrame {
                    markup: plot::markup(&lines, &[]),
                    x0: 0.0,
                    x1,
                    y0,
                    y1,
                    unit: metric_now.unit().to_string(),
                    y_text: move |v: f64| format!("{v:.0}"),
                    x_ticks,
                    empty: chart_empty.then(|| empty_text.to_string()),
                    class: "rv-time-plot".to_string(),
                }
            }

            section { class: "rv-card", "aria-label": t("ui_review.detail.korok"),
                h2 { class: "rv-card-title",
                    if from_workout {
                        {t("ui_review.detail.edzes_lepesei")}
                    } else {
                        {t("ui_review.detail.kemeny_szakaszok")}
                    }
                }
                p { class: "rv-card-note",
                    if from_workout {
                        {t("ui_review.detail.lepesek_magyarazat")}
                    } else {
                        {tf("ui_review.detail.szakaszok_magyarazat", &[("w", &format!("{:.0}", ftp * ride_detail::AUTO_THRESHOLD))])}
                    }
                }
                if laps.is_empty() {
                    div { class: "rv-note", {t("ui_review.detail.nincs_kemeny_szakasz")} }
                } else {
                    div { class: "rv-table-wrap",
                        table { class: "rv-table rv-laps",
                            thead {
                                tr {
                                    th { "#" }
                                    th { {t("ui_review.detail.nev")} }
                                    th { {t("ui_review.detail.kezdet")} }
                                    th { {t("ui_review.ido")} }
                                    if from_workout {
                                        th { {t("ui_review.detail.cel_w")} }
                                    }
                                    th { {t("ui_review.atl_watt")} }
                                    if from_workout {
                                        th { "%" }
                                    }
                                    th { "NP" }
                                    th { "W/kg" }
                                    th { {t("ui_review.detail.pulzus_atl_max")} }
                                    th { "rpm" }
                                }
                            }
                            tbody {
                                for (i , lap) in laps.iter().enumerate() {
                                    {
                                        let compliance = lap.compliance_pct;
                                        let class = match compliance {
                                            Some(c) if !(90.0..=110.0).contains(&c) => "num is-off",
                                            _ => "num",
                                        };
                                        rsx! {
                                            tr { key: "lap-{i}",
                                                td { class: "num", "{i + 1}" }
                                                th { scope: "row", "{lap_name(lap, i)}" }
                                                td { class: "num", "{plot::clock_label(lap.start_s as f64)}" }
                                                td { class: "num", "{fmt_duration(lap.duration_s() as f64)}" }
                                                if from_workout {
                                                    td { class: "num", "{watts_text(lap.target_w)}" }
                                                }
                                                td { class: "num", "{tilde(watts_text(lap.avg_power_w))}" }
                                                if from_workout {
                                                    td { class: "{class}", {compliance.map_or(DASH.to_string(), |c| format!("{c:.0}"))} }
                                                }
                                                td { class: "num", "{tilde(watts_text(lap.np_w))}" }
                                                td { class: "num", "{wkg_text(lap.avg_power_w, kg)}" }
                                                td { class: "num", "{watts_text(lap.avg_hr)} / {watts_text(lap.max_hr)}" }
                                                td { class: "num", "{watts_text(lap.avg_cadence)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if from_workout {
                        p { class: "rv-footnote", {t("ui_review.detail.teljesites_labjegyzet")} }
                    }
                }
            }

            section { class: "rv-card", "aria-label": t("ui_review.power.zonak"),
                h2 { class: "rv-card-title", {t("ui_review.power.zonak")} }
                div { class: "rv-zone-grid",
                    div { class: "rv-zone-col",
                        h3 { class: "rv-card-sub", {t("ui_review.power.teljesitmenyzonak")} }
                        if detail.power_zone_secs.iter().any(|s| *s > 0.0) {
                            ZoneBars { secs: detail.power_zone_secs.to_vec(), prefix: "Z".to_string(), aria: t("ui_review.power.teljesitmenyzonak").to_string() }
                        } else {
                            div { class: "rv-note", {t("ui_review.detail.nincs_watt")} }
                        }
                    }
                    div { class: "rv-zone-col",
                        h3 { class: "rv-card-sub",
                            {tf("ui_review.power.pulzuszonak_modell", &[("model", crate::power::hr_model_label(rider_now.hr_zone_model))])}
                        }
                        if detail.hr_zone_secs.iter().any(|s| *s > 0.0) {
                            ZoneBars { secs: detail.hr_zone_secs.to_vec(), prefix: "P".to_string(), aria: t("ui_review.power.pulzus").to_string() }
                        } else {
                            div { class: "rv-note", {t("ui_review.detail.nincs_pulzus")} }
                        }
                    }
                    div { class: "rv-zone-col",
                        h3 { class: "rv-card-sub", {t("ui_review.detail.kadencia_eloszlas")} }
                        if cadence_total > 0.0 {
                            div { class: "rv-zones",
                                for (i , secs) in detail.cadence_secs.iter().enumerate() {
                                    div { key: "cad-{i}", class: "rv-zone",
                                        span { class: "rv-zone-name", "{cadence_labels[i]}" }
                                        span { class: "rv-zone-bar",
                                            i { style: "width: {crate::power::bar_pct(*secs, cadence_max):.1}%; background: var(--series-2)" }
                                        }
                                        span { class: "rv-zone-time num", "{fmt_duration(*secs)}" }
                                        span { class: "rv-zone-pct num", "{secs / cadence_total * 100.0:.0} %" }
                                    }
                                }
                            }
                        } else {
                            div { class: "rv-note", {t("ui_review.detail.nincs_kadencia")} }
                        }
                    }
                }
            }

            section { class: "rv-card", "aria-label": t("ui_review.detail.naplo_bejegyzes"),
                h2 { class: "rv-card-title", {t("ui_review.detail.naplo_bejegyzes")} }
                div { class: "rv-rpe-row", role: "group", "aria-label": t("ui_review.detail.rpe_cim"),
                    span { class: "t-caption", {t("ui_review.detail.rpe_cim")} }
                    div { class: "rv-rpe-scale",
                        for value in 1u8..=10 {
                            button {
                                key: "rpe-{value}",
                                class: if rpe_now == Some(value) { "rv-rpe-btn is-on" } else { "rv-rpe-btn" },
                                r#type: "button",
                                "aria-pressed": "{rpe_now == Some(value)}",
                                onclick: move |_| {
                                    let next = if *rpe.peek() == Some(value) { None } else { Some(value) };
                                    rpe.set(next);
                                    dirty.set(true);
                                },
                                "{value}"
                            }
                        }
                    }
                    span { class: "rv-footnote", {t("ui_review.detail.rpe_skala")} }
                }
                label { class: "t-caption", r#for: "rv-note-text", {t("ui_review.detail.megjegyzes")} }
                textarea {
                    id: "rv-note-text",
                    class: "rv-note-text",
                    rows: "4",
                    maxlength: "{MAX_RIDE_NOTE_CHARS}",
                    placeholder: t("ui_review.detail.megjegyzes_pelda").to_string(),
                    value: "{note_now}",
                    oninput: move |event: FormEvent| {
                        note.set(event.value());
                        dirty.set(true);
                    },
                }
                button {
                    class: "btn rv-ftp-action",
                    r#type: "button",
                    disabled: !dirty_now,
                    onclick: move |_| {
                        on_save_journal.call((*rpe.peek(), note.peek().clone()));
                        dirty.set(false);
                    },
                    {t("ui_review.detail.bejegyzes_mentese")}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_metric_has_a_distinct_key_and_labels() {
        let keys: std::collections::HashSet<_> = TimeMetric::ALL.iter().map(|m| m.key()).collect();
        assert_eq!(keys.len(), TimeMetric::ALL.len());
        for m in TimeMetric::ALL {
            assert!(!m.label(true).is_empty() && !m.label(false).is_empty());
        }
    }

    #[test]
    fn the_power_chart_carries_the_target_when_there_is_one() {
        let dense = Dense {
            power_w: vec![Some(200.0); 100],
            hr_bpm: vec![None; 100],
            cadence_rpm: vec![None; 100],
            speed_ms: vec![Some(8.0); 100],
            target_w: vec![Some(210.0); 100],
        };
        assert_eq!(time_lines(&dense, TimeMetric::Power, None).len(), 2);
        let free = Dense {
            target_w: vec![None; 100],
            ..dense.clone()
        };
        assert_eq!(time_lines(&free, TimeMetric::Power, None).len(), 1);
        assert!(
            time_lines(&dense, TimeMetric::WPrime, None).is_empty(),
            "no model"
        );
        let speed = time_lines(&dense, TimeMetric::Speed, None);
        assert!((speed[0].points[0].1.unwrap() - 28.8).abs() < 1e-9);
    }
}
