//! Devices dialog: scan for sensors, inspect the four slots and connect/disconnect.

use dioxus::prelude::*;
use ride_model::{ConnState, DeviceInfo, DeviceKind, SensorCommand};
use ride_ui_kit::{Dialog, Icon, IconName};

use crate::helpers::{
    conn_chip_class, conn_failure, conn_icon, conn_label, conn_tone, device_title, is_simulated,
    kind_hint, kind_icon, kind_label, rssi_label, rssi_text, rssi_tone, shared_slot_names,
    slot_connecting, slot_matches, slot_of, sorted_discovered, suggested_kind, SLOT_ORDER,
};
use crate::DeviceSlot;

/// Scan, list discovered devices, connect/disconnect per kind (trainer, HR, power, cadence).
/// Emits `SensorCommand`s (StartScan / StopScan / Connect / Disconnect).
#[component]
pub fn DevicesDialog(
    open: bool,
    discovered: ReadSignal<Vec<DeviceInfo>>,
    slots: ReadSignal<Vec<DeviceSlot>>,
    scanning: bool,
    on_command: EventHandler<SensorCommand>,
    on_close: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    let found = sorted_discovered(&discovered.read());
    let slot_list = slots.read().clone();
    let shared_names = shared_slot_names(&slot_list);
    let connected = slot_list
        .iter()
        .filter(|s| s.state == ConnState::Connected)
        .count();
    let found_count = found.len();
    let found_note = if scanning {
        "A Bluetooth-környezet vizsgálata…".to_string()
    } else if found_count == 0 {
        "Indíts keresést a közeli eszközök felderítéséhez.".to_string()
    } else {
        format!("{found_count} találat – egy eszköz több szerepet is elláthat.")
    };
    let scan_label = if scanning {
        "Keresés leállítása"
    } else {
        "Keresés indítása"
    };
    let scan_icon = if scanning {
        IconName::Stop
    } else {
        IconName::Search
    };
    let scan_class = if scanning {
        "btn btn--sm"
    } else {
        "btn btn--primary btn--sm"
    };

    rsx! {
        Dialog {
            open: true,
            title: "Eszközök".to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note", "Hardver nélkül a szimulált kerékpárossal is kipróbálhatod." }
            },
            div { class: "dlg-devices",
                div { class: "dlg-toolbar",
                    div { class: "dlg-toolbar__status",
                        if scanning {
                            span { class: "spinner" }
                            span { class: "t-body", "Keresés folyamatban…" }
                        } else {
                            span { class: "dot" }
                            span { class: "t-body", "Indíthatod a keresést" }
                        }
                        span { class: "t-caption", "{found_count} találat" }
                        span { class: "t-caption", "{connected} csatlakoztatva" }
                    }
                    button {
                        class: scan_class,
                        r#type: "button",
                        onclick: move |_| {
                            on_command.call(if scanning {
                                SensorCommand::StopScan
                            } else {
                                SensorCommand::StartScan
                            });
                        },
                        Icon { name: scan_icon, size: 14 }
                        "{scan_label}"
                    }
                }

                div { class: "dlg-slots",
                    for kind in SLOT_ORDER {
                        SlotCard {
                            key: "{kind_label(kind)}",
                            kind,
                            slot: slot_of(&slot_list, kind),
                            shared: slot_list
                                .iter()
                                .find(|s| s.kind == kind)
                                .and_then(|s| s.name.clone())
                                .is_some_and(|name| shared_names.contains(&name)),
                            on_command,
                        }
                    }
                }

                section { class: "dlg-found-sec",
                    div { class: "dlg-head",
                        h3 { class: "dlg-head__title", "Talált eszközök" }
                        span { class: "dlg-head__aside", "{found_note}" }
                    }
                    if found.is_empty() {
                        div { class: "dlg-empty",
                            span { class: "dlg-empty__lead", "Nincs találat." }
                            span { "Ellenőrizd, hogy be van kapcsolva a Bluetooth, majd indíts keresést." }
                        }
                    } else {
                        ul { class: "dlg-found-list",
                            for device in found {
                                FoundRow {
                                    key: "{device.id}",
                                    device,
                                    slots: slot_list.clone(),
                                    on_command,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One of the four slots, with its state badge and a disconnect action. `shared` marks a
/// slot of a device that serves several roles: its "Leválasztás" releases the whole device.
#[component]
fn SlotCard(
    kind: DeviceKind,
    slot: Option<DeviceSlot>,
    shared: bool,
    on_command: EventHandler<SensorCommand>,
) -> Element {
    let state = slot
        .as_ref()
        .map(|s| s.state.clone())
        .unwrap_or(ConnState::Disconnected);
    let name_text = slot
        .and_then(|s| s.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Nincs csatlakoztatott eszköz".to_string());
    let tone = conn_tone(&state);
    let chip_class = format!("{} dlg-slot__chip", conn_chip_class(&state));
    let state_text = conn_label(&state);
    let failure = conn_failure(&state).map(str::to_string);
    let can_disconnect = matches!(state, ConnState::Connected | ConnState::Connecting);
    let busy = tone == "busy";
    let action_title = if shared {
        "Az egész eszköz leválasztása (minden szerepével)"
    } else {
        "Leválasztás"
    };

    rsx! {
        article { class: "dlg-slot", "data-state": tone,
            span { class: "dlg-slot__icon", Icon { name: kind_icon(kind), size: 20 } }
            div { class: "dlg-slot__titles",
                span { class: "dlg-slot__title", "{kind_label(kind)}" }
                span { class: "dlg-slot__hint", "{kind_hint(kind)}" }
                if shared {
                    span { class: "tag", title: "Ugyanaz az eszköz több szerepet is ellát", "közös eszköz" }
                }
            }
            div { class: "dlg-slot__body",
                span { class: "dlg-slot__name truncate", "{name_text}" }
                if let Some(reason) = failure {
                    span { class: "dlg-slot__error", "{reason}" }
                }
            }
            div { class: "dlg-slot__state",
                span { class: "{chip_class}",
                    if let Some(icon) = conn_icon(&state) {
                        Icon { name: icon, size: 13 }
                    }
                    if busy {
                        span { class: "dot dot--busy" }
                    }
                    "{state_text}"
                }
                if can_disconnect {
                    button {
                        class: "btn btn--ghost btn--sm dlg-slot__action",
                        r#type: "button",
                        title: "{action_title}",
                        onclick: move |_| on_command.call(SensorCommand::Disconnect(kind)),
                        Icon { name: IconName::Close, size: 13 }
                        "Leválasztás"
                    }
                }
            }
        }
    }
}

/// One discovered device: name, RSSI, the roles it offers and connect/disconnect. A real
/// device is connected once and serves every role it advertises; the simulated rider keeps
/// its per-role buttons so any slot can be driven without hardware.
#[component]
fn FoundRow(
    device: DeviceInfo,
    slots: Vec<DeviceSlot>,
    on_command: EventHandler<SensorCommand>,
) -> Element {
    let title = device_title(&device);
    let suggested = suggested_kind(&device);
    let simulated = is_simulated(&device);
    let tone = rssi_tone(device.rssi);
    let rssi = rssi_text(device.rssi);
    let rssi_hint = rssi_label(device.rssi);
    let icon = suggested.map(kind_icon).unwrap_or(IconName::Bluetooth);
    let connecting = device
        .kinds
        .iter()
        .any(|kind| slot_connecting(&slots, *kind, &title));
    let connected = device
        .kinds
        .iter()
        .any(|kind| slot_matches(&slots, *kind, &title));
    let roles = device
        .kinds
        .iter()
        .map(|kind| kind_label(*kind))
        .collect::<Vec<_>>()
        .join(", ");
    let connect_title = if roles.is_empty() {
        "Csatlakozás".to_string()
    } else {
        format!("Csatlakozás – szerepek: {roles}")
    };

    rsx! {
        li { class: "dlg-found", "data-state": tone,
            span { class: "dlg-found__icon", Icon { name: icon, size: 20 } }
            div { class: "dlg-found__main",
                div { class: "dlg-found__line",
                    span { class: "dlg-found__name truncate", "{title}" }
                    if simulated {
                        span { class: "tag tag--solid", "Szimulált" }
                    }
                    for kind in device.kinds.iter().copied() {
                        span { class: "tag", key: "role-{kind_label(kind)}", "{kind_label(kind)}" }
                    }
                }
                div { class: "dlg-found__line",
                    span { class: "dlg-found__id truncate", "{device.id}" }
                    if !simulated {
                        span { class: "dlg-found__rssi", "data-tone": tone,
                            span { class: "dot dlg-found__dot" }
                            "{rssi}"
                        }
                        span { class: "dlg-found__rssi-text", "{rssi_hint}" }
                    }
                }
            }
            div { class: "dlg-found__actions",
                if connecting {
                    span { class: "dlg-found__wait", role: "status",
                        span { class: "spinner" }
                        "Kapcsolódás…"
                    }
                } else if connected {
                    if let Some(kind) = suggested {
                        button {
                            class: "btn btn--ghost btn--sm",
                            r#type: "button",
                            title: "Az egész eszköz leválasztása",
                            onclick: move |_| on_command.call(SensorCommand::Disconnect(kind)),
                            Icon { name: IconName::Close, size: 13 }
                            "Leválasztás"
                        }
                    }
                } else if let Some(kind) = suggested {
                    button {
                        class: "btn btn--primary btn--sm",
                        r#type: "button",
                        title: "{connect_title}",
                        onclick: {
                            let id = device.id.clone();
                            move |_| on_command.call(SensorCommand::Connect { id: id.clone(), kind })
                        },
                        Icon { name: IconName::Bluetooth, size: 13 }
                        "Csatlakozás"
                    }
                }
                if simulated {
                    for kind in device.kinds.iter().copied() {
                        if Some(kind) != suggested {
                            button {
                                class: "btn btn--ghost btn--sm",
                                r#type: "button",
                                key: "connect-{kind_label(kind)}",
                                title: "Csatlakozás másik szerepként",
                                onclick: {
                                    let id = device.id.clone();
                                    move |_| on_command.call(SensorCommand::Connect { id: id.clone(), kind })
                                },
                                "{kind_label(kind)}"
                            }
                        }
                    }
                }
            }
        }
    }
}
