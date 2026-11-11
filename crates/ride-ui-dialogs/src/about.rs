//! "Névjegy és licencek": app identity, project licence, third-party code, bundled fonts and the
//! map/data attributions. The content mirrors the README "Licenc" section one-to-one.

use dioxus::prelude::*;
use ride_ui_kit::Dialog;

use crate::settings::Group;
use ride_i18n::t;

/// Name shown in the about dialog (also the window title of the app).
pub const APP_NAME: &str = "bike-osm-ride";
/// Version of the crate, i.e. of the application.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// SPDX expression of the project licence (`license.workspace = true`).
pub const PROJECT_LICENSE: &str = "MIT OR Apache-2.0";

/// Third-party code we build on: `(source, licence, what we use it for)`.
pub fn third_party_code() -> [(&'static str, &'static str, &'static str); 8] {
    [
        (
            "ftms",
            "MIT",
            t("ui_dialogs.about.bluetooth_ftms_okos_trainer_adatok_es_vezerloparancs"),
        ),
        (
            "cycling-ble",
            "Apache-2.0 OR MIT",
            t("ui_dialogs.about.teljesitmenymero_kadencia_es_pulzusszenzor_adatok_er"),
        ),
        (
            "OsmolControl",
            "MIT",
            t("ui_dialogs.about.trainer_kapcsolat_es_parancs_biztonsagi_szuro_atulte"),
        ),
        (
            "iron-heart",
            "MIT",
            t("ui_dialogs.about.pulzusmero_kereses_es_ujracsatlakozas_mintaja"),
        ),
        (
            "virtual-cog",
            "Unlicense",
            t("ui_dialogs.about.zwift_click_play_es_hub_ble_protokoll"),
        ),
        (
            "p256, hkdf, sha2, aes, ccm (RustCrypto)",
            "MIT / Apache-2.0",
            t("ui_dialogs.about.zwift_click_kulcscsere_es_titkositas"),
        ),
        (
            "Mozilla Firefox",
            "MPL-2.0",
            t("ui_dialogs.about.bongeszo_panel_a_telepitett_firefox"),
        ),
        (
            t("ui_dialogs.about.dioxus_btleplug_tokio_quick_xml_es_a_tobbi_rust_fugg"),
            t("ui_dialogs.about.foleg_mit_apache_2_0"),
            t("ui_dialogs.about.a_teljes_automatikusan_generalt_lista_third_party_li"),
        ),
    ]
}

/// Fonts that are embedded in the program (offline): `(font, licence, what we use it for)`.
/// Mirrors `NOTICE.md`; the licence text is `crates/ride-ui-kit/assets/fonts/OFL.txt`.
pub fn bundled_fonts() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "Barlow",
            "SIL OFL 1.1",
            t("ui_dialogs.about.szovegbetutipus_the_barlow_project_authors_a_program"),
        ),
        (
            "Barlow Condensed",
            "SIL OFL 1.1",
            t("ui_dialogs.about.szamok_es_cimsorok_ugyanaz_a_csalad_keskeny_valtozat"),
        ),
    ]
}

/// External map/data sources: `(source, licence, what we use it for)`.
pub fn data_sources() -> [(&'static str, &'static str, &'static str); 6] {
    [
        (
            t("ui_dialogs.about.openstreetmap_kozremukodok"),
            "ODbL 1.0",
            t("ui_dialogs.about.terkepadatok_openstreetmap_org_copyright"),
        ),
        (
            t("ui_dialogs.about.openstreetmap_csempek"),
            "ODbL 1.0 + tile usage policy",
            t("ui_dialogs.about.a_terkep_alapja_a_sotet_vilagos_stilust_helyben_szur"),
        ),
        (
            "BRouter / bikerouter.de",
            t("ui_dialogs.about.nyilvanos_szolgaltatas"),
            t("ui_dialogs.about.utvonaltervezes_bikerouter_linkbol_brouter_de"),
        ),
        (
            "Open-Meteo Elevation API",
            "CC BY 4.0",
            t("ui_dialogs.about.hianyzo_magassagadatok_potlasa_open_meteo_com"),
        ),
        (
            "Google Maps Platform / Street View",
            t("ui_dialogs.about.google_maps_platform_feltetelei"),
            t("ui_dialogs.about.opcionalis_utcakep_a_sajat_api_kulcsoddal_a_google_f"),
        ),
        (
            t("ui_dialogs.about.edzesleirasok"),
            PROJECT_LICENSE,
            t("ui_dialogs.about.edzesleirasok_sajat_szoveg"),
        ),
    ]
}

/// The generated dependency-licence list (`THIRD_PARTY_LICENSES.md` in the repository root),
/// produced by `scripts/third_party_licenses.ps1`. Embedded at compile time, so the dialog and
/// the file can never disagree; regenerating the list is enough to update both.
pub fn third_party_licenses_text() -> &'static str {
    include_str!("../../../THIRD_PARTY_LICENSES.md")
}

/// "Névjegy és licencek": app name/version, project licence (MIT OR Apache-2.0),
/// third-party code, bundled fonts and map/data attributions (incl. Google Maps Platform /
/// Street View), and the generated dependency licence list. Must mirror README "Licenc".
#[component]
pub fn AboutDialog(open: bool, on_close: EventHandler<()>) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        Dialog {
            open: true,
            title: t("ui_dialogs.about.nevjegy_es_licencek").to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note", {t("ui_dialogs.about.koszonjuk_a_nyilt_forrasu_kozossegnek")} }
            },
            div { class: "dlg-about",
                header { class: "dlg-about__hero",
                    h3 { class: "dlg-about__name", "{APP_NAME}" }
                    span { class: "dlg-about__version", "{APP_VERSION}" }
                    span { class: "tag tag--solid", "{PROJECT_LICENSE}" }
                }
                p { class: "dlg-about__lead",
                    {t("ui_dialogs.about.virtualis_tekeres_gpx_utvonalakon_okos_trainerrel_va")}
                }

                Group { title: t("ui_dialogs.about.a_projekt_licence").to_string(),
                    p { class: "dlg-about__text",
                        {t("ui_dialogs.about.a_bike_osm_ride_sajat_kodja_kettos_licencu_mit_vagy_")}
                    }
                    p { class: "dlg-about__text dlg-about__text--dim",
                        {t("ui_dialogs.about.a_szoftver_ahogy_van_all_rendelkezesre_garancia_nelk")}
                    }
                }

                Group { title: t("ui_dialogs.about.felhasznalt_nyilt_forrasu_kod").to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        {t("ui_dialogs.about.szandekosan_csak_megengedo_permissive_licencu_kodot_")}
                    }
                    SourceTable { rows: third_party_code().to_vec() }
                }

                Group { title: t("ui_dialogs.about.beagyazott_betutipusok").to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        {t("ui_dialogs.about.a_betutipusok_a_programba_vannak_epitve_nem_toltodne")}
                    }
                    SourceTable { rows: bundled_fonts().to_vec() }
                }

                Group { title: t("ui_dialogs.about.terkep_es_adatforrasok").to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        {t("ui_dialogs.about.az_alkalmazas_mukodes_kozben_ezeket_a_szolgaltatasok")}
                    }
                    SourceTable { rows: data_sources().to_vec() }
                    p { class: "dlg-note",
                        {t("ui_dialogs.about.streetview_notice")}
                    }
                }

                Group { title: t("ui_dialogs.about.teljes_fuggosegi_licenclista").to_string(),
                    pre { class: "dlg-about__licenses", "{third_party_licenses_text()}" }
                }
            }
        }
    }
}

/// A ruled three-column table: source, licence, purpose.
#[component]
fn SourceTable(rows: Vec<(&'static str, &'static str, &'static str)>) -> Element {
    rsx! {
        table { class: "dlg-table",
            thead {
                tr {
                    th { {t("ui_dialogs.about.forras")} }
                    th { {t("ui_dialogs.about.licenc")} }
                    th { {t("ui_dialogs.about.mire_hasznaljuk")} }
                }
            }
            tbody {
                for (source , licence , purpose) in rows {
                    tr { key: "{source}",
                        td { "{source}" }
                        td { class: "dlg-table__licence", "{licence}" }
                        td { "{purpose}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_the_package() {
        assert_eq!(APP_NAME, "bike-osm-ride");
        assert_eq!(APP_VERSION, env!("CARGO_PKG_VERSION"));
        assert!(PROJECT_LICENSE.contains("MIT") && PROJECT_LICENSE.contains("Apache-2.0"));
    }

    #[test]
    fn third_party_code_mirrors_the_readme() {
        let names: Vec<&str> = third_party_code()
            .iter()
            .map(|(name, _, _)| *name)
            .collect();
        for required in ["ftms", "cycling-ble", "OsmolControl", "iron-heart"] {
            assert!(names.contains(&required), "missing {required} in {names:?}");
        }
        // every row names a licence and a purpose
        for (name, licence, purpose) in third_party_code() {
            assert!(
                !name.is_empty() && !licence.is_empty() && !purpose.is_empty(),
                "{name}"
            );
        }
        // the permissive licences of the vendored crates are stated
        assert!(third_party_code()
            .iter()
            .any(|(name, _, _)| *name == "ftms"));
        assert!(third_party_code()
            .iter()
            .any(|(_, licence, _)| licence.contains("Apache-2.0")));
    }

    /// The first word of the source column of every row of the README's third-party table
    /// ("Felhasznált nyílt forrású kód"): `ftms`, `virtual-cog`, `Mozilla`, ...
    fn readme_sources() -> Vec<String> {
        let readme = include_str!("../../../README.md");
        let start = readme
            .find("### Felhasznált nyílt forrású kód")
            .expect("README third-party section");
        let section = &readme[start..];
        let end = section[4..].find("\n### ").map_or(section.len(), |i| i + 4);
        section[..end]
            .lines()
            .filter(|line| line.starts_with("| [") || line.starts_with("| `"))
            .filter_map(|line| {
                let cell = line.trim_start_matches('|').trim();
                let text = cell.strip_prefix('[')?.split(']').next()?;
                let word = text.trim_matches('`').split_whitespace().next()?;
                Some(word.trim_matches('`').to_string())
            })
            .collect()
    }

    #[test]
    fn every_row_of_the_readme_table_is_in_the_dialog() {
        let readme = readme_sources();
        assert!(readme.len() >= 9, "{readme:?}");
        let names: Vec<&str> = third_party_code()
            .iter()
            .chain(bundled_fonts().iter())
            .map(|(name, _, _)| *name)
            .collect();
        for source in &readme {
            assert!(
                names.iter().any(|name| name.contains(source.as_str())),
                "README lists {source}, the About dialog does not: {names:?}"
            );
        }
        // and the other way round: nothing in the dialog that the README does not name
        for name in third_party_code().iter().map(|(name, _, _)| *name) {
            assert!(
                readme.iter().any(|source| name.contains(source.as_str())),
                "the About dialog lists {name}, the README does not"
            );
        }
    }

    #[test]
    fn data_sources_mirror_the_readme_attributions() {
        let text = format!("{:?}", data_sources());
        for required in [
            "OpenStreetMap",
            "ODbL",
            "BRouter",
            "Open-Meteo",
            "CC BY",
            "Google Maps Platform",
            "Street View",
        ] {
            assert!(text.contains(required), "missing {required} in {text}");
        }
        // The former CARTO basemaps need an API key, so the tiles come from OpenStreetMap only.
        assert!(!text.contains("CARTO"), "stale CARTO attribution: {text}");
        // no GPL/AGPL source may ever appear in this list
        let lower = text.to_lowercase();
        for forbidden in ["gpl", "copyleft", "goldencheetah", "zwift"] {
            assert!(!lower.contains(forbidden), "{forbidden} listed as a source");
        }
    }

    #[test]
    fn street_view_attribution_is_honest_about_storage() {
        let (name, licence, purpose) = data_sources()
            .iter()
            .find(|(name, _, _)| name.contains("Street View"))
            .copied()
            .expect("Google Maps Platform / Street View row");
        assert!(name.contains("Google Maps Platform"));
        assert!(
            licence.contains("Google"),
            "Google's terms apply: {licence}"
        );
        assert!(purpose.contains("saját API-kulcsoddal"), "{purpose}");
        // images are stored only after an explicit download (the offline option)
        assert!(purpose.contains("csak kérésre"), "{purpose}");
    }

    #[test]
    fn bundled_fonts_mirror_the_notice_file() {
        let notice = include_str!("../../../NOTICE.md");
        for (font, licence, purpose) in bundled_fonts() {
            assert!(!purpose.is_empty());
            assert!(
                notice.contains(font),
                "{font} is bundled but missing from NOTICE.md"
            );
            assert!(licence.contains("OFL"), "{font}: {licence}");
        }
        let names: Vec<&str> = bundled_fonts().iter().map(|(n, _, _)| *n).collect();
        assert!(names.contains(&"Barlow") && names.contains(&"Barlow Condensed"));
    }

    #[test]
    fn generated_licence_list_hook_is_safe_to_embed() {
        // the generated list is embedded and names the dependencies it covers
        let text = third_party_licenses_text();
        assert!(
            text.contains("dioxus") && text.contains("btleplug"),
            "{}",
            &text[..text.len().min(200)]
        );
    }
}
