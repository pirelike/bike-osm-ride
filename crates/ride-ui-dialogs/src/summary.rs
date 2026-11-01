//! End-of-ride summary dialog: hero statistics and the power / heart-rate zone distribution.

use dioxus::prelude::*;
use ride_model::RideSummary;
use ride_ui_kit::format::{fmt_duration, fmt_km, fmt_m, fmt_opt0};
use ride_ui_kit::palette::{hr_zone_name, hr_zone_var, power_zone_name, power_zone_var};
use ride_ui_kit::{Dialog, Icon, IconName};

use crate::helpers::{fmt_date, fmt_if, hr_zone_bars, power_zone_bars};

/// End-of-ride summary: headline numbers, zone distribution bars, "Mentés TCX-ként",
/// "Új útvonal" / "Bezárás".
#[component]
pub fn SummaryDialog(
    open: bool,
    summary: ReadSignal<Option<RideSummary>>,
    on_save_tcx: EventHandler<()>,
    on_close: EventHandler<()>,
    on_new_ride: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    let value = summary.read().clone();
    let has_summary = value.is_some();

    rsx! {
        Dialog {
            open: true,
            title: "Túra összegzése".to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                button {
                    class: "btn btn--ghost",
                    r#type: "button",
                    disabled: !has_summary,
                    onclick: move |_| on_new_ride.call(()),
                    Icon { name: IconName::ChevronLeft, size: 14 }
                    "Új útvonal"
                }
                div { class: "dlg-foot__end",
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| on_close.call(()),
                        "Bezárás"
                    }
                    button {
                        class: "btn btn--primary",
                        r#type: "button",
                        disabled: !has_summary,
                        onclick: move |_| on_save_tcx.call(()),
                        Icon { name: IconName::Download, size: 14 }
                        "Mentés TCX-ként"
                    }
                }
            },
            if let Some(ride) = value {
                SummaryBody { summary: ride }
            } else {
                div { class: "dlg-empty",
                    span { class: "dlg-empty__lead", "Még nincs összegzés." }
                    span { "A túra befejezése után itt jelennek meg az adatok." }
                }
            }
        }
    }
}

/// The statistics and zone bars of one finished ride.
#[component]
fn SummaryBody(summary: RideSummary) -> Element {
    let power_bars = power_zone_bars(&summary.power_zone_secs);
    let hr_bars = hr_zone_bars(&summary.hr_zone_secs);
    let headline = format!(
        "{} · {}",
        fmt_date(summary.started_unix_s),
        fmt_duration(summary.elapsed_s)
    );
    let avg_speed_kmh = fmt_opt0(Some(summary.avg_speed_ms.max(0.0) * 3.6));

    rsx! {
        div { class: "dlg-summary",
            header { class: "dlg-summary__head",
                div { class: "dlg-summary__names",
                    span { class: "t-eyebrow", "Befejezett túra" }
                    h3 { class: "dlg-summary__route", "{summary.route_name}" }
                }
                div { class: "dlg-summary__meta",
                    span { class: "badge badge--ok", "Teljesítve" }
                    span { class: "t-caption", "{headline}" }
                }
            }

            // one ruled sheet: the first cell is the distance, 2 columns wide, everything else 1
            div { class: "cells cells--4",
                Stat { wide: true, label: "Távolság".to_string(), value: fmt_km(summary.dist), unit: "km".to_string() }
                Stat { label: "Idő".to_string(), value: fmt_duration(summary.elapsed_s), unit: "".to_string() }
                Stat { label: "Szintemelkedés".to_string(), value: fmt_m(summary.ascent), unit: "m".to_string() }
                Stat { label: "Átlagteljesítmény".to_string(), value: fmt_opt0(summary.avg_power_w), unit: "W".to_string() }
                Stat { label: "Normalizált teljesítmény".to_string(), value: fmt_opt0(summary.normalized_power_w), unit: "W".to_string() }
                Stat { label: "Intenzitás (IF)".to_string(), value: fmt_if(summary.intensity_factor), unit: "".to_string() }
                Stat { label: "TSS".to_string(), value: fmt_opt0(summary.tss), unit: "".to_string() }
                Stat { label: "Energia".to_string(), value: fmt_opt0(Some(summary.kcal)), unit: "kcal".to_string() }
                Stat { label: "Átlagsebesség".to_string(), value: avg_speed_kmh, unit: "km/h".to_string() }
                Stat { label: "Átlagpulzus".to_string(), value: fmt_opt0(summary.avg_hr), unit: "bpm".to_string() }
                Stat { label: "Max. pulzus".to_string(), value: fmt_opt0(summary.max_hr), unit: "bpm".to_string() }
            }

            div { class: "dlg-zones",
                section { class: "dlg-zonecard", aria_label: "Teljesítményzónák",
                    div { class: "dlg-head",
                        h4 { class: "dlg-head__title", "Teljesítményzónák" }
                        span { class: "dlg-head__aside", "Mért idő" }
                    }
                    for bar in power_bars {
                        ZoneRow {
                            key: "p{bar.zone}",
                            label: format!("Z{} · {}", bar.zone, power_zone_name(bar.zone)),
                            var: power_zone_var(bar.zone).to_string(),
                            secs: bar.secs,
                            pct: bar.pct,
                        }
                    }
                }
                section { class: "dlg-zonecard", aria_label: "Pulzuszónák",
                    div { class: "dlg-head",
                        h4 { class: "dlg-head__title", "Pulzuszónák" }
                        span { class: "dlg-head__aside", "Mért idő" }
                    }
                    for bar in hr_bars {
                        ZoneRow {
                            key: "h{bar.zone}",
                            label: format!("H{} · {}", bar.zone, hr_zone_name(bar.zone)),
                            var: hr_zone_var(bar.zone).to_string(),
                            secs: bar.secs,
                            pct: bar.pct,
                        }
                    }
                }
            }
        }
    }
}

/// One headline number as a ruled cell: small label on top, the numeral on the baseline with
/// its unit. `wide` makes it the 2-column lead cell with the 56 px numeral.
#[component]
fn Stat(
    #[props(default = false)] wide: bool,
    label: String,
    value: String,
    unit: String,
) -> Element {
    let class = if wide {
        "cell cell--span2"
    } else {
        "cell cell--md"
    };
    rsx! {
        div { class,
            span { class: "cell__label", "{label}" }
            span { class: "cell__value",
                "{value}"
                if !unit.is_empty() {
                    span { class: "cell__unit", "{unit}" }
                }
            }
        }
    }
}

/// One zone row: name, flat bar, time and percentage. The colour comes from the token variable
/// of the zone, so the same markup works in both themes.
#[component]
fn ZoneRow(label: String, var: String, secs: f64, pct: f64) -> Element {
    // A zone with a sliver of time still gets a visible bar.
    let width = if pct > 0.0 {
        pct.clamp(1.5, 100.0)
    } else {
        0.0
    };
    rsx! {
        div { class: "dlg-zone", style: "--c: {var}; --p: {width:.2}",
            span { class: "dlg-zone__name", "{label}" }
            span { class: "dlg-zone__track", i {} }
            span { class: "dlg-zone__time", "{fmt_duration(secs)}" }
            span { class: "dlg-zone__pct", "{fmt_opt0(Some(pct))}%" }
        }
    }
}
