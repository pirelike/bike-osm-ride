//! The phone ride screen's head unit: one card with everything needed while pedalling, so the
//! rider never has to switch tabs mid-effort.
//!
//! With a workout the card leads with the block (name, the big countdown, what comes next, the
//! coach's message), and the power hero is measured against the block's target: the target and
//! the difference sit beside the numeral and the target is marked on the zone bar. Without a
//! workout the same hero shows the instant power and W/kg instead. A row of four small cells
//! (cadence, heart rate, speed, grade) closes the card; the shell puts the profile under it.

use dioxus::prelude::*;
use ride_model::{RideSnapshot, RideState, RiderSettings, Theme, WorkoutProgress};
use ride_ui_kit::format::{fmt_duration, fmt_hm_text, fmt_opt0};
use ride_ui_kit::palette;

use crate::{zone_bar_pct, MetricView, DASH};

/// How far the power may be from the target and still count as "on target": 5 % of the target,
/// at least 5 W.
pub fn target_tolerance_w(target_w: f64) -> f64 {
    (target_w.abs() * 0.05).max(5.0)
}

/// How the live power relates to the target: `("ok" | "low" | "high", text)`. `None` without
/// a power or a target.
pub fn target_state(power_w: Option<f64>, target_w: Option<f64>) -> Option<(&'static str, String)> {
    let power = power_w.filter(|p| p.is_finite())?;
    let target = target_w.filter(|t| t.is_finite() && *t > 0.0)?;
    let delta = power - target;
    Some(if delta.abs() <= target_tolerance_w(target) {
        ("ok", "célban".to_string())
    } else if delta < 0.0 {
        ("low", format!("−{:.0} W", delta.abs()))
    } else {
        ("high", format!("+{delta:.0} W"))
    })
}

/// The workout block at the top of the card.
#[component]
fn DashWorkout(w: WorkoutProgress) -> Element {
    let zone = w
        .ftp_pct
        .map(ride_model::power_zone_of_pct)
        .filter(|z| *z > 0);
    let color = zone.map_or("var(--text)", palette::power_zone_var);
    let block_pct = format!("{:.1}", w.step_progress * 100.0);
    let next = match (&w.next_step_name, w.next_target_w) {
        (Some(name), Some(target)) => Some(format!("{name} · {target:.0} W")),
        (Some(name), None) => Some(name.clone()),
        _ => None,
    };
    if w.finished {
        return rsx! {
            div { class: "hud-dash__wo hud-dash__wo--done",
                span { class: "hud-dash__step", "Edzés kész" }
                span { class: "hud-label", "{w.workout_name}" }
            }
        };
    }
    rsx! {
        div { class: "hud-dash__wo",
            div { class: "hud-dash__wohead",
                div { class: "hud-dash__wotext",
                    span { class: "hud-dash__step", "{w.step_name}" }
                    span { class: "hud-label",
                        "{w.step_index + 1}/{w.step_count} · edzés vége {fmt_hm_text(w.total_remaining_s)}"
                    }
                }
                span { class: "num num--lg hud-dash__left", "{fmt_duration(w.step_remaining_s)}" }
            }
            div { class: "progress hud-dash__bar", style: "--p: {block_pct}; --c: {color}", i {} }
            if let Some(next) = next {
                div { class: "hud-dash__next",
                    span { class: "hud-label", "Következik" }
                    span { class: "hud-dash__nexttext", "{next}" }
                }
            }
            if let Some(message) = &w.message {
                p { key: "{message.id}", class: "hud-dash__coach", role: "status", "{message.text}" }
            }
        }
    }
}

/// The seven zones with the live power as a solid pin and the target as an open bracket.
#[component]
fn ZoneBar(zone: Option<u8>, power_pin: Option<f64>, target_pin: Option<f64>) -> Element {
    rsx! {
        div { class: "hud-dash__zones", aria_hidden: "true",
            for z in 1..=7u8 {
                span {
                    key: "{z}",
                    class: if zone == Some(z) { "hud-dash__seg is-current" } else { "hud-dash__seg" },
                    style: "--c: {palette::power_zone_var(z)}",
                }
            }
            if let Some(pct) = target_pin {
                span { class: "hud-dash__goal", style: "left: {pct:.2}%" }
            }
            if let Some(pct) = power_pin {
                span { class: "hud-dash__pin", style: "left: {pct:.2}%" }
            }
        }
    }
}

/// The phone head unit: workout block (when one runs), power against its target or with the
/// instant power and W/kg, the zone bar and four small cells.
#[component]
pub fn RideDash(
    snapshot: ReadSignal<RideSnapshot>,
    rider: ReadSignal<RiderSettings>,
    theme: ReadSignal<Theme>,
    /// The map tab's version: one line for the workout instead of its block.
    #[props(default)]
    slim: bool,
) -> Element {
    let snap = snapshot.read();
    let rider_v = rider.read();
    let view = MetricView::new(&snap, &rider_v);
    let theme = *theme.read();
    let workout = snap.workout.clone();
    let running = workout.as_ref().filter(|w| !w.finished);
    let target = running.and_then(|w| w.target_w);
    let power = snap.power_3s.filter(|p| p.is_finite());
    let state = target_state(power, target);
    let ftp = rider_v.ftp_w;
    let power_pin = zone_bar_pct(power, ftp);
    let target_pin = target.and_then(|t| zone_bar_pct(Some(t), ftp));
    let cadence_goal = running.and_then(|w| w.cadence);
    let grade_color = view.grade.map(|g| palette::grade_color(g, theme));
    let hr_color = view
        .hr_zone
        .map_or("var(--text-faint)", palette::hr_zone_var);
    let power_dim = view.power_text == DASH;

    rsx! {
        section { class: "hud-dash", aria_label: "Tekerés közben",
            if let Some(w) = workout.clone() {
                if slim {
                    if !w.finished {
                        div { class: "hud-dash__woline",
                            span { class: "hud-dash__nexttext", "{w.step_name}" }
                            span { class: "num num--sm", "{fmt_duration(w.step_remaining_s)}" }
                            if let Some(name) = &w.next_step_name {
                                span { class: "hud-label hud-dash__wonext", "→ {name}" }
                            }
                        }
                    }
                } else {
                    DashWorkout { w }
                }
            }
            div { class: "hud-dash__power",
                div { class: if power_dim { "num num--xl hud-dash__watts is-dim" } else { "num num--xl hud-dash__watts" },
                    "{view.power_text}"
                    span { class: "unit", "W" }
                }
                div { class: "hud-dash__side",
                    if let Some(t) = target {
                        div { class: "hud-kv",
                            span { class: "hud-label", "Cél" }
                            span { class: "num num--md", "{fmt_opt0(Some(t))}" span { class: "unit", "W" } }
                        }
                        if let Some((kind, text)) = state.clone() {
                            span { class: "hud-dash__delta hud-dash__delta--{kind}", "{text}" }
                        }
                    } else {
                        div { class: "hud-kv",
                            span { class: "hud-label", "Azonnali" }
                            span { class: "num num--md", "{view.power_instant_text}" span { class: "unit", "W" } }
                        }
                        div { class: "hud-kv",
                            span { class: "hud-label", "Fajlagos" }
                            span { class: "num num--md", "{view.wkg_text}" span { class: "unit", "W/kg" } }
                        }
                    }
                }
            }
            ZoneBar { zone: view.zone, power_pin, target_pin }
            div { class: "hud-dash__zonefoot",
                if let Some(z) = view.zone {
                    span { class: "hud-zone",
                        span { class: "hud-sw", style: "--c: {palette::power_zone_var(z)}" }
                        span { class: "hud-zone__name", "Z{z} {view.zone_name}" }
                    }
                } else {
                    span { class: "hud-label", "Nincs teljesítmény" }
                }
                span { class: "hud-label", "FTP {view.ftp_text} W" }
            }
            div { class: "cells cells--4 cells--flush hud-dash__cells",
                div { class: "cell cell--md hud-dash__cell",
                    div { class: "cell__label",
                        span { "Kadencia" }
                        if let Some(c) = cadence_goal {
                            span { class: "hud-label--faint", "cél {c}" }
                        }
                    }
                    div { class: "cell__value", "{view.cadence_text}" span { class: "cell__unit", "rpm" } }
                }
                div { class: "cell cell--md hud-dash__cell",
                    div { class: "cell__label",
                        span { "Pulzus" }
                        if view.hr_zone.is_some() {
                            span { class: "hud-sw", style: "--c: {hr_color}" }
                        }
                    }
                    div { class: "cell__value", "{view.hr_text}" span { class: "cell__unit", "bpm" } }
                }
                div { class: "cell cell--md hud-dash__cell",
                    div { class: "cell__label", span { "Sebesség" } }
                    div { class: "cell__value", "{view.speed_text}" span { class: "cell__unit", "km/h" } }
                }
                div { class: "cell cell--md hud-dash__cell",
                    div { class: "cell__label",
                        span { "Meredekség" }
                        if let Some(color) = grade_color.clone() {
                            span { class: "hud-sw", style: "--c: {color}" }
                        }
                    }
                    div { class: "cell__value", "{view.grade_text}" span { class: "cell__unit", "%" } }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_power_is_judged_against_the_target_with_a_tolerance() {
        assert_eq!(target_state(Some(242.0), Some(240.0)).unwrap().0, "ok");
        assert_eq!(
            target_state(Some(236.0), Some(240.0)).unwrap().0,
            "ok",
            "within 5 %"
        );
        let (kind, text) = target_state(Some(200.0), Some(240.0)).unwrap();
        assert_eq!((kind, text.as_str()), ("low", "−40 W"));
        let (kind, text) = target_state(Some(300.0), Some(240.0)).unwrap();
        assert_eq!((kind, text.as_str()), ("high", "+60 W"));
        // a low target still has a 5 W window
        assert_eq!(target_state(Some(84.0), Some(80.0)).unwrap().0, "ok");
        assert_eq!(target_state(None, Some(240.0)), None);
        assert_eq!(target_state(Some(200.0), None), None);
        assert_eq!(target_state(Some(f64::NAN), Some(240.0)), None);
    }
}
