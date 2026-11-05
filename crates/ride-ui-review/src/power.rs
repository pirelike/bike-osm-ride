//! The "Teljesítmény" view of the review page: the power curve, the FTP the best 20 minutes
//! suggest, and the weekly / monthly training load.
//!
//! The numbers come from `ride_sim::power_curve` (pure, tested); this file only lays them out.
//! Only measured watts are in them, which the page says out loud: an FTP derived from a physics
//! model's own watts would confirm nothing.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::RideRecord;
use ride_sim::power_curve::{self, Analysis, Bucket, Period, DURATIONS};
use ride_ui_kit::format::fmt_duration;
use ride_ui_kit::Segmented;

use crate::{chart, letter, var_of, DASH};

/// "5 mp", "1 perc", ...
pub fn duration_label(seconds: u32) -> String {
    if seconds < 60 {
        format!("{seconds} mp")
    } else {
        format!("{} perc", seconds / 60)
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

fn watts_text(value: Option<f64>) -> String {
    match value {
        Some(w) if w.is_finite() => format!("{w:.0}"),
        _ => DASH.to_string(),
    }
}

/// Height of a bar, percent of the tallest one (at least a sliver when there is something).
fn bar_pct(value: f64, max: f64) -> f64 {
    if !(value.is_finite() && max.is_finite()) || max <= 0.0 || value <= 0.0 {
        return 0.0;
    }
    (value / max * 100.0).clamp(2.0, 100.0)
}

/// The curve of the ticked rides next to the best of all rides, the FTP card and the trend.
#[component]
pub fn PowerView(
    analysis: ReadSignal<Arc<Analysis>>,
    /// The FTP in the settings, watts.
    ftp_w: ReadSignal<f64>,
    /// The ticked rides of the comparison view (their curves are shown next to the best ones).
    loaded: ReadSignal<Arc<Vec<Arc<RideRecord>>>>,
    on_apply_ftp: EventHandler<f64>,
) -> Element {
    let mut period = use_signal(|| Period::Week);

    let analysis_now = analysis.read().clone();
    let current_ftp = *ftp_w.read();
    let period_now = *period.read();

    // the curve of every ticked ride; the watts of an estimated one are marked with a tilde
    let ticked: Vec<(usize, String, bool, Vec<Option<f64>>)> = loaded
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
        .collect();

    let best_max = analysis_now
        .best
        .iter()
        .flatten()
        .map(|b| b.watts)
        .reduce(f64::max)
        .unwrap_or(0.0);
    let nothing_yet = analysis_now.best.iter().all(Option::is_none);

    let buckets: &[Bucket] = match period_now {
        Period::Week => &analysis_now.weeks,
        Period::Month => &analysis_now.months,
    };
    let tss_max = buckets.iter().map(|b| b.tss).fold(0.0, f64::max);
    let total_rides: usize = buckets.iter().map(|b| b.rides).sum();
    let total_time: f64 = buckets.iter().map(|b| b.time_s).sum();
    let total_tss: f64 = buckets.iter().map(|b| b.tss).sum();
    let period_name = match period_now {
        Period::Week => "hét",
        Period::Month => "hónap",
    };

    rsx! {
        div { class: "rv-power", role: "region", "aria-label": "Teljesítmény",
            if analysis_now.rides_pending > 0 {
                div { class: "rv-note", role: "status",
                    "{analysis_now.rides_pending} régebbi edzés számai még nincsenek meg, a következő frissítésnél készülnek el."
                }
            }
            section { class: "rv-card", "aria-label": "Csúcsteljesítmény",
                h2 { class: "rv-card-title", "Csúcsteljesítmény" }
                p { class: "rv-card-note",
                    "A legjobb átlagos watt a megadott ideig, az összes mért edzésed közül. A rövidebb edzésnek nincs értéke a hosszabb időre."
                }
                if nothing_yet {
                    div { class: "rv-note",
                        "Még nincs mért teljesítményű edzés. A szimulált tekerő és a becsült (~) wattok nem számítanak."
                    }
                }
                div { class: "rv-table-wrap",
                    table { class: "rv-table rv-curve",
                        thead {
                            tr {
                                th { "Idő" }
                                th { "Legjobb (W)" }
                                th { "Mikor" }
                                th { class: "rv-bar-head", "" }
                                for (slot , stamp , _estimated , _curve) in ticked.iter() {
                                    th { key: "head-{slot}", title: "{stamp}",
                                        span { class: "rv-swatch rv-swatch--sm", style: "--c: {var_of(*slot)}", "{letter(*slot)}" }
                                    }
                                }
                            }
                        }
                        tbody {
                            for (index , seconds) in DURATIONS.iter().copied().enumerate() {
                                {
                                    let best = analysis_now.best.get(index).and_then(|b| b.as_ref());
                                    let pct = bar_pct(best.map_or(0.0, |b| b.watts), best_max);
                                    rsx! {
                                        tr { key: "dur-{seconds}",
                                            th { scope: "row", "{duration_label(seconds)}" }
                                            td { class: "num", "{watts_text(best.map(|b| b.watts))}" }
                                            td { class: "rv-dim",
                                                if let Some(b) = best {
                                                    "{chart::fmt_stamp(b.started_unix_s)} · {b.route_name}"
                                                } else {
                                                    "{DASH}"
                                                }
                                            }
                                            td { class: "rv-bar-cell",
                                                span { class: "rv-bar",
                                                    i { style: "width: {pct:.1}%" }
                                                }
                                            }
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
                        "A ~ jelű értékek becsült vagy szimulált wattból számolódnak, ezért nem kerülnek a legjobbak közé."
                    }
                }
            }
            section { class: "rv-card rv-ftp", "aria-label": "FTP",
                h2 { class: "rv-card-title", "FTP" }
                if let Some(estimate) = analysis_now.ftp_estimate.clone() {
                    div { class: "rv-ftp-row",
                        span { class: "rv-ftp-value num", "{estimate.ftp_w:.0}" }
                        span { class: "rv-ftp-unit", "W" }
                        span { class: "rv-ftp-now", "jelenleg beállítva: {current_ftp:.0} W" }
                    }
                    p { class: "rv-card-note",
                        "A legjobb 20 perced {estimate.best_20min_w:.0} W volt ({chart::fmt_stamp(estimate.started_unix_s)}); ennek 95 %-a a becsült FTP."
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
                            "Az FTP már ennyi"
                        } else {
                            "FTP beállítása {estimate.ftp_w:.0} W-ra"
                        }
                    }
                } else {
                    div { class: "rv-note",
                        "Még nincs 20 perces mért teljesítmény. Az Edzések között az „FTP-teszt 20 perc” segít, vagy tekerj (importálj) legalább 20 percet egyben, a lehető legerősebben."
                    }
                }
                p { class: "rv-footnote",
                    "Csak mért watt számít. A trainer saját wattja nem teljesítménymérő, ezért a javaslat is csak annyira pontos, amennyire az. Az FTP-t te állítod át, az app nem teszi meg magától."
                }
            }
            section { class: "rv-card", "aria-label": "Terhelés",
                div { class: "rv-card-head",
                    h2 { class: "rv-card-title", "Terhelés" }
                    Segmented {
                        options: vec![
                            ("week".to_string(), "Heti".to_string()),
                            ("month".to_string(), "Havi".to_string()),
                        ],
                        value: if period_now == Period::Week { "week".to_string() } else { "month".to_string() },
                        on_change: move |value: String| {
                            period.set(if value == "month" { Period::Month } else { Period::Week });
                        },
                        aria_label: "Időszak".to_string(),
                        small: true,
                    }
                }
                p { class: "rv-card-note",
                    "Az utolsó {buckets.len()} {period_name}: {total_rides} edzés, {fmt_duration(total_time)}, TSS {total_tss:.0}. Az oszlop a TSS-t mutatja; a szimulált edzések nem számítanak, a becsült wattú edzések ideje igen, a TSS-ük nem."
                }
                div { class: "rv-trend", role: "img", "aria-label": "Terhelés időszakonként",
                    for bucket in buckets.iter() {
                        {
                            let height = bar_pct(bucket.tss, tss_max);
                            let label = period_label(bucket.start_unix_s, period_now);
                            let title = format!(
                                "{label}: {} edzés, {}, TSS {:.0}",
                                bucket.rides,
                                fmt_duration(bucket.time_s),
                                bucket.tss
                            );
                            rsx! {
                                div { key: "{bucket.start_unix_s}", class: "rv-trend-col", title: "{title}",
                                    span { class: "rv-trend-value num",
                                        if bucket.tss > 0.0 {
                                            "{bucket.tss:.0}"
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
            }
        }
    }
}
