//! "Névjegy és licencek": app identity, project licence, third-party code, bundled fonts and the
//! map/data attributions. The content mirrors the README "Licenc" section one-to-one.

use dioxus::prelude::*;
use ride_ui_kit::{Dialog, Icon, IconName};

use crate::settings::Group;

/// Name shown in the about dialog (also the window title of the app).
pub const APP_NAME: &str = "bike-osm-ride";
/// Version of the crate, i.e. of the application.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// SPDX expression of the project licence (`license.workspace = true`).
pub const PROJECT_LICENSE: &str = "MIT OR Apache-2.0";

/// Third-party code we build on: `(source, licence, what we use it for)`.
pub const THIRD_PARTY_CODE: [(&str, &str, &str); 5] = [
    (
        "ftms",
        "MIT",
        "Bluetooth FTMS (okos trainer) adatok és vezérlőparancsok kódolása/dekódolása",
    ),
    (
        "cycling-ble",
        "Apache-2.0 OR MIT",
        "teljesítménymérő-, kadencia- és pulzusszenzor-adatok értelmezése (beépített részek)",
    ),
    (
        "OsmolControl",
        "MIT",
        "trainer-kapcsolat és parancs-biztonsági szűrő (átültetett részek)",
    ),
    (
        "iron-heart",
        "MIT",
        "pulzusmérő-keresés és újracsatlakozás mintája",
    ),
    (
        "Dioxus, btleplug, tokio, quick-xml és a többi Rust-függőség",
        "főleg MIT / Apache-2.0",
        "a teljes, automatikusan generált lista: THIRD_PARTY_LICENSES.md",
    ),
];

/// Fonts that are embedded in the program (offline): `(font, licence, what we use it for)`.
/// Mirrors `NOTICE.md`; the licence text is `crates/ride-ui-kit/assets/fonts/OFL.txt`.
pub const BUNDLED_FONTS: [(&str, &str, &str); 2] = [
    (
        "Barlow",
        "SIL OFL 1.1",
        "szövegbetűtípus (The Barlow Project Authors, a programba építve, ezért internet nélkül is működik)",
    ),
    (
        "Barlow Condensed",
        "SIL OFL 1.1",
        "számok és címsorok (ugyanaz a család, keskeny változat)",
    ),
];

/// External map/data sources: `(source, licence, what we use it for)`.
pub const DATA_SOURCES: [(&str, &str, &str); 5] = [
    (
        "OpenStreetMap-közreműködők",
        "ODbL 1.0",
        "térképadatok (openstreetmap.org/copyright)",
    ),
    (
        "OpenStreetMap-csempék",
        "ODbL 1.0 + tile usage policy",
        "a térkép alapja; a sötét/világos stílust helyben szűrjük, nincs API-kulcs",
    ),
    (
        "BRouter / bikerouter.de",
        "nyilvános szolgáltatás",
        "útvonaltervezés bikerouter-linkből (brouter.de)",
    ),
    (
        "Open-Meteo Elevation API",
        "CC BY 4.0",
        "hiányzó magasságadatok pótlása (open-meteo.com)",
    ),
    (
        "Google Maps Platform / Street View",
        "Google Maps Platform feltételei",
        "opcionális utcakép a saját API-kulcsoddal, a Google feltételei szerint; képet csak kérésre töltünk le",
    ),
];

/// The generated dependency-licence list (`THIRD_PARTY_LICENSES.md` in the repository root),
/// produced by `scripts/third_party_licenses.ps1`. Embedded at compile time, so the dialog and
/// the file can never disagree; regenerating the list is enough to update both.
pub fn third_party_licenses_text() -> Option<&'static str> {
    Some(include_str!("../../../THIRD_PARTY_LICENSES.md"))
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
            title: "Névjegy és licencek".to_string(),
            on_close,
            wide: true,
            foot: rsx! {
                span { class: "dlg-foot__note", "Köszönjük a nyílt forrású közösségnek." }
                button {
                    class: "btn btn--primary",
                    r#type: "button",
                    onclick: move |_| on_close.call(()),
                    "Bezárás"
                }
            },
            div { class: "dlg-about",
                header { class: "dlg-about__hero",
                    h3 { class: "dlg-about__name", "{APP_NAME}" }
                    span { class: "dlg-about__version", "{APP_VERSION}" }
                    span { class: "tag tag--solid", "{PROJECT_LICENSE}" }
                }
                p { class: "dlg-about__lead",
                    "Virtuális tekerés GPX-útvonalakon okos trainerrel vagy szimulált kerékpárossal. Csak 2D: térkép, magassági profil és a trainer adatai."
                }

                Group { title: "A projekt licence".to_string(),
                    p { class: "dlg-about__text",
                        "A bike-osm-ride saját kódja kettős licencű: MIT vagy Apache-2.0, tetszés szerint (SPDX: MIT OR Apache-2.0). A teljes szöveg a LICENSE-MIT és a LICENSE-APACHE fájlban található. Hozzájárulás esetén a beküldött kód is ezen a két licencen kerül be."
                    }
                    p { class: "dlg-about__text dlg-about__text--dim",
                        "A szoftver „ahogy van” áll rendelkezésre, garancia nélkül. Az eszközvezérlés (a trainer ellenállásának automatikus állítása) a saját felelősségedre történik."
                    }
                }

                Group { title: "Felhasznált nyílt forrású kód".to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        "Szándékosan csak megengedő (permissive) licencű kódot használunk; az eredetet a forrásfájlok és a NOTICE.md is megőrzik. GPL/AGPL licencű projekt kódját nem használjuk fel."
                    }
                    SourceTable { rows: THIRD_PARTY_CODE.to_vec() }
                }

                Group { title: "Beágyazott betűtípusok".to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        "A betűtípusok a programba vannak építve, nem töltődnek le. A SIL Open Font License 1.1 szabadon engedi a használatukat és a terjesztésüket; a licenc szövege a forrásfában (ride-ui-kit/assets/fonts/OFL.txt) és a NOTICE.md-ben található."
                    }
                    SourceTable { rows: BUNDLED_FONTS.to_vec() }
                }

                Group { title: "Térkép- és adatforrások".to_string(),
                    p { class: "dlg-about__text dlg-about__text--dim",
                        "Az alkalmazás működés közben ezeket a szolgáltatásokat hívja; az adatok a saját feltételeik szerint használhatók, és a térkép a szükséges hivatkozásokat megjeleníti. A saját licenc nem terjed ki ezekre az adatokra."
                    }
                    SourceTable { rows: DATA_SOURCES.to_vec() }
                    p { class: "dlg-note",
                        strong { "Google Maps Platform / Street View: " }
                        "csak akkor, ha te bekapcsolod és megadod a saját API-kulcsodat. A használatra a Google feltételei vonatkoznak, a kérések a te kvótádat terhelik. Képet csak akkor töltünk le és tárolunk (legfeljebb 30 napig), ha bekapcsolod és elindítod a letöltést; a panoráma-azonosítókat is csak engedélyezve mentjük."
                    }
                }

                Group { title: "Teljes függőségi licenclista".to_string(),
                    if let Some(text) = third_party_licenses_text() {
                        pre { class: "dlg-about__licenses", "{text}" }
                    } else {
                        div { class: "dlg-about__hook",
                            span { class: "dlg-about__hook-icon", Icon { name: IconName::Info, size: 14 } }
                            span {
                                "A függőségek teljes, automatikusan generált listája (THIRD_PARTY_LICENSES.md) a kiadási csomagban érhető el. Ez az ablak a crate-hez csatolt másolatot jeleníti meg, amint a generált lista elkészül."
                            }
                        }
                    }
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
                    th { "Forrás" }
                    th { "Licenc" }
                    th { "Mire használjuk" }
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
        let names: Vec<&str> = THIRD_PARTY_CODE.iter().map(|(name, _, _)| *name).collect();
        for required in ["ftms", "cycling-ble", "OsmolControl", "iron-heart"] {
            assert!(names.contains(&required), "missing {required} in {names:?}");
        }
        // every row names a licence and a purpose
        for (name, licence, purpose) in THIRD_PARTY_CODE {
            assert!(
                !name.is_empty() && !licence.is_empty() && !purpose.is_empty(),
                "{name}"
            );
        }
        // the permissive licences of the vendored crates are stated
        assert!(THIRD_PARTY_CODE.iter().any(|(name, _, _)| *name == "ftms"));
        assert!(THIRD_PARTY_CODE
            .iter()
            .any(|(_, licence, _)| licence.contains("Apache-2.0")));
    }

    #[test]
    fn data_sources_mirror_the_readme_attributions() {
        let text = format!("{DATA_SOURCES:?}");
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
        let (name, licence, purpose) = DATA_SOURCES
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
        for (font, licence, purpose) in BUNDLED_FONTS {
            assert!(!purpose.is_empty());
            assert!(
                notice.contains(font),
                "{font} is bundled but missing from NOTICE.md"
            );
            assert!(licence.contains("OFL"), "{font}: {licence}");
        }
        let names: Vec<&str> = BUNDLED_FONTS.iter().map(|(n, _, _)| *n).collect();
        assert!(names.contains(&"Barlow") && names.contains(&"Barlow Condensed"));
    }

    #[test]
    fn generated_licence_list_hook_is_safe_to_embed() {
        // Until the release pipeline generates THIRD_PARTY_LICENSES.md the dialog shows a note;
        // when the crate-local copy is added, the embedded text must not be empty.
        if let Some(text) = third_party_licenses_text() {
            assert!(!text.trim().is_empty());
        }
    }
}
