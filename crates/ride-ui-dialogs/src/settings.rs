//! Settings dialog: a form sheet of grouped rider / trainer / appearance / map / Street View
//! settings, applied immediately.

use dioxus::prelude::*;
use ride_model::{GhostStatus, Settings};
use ride_ui_kit::{Dialog, NumberField, Segmented, Slider, Switch};

use crate::helpers::{
    api_key_hint, check_api_key, ghost_hint, ghost_mode_from_value, ghost_mode_value,
    map_style_from_value, map_style_value, normalize_api_key, profile_mode_from_value,
    profile_mode_value, text_scale_from_percent, text_scale_percent, theme_from_value, theme_value,
    trainer_scale_from_percent, trainer_scale_percent, ApiKeyCheck,
};

/// Rider (weight, bike, FTP, max HR, CdA, Crr), trainer difficulty (grade scale), speed
/// source, theme, map style, follow, look-ahead length and the Street View block (switch,
/// Google Maps API key, panorama-ID cache). Emits the whole updated `Settings`.
#[component]
pub fn SettingsDialog(
    open: bool,
    settings: ReadSignal<Settings>,
    /// What the ride log of the loaded route holds, so the ghost selector can say why it is
    /// empty (no route, no ride yet, only simulated rides).
    #[props(default)]
    ghost_status: ReadSignal<GhostStatus>,
    on_change: EventHandler<Settings>,
    on_close: EventHandler<()>,
    /// "Mentés fájlba": the app asks where to put the zip and writes it.
    #[props(default)]
    on_backup: EventHandler<()>,
    /// "Visszaállítás fájlból": the app asks for the zip and merges it in. Called only after the
    /// rider confirmed in the dialog.
    #[props(default)]
    on_restore: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    let current = settings.read().clone();
    let rider = current.rider.clone();
    let ui = current.ui.clone();
    let ghost_hint = ghost_hint(*ghost_status.read(), ui.ghost);

    // Every control emits the whole updated `Settings`; unknown fields survive the patch.
    let set_kg = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.rider_kg = v
    });
    let set_bike = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.bike_kg = v
    });
    let set_ftp = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.ftp_w = v
    });
    let set_max_hr = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.max_hr = v
    });
    let set_cda = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.cda = v
    });
    let set_crr = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.crr = v
    });
    let set_grade_scale = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.rider.trainer_grade_scale = trainer_scale_from_percent(v);
    });
    let set_speed_source = setter(&current, on_change, |s: &mut Settings, v: bool| {
        s.rider.use_trainer_speed = v;
    });
    let set_text_scale = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.ui.text_scale = text_scale_from_percent(v);
    });
    let set_gears = setter(&current, on_change, |s: &mut Settings, v: bool| {
        s.ui.virtual_gears = v;
    });
    let set_theme = setter(&current, on_change, |s: &mut Settings, v: String| {
        s.ui.theme = theme_from_value(&v);
    });
    let set_map_style = setter(&current, on_change, |s: &mut Settings, v: String| {
        s.ui.map_style = map_style_from_value(&v);
    });
    let set_follow = setter(&current, on_change, |s: &mut Settings, v: bool| {
        s.ui.follow_rider = v;
    });
    let set_profile_mode = setter(&current, on_change, |s: &mut Settings, v: String| {
        s.ui.set_active_profile_mode(profile_mode_from_value(&v));
    });
    let set_lookahead = setter(&current, on_change, |s: &mut Settings, v: f64| {
        s.ui.lookahead_m = v;
    });
    let set_ghost = setter(&current, on_change, |s: &mut Settings, v: String| {
        s.ui.ghost = ghost_mode_from_value(&v);
    });
    let reset_layout = {
        let base = current.clone();
        move |_| {
            let mut s = base.clone();
            s.ui.tiles = None;
            s.ui.tiles_workout = None;
            on_change.call(s);
        }
    };

    rsx! {
        Dialog {
            open: true,
            title: "Beállítások".to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note", "Minden módosítás azonnal érvénybe lép." }
            },
            div { class: "dlg-settings",
                Group {
                    title: "Kerékpáros".to_string(),
                    note: "A fizikai modell, a sebességszámítás és a zónák alapja.".to_string(),
                    div { class: "dlg-grid2",
                        NumberField {
                            label: "Testsúly".to_string(),
                            value: rider.rider_kg,
                            min: 30.0,
                            max: 200.0,
                            step: 0.5,
                            unit: "kg".to_string(),
                            hint: "A saját testsúlyod.".to_string(),
                            on_change: set_kg,
                        }
                        NumberField {
                            label: "Kerékpár".to_string(),
                            value: rider.bike_kg,
                            min: 3.0,
                            max: 30.0,
                            step: 0.1,
                            unit: "kg".to_string(),
                            hint: "A kerékpár tömege; a modell a testsúlyoddal együtt számol vele.".to_string(),
                            on_change: set_bike,
                        }
                        NumberField {
                            label: "FTP".to_string(),
                            value: rider.ftp_w,
                            min: 50.0,
                            max: 600.0,
                            step: 5.0,
                            unit: "W".to_string(),
                            hint: "Küszöbteljesítmény a zónákhoz és a TSS-hez.".to_string(),
                            on_change: set_ftp,
                        }
                        NumberField {
                            label: "Max. pulzus".to_string(),
                            value: rider.max_hr,
                            min: 120.0,
                            max: 230.0,
                            step: 1.0,
                            unit: "bpm".to_string(),
                            hint: "A pulzuszónák felső határa.".to_string(),
                            on_change: set_max_hr,
                        }
                        NumberField {
                            label: "CdA".to_string(),
                            value: rider.cda,
                            min: 0.1,
                            max: 1.0,
                            step: 0.01,
                            unit: "m²".to_string(),
                            hint: "Légellenállási felület (CdA): versenypozícióban kb. 0,32, egyenes háttal kb. 0,40.".to_string(),
                            on_change: set_cda,
                        }
                        NumberField {
                            label: "Crr".to_string(),
                            value: rider.crr,
                            min: 0.002,
                            max: 0.02,
                            step: 0.001,
                            unit: "".to_string(),
                            hint: "Gördülési ellenállási együttható (Crr): jó aszfalton kb. 0,004.".to_string(),
                            on_change: set_crr,
                        }
                    }
                }

                Group {
                    title: "Trainer".to_string(),
                    note: "Mennyire érződjenek a valódi emelkedők a traineren.".to_string(),
                    Slider {
                        label: "Meredekség erőssége".to_string(),
                        value: trainer_scale_percent(rider.trainer_grade_scale),
                        min: 0.0,
                        max: 150.0,
                        step: 5.0,
                        unit: "%".to_string(),
                        hint: "A valódi meredekség ennyi százalékát kapja a trainer (0 % = mindig sík).".to_string(),
                        on_change: set_grade_scale,
                    }
                    Switch {
                        checked: rider.use_trainer_speed,
                        on_change: set_speed_source,
                        label: "A trainer sebességének használata".to_string(),
                        hint: "A fizikai modell helyett a trainer által jelentett sebességet mutatjuk.".to_string(),
                        row: true,
                    }
                    Switch {
                        checked: ui.virtual_gears,
                        on_change: set_gears,
                        label: "Virtuális váltó".to_string(),
                        hint: "A ← és → billentyűvel vagy a Váltó panelen válthatsz; a fokozat kb. ±5 %-kal módosítja a trainer ellenállását.".to_string(),
                        row: true,
                    }
                }

                Group { title: "Megjelenés".to_string(),
                    Slider {
                        label: "Szövegméret".to_string(),
                        value: text_scale_percent(ui.text_scale()),
                        min: 80.0,
                        max: 160.0,
                        step: 5.0,
                        unit: "%".to_string(),
                        hint: "Minden szöveg és szám mérete, kis vagy nagy kijelzőn érdemes állítani (100 % az alapméret). A panelek méretét az elrendezés adja, nagyobb szöveghez szükség lehet egy nagyobb panelre.".to_string(),
                        on_change: set_text_scale,
                    }
                    div { class: "dlg-row2",
                        div { class: "dlg-control",
                            span { class: "label", "Téma" }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("dark".to_string(), "Sötét".to_string()),
                                        ("light".to_string(), "Világos".to_string()),
                                    ],
                                    value: theme_value(ui.theme),
                                    aria_label: "Téma".to_string(),
                                    on_change: set_theme,
                                }
                            }
                        }
                        div { class: "dlg-control",
                            span { class: "label", "Térképstílus" }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("dark".to_string(), "Sötét".to_string()),
                                        ("light".to_string(), "Világos".to_string()),
                                        ("osm".to_string(), "OSM".to_string()),
                                    ],
                                    value: map_style_value(ui.map_style),
                                    aria_label: "Térképstílus".to_string(),
                                    on_change: set_map_style,
                                }
                            }
                        }
                    }
                }

                Group { title: "Térkép és profil".to_string(),
                    Switch {
                        checked: ui.follow_rider,
                        on_change: set_follow,
                        label: "A térkép kövesse a kerékpárost".to_string(),
                        hint: "Kikapcsolva szabadon mozgatható a térkép (M).".to_string(),
                        row: true,
                    }
                    div { class: "dlg-row2",
                        div { class: "dlg-control",
                            span { class: "label", "Profilnézet" }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("ahead".to_string(), "Előretekintés".to_string()),
                                        ("full".to_string(), "Teljes".to_string()),
                                    ],
                                    value: profile_mode_value(ui.active_profile_mode()),
                                    aria_label: "Profilnézet".to_string(),
                                    small: true,
                                    on_change: set_profile_mode,
                                }
                            }
                        }
                        Slider {
                            label: "Előretekintés hossza".to_string(),
                            value: ui.lookahead_m,
                            min: 500.0,
                            max: 10_000.0,
                            step: 100.0,
                            unit: "m".to_string(),
                            hint: "Csak előretekintő nézetben számít.".to_string(),
                            on_change: set_lookahead,
                        }
                    }
                    div { class: "dlg-control",
                        span { class: "label", "Szellem" }
                        div { class: "dlg-segrow",
                            Segmented {
                                options: vec![
                                    ("off".to_string(), "Ki".to_string()),
                                    ("route".to_string(), "Az útvonal ideje".to_string()),
                                    ("last".to_string(), "Legutóbbi menet".to_string()),
                                    ("best".to_string(), "Legjobb (PR)".to_string()),
                                ],
                                value: ghost_mode_value(ui.ghost),
                                aria_label: "Szellem".to_string(),
                                small: true,
                                on_change: set_ghost,
                            }
                        }
                        span { class: "dlg-note dlg-note--ghost", "{ghost_hint}" }
                        span { class: "dlg-note", "A korábbi, ugyanezen az útvonalon tekert menet visszajátszása a térképen és a profilon, másodpercben mért előnnyel vagy hátránnyal. A „Legjobb” a legkisebb idejű teljes menet; a szimulált menetek nem PR-ok. Az „Az útvonal ideje” a betöltött GPX saját időbélyegeit játssza vissza, mentett menet nélkül is." }
                    }
                }

                Group {
                    title: "Elrendezés".to_string(),
                    note: "A paneleket a fejlécüknél fogva húzhatod: egy másik panel szélére ejtve megosztoznak a helyén, a közepére ejtve helyet cserélnek. Az elválasztókat húzva átméretezheted őket, és mindig kitöltik a felületet. A bezárt panelek a felső sávban várnak. Az elrendezést külön jegyzi az app arra az esetre, amikor edzés van kiválasztva, és amikor nincs: amíg edzésen belül nem alakítod át, az edzéses elrendezés a szabadét követi.".to_string(),
                    div { class: "dlg-control",
                        span { class: "label", "Alaphelyzet" }
                        button {
                            class: "btn",
                            r#type: "button",
                            onclick: reset_layout,
                            "Elrendezés visszaállítása"
                        }
                    }
                }

                BackupGroup { on_backup, on_restore }

                StreetViewGroup { settings: current.clone(), on_change }
            }
        }
    }
}

/// Backup and restore of the rider's data as one zip. Restoring asks first, because it also
/// brings the settings of the backup along; the data itself is only ever added, never replaced.
#[component]
fn BackupGroup(on_backup: EventHandler<()>, on_restore: EventHandler<()>) -> Element {
    let mut confirming = use_signal(|| false);
    rsx! {
        Group {
            title: "Biztonsági mentés".to_string(),
            note: "Az útvonalak, a mentett túrák, az importált edzések és a beállítások egyetlen zip-fájlba kerülnek, ezen a gépen: semmi nem megy ki a hálózatra. A Street View kulcs, a térképcsempék és a letöltött képek nincsenek benne.".to_string(),
            div { class: "dlg-backup",
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| on_backup.call(()),
                    "Mentés fájlba"
                }
                if confirming() {
                    button {
                        class: "btn btn--danger",
                        r#type: "button",
                        onclick: move |_| {
                            confirming.set(false);
                            on_restore.call(());
                        },
                        "Igen, visszaállítom"
                    }
                    button {
                        class: "btn btn--ghost",
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        "Mégse"
                    }
                } else {
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| confirming.set(true),
                        "Visszaállítás fájlból"
                    }
                }
            }
            if confirming() {
                span { class: "dlg-note",
                    "A mentésben lévő útvonalak, túrák és edzések hozzáadódnak a meglévőkhöz: semmi nem törlődik és semmi nem íródik felül. A beállításaid viszont a mentésből valók lesznek (a Street View kulcsod megmarad). Tekerés közben nem lehet visszaállítani."
                }
            }
        }
    }
}

/// One group of the form sheet: title and note on the left, the controls on the right.
#[component]
pub(crate) fn Group(
    title: String,
    #[props(default = String::new())] note: String,
    children: Element,
) -> Element {
    rsx! {
        section { class: "dlg-group",
            header { class: "dlg-group__head",
                h3 { class: "dlg-group__title", "{title}" }
                if !note.is_empty() {
                    p { class: "dlg-group__note", "{note}" }
                }
            }
            div { class: "dlg-group__body", {children} }
        }
    }
}

/// The Street View block: enable switch, Google Maps API key (hidden by default, trimmed),
/// how-to, the panorama-ID cache switch and the opt-in offline image download with its
/// storage readout.
#[component]
fn StreetViewGroup(settings: Settings, on_change: EventHandler<Settings>) -> Element {
    // show / hide the key; the field is a password input until the user asks otherwise
    let mut revealed = use_signal(|| false);
    let shown = revealed();

    let ui = settings.ui.clone();
    let check = check_api_key(&ui.google_maps_key);
    let hint = api_key_hint(check);
    let key_class = match check {
        ApiKeyCheck::Invalid => "dlg-key dlg-key--invalid",
        ApiKeyCheck::Unusual => "dlg-key dlg-key--warn",
        ApiKeyCheck::Empty | ApiKeyCheck::Plausible => "dlg-key",
    };

    let set_enabled = setter(&settings, on_change, |s: &mut Settings, v: bool| {
        s.ui.streetview_enabled = v;
    });
    let set_cache = setter(&settings, on_change, |s: &mut Settings, v: bool| {
        s.ui.streetview_cache_panos = v;
    });
    let set_offline = setter(&settings, on_change, |s: &mut Settings, v: bool| {
        s.ui.streetview_offline = v;
    });
    let set_key = setter(&settings, on_change, |s: &mut Settings, v: String| {
        s.ui.google_maps_key = normalize_api_key(&v);
    });

    let mut storage = use_signal(ride_ui_streetview::imagestore::overview_default);
    let stored = storage();
    let stored_text = if stored.images == 0 {
        "Még nincs letöltött kép.".to_string()
    } else {
        format!(
            "{} kép ({}), {} útvonalhoz.",
            stored.images,
            format_bytes(stored.bytes),
            stored.routes
        )
    };
    let clear_storage = move |_| {
        let _ = ride_ui_streetview::imagestore::clear_all_default();
        let _ = ride_ui_streetview::cache::clear_saved_default();
        storage.set(ride_ui_streetview::imagestore::overview_default());
    };

    rsx! {
        Group {
            title: "Street View".to_string(),
            note: "Google utcakép ott, ahol éppen tekersz, a saját API-kulcsoddal. Kulcs nélkül nem jelenik meg kép; a kérések a te Google-kvótádat terhelik.".to_string(),
            Switch {
                checked: ui.streetview_enabled,
                on_change: set_enabled,
                label: "Street View megjelenítése a térképen".to_string(),
                hint: "Kis ablak a térkép jobb alsó sarkában, a címsor gombjával vagy a V billentyűvel is kapcsolható; az X felcseréli a kettőt.".to_string(),
                row: true,
            }

            div { class: key_class,
                label { class: "dlg-key__label", r#for: "dlg-gmaps-key", "Google Maps API-kulcs" }
                div { class: "dlg-key__control",
                    input {
                        id: "dlg-gmaps-key",
                        r#type: if shown { "text" } else { "password" },
                        value: "{ui.google_maps_key}",
                        placeholder: "AIza…",
                        spellcheck: "false",
                        autocomplete: "off",
                        maxlength: "128",
                        "aria-invalid": if check == ApiKeyCheck::Invalid { "true" } else { "false" },
                        oninput: move |e| set_key.clone()(e.value()),
                    }
                    button {
                        class: "dlg-key__toggle",
                        r#type: "button",
                        "aria-pressed": if shown { "true" } else { "false" },
                        "aria-label": if shown { "A kulcs elrejtése" } else { "A kulcs megjelenítése" },
                        onclick: move |_| revealed.set(!shown),
                        if shown {
                            "Elrejt"
                        } else {
                            "Mutat"
                        }
                    }
                }
                span {
                    class: "dlg-key__hint",
                    role: if check == ApiKeyCheck::Invalid { "alert" } else { "note" },
                    "{hint}"
                }
            }

            ol { class: "dlg-howto",
                li {
                    "Nyiss meg egy projektet a "
                    strong { "Google Cloud Console" }
                    "-ban, és kapcsold be a számlázást (a Google havi ingyenes keretet ad, a díjszabást az oldalukon nézd meg)."
                }
                li {
                    "Az API Library oldalon engedélyezd a két szolgáltatást: "
                    strong { "Maps Embed API" }
                    " és "
                    strong { "Street View Static API" }
                    "."
                }
                li { "A Hitelesítő adatok oldalon hozz létre API-kulcsot, és korlátozd erre a két API-ra." }
                li { "Másold ide a kulcsot. Csak ezen a gépen, a beállításfájlban van, és csak a Google felé megy el." }
            }

            Switch {
                checked: ui.streetview_cache_panos,
                on_change: set_cache,
                label: "Panoráma-azonosítók mentése az útvonalhoz".to_string(),
                hint: "Ismételt tekerésnél kevesebb keresés kell, így kevesebb kvótát használsz.".to_string(),
                row: true,
            }

            Switch {
                checked: ui.streetview_offline,
                on_change: set_offline,
                label: "Street View képek letöltése (hálózat nélküli tekeréshez)".to_string(),
                hint: "A panel Letöltés gombja elmenti az útvonal képeit, így legközelebb API-kulcs és hálózat nélkül is látszanak.".to_string(),
                row: true,
            }
            div { class: "dlg-storage",
                span { class: "dlg-storage__text", "{stored_text}" }
                button {
                    class: "btn btn--ghost btn--sm",
                    r#type: "button",
                    disabled: stored.images == 0,
                    onclick: clear_storage,
                    "Törlés"
                }
            }
            p { class: "dlg-note",
                strong { "A letöltött képek legfeljebb 30 napig maradnak a gépen. " }
                "Csak a bekapcsolt offline opció használja őket, és csak kulcs nélkül vagy ha az éles nézet nem tölthető be: a Google legfeljebb 640 × 640 pixeles képet ad, ezért élőben a teljes felbontású beágyazott nézet látszik. A Street View képek tartós tárolását a Google feltételei korlátozzák: a letöltés a te döntésed, és a saját Google-fiókod kvótáját használja."
            }
        }
    }
}

/// `1 234 567` bytes as `1.2 MB` (or `345 kB`), Hungarian decimal comma.
fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 {
        format!("{mb:.1} MB").replace('.', ",")
    } else {
        format!("{:.0} kB", bytes as f64 / 1000.0)
    }
}

/// Builds an event handler that emits `base` with one field patched. The dialog never keeps a
/// draft copy: the app owns the settings, the dialog only describes the change.
fn setter<T, F>(
    base: &Settings,
    on_change: EventHandler<Settings>,
    patch: F,
) -> impl FnMut(T) + Clone + 'static
where
    T: 'static,
    F: Fn(&mut Settings, T) + Clone + 'static,
{
    let base = base.clone();
    move |value: T| {
        let mut next = base.clone();
        patch(&mut next, value);
        on_change.call(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders the dialog server-side for a given settings value.
    fn render(settings: Settings) -> String {
        render_with_status(settings, GhostStatus::default())
    }

    fn render_with_status(settings: Settings, status: GhostStatus) -> String {
        #[derive(Clone, PartialEq, Props)]
        struct Harness {
            settings: Settings,
            status: GhostStatus,
        }
        #[component]
        fn Host(props: Harness) -> Element {
            let initial = props.settings.clone();
            let status = props.status;
            let settings = use_signal(move || initial);
            let ghost_status = use_signal(move || status);
            rsx! {
                SettingsDialog {
                    open: true,
                    settings,
                    ghost_status,
                    on_change: move |_| {},
                    on_close: move |_| {},
                }
            }
        }
        let mut vdom = VirtualDom::new_with_props(Host, Harness { settings, status });
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    #[test]
    fn the_street_view_section_is_complete_and_hungarian() {
        let html = render(Settings::default());
        for text in [
            "Street View",
            "Street View megjelenítése a térképen",
            "Virtuális váltó",
            "Google Maps API-kulcs",
            "Maps Embed API",
            "Street View Static API",
            "Panoráma-azonosítók mentése az útvonalhoz",
            "Street View képek letöltése (hálózat nélküli tekeréshez)",
            "A letöltött képek legfeljebb 30 napig maradnak a gépen.",
            "a saját Google-fiókod kvótáját használja",
            "Törlés",
            "Mutat",
        ] {
            assert!(html.contains(text), "missing {text:?}");
        }
        // the key is hidden until the user asks for it
        assert!(
            html.contains(r#"type="password""#),
            "key field must be a password input"
        );
        assert!(html.contains(r#"aria-pressed="false""#));
        // an empty key is not an error
        assert!(!html.contains("dlg-key--invalid"));
    }

    #[test]
    fn the_key_hint_follows_the_key() {
        let mut settings = Settings::default();
        settings.ui.google_maps_key = format!("AIza{}", "x".repeat(35));
        let ok = render(settings.clone());
        assert!(ok.contains("A kulcs alakja rendben van"));
        assert!(!ok.contains("dlg-key--invalid") && !ok.contains("dlg-key--warn"));

        settings.ui.google_maps_key = "AIza.nem-jo".into();
        let bad = render(settings.clone());
        assert!(bad.contains("dlg-key--invalid"));
        assert!(bad.contains("csak betűt, számot"));

        settings.ui.google_maps_key = "rovid".into();
        let odd = render(settings);
        assert!(odd.contains("dlg-key--warn"));
        assert!(odd.contains("Szokatlan kulcs"));
    }

    #[test]
    fn switches_reflect_the_street_view_settings() {
        let mut settings = Settings::default();
        let off = render(settings.clone());
        // every switch of the sheet is off by default except the follow / speed ones
        let checked_off = off.matches("checked").count();
        settings.ui.streetview_cache_panos = true;
        let on = render(settings);
        assert!(
            on.matches("checked").count() > checked_off,
            "the Street View switch must show as checked"
        );
    }

    #[test]
    fn the_layout_section_offers_a_reset() {
        let html = render(Settings::default());
        for text in ["Elrendezés", "Elrendezés visszaállítása"] {
            assert!(html.contains(text), "missing {text:?}");
        }
    }

    #[test]
    fn the_backup_section_offers_both_actions_and_asks_before_restoring() {
        let html = render(Settings::default());
        for text in [
            "Biztonsági mentés",
            "Mentés fájlba",
            "Visszaállítás fájlból",
            "semmi nem megy ki a hálózatra",
        ] {
            assert!(html.contains(text), "missing {text:?}");
        }
        // the confirmation only appears after the first click
        assert!(!html.contains("Igen, visszaállítom"));
        assert!(!html.contains("semmi nem íródik felül"));
    }

    #[test]
    fn the_ghost_selector_offers_the_four_modes() {
        let html = render(Settings::default());
        for text in [
            "Szellem",
            "Ki",
            "Az útvonal ideje",
            "Legutóbbi menet",
            "Legjobb (PR)",
            "másodpercben mért előnnyel",
        ] {
            assert!(html.contains(text), "missing {text:?}");
        }
        // the default is "Best": the segmented control marks exactly one option per group
        assert_eq!(
            html.matches(r#"aria-checked="true""#).count(),
            4,
            "theme, map style, profile mode and ghost each have one selected option"
        );
        // choosing "Ki" moves the selection to that option
        let mut off = Settings::default();
        off.ui.ghost = ride_model::GhostMode::Off;
        let html = render(off);
        assert_eq!(html.matches(r#"aria-checked="true""#).count(), 4);
        // inside the ghost group the selected option is the first one ("Ki")
        let group = html.find("Szellem</span>").expect("the ghost group label");
        let after = &html[group..];
        let active = after.find("is-active").expect("a selected ghost mode");
        assert!(
            active < after.find(">Ki</button>").expect("the Ki option"),
            "Ki must be the selected ghost mode"
        );
    }

    #[test]
    fn the_ghost_selector_says_why_there_is_no_ghost_yet() {
        // no ride on the route: the hint explains that the first ride creates the ghost
        let html = render_with_status(
            Settings::default(),
            GhostStatus {
                loaded: true,
                rides: 0,
                simulated: 0,
                replayable: false,
                partial: false,
                route_time_s: None,
            },
        );
        assert!(
            html.contains("még nincs mentett menet"),
            "the no-history hint is missing"
        );
        // ... and with a GPX timeline it offers the route's own time instead
        let html = render_with_status(
            Settings::default(),
            GhostStatus {
                loaded: true,
                rides: 0,
                simulated: 0,
                replayable: false,
                partial: false,
                route_time_s: Some(5_025.0),
            },
        );
        assert!(
            html.contains("A GPX saját ideje 1:23:45"),
            "the GPX offer is missing: {html}"
        );
        // only simulated rides with the "Best" mode: point at "Last"
        let html = render_with_status(
            Settings::default(),
            GhostStatus {
                loaded: true,
                rides: 2,
                simulated: 2,
                replayable: false,
                partial: false,
                route_time_s: None,
            },
        );
        assert!(
            html.contains("mind szimulált"),
            "the simulated hint is missing"
        );
        // a replayable ride: the hint says the ghost is waiting on the start line
        let html = render_with_status(
            Settings::default(),
            GhostStatus {
                loaded: true,
                rides: 3,
                simulated: 1,
                replayable: true,
                partial: false,
                route_time_s: None,
            },
        );
        assert!(html.contains("3 korábbi menet"), "the count is missing");
        assert!(
            html.contains("rajtvonalon vár"),
            "the waiting hint is missing"
        );
        // a ride that never reached the end is still replayed: say where the ghost stops
        let html = render_with_status(
            Settings::default(),
            GhostStatus {
                loaded: true,
                rides: 1,
                simulated: 0,
                replayable: true,
                partial: true,
                route_time_s: None,
            },
        );
        assert!(
            html.contains("egyik sem ért célba"),
            "the partial hint is missing"
        );
    }

    #[test]
    fn the_route_mode_explains_itself_with_and_without_a_gpx_timeline() {
        let mut settings = Settings::default();
        settings.ui.ghost = ride_model::GhostMode::Route;
        let html = render_with_status(
            settings.clone(),
            GhostStatus {
                loaded: true,
                rides: 0,
                simulated: 0,
                replayable: true,
                partial: false,
                route_time_s: Some(4_500.0),
            },
        );
        assert!(html.contains("A GPX-be írt menet ideje 1:15:00"), "{html}");
        let html = render_with_status(
            settings,
            GhostStatus {
                loaded: true,
                rides: 0,
                simulated: 0,
                replayable: false,
                partial: false,
                route_time_s: None,
            },
        );
        assert!(html.contains("nincsenek időbélyegek"), "{html}");
    }

    #[test]
    fn the_key_patch_trims_and_keeps_the_rest() {
        let mut base = Settings::default();
        base.rider.ftp_w = 260.0;
        base.ui.google_maps_key = "AIzaOld".into();
        let mut next = base.clone();
        next.ui.google_maps_key = normalize_api_key("  AIzaNew\n");
        assert_eq!(next.ui.google_maps_key, "AIzaNew");
        assert_eq!(next.rider.ftp_w, 260.0);
        assert!(!next.ui.streetview_cache_panos);
    }
}
