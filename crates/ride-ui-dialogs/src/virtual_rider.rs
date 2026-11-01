//! Floating panel for the simulated rider: the power control shown instead of hardware.

use dioxus::prelude::*;
use ride_ui_kit::format::fmt_opt0;
use ride_ui_kit::Slider;

use crate::helpers::{
    clamp_virtual_power, VIRTUAL_QUICK, VIRTUAL_RIDER_MAX_W, VIRTUAL_RIDER_STEP_W,
};

/// Floating panel shown while the simulated rider is connected: power slider
/// (`on_power` in watts), quick buttons (Z2 / Z4 / sprint / coast) and key hints.
#[component]
pub fn VirtualRiderPanel(power_w: f64, on_power: EventHandler<f64>) -> Element {
    let watts = clamp_virtual_power(power_w);
    let watts_text = fmt_opt0(Some(watts));

    rsx! {
        section { class: "dlg-vr panel", "aria-label": "Szimulált kerékpáros",
            header { class: "panel__head",
                span { class: "dlg-vr__title", "Szimulált kerékpáros" }
                span { class: "badge badge--ok", "Aktív" }
            }
            div { class: "dlg-vr__body",
                div { class: "dlg-vr__meter",
                    span { class: "num num--xl", "{watts_text}", span { class: "unit", "W" } }
                }
                Slider {
                    label: "Teljesítmény".to_string(),
                    value: watts,
                    min: 0.0,
                    max: VIRTUAL_RIDER_MAX_W,
                    step: VIRTUAL_RIDER_STEP_W,
                    unit: "W".to_string(),
                    hint: "Ennyi wattot teker a szimulált kerékpáros.".to_string(),
                    on_change: move |value: f64| on_power.call(clamp_virtual_power(value)),
                }
                div { class: "dlg-vr__quick",
                    for (label , preset_w , hint) in VIRTUAL_QUICK {
                        button {
                            class: "btn btn--sm dlg-quick",
                            r#type: "button",
                            key: "{label}",
                            title: "{hint}",
                            "aria-label": "{hint}",
                            onclick: move |_| on_power.call(clamp_virtual_power(preset_w)),
                            span { class: "dlg-quick__label", "{label}" }
                            span { class: "dlg-quick__w", "{fmt_opt0(Some(preset_w))} W" }
                        }
                    }
                }
                div { class: "dlg-vr__keys",
                    span { class: "kbd", "↑" }
                    span { class: "kbd", "↓" }
                    span { class: "t-caption", "±10 W" }
                }
            }
        }
    }
}
