//! End-of-ride summary dialog: hero statistics, the comparison with the ghost and the segment
//! table, then the power / heart-rate zone distribution.

use std::sync::Arc;

use dioxus::prelude::*;
use ride_model::{RideComparison, RideSummary, Route};
use ride_ui_kit::format::{fmt_delta_duration, fmt_duration, fmt_km, fmt_m, fmt_opt0};
use ride_ui_kit::palette::{hr_zone_name, hr_zone_var, power_zone_name, power_zone_var};
use ride_ui_kit::{Dialog, Icon, IconName};

use crate::helpers::{fmt_date, fmt_if, hr_zone_bars, power_zone_bars, segment_rows, SegmentRow};
use ride_i18n::t;

/// End-of-ride summary: headline numbers, the ghost difference and the segment table, zone
/// distribution bars, "Mentés TCX-ként", "Új útvonal". Closing is the title bar's × (the footer
/// carries actions only).
#[component]
pub fn SummaryDialog(
    open: bool,
    summary: ReadSignal<Option<RideSummary>>,
    /// The loaded route: the segment table takes the climbs' distance ranges from it.
    route: ReadSignal<Option<Arc<Route>>>,
    /// What the ride did against the history (ghost, segment PRs).
    comparison: ReadSignal<Option<RideComparison>>,
    on_save_tcx: EventHandler<()>,
    on_close: EventHandler<()>,
    on_new_ride: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    let value = summary.read().clone();
    let comparison_value = comparison.read().clone();
    let route_value = route.read().clone();
    let has_summary = value.is_some();

    rsx! {
        Dialog {
            open: true,
            title: t("ui_dialogs.summary.tura_osszegzese").to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                button {
                    class: "btn btn--ghost",
                    r#type: "button",
                    disabled: !has_summary,
                    onclick: move |_| on_new_ride.call(()),
                    Icon { name: IconName::ChevronLeft, size: 14 }
                    {t("ui_dialogs.summary.uj_utvonal")}
                }
                div { class: "dlg-foot__end",
                    button {
                        class: "btn btn--primary",
                        r#type: "button",
                        disabled: !has_summary,
                        onclick: move |_| on_save_tcx.call(()),
                        Icon { name: IconName::Download, size: 14 }
                        {t("ui_dialogs.summary.mentes_tcx_kent")}
                    }
                }
            },
            if let Some(ride) = value {
                SummaryBody {
                    summary: ride,
                    route: route_value,
                    comparison: comparison_value,
                }
            } else {
                div { class: "dlg-empty",
                    span { class: "dlg-empty__lead", {t("ui_dialogs.summary.meg_nincs_osszegzes")} }
                    span { {t("ui_dialogs.summary.a_tura_befejezese_utan_itt_jelennek_meg_az_adatok")} }
                }
            }
        }
    }
}

/// The statistics and zone bars of one finished ride.
#[component]
fn SummaryBody(
    summary: RideSummary,
    route: Option<Arc<Route>>,
    comparison: Option<RideComparison>,
) -> Element {
    let power_bars = power_zone_bars(&summary.power_zone_secs);
    let hr_bars = hr_zone_bars(&summary.hr_zone_secs);
    let headline = format!(
        "{} · {}",
        fmt_date(summary.started_unix_s),
        fmt_duration(summary.elapsed_s)
    );
    let avg_speed_kmh = fmt_opt0(Some(summary.avg_speed_ms.max(0.0) * 3.6));
    let rows = comparison
        .as_ref()
        .map(|comparison| segment_rows(comparison, route.as_deref()))
        .unwrap_or_default();
    let simulated = comparison.as_ref().is_some_and(|c| c.simulated);
    let ghost_delta = comparison
        .as_ref()
        .and_then(|comparison| comparison.ghost_delta_s);

    rsx! {
        div { class: "dlg-summary",
            header { class: "dlg-summary__head",
                div { class: "dlg-summary__names",
                    span { class: "t-eyebrow", {t("ui_dialogs.summary.befejezett_tura")} }
                    h3 { class: "dlg-summary__route", "{summary.route_name}" }
                }
                div { class: "dlg-summary__meta",
                    span { class: "badge badge--ok", {t("ui_dialogs.summary.teljesitve")} }
                    span { class: "t-caption", "{headline}" }
                }
            }

            // one ruled sheet: the first cell is the distance, 2 columns wide, everything else 1
            div { class: "cells cells--4",
                Stat { wide: true, label: t("ui_dialogs.summary.tavolsag").to_string(), value: fmt_km(summary.dist), unit: "km".to_string() }
                Stat { label: t("ui_dialogs.summary.ido").to_string(), value: fmt_duration(summary.elapsed_s), unit: "".to_string() }
                Stat { label: t("ui_dialogs.summary.szintemelkedes").to_string(), value: fmt_m(summary.ascent), unit: "m".to_string() }
                Stat { label: t("ui_dialogs.summary.atlagteljesitmeny").to_string(), value: fmt_opt0(summary.avg_power_w), unit: "W".to_string() }
                Stat { label: t("ui_dialogs.summary.normalizalt_teljesitmeny").to_string(), value: fmt_opt0(summary.normalized_power_w), unit: "W".to_string() }
                Stat { label: t("ui_dialogs.summary.intenzitas_if").to_string(), value: fmt_if(summary.intensity_factor), unit: "".to_string() }
                Stat { label: "TSS".to_string(), value: fmt_opt0(summary.tss), unit: "".to_string() }
                Stat { label: t("ui_dialogs.summary.energia").to_string(), value: fmt_opt0(Some(summary.kcal)), unit: "kcal".to_string() }
                Stat { label: t("ui_dialogs.summary.atlagsebesseg").to_string(), value: avg_speed_kmh, unit: "km/h".to_string() }
                Stat { label: t("ui_dialogs.summary.atlagpulzus").to_string(), value: fmt_opt0(summary.avg_hr), unit: "bpm".to_string() }
                Stat { label: t("ui_dialogs.settings.max_pulzus").to_string(), value: fmt_opt0(summary.max_hr), unit: "bpm".to_string() }
            }

            // The comparison with the history: the ghost at the finish line and one row per
            // segment. A simulated ride shows the numbers but is marked as not counting.
            if simulated || ghost_delta.is_some() || !rows.is_empty() {
                section { class: "dlg-compare", aria_label: t("ui_dialogs.summary.osszehasonlitas_a_korabbi_menetekkel").to_string(),
                    if simulated {
                        p { class: "dlg-compare__note",
                            {t("ui_dialogs.summary.szimulalt_menet_nem_szamit_pr_nak_es_a_szellemek_koz")}
                        }
                    }
                    if let Some(delta) = ghost_delta {
                        div { class: "dlg-ghost",
                            span { class: "dlg-ghost__label", {t("ui_dialogs.summary.szellemhez_kepest")} }
                            span {
                                class: if delta < 0.0 { "dlg-ghost__value dlg-ghost__value--ahead" } else { "dlg-ghost__value dlg-ghost__value--behind" },
                                "{fmt_delta_duration(delta)}"
                            }
                        }
                    }
                    if !rows.is_empty() {
                        div { class: "dlg-head",
                            h4 { class: "dlg-head__title", {t("ui_dialogs.summary.szegmensek")} }
                            span { class: "dlg-head__aside", {t("ui_dialogs.summary.ido_korabbi_pr")} }
                        }
                        div { class: "dlg-segment",
                            for row in rows {
                                SegmentLine { key: "{row.climb}", row: row.clone() }
                            }
                        }
                    }
                }
            } else if comparison.is_some() {
                // The ride was compared, but there was nothing to compare it with: say so,
                // otherwise the missing ghost looks like a bug.
                section { class: "dlg-compare", aria_label: t("ui_dialogs.summary.osszehasonlitas_a_korabbi_menetekkel").to_string(),
                    p { class: "dlg-compare__note",
                        {t("ui_dialogs.summary.ehhez_az_utvonalhoz_meg_nincs_korabbi_menet_ezert_ni")}
                    }
                }
            }

            div { class: "dlg-zones",
                section { class: "dlg-zonecard", aria_label: t("ui_dialogs.summary.teljesitmenyzonak").to_string(),
                    div { class: "dlg-head",
                        h4 { class: "dlg-head__title", {t("ui_dialogs.summary.teljesitmenyzonak")} }
                        span { class: "dlg-head__aside", {t("ui_dialogs.summary.mert_ido")} }
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
                section { class: "dlg-zonecard", aria_label: t("ui_dialogs.summary.pulzuszonak").to_string(),
                    div { class: "dlg-head",
                        h4 { class: "dlg-head__title", {t("ui_dialogs.summary.pulzuszonak")} }
                        span { class: "dlg-head__aside", {t("ui_dialogs.summary.mert_ido")} }
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

/// One segment of the ride: the climb (with its distance range), this ride's time, the PR it
/// was measured against and the "new PR" badge.
#[component]
fn SegmentLine(row: SegmentRow) -> Element {
    rsx! {
        div { class: "dlg-segment__row",
            span { class: "dlg-segment__name", "{row.name}" }
            if !row.range.is_empty() {
                span { class: "dlg-segment__range", "{row.range}" }
            }
            span { class: "dlg-segment__cell", aria_label: t("ui_dialogs.summary.ido").to_string(), "{row.time}" }
            span { class: "dlg-segment__cell dlg-segment__cell--pr", aria_label: t("ui_dialogs.summary.korabbi_pr").to_string(), "{row.previous}" }
            if row.is_pr {
                span { class: "badge badge--ok", {t("ui_dialogs.summary.uj_pr")} }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use ride_model::{Climb, SegmentResult};

    #[derive(Clone, PartialEq, Props)]
    struct HarnessProps {
        summary: RideSummary,
        comparison: Option<RideComparison>,
        route: Option<Arc<Route>>,
    }

    #[component]
    fn Harness(props: HarnessProps) -> Element {
        let summary = use_signal(|| Some(props.summary.clone()));
        let comparison = use_signal(|| props.comparison.clone());
        let route = use_signal(|| props.route.clone());
        rsx! {
            SummaryDialog {
                open: true,
                summary,
                route,
                comparison,
                on_save_tcx: move |_| {},
                on_close: move |_| {},
                on_new_ride: move |_| {},
            }
        }
    }

    fn render(
        summary: RideSummary,
        comparison: Option<RideComparison>,
        route: Option<Arc<Route>>,
    ) -> String {
        let mut vdom = VirtualDom::new_with_props(
            Harness,
            HarnessProps {
                summary,
                comparison,
                route,
            },
        );
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    fn ride() -> RideSummary {
        RideSummary {
            route_name: "Budai kör".into(),
            route_id: "demo".into(),
            started_unix_s: 1_790_000_000.0,
            elapsed_s: 6_129.0,
            dist: 24_100.0,
            ascent: 412.0,
            avg_speed_ms: 6.9,
            max_speed_ms: 13.2,
            avg_power_w: Some(212.0),
            max_power_w: Some(540.0),
            normalized_power_w: Some(238.0),
            intensity_factor: Some(1.01),
            tss: Some(103.0),
            work_kj: 980.0,
            kcal: 980.0,
            avg_hr: Some(141.0),
            max_hr: Some(172.0),
            avg_cadence: Some(86.0),
            power_zone_secs: [900.0, 1_200.0, 1_400.0, 1_300.0, 900.0, 400.0, 29.0],
            hr_zone_secs: [300.0, 1_500.0, 2_600.0, 1_600.0, 129.0],
        }
    }

    fn route() -> Route {
        let mut route = straight_route(&[(24_000.0, 2.0)], 100.0);
        route.id = "demo".into();
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
                category: None,
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
                category: None,
            },
        ];
        route
    }

    fn comparison() -> RideComparison {
        RideComparison {
            simulated: false,
            ghost_delta_s: Some(-32.0),
            segments: vec![
                SegmentResult {
                    climb: 0,
                    time_s: 1_104.0,
                    previous_pr_s: Some(1_121.0),
                    is_pr: true,
                },
                SegmentResult {
                    climb: 1,
                    time_s: 512.0,
                    previous_pr_s: Some(505.0),
                    is_pr: false,
                },
            ],
        }
    }

    #[test]
    fn the_summary_shows_the_ghost_difference_and_the_segment_table() {
        let html = render(ride(), Some(comparison()), Some(Arc::new(route())));
        assert!(html.contains("dlg-compare"), "comparison block missing");
        assert!(html.contains("Szellemhez képest"), "ghost line missing");
        assert!(html.contains("-0:32"), "signed ghost difference missing");
        assert!(html.contains("dlg-ghost__value--ahead"), "ahead is green");
        assert!(html.contains("Szegmensek"), "segment table missing");
        assert_eq!(html.matches("dlg-segment__row").count(), 2, "two segments");
        assert!(html.contains("1. emelkedő") && html.contains("2. emelkedő"));
        assert!(html.contains("4.30–11.10 km"), "distance range missing");
        assert!(html.contains("18:24"), "segment time missing");
        assert!(html.contains("18:41"), "previous PR missing");
        assert_eq!(html.matches("Új PR!").count(), 1, "one new PR badge");
        assert!(!html.contains("Szimulált"), "a real ride is not marked");
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn a_simulated_ride_is_marked_as_not_counting() {
        let mut comparison = comparison();
        comparison.simulated = true;
        let html = render(ride(), Some(comparison), Some(Arc::new(route())));
        assert!(html.contains("Szimulált menet"), "{html}");
        assert!(html.contains("nem számít PR-nak"));
    }

    #[test]
    fn a_first_ride_says_that_there_is_nothing_to_compare_yet() {
        let html = render(
            ride(),
            Some(RideComparison::default()),
            Some(Arc::new(route())),
        );
        assert!(html.contains("dlg-compare"), "the note block is missing");
        assert!(
            html.contains("még nincs korábbi menet"),
            "the first-ride note is missing"
        );
        assert!(
            !html.contains("Szellemhez képest"),
            "no ghost difference yet"
        );
    }

    #[test]
    fn without_a_comparison_the_summary_is_the_old_sheet() {
        let html = render(ride(), None, Some(Arc::new(route())));
        assert!(!html.contains("dlg-compare"));
        assert!(!html.contains("Szegmensek"));
        assert!(html.contains("Teljesítményzónák"), "zone bars still there");
        assert!(html.contains("Mentés TCX-ként"));
        assert!(!html.contains("Strava"), "no upload any more");
    }

    #[test]
    fn a_segment_without_a_previous_pr_prints_a_dash() {
        let mut partial = comparison();
        partial.ghost_delta_s = None;
        partial.segments[0].previous_pr_s = None;
        partial.segments[0].is_pr = false;
        let html = render(ride(), Some(partial.clone()), Some(Arc::new(route())));
        assert!(html.contains('\u{2014}'), "missing PR is an em dash");
        assert!(!html.contains("dlg-ghost__value"), "no ghost line");
        // the route may be gone (the dialog was opened after an unload): the rows survive
        let html = render(ride(), Some(partial), None);
        assert_eq!(html.matches("dlg-segment__row").count(), 2);
        assert!(
            !html.contains("4.30\u{2013}11.10 km"),
            "no range without a route"
        );
    }
}
