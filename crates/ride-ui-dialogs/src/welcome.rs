//! Route dialog: the one place where a route is chosen, at start-up and whenever the top bar's
//! "Útvonal" is pressed. Three numbered ways to start (file, link, demo) and a ruled index of
//! recent routes, in the same dialog frame as every other dialog.

use dioxus::prelude::*;
use ride_i18n::{t, tf};
use ride_model::RecentRoute;
use ride_ui_kit::format::{fmt_km, fmt_m};
use ride_ui_kit::{Dialog, Icon, IconName, TextField};

use crate::helpers::{
    bikerouter_hint, fmt_date, normalize_link_input, route_source_label, sorted_recents,
};

/// The route dialog: file row / "GPX megnyitása" (`on_pick_file`), bikerouter-link field
/// (`on_import_link` with the pasted text), recent routes (open / delete), the demo route
/// button, busy spinner and an inline error line. Closes with Esc or the title bar's × (every
/// dialog has one, so there is no close button in the footer); the shell also closes it by itself
/// once a route has loaded.
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
    /// Opens the review page of the stored rides (the phone's top bar has no room for its own
    /// button). `None` leaves the link out.
    #[props(default)]
    on_review: Option<EventHandler<()>>,
) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        Dialog {
            open: true,
            title: t("ui_dialogs.welcome.utvonal").to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note",
                    {t("ui_dialogs.welcome.a_gpx_fajlt_az_ablakra_huzva_is_megnyithatod_ctrl_o_")}
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
            if let Some(on_review) = on_review {
                div { class: "dlg-review-link",
                    button {
                        class: "btn btn--sm",
                        r#type: "button",
                        onclick: move |_| on_review.call(()),
                        Icon { name: IconName::Gauge, size: 16 }
                        {t("ui_dialogs.welcome.mentett_edzesek_attekintese")}
                    }
                }
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
        t("ui_dialogs.welcome.ervenyes_bikerouter_link_betoltheto").to_string()
    } else {
        t("ui_dialogs.welcome.a_bikerouter_de_megosztas_linkje_tartalmazza_az_utvo").to_string()
    };
    let can_import = hint.is_none() && !text.trim().is_empty() && !busy_now;
    let paste_message = paste_note.read().clone();
    let recent_count = recents_now.len();

    rsx! {
        div { class: "dlg-welcome",
            header { class: "dlg-hero",
                p { class: "dlg-hero__lead",
                    {t("ui_dialogs.welcome.valassz_gpx_fajlt_illessz_be_egy_bikerouter_linket_v")}
                }
            }

            section { class: "dlg-sources", aria_label: t("ui_dialogs.welcome.utvonal_betoltese").to_string(),
                button {
                    class: "dlg-source dlg-drop",
                    r#type: "button",
                    disabled: busy_now,
                    "aria-label": t("ui_dialogs.welcome.gpx_fajl_kivalasztasa"),
                    onclick: move |_| on_pick_file.call(()),
                    span { class: "dlg-source__num", "1" }
                    span { class: "dlg-source__text",
                        span { class: "dlg-source__title", {t("ui_dialogs.welcome.gpx_fajl")} }
                        span { class: "dlg-source__sub",
                            {t("ui_dialogs.welcome.huzd_ide_a_fajlt_vagy_valaszd_ki_a_gepedrol")}
                        }
                    }
                    span { class: "dlg-source__go dlg-drop__go",
                        Icon { name: IconName::Folder, size: 16 }
                        {t("ui_dialogs.welcome.gpx_megnyitasa")}
                    }
                }

                div { class: "dlg-source dlg-source--form",
                    span { class: "dlg-source__num", "2" }
                    span { class: "dlg-source__text",
                        span { class: "dlg-source__title", "Bikerouter-link" }
                        span { class: "dlg-source__sub",
                            {t("ui_dialogs.welcome.tervezz_utvonalat_a_bikerouter_de_n_es_masold_ide_a_")}
                        }
                    }
                    div { class: "dlg-link",
                        TextField {
                            label: t("ui_dialogs.welcome.a_megosztott_link").to_string(),
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
                                title: t("ui_dialogs.welcome.a_vagolap_tartalmanak_beillesztese").to_string(),
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
                                                t("ui_dialogs.welcome.a_vagolap_nem_olvashato_illeszd_be_a_linket_ide_a_ct").to_string(),
                                            )),
                                        }
                                    });
                                },
                                Icon { name: IconName::Link, size: 14 }
                                {t("ui_dialogs.welcome.beillesztes")}
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
                                {t("ui_dialogs.welcome.utvonal_betoltese")}
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
                        span { class: "dlg-source__title", {t("ui_dialogs.welcome.bemutato_utvonal")} }
                        span { class: "dlg-source__sub",
                            {t("ui_dialogs.welcome.rovid_dombos_budapesti_kor_a_kiprobalashoz_hardver_n")}
                        }
                    }
                    button {
                        class: "btn dlg-source__go",
                        r#type: "button",
                        disabled: busy_now,
                        title: t("ui_dialogs.welcome.rovid_dombos_budapesti_kor_a_kiprobalashoz").to_string(),
                        onclick: move |_| on_demo.call(()),
                        Icon { name: IconName::Compass, size: 14 }
                        {t("ui_dialogs.welcome.bemutato_utvonal")}
                    }
                }
            }

            if busy_now {
                div { class: "dlg-busy", role: "status",
                    span { class: "spinner" }
                    span { class: "t-body", {t("ui_dialogs.welcome.utvonal_feldolgozasa")} }
                }
            }
            if let Some(message) = error_now {
                div { class: "dlg-error", role: "alert",
                    span { class: "dlg-error__icon", Icon { name: IconName::Warning, size: 16 } }
                    span { "{message}" }
                }
            }

            section { class: "dlg-recents-sec", aria_label: t("ui_dialogs.welcome.legutobbi_utvonalak").to_string(),
                div { class: "dlg-head",
                    h2 { class: "dlg-head__title", {t("ui_dialogs.welcome.legutobbi_utvonalak")} }
                    span { class: "dlg-head__aside", {tf("ui_dialogs.welcome.recent_count_utvonal", &[("recent_count", &recent_count.to_string())])} }
                }
                if recents_now.is_empty() {
                    div { class: "dlg-empty",
                        span { class: "dlg-empty__lead", {t("ui_dialogs.welcome.meg_nincs_legutobbi_utvonal")} }
                        span { {t("ui_dialogs.welcome.az_itt_megnyitott_gpx_ek_automatikusan_ide_kerulnek")} }
                    }
                } else {
                    div { class: "dlg-recents__head", aria_hidden: "true",
                        span { {t("ui_dialogs.welcome.utvonal")} }
                        span { {t("ui_dialogs.welcome.tav")} }
                        span { {t("ui_dialogs.welcome.szint")} }
                        span { {t("ui_dialogs.welcome.megnyitva")} }
                        span { {t("ui_dialogs.about.forras")} }
                    }
                    ul { class: "dlg-recents",
                        for route in recents_now {
                            li { class: "dlg-recent", key: "{route.id}",
                                button {
                                    class: "dlg-recent__open",
                                    r#type: "button",
                                    disabled: busy_now,
                                    title: t("ui_dialogs.welcome.megnyitas").to_string(),
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
                                    "aria-label": t("ui_dialogs.welcome.torles_a_legutobbi_utvonalak_kozul"),
                                    title: t("ui_dialogs.welcome.torles_a_legutobbiak_kozul").to_string(),
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
