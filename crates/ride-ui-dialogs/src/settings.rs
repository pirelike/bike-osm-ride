//! Settings dialog: a form sheet of grouped rider / trainer / appearance / map / Street View
//! settings, applied immediately.

use dioxus::prelude::*;
use ride_i18n::{t, tf, tn};
use ride_model::{GhostStatus, Settings};
use ride_ui_kit::{Dialog, NumberField, Segmented, Slider, Switch};
use ride_ui_streetview::imagestore::ImageOverview;

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
    let set_language = setter(&current, on_change, |s: &mut Settings, v: String| {
        s.ui.language = v;
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
            title: t("ui_dialogs.settings.beallitasok").to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note", {t("ui_dialogs.settings.minden_modositas_azonnal_ervenybe_lep")} }
            },
            div { class: "dlg-settings",
                Group {
                    title: t("ui_dialogs.settings.kerekparos").to_string(),
                    note: t("ui_dialogs.settings.a_fizikai_modell_a_sebessegszamitas_es_a_zonak_alapj").to_string(),
                    div { class: "dlg-grid2",
                        NumberField {
                            label: t("ui_dialogs.settings.testsuly").to_string(),
                            value: rider.rider_kg,
                            min: 30.0,
                            max: 200.0,
                            step: 0.5,
                            unit: "kg".to_string(),
                            hint: t("ui_dialogs.settings.a_sajat_testsulyod").to_string(),
                            on_change: set_kg,
                        }
                        NumberField {
                            label: t("ui_dialogs.settings.kerekpar").to_string(),
                            value: rider.bike_kg,
                            min: 3.0,
                            max: 30.0,
                            step: 0.1,
                            unit: "kg".to_string(),
                            hint: t("ui_dialogs.settings.a_kerekpar_tomege_a_modell_a_testsulyoddal_egyutt_sz").to_string(),
                            on_change: set_bike,
                        }
                        NumberField {
                            label: "FTP".to_string(),
                            value: rider.ftp_w,
                            min: 50.0,
                            max: 600.0,
                            step: 5.0,
                            unit: "W".to_string(),
                            hint: t("ui_dialogs.settings.kuszobteljesitmeny_a_zonakhoz_es_a_tss_hez").to_string(),
                            on_change: set_ftp,
                        }
                        NumberField {
                            label: t("ui_dialogs.settings.max_pulzus").to_string(),
                            value: rider.max_hr,
                            min: 120.0,
                            max: 230.0,
                            step: 1.0,
                            unit: "bpm".to_string(),
                            hint: t("ui_dialogs.settings.a_pulzuszonak_felso_hatara").to_string(),
                            on_change: set_max_hr,
                        }
                        NumberField {
                            label: "CdA".to_string(),
                            value: rider.cda,
                            min: 0.1,
                            max: 1.0,
                            step: 0.01,
                            unit: "m²".to_string(),
                            hint: t("ui_dialogs.settings.legellenallasi_felulet_cda_versenypozicioban_kb_0_32").to_string(),
                            on_change: set_cda,
                        }
                        NumberField {
                            label: "Crr".to_string(),
                            value: rider.crr,
                            min: 0.002,
                            max: 0.02,
                            step: 0.001,
                            unit: "".to_string(),
                            hint: t("ui_dialogs.settings.gordulesi_ellenallasi_egyutthato_crr_jo_aszfalton_kb").to_string(),
                            on_change: set_crr,
                        }
                    }
                }

                Group {
                    title: t("ui_dialogs.settings.trainer_csoport").to_string(),
                    note: t("ui_dialogs.settings.mennyire_erzodjenek_a_valodi_emelkedok_a_traineren").to_string(),
                    Slider {
                        label: t("ui_dialogs.settings.meredekseg_erossege").to_string(),
                        value: trainer_scale_percent(rider.trainer_grade_scale),
                        min: 0.0,
                        max: 150.0,
                        step: 5.0,
                        unit: "%".to_string(),
                        hint: t("ui_dialogs.settings.a_valodi_meredekseg_ennyi_szazalekat_kapja_a_trainer").to_string(),
                        on_change: set_grade_scale,
                    }
                    Switch {
                        checked: rider.use_trainer_speed,
                        on_change: set_speed_source,
                        label: t("ui_dialogs.settings.a_trainer_sebessegenek_hasznalata").to_string(),
                        hint: t("ui_dialogs.settings.a_fizikai_modell_helyett_a_trainer_altal_jelentett_s").to_string(),
                        row: true,
                    }
                    Switch {
                        checked: ui.virtual_gears,
                        on_change: set_gears,
                        label: t("ui_dialogs.settings.virtualis_valto").to_string(),
                        hint: t("ui_dialogs.settings.a_es_billentyuvel_vagy_a_valto_panelen_valthatsz_a_f").to_string(),
                        row: true,
                    }
                }

                Group { title: t("ui_dialogs.settings.megjelenes").to_string(),
                    Slider {
                        label: t("ui_dialogs.settings.szovegmeret").to_string(),
                        value: text_scale_percent(ui.text_scale()),
                        min: 80.0,
                        max: 160.0,
                        step: 5.0,
                        unit: "%".to_string(),
                        hint: t("ui_dialogs.settings.minden_szoveg_es_szam_merete_kis_vagy_nagy_kijelzon_").to_string(),
                        on_change: set_text_scale,
                    }
                    div { class: "dlg-control",
                        span { class: "label", {t("ui_dialogs.settings.nyelv")} }
                        div { class: "dlg-segrow",
                            Segmented {
                                options: language_options(),
                                value: ui.language.clone(),
                                aria_label: t("ui_dialogs.settings.nyelv").to_string(),
                                small: true,
                                wrap: true,
                                on_change: set_language,
                            }
                        }
                        span { class: "dlg-note", {t("ui_dialogs.settings.az_automatikus_a_keszulek_nyelvet_hasznalja_ha_az_ta")} }
                    }
                    div { class: "dlg-row2",
                        div { class: "dlg-control",
                            span { class: "label", {t("ui_dialogs.settings.tema")} }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("dark".to_string(), t("ui_dialogs.settings.sotet").to_string()),
                                        ("light".to_string(), t("ui_dialogs.settings.vilagos").to_string()),
                                    ],
                                    value: theme_value(ui.theme),
                                    aria_label: t("ui_dialogs.settings.tema").to_string(),
                                    on_change: set_theme,
                                }
                            }
                        }
                        div { class: "dlg-control",
                            span { class: "label", {t("ui_dialogs.settings.terkepstilus")} }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("dark".to_string(), t("ui_dialogs.settings.sotet").to_string()),
                                        ("light".to_string(), t("ui_dialogs.settings.vilagos").to_string()),
                                        ("osm".to_string(), "OSM".to_string()),
                                    ],
                                    value: map_style_value(ui.map_style),
                                    aria_label: t("ui_dialogs.settings.terkepstilus").to_string(),
                                    on_change: set_map_style,
                                }
                            }
                        }
                    }
                }

                Group { title: t("ui_dialogs.settings.terkep_es_profil").to_string(),
                    Switch {
                        checked: ui.follow_rider,
                        on_change: set_follow,
                        label: t("ui_dialogs.settings.a_terkep_kovesse_a_kerekparost").to_string(),
                        hint: t("ui_dialogs.settings.kikapcsolva_szabadon_mozgathato_a_terkep_m").to_string(),
                        row: true,
                    }
                    div { class: "dlg-row2",
                        div { class: "dlg-control",
                            span { class: "label", {t("ui_dialogs.settings.profilnezet")} }
                            div { class: "dlg-segrow",
                                Segmented {
                                    options: vec![
                                        ("ahead".to_string(), t("ui_dialogs.settings.eloretekintes").to_string()),
                                        ("full".to_string(), t("ui_dialogs.settings.teljes").to_string()),
                                    ],
                                    value: profile_mode_value(ui.active_profile_mode()),
                                    aria_label: t("ui_dialogs.settings.profilnezet").to_string(),
                                    small: true,
                                    on_change: set_profile_mode,
                                }
                            }
                        }
                        Slider {
                            label: t("ui_dialogs.settings.eloretekintes_hossza").to_string(),
                            value: ui.lookahead_m,
                            min: 500.0,
                            max: 10_000.0,
                            step: 100.0,
                            unit: "m".to_string(),
                            hint: t("ui_dialogs.settings.csak_eloretekinto_nezetben_szamit").to_string(),
                            on_change: set_lookahead,
                        }
                    }
                    div { class: "dlg-control",
                        span { class: "label", {t("ui_dialogs.settings.szellem")} }
                        div { class: "dlg-segrow",
                            Segmented {
                                options: vec![
                                    ("off".to_string(), t("ui_dialogs.settings.ki").to_string()),
                                    ("route".to_string(), t("ui_dialogs.settings.az_utvonal_ideje").to_string()),
                                    ("last".to_string(), t("ui_dialogs.settings.legutobbi_menet").to_string()),
                                    ("best".to_string(), t("ui_dialogs.settings.legjobb_pr").to_string()),
                                ],
                                value: ghost_mode_value(ui.ghost),
                                aria_label: t("ui_dialogs.settings.szellem").to_string(),
                                small: true,
                                on_change: set_ghost,
                            }
                        }
                        span { class: "dlg-note dlg-note--ghost", "{ghost_hint}" }
                        span { class: "dlg-note", {t("ui_dialogs.settings.a_korabbi_ugyanezen_az_utvonalon_tekert_menet_vissza")} }
                    }
                }

                Group {
                    title: t("ui_dialogs.settings.elrendezes").to_string(),
                    variant: "layout".to_string(),
                    note: t("ui_dialogs.settings.a_paneleket_a_fejlecuknel_fogva_huzhatod_egy_masik_p").to_string(),
                    div { class: "dlg-control",
                        span { class: "label", {t("ui_dialogs.settings.alaphelyzet")} }
                        button {
                            class: "btn",
                            r#type: "button",
                            onclick: reset_layout,
                            {t("ui_dialogs.settings.elrendezes_visszaallitasa")}
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
            title: t("ui_dialogs.settings.biztonsagi_mentes").to_string(),
            note: t("ui_dialogs.settings.az_utvonalak_a_mentett_turak_az_importalt_edzesek_es").to_string(),
            div { class: "dlg-backup",
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| on_backup.call(()),
                    {t("ui_dialogs.settings.mentes_fajlba")}
                }
                if confirming() {
                    button {
                        class: "btn btn--danger",
                        r#type: "button",
                        onclick: move |_| {
                            confirming.set(false);
                            on_restore.call(());
                        },
                        {t("ui_dialogs.settings.igen_visszaallitom")}
                    }
                    button {
                        class: "btn btn--ghost",
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        {t("ui_dialogs.settings.megse")}
                    }
                } else {
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| confirming.set(true),
                        {t("ui_dialogs.settings.visszaallitas_fajlbol")}
                    }
                }
            }
            if confirming() {
                span { class: "dlg-note",
                    {t("ui_dialogs.settings.a_mentesben_levo_utvonalak_turak_es_edzesek_hozzaado")}
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
    /// A hook for the shell (`dlg-group--layout`: the tiling has no meaning on a phone).
    #[props(default = String::new())]
    variant: String,
    children: Element,
) -> Element {
    let class = if variant.is_empty() {
        "dlg-group".to_string()
    } else {
        format!("dlg-group dlg-group--{variant}")
    };
    rsx! {
        section { class: "{class}",
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

    // Counting (and deleting) thousands of JPEGs is file work: it runs off the UI thread, and the
    // delete asks once more first (the images came out of the rider's own Google quota).
    let storage = use_signal(ImageOverview::default);
    let busy = use_signal(|| false);
    let mut confirm_clear = use_signal(|| false);
    let request = use_signal(|| 0u64);
    let storage_error = use_signal(|| None::<String>);
    use_hook(move || refresh_storage(storage, busy, request, storage_error, false));
    let stored = storage();
    let stored_text = if let Some(error) = storage_error.read().as_ref() {
        error.clone()
    } else if stored.images == 0 {
        t("ui_dialogs.settings.meg_nincs_letoltott_kep").to_string()
    } else {
        let images = tn(
            "ui_dialogs.settings.kep_count",
            stored.images as i64,
            &[("count", &stored.images.to_string())],
        );
        let routes = tn(
            "ui_dialogs.settings.utvonal_count",
            stored.routes as i64,
            &[("count", &stored.routes.to_string())],
        );
        tf(
            "ui_dialogs.settings.kep_utvonalhoz",
            &[
                ("images", &images),
                ("size", &format_bytes(stored.bytes)),
                ("routes", &routes),
            ],
        )
    };
    let clear_storage = move |_| {
        confirm_clear.set(false);
        refresh_storage(storage, busy, request, storage_error, true);
    };

    rsx! {
        Group {
            title: "Street View".to_string(),
            note: t("ui_dialogs.settings.google_utcakep_ott_ahol_eppen_tekersz_a_sajat_api_ku").to_string(),
            Switch {
                checked: ui.streetview_enabled,
                on_change: set_enabled,
                label: t("ui_dialogs.settings.street_view_megjelenitese_a_terkepen").to_string(),
                hint: t("ui_dialogs.settings.kis_ablak_a_terkep_jobb_also_sarkaban_a_cimsor_gombj").to_string(),
                row: true,
            }

            div { class: key_class,
                label { class: "dlg-key__label", r#for: "dlg-gmaps-key", {t("ui_dialogs.settings.google_maps_api_kulcs")} }
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
                        "aria-label": if shown { t("ui_dialogs.settings.a_kulcs_elrejtese") } else { t("ui_dialogs.settings.a_kulcs_megjelenitese") },
                        onclick: move |_| revealed.set(!shown),
                        if shown {
                            {t("ui_dialogs.settings.elrejt")}
                        } else {
                            {t("ui_dialogs.settings.mutat")}
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
                    {t("ui_dialogs.settings.cloud_project")}
                }
                li {
                    {t("ui_dialogs.settings.enable_apis")}
                }
                li { {t("ui_dialogs.settings.a_hitelesito_adatok_oldalon_hozz_letre_api_kulcsot_e")} }
                li { {t("ui_dialogs.settings.masold_ide_a_kulcsot_csak_ezen_a_gepen_a_beallitasfa")} }
            }

            Switch {
                checked: ui.streetview_cache_panos,
                on_change: set_cache,
                label: t("ui_dialogs.settings.panorama_azonositok_mentese_az_utvonalhoz").to_string(),
                hint: t("ui_dialogs.settings.ismetelt_tekeresnel_kevesebb_kereses_kell_igy_kevese").to_string(),
                row: true,
            }

            Switch {
                checked: ui.streetview_offline,
                on_change: set_offline,
                label: t("ui_dialogs.settings.street_view_kepek_letoltese_halozat_nelkuli_tekeresh").to_string(),
                hint: t("ui_dialogs.settings.a_panel_letoltes_gombja_elmenti_az_utvonal_kepeit_ig").to_string(),
                row: true,
            }
            div { class: "dlg-storage",
                if confirm_clear() {
                    span { class: "dlg-storage__text", {t("ui_dialogs.settings.biztosan_torlod_a_letoltott_kepeket")} }
                    button {
                        class: "btn btn--danger btn--sm",
                        r#type: "button",
                        onclick: clear_storage,
                        {t("ui_dialogs.settings.torles")}
                    }
                    button {
                        class: "btn btn--ghost btn--sm",
                        r#type: "button",
                        onclick: move |_| confirm_clear.set(false),
                        {t("ui_dialogs.settings.megse")}
                    }
                } else {
                    span { class: "dlg-storage__text", "{stored_text}" }
                    button {
                        class: "btn btn--ghost btn--sm",
                        r#type: "button",
                        disabled: stored.images == 0 || busy(),
                        onclick: move |_| confirm_clear.set(true),
                        {t("ui_dialogs.settings.torles")}
                    }
                }
            }
            p { class: "dlg-note",
                strong { {t("ui_dialogs.settings.a_letoltott_kepek_legfeljebb_30_napig_maradnak_a_gep")} }
                {t("ui_dialogs.settings.csak_a_bekapcsolt_offline_opcio_hasznalja_oket_es_cs")}
            }
        }
    }
}

/// Recounts the downloaded Street View images (after deleting them and the saved panorama ids
/// when `clear`), on a blocking worker when a runtime is there; SSR and tests count in place.
fn refresh_storage(
    mut storage: Signal<ImageOverview>,
    mut busy: Signal<bool>,
    mut request: Signal<u64>,
    mut error: Signal<Option<String>>,
    clear: bool,
) {
    let id = request.peek().wrapping_add(1);
    request.set(id);
    let work = move || {
        let mut errors = Vec::new();
        if clear {
            if let Err(e) = ride_ui_streetview::imagestore::clear_all_default() {
                errors.push(e.to_string());
            }
            if let Err(e) = ride_ui_streetview::cache::clear_saved_default() {
                errors.push(e.to_string());
            }
        }
        (
            ride_ui_streetview::imagestore::overview_default(),
            (!errors.is_empty()).then(|| errors.join("; ")),
        )
    };
    let mut apply = move |(overview, reason): (ImageOverview, Option<String>)| {
        if *request.peek() != id {
            return;
        }
        storage.set(overview);
        error.set(reason.map(|reason| {
            tf(
                "ui_dialogs.settings.storage_clear_failed",
                &[("reason", &reason)],
            )
        }));
        busy.set(false);
    };
    if tokio::runtime::Handle::try_current().is_err() {
        apply(work());
        return;
    }
    busy.set(true);
    spawn(async move {
        match tokio::task::spawn_blocking(work).await {
            Ok(result) => apply(result),
            Err(e) => apply((ImageOverview::default(), Some(e.to_string()))),
        }
    });
}

/// `1 234 567` bytes as `1.2 MB` (or `345 kB`), with the language's decimal separator; decimal
/// units both (a megabyte is 1 000 kB here, as the kB line implies).
fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 1.0 {
        format!("{} MB", ride_i18n::decimal(mb, 1))
    } else {
        format!("{:.0} kB", bytes as f64 / 1000.0)
    }
}

/// `Automatic` plus every language with a finished catalog, in the registry's order. The
/// language's own name is never translated (a rider looking for their language recognizes it as
/// written there).
fn language_options() -> Vec<(String, String)> {
    let mut options = vec![(
        "auto".to_string(),
        t("ui_dialogs.settings.automatikus").to_string(),
    )];
    for lang in ride_i18n::Lang::complete() {
        options.push((lang.code().to_string(), lang.self_name().to_string()));
    }
    options
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
            5,
            "theme, map style, language, profile mode and ghost each have one selected option"
        );
        // choosing "Ki" moves the selection to that option
        let mut off = Settings::default();
        off.ui.ghost = ride_model::GhostMode::Off;
        let html = render(off);
        assert_eq!(html.matches(r#"aria-checked="true""#).count(), 5);
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
