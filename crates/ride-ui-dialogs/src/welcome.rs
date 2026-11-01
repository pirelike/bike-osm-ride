//! Route dialog: the one place where a route is chosen, at start-up and whenever the top bar's
//! "Útvonal" is pressed. Three numbered ways to start (file, link, demo) and a ruled index of
//! recent routes, in the same dialog frame as every other dialog.

use dioxus::prelude::*;
use ride_model::RecentRoute;
use ride_ui_kit::format::{fmt_km, fmt_m};
use ride_ui_kit::{Dialog, Icon, IconName, TextField};

use crate::helpers::{
    bikerouter_hint, fmt_date, normalize_link_input, route_source_label, sorted_recents,
};

/// The route dialog: file row / "GPX megnyitása" (`on_pick_file`), bikerouter-link field
/// (`on_import_link` with the pasted text), recent routes (open / delete), the demo route
/// button, busy spinner and an inline error line. Closes with Esc, the × or "Bezárás"; the shell
/// also closes it by itself once a route has loaded.
#[component]
pub fn RouteDialog(
    open: bool,
    on_close: EventHandler<()>,
    recents: ReadSignal<Vec<RecentRoute>>,
    busy: ReadSignal<bool>,
    error: ReadSignal<Option<String>>,
    on_pick_file: EventHandler<()>,
    on_import_link: EventHandler<String>,
    on_open_recent: EventHandler<String>,
    on_delete_recent: EventHandler<String>,
    on_demo: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        Dialog {
            open: true,
            title: "Útvonal".to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note",
                    "A GPX-fájlt az ablakra húzva is megnyithatod (Ctrl+O). A betöltés után az ablak bezárul."
                }
                button {
                    class: "btn btn--primary",
                    r#type: "button",
                    onclick: move |_| on_close.call(()),
                    "Bezárás"
                }
            },
            RouteSheet {
                recents,
                busy,
                error,
                on_pick_file,
                on_import_link,
                on_open_recent,
                on_delete_recent,
                on_demo,
            }
        }
    }
}

#[component]
fn RouteSheet(
    recents: ReadSignal<Vec<RecentRoute>>,
    busy: ReadSignal<bool>,
    error: ReadSignal<Option<String>>,
    on_pick_file: EventHandler<()>,
    on_import_link: EventHandler<String>,
    on_open_recent: EventHandler<String>,
    on_delete_recent: EventHandler<String>,
    on_demo: EventHandler<()>,
) -> Element {
    let mut link = use_signal(String::new);
    let mut paste_note = use_signal(|| None::<String>);

    let busy_now = *busy.read();
    let error_now = error.read().clone();
    let recents_now = sorted_recents(&recents.read());
    let text = link.read().clone();
    let hint = bikerouter_hint(&text);
    // An empty field is not an error, it is just empty.
    let field_error = if text.trim().is_empty() {
        None
    } else {
        hint.clone()
    };
    let field_hint = if hint.is_none() {
        "Érvényes bikerouter-link, betölthető.".to_string()
    } else {
        "A bikerouter.de „Megosztás” linkje tartalmazza az útvonalat (lonlats=…).".to_string()
    };
    let can_import = hint.is_none() && !text.trim().is_empty() && !busy_now;
    let paste_message = paste_note.read().clone();
    let recent_count = recents_now.len();

    rsx! {
        div { class: "dlg-welcome",
            header { class: "dlg-hero",
                p { class: "dlg-hero__lead",
                    "Válassz GPX-fájlt, illessz be egy bikerouter-linket, vagy folytasd egy korábbi útvonallal."
                }
            }

            section { class: "dlg-sources", aria_label: "Útvonal betöltése",
                button {
                    class: "dlg-source dlg-drop",
                    r#type: "button",
                    disabled: busy_now,
                    "aria-label": "GPX-fájl kiválasztása",
                    onclick: move |_| on_pick_file.call(()),
                    span { class: "dlg-source__num", "1" }
                    span { class: "dlg-source__text",
                        span { class: "dlg-source__title", "GPX-fájl" }
                        span { class: "dlg-source__sub",
                            "Húzd ide a fájlt, vagy válaszd ki a gépedről."
                        }
                    }
                    span { class: "dlg-source__go dlg-drop__go",
                        Icon { name: IconName::Folder, size: 16 }
                        "GPX megnyitása"
                    }
                }

                div { class: "dlg-source dlg-source--form",
                    span { class: "dlg-source__num", "2" }
                    span { class: "dlg-source__text",
                        span { class: "dlg-source__title", "Bikerouter-link" }
                        span { class: "dlg-source__sub",
                            "Tervezz útvonalat a bikerouter.de-n, és másold ide a megosztási linket."
                        }
                    }
                    div { class: "dlg-link",
                        TextField {
                            label: "A megosztott link".to_string(),
                            value: text.clone(),
                            placeholder: "https://bikerouter.de/#map=…&lonlats=…".to_string(),
                            hint: field_hint,
                            error: field_error,
                            disabled: busy_now,
                            on_input: move |value: String| {
                                link.set(value);
                                paste_note.set(None);
                            },
                        }
                        div { class: "dlg-link__row",
                            button {
                                class: "btn btn--sm",
                                r#type: "button",
                                disabled: busy_now,
                                title: "A vágólap tartalmának beillesztése",
                                onclick: move |_| {
                                    let mut eval = document::eval(
                                        "try { const t = await navigator.clipboard.readText(); dioxus.send(t); } \
                                         catch (e) { dioxus.send(\"\"); }",
                                    );
                                    spawn(async move {
                                        match eval.recv::<String>().await {
                                            Ok(clip) if !clip.trim().is_empty() => {
                                                link.set(clip);
                                                paste_note.set(None);
                                            }
                                            _ => paste_note.set(Some(
                                                "A vágólap nem olvasható. Illeszd be a linket ide a Ctrl+V-vel.".to_string(),
                                            )),
                                        }
                                    });
                                },
                                Icon { name: IconName::Link, size: 14 }
                                "Beillesztés"
                            }
                            button {
                                class: "btn btn--sm",
                                r#type: "button",
                                disabled: !can_import,
                                onclick: move |_| {
                                    let link_text = normalize_link_input(&link.read().clone());
                                    if !link_text.is_empty() {
                                        on_import_link.call(link_text);
                                    }
                                },
                                Icon { name: IconName::ChevronRight, size: 14 }
                                "Útvonal betöltése"
                            }
                        }
                        if let Some(note) = paste_message {
                            div { class: "dlg-hintline", role: "status",
                                span { class: "dlg-hintline__icon", Icon { name: IconName::Info, size: 14 } }
                                span { "{note}" }
                            }
                        }
                    }
                }

                div { class: "dlg-source",
                    span { class: "dlg-source__num", "3" }
                    span { class: "dlg-source__text",
                        span { class: "dlg-source__title", "Bemutató útvonal" }
                        span { class: "dlg-source__sub",
                            "Rövid, dombos budapesti kör a kipróbáláshoz, hardver nélkül is."
                        }
                    }
                    button {
                        class: "btn dlg-source__go",
                        r#type: "button",
                        disabled: busy_now,
                        title: "Rövid, dombos budapesti kör a kipróbáláshoz",
                        onclick: move |_| on_demo.call(()),
                        Icon { name: IconName::Compass, size: 14 }
                        "Bemutató útvonal"
                    }
                }
            }

            if busy_now {
                div { class: "dlg-busy", role: "status",
                    span { class: "spinner" }
                    span { class: "t-body", "Útvonal feldolgozása…" }
                }
            }
            if let Some(message) = error_now {
                div { class: "dlg-error", role: "alert",
                    span { class: "dlg-error__icon", Icon { name: IconName::Warning, size: 16 } }
                    span { "{message}" }
                }
            }

            section { class: "dlg-recents-sec", aria_label: "Legutóbbi útvonalak",
                div { class: "dlg-head",
                    h2 { class: "dlg-head__title", "Legutóbbi útvonalak" }
                    span { class: "dlg-head__aside", "{recent_count} útvonal" }
                }
                if recents_now.is_empty() {
                    div { class: "dlg-empty",
                        span { class: "dlg-empty__lead", "Még nincs legutóbbi útvonal." }
                        span { "Az itt megnyitott GPX-ek automatikusan ide kerülnek." }
                    }
                } else {
                    div { class: "dlg-recents__head", aria_hidden: "true",
                        span { "Útvonal" }
                        span { "Táv" }
                        span { "Szint" }
                        span { "Megnyitva" }
                        span { "Forrás" }
                    }
                    ul { class: "dlg-recents",
                        for route in recents_now {
                            li { class: "dlg-recent", key: "{route.id}",
                                button {
                                    class: "dlg-recent__open",
                                    r#type: "button",
                                    disabled: busy_now,
                                    title: "Megnyitás",
                                    onclick: {
                                        let id = route.id.clone();
                                        move |_| on_open_recent.call(id.clone())
                                    },
                                    span { class: "dlg-recent__name truncate", "{route.name}" }
                                    span { class: "dlg-recent__num",
                                        "{fmt_km(route.total_dist)}"
                                        span { class: "unit", "km" }
                                    }
                                    span { class: "dlg-recent__num",
                                        "{fmt_m(route.ascent)}"
                                        span { class: "unit", "m" }
                                    }
                                    span { class: "dlg-recent__date", "{fmt_date(route.last_opened_unix)}" }
                                    span { class: "tag dlg-recent__src", "{route_source_label(route.source)}" }
                                }
                                button {
                                    class: "btn btn--ghost btn--icon btn--sm dlg-recent__del",
                                    r#type: "button",
                                    disabled: busy_now,
                                    "aria-label": "Törlés a legutóbbi útvonalak közül",
                                    title: "Törlés a legutóbbiak közül",
                                    onclick: {
                                        let id = route.id.clone();
                                        move |_| on_delete_recent.call(id.clone())
                                    },
                                    Icon { name: IconName::Trash, size: 14 }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
