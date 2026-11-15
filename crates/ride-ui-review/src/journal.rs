//! The "Napló" view of the review page: every ride of every route in time order, a week at a
//! time, with the week's totals, its load day by day, and the rider's own rating and note.
//!
//! The weeks come from `ride_sim::journal` (pure, tested); a ride opens the ride view.

use dioxus::prelude::*;
use ride_model::{RiderSettings, RouteRides};
use ride_sim::fitness::LoadSource;
use ride_sim::journal::{journal, JournalRide};
use ride_sim::power_curve::Period;
use ride_ui_kit::format::{fmt_duration, fmt_km};

use crate::power::{bar_pct, period_label, watts_text};
use crate::{chart, kind_tags, DASH};
use ride_i18n::{t, tf};

/// Weeks shown at first, and added by "older weeks".
pub const WEEKS_PER_PAGE: usize = 8;

/// A note longer than this is cut in the list (the ride view shows all of it).
const NOTE_PREVIEW_CHARS: usize = 90;

/// The short names of the days, Monday first.
pub fn day_names() -> [&'static str; 7] {
    [
        t("ui_review.journal.nap_h"),
        t("ui_review.journal.nap_k"),
        t("ui_review.journal.nap_sze"),
        t("ui_review.journal.nap_cs"),
        t("ui_review.journal.nap_p"),
        t("ui_review.journal.nap_szo"),
        t("ui_review.journal.nap_v"),
    ]
}

/// The load of a ride as the list shows it: the number, with a mark of where it came from.
pub fn load_text(load: Option<(f64, LoadSource)>) -> String {
    match load {
        Some((value, LoadSource::Power)) => format!("{value:.0}"),
        Some((value, LoadSource::HeartRate)) => tf(
            "ui_review.journal.load_hr",
            &[("n", &format!("{value:.0}"))],
        ),
        Some((value, LoadSource::Estimated)) => format!("~{value:.0}"),
        None => DASH.to_string(),
    }
}

fn preview(note: &str) -> String {
    let mut out: String = note.chars().take(NOTE_PREVIEW_CHARS).collect();
    if note.chars().count() > NOTE_PREVIEW_CHARS {
        out.push('…');
    }
    out
}

#[component]
pub fn JournalView(
    routes: ReadSignal<Vec<RouteRides>>,
    rider: ReadSignal<RiderSettings>,
    /// Opens a ride: `(route id, stem)`.
    on_open_ride: EventHandler<(String, String)>,
) -> Element {
    let mut shown = use_signal(|| WEEKS_PER_PAGE);
    let weeks = use_memo(move || journal(&routes.read(), &rider.read()));
    let weeks = weeks.read();
    let shown_now = *shown.read();
    let days = day_names();

    rsx! {
        div { class: "rv-power rv-journal", role: "region", "aria-label": t("ui_review.journal.naplo"),
            if weeks.is_empty() {
                div { class: "rv-empty",
                    div { class: "rv-empty-title", {t("ui_review.meg_nincs_mentett_edzes")} }
                    div { class: "rv-empty-hint",
                        {t("ui_review.az_edzesek_a_befejezesuk_utan_itt_jelennek_meg_mas_a")}
                    }
                }
            }
            for week in weeks.iter().take(shown_now) {
                {
                    let from = period_label(week.start_unix_s, Period::Week);
                    let to = period_label(week.start_unix_s + 6.0 * 86_400.0, Period::Week);
                    let day_max = week.daily_load.iter().copied().fold(0.0, f64::max);
                    rsx! {
                        section { key: "{week.start_unix_s}", class: "rv-card rv-week",
                            div { class: "rv-card-head",
                                h2 { class: "rv-card-title", "{from} – {to}" }
                                span { class: "rv-week-total",
                                    {tf(
                                        "ui_review.journal.het_osszesen",
                                        &[
                                            ("rides", &week.rides.len().to_string()),
                                            ("time", &fmt_duration(week.time_s)),
                                            ("km", &fmt_km(week.dist_m)),
                                            ("load", &format!("{:.0}", week.load)),
                                        ],
                                    )}
                                }
                            }
                            div { class: "rv-days", role: "img", "aria-label": t("ui_review.journal.napi_terheles"),
                                for (i , load) in week.daily_load.iter().enumerate() {
                                    div { key: "d{i}", class: "rv-day",
                                        span { class: "rv-day-bar", i { style: "height: {bar_pct(*load, day_max):.1}%" } }
                                        span { class: "rv-day-name", "{days[i]}" }
                                        span { class: "rv-day-load num", if *load > 0.0 { "{load:.0}" } }
                                    }
                                }
                            }
                            ul { class: "rv-journal-list",
                                for ride in week.rides.iter() {
                                    JournalRow { key: "{ride.route_id}-{ride.meta.stem}", ride: ride.clone(), on_open_ride }
                                }
                            }
                        }
                    }
                }
            }
            if weeks.len() > shown_now {
                button {
                    class: "btn rv-more",
                    r#type: "button",
                    onclick: move |_| shown.with_mut(|n| *n += WEEKS_PER_PAGE),
                    {t("ui_review.journal.regebbi_hetek")}
                }
            }
        }
    }
}

#[component]
fn JournalRow(ride: JournalRide, on_open_ride: EventHandler<(String, String)>) -> Element {
    let meta = &ride.meta;
    let tags = kind_tags(meta);
    let route_id = ride.route_id.clone();
    let stem = meta.stem.clone();
    let power = meta.np_w.or(meta.avg_power_w);
    let estimated = meta.power_estimated || meta.simulated;
    rsx! {
        li { class: "rv-journal-ride",
            button {
                class: "rv-journal-main",
                r#type: "button",
                title: t("ui_review.reszletek").to_string(),
                onclick: move |_| on_open_ride.call((route_id.clone(), stem.clone())),
                span { class: "rv-journal-when",
                    span { class: "rv-ride-date", "{chart::fmt_stamp(meta.started_unix_s)}" }
                    span { class: "rv-ride-meta", "{ride.route_name}" }
                }
                span { class: "rv-journal-what",
                    if let Some(name) = meta.workout_name.as_ref() {
                        span { class: "rv-journal-workout", "{name}" }
                    }
                    span { class: "rv-ride-meta",
                        {tf(
                            "ui_review.journal.sor",
                            &[
                                ("time", &fmt_duration(meta.elapsed_s)),
                                ("km", &fmt_km(meta.dist_m)),
                                ("w", &format!("{}{}", if estimated && power.is_some() { "~" } else { "" }, watts_text(power))),
                                ("load", &load_text(ride.load)),
                            ],
                        )}
                    }
                    if !meta.note.is_empty() {
                        span { class: "rv-journal-note", "{preview(&meta.note)}" }
                    }
                    if !tags.is_empty() {
                        span { class: "rv-tags",
                            for tag in tags.iter() {
                                span { key: "{tag}", class: "tag", "{tag}" }
                            }
                        }
                    }
                }
                span { class: "rv-journal-rpe",
                    if let Some(rpe) = meta.rpe {
                        span { class: "rv-rpe num", title: t("ui_review.detail.rpe_cim").to_string(), "{rpe}" }
                    }
                }
            }
        }
    }
}
