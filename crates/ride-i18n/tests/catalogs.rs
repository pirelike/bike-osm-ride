//! Quality gates for the translation catalogs.
//!
//! These read the JSON files directly (the same way `build.rs` does) instead of going through
//! `t()`: a translator's mistake is a data problem, and the failure message should name the key.
//!
//! The source language is Hungarian. A catalog is *complete* when it carries every source key
//! except the `i18n.test.*` fixtures; the picker only offers complete catalogs, and
//! `build.rs` derives that list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SOURCE: &str = "hu";

fn locales() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("locales")
}

fn read_catalog(code: &str) -> BTreeMap<String, String> {
    let path = locales().join(format!("{code}.json"));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Every language code with a catalog file, in a stable order.
fn catalog_codes() -> Vec<String> {
    let mut codes: Vec<String> = std::fs::read_dir(locales())
        .expect("locales directory")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            (path.extension()?.to_str()? == "json" && !stem.starts_with('_')).then_some(stem)
        })
        .collect();
    codes.sort();
    codes
}

fn registry_codes() -> BTreeSet<String> {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("languages.tsv"))
        .expect("languages.tsv")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split('\t').next().map(|code| code.trim().to_string()))
        .collect()
}

/// A catalog's keys, plural forms collapsed (`items.one` → `items`) and test fixtures dropped.
/// `build.rs` derives `COMPLETE_LOCALES` the same way; this is the reference the tests use.
fn keys(catalog: &BTreeMap<String, String>) -> BTreeSet<String> {
    catalog
        .keys()
        .filter(|key| !key.starts_with("i18n.test."))
        .map(|key| match key.rsplit_once('.') {
            Some((head, "one" | "other")) => head.to_string(),
            _ => key.clone(),
        })
        .collect()
}

#[test]
fn every_registry_language_has_a_catalog() {
    for code in registry_codes() {
        assert!(
            locales().join(format!("{code}.json")).is_file(),
            "{code} is in languages.tsv but locales/{code}.json is missing"
        );
    }
}

#[test]
fn a_catalog_covers_every_source_key() {
    // `build.rs` derives COMPLETE_LOCALES from exactly this comparison: a catalog missing a key is
    // not offered in the picker (and falls back to the source text), so this test says the shipped
    // translations are actually reachable. A half-finished catalog is *not* a failure here — it is
    // simply not offered — but the shipped, complete ones must all be complete.
    const COMPLETE: &[&str] = &["en", "es", "pt", "de", "it", "zh"];
    let source = keys(&read_catalog(SOURCE));
    for code in COMPLETE {
        let catalog = keys(&read_catalog(code));
        let missing: Vec<&String> = source.difference(&catalog).collect();
        assert!(
            missing.is_empty(),
            "{code}: {} source key(s) missing, e.g. {:?}",
            missing.len(),
            &missing[..missing.len().min(5)]
        );
    }
}

#[test]
fn a_catalog_never_invents_a_key() {
    // A key the source does not know is dead text: it can never be looked up.
    let source = keys(&read_catalog(SOURCE));
    for code in catalog_codes() {
        if code == SOURCE {
            continue;
        }
        for key in keys(&read_catalog(&code)) {
            assert!(
                source.contains(&key),
                "{code}: {key} does not exist in the source catalog"
            );
        }
    }
}

/// `code` → plural rule (`one-other`, `other-only`) from `languages.tsv`.
fn plural_rules() -> BTreeMap<String, String> {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("languages.tsv"))
        .expect("languages.tsv")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let code = cols.next()?.trim().to_string();
            let rule = cols.nth(2)?.trim().to_string();
            Some((code, rule))
        })
        .collect()
}

#[test]
fn a_language_with_a_singular_has_every_singular_form() {
    // Regression: the Italian catalog (rule `one-other`) carried only the `.other` forms, so a
    // count of one read "1 immagini" / "1 percorsi". `tn()` falls back to the bare key and then
    // to Hungarian, never to `.other`, so a missing `.one` is never caught at run time.
    let source = read_catalog(SOURCE);
    let counted: BTreeSet<&str> = source
        .keys()
        .filter(|key| !key.starts_with("i18n.test."))
        .filter_map(|key| key.strip_suffix(".other"))
        .collect();
    assert!(!counted.is_empty(), "the source has counted keys");
    for (code, rule) in plural_rules() {
        if rule != "one-other" || !locales().join(format!("{code}.json")).is_file() {
            continue;
        }
        let catalog = read_catalog(&code);
        for key in &counted {
            if catalog.contains_key(&format!("{key}.other")) {
                assert!(
                    catalog.contains_key(&format!("{key}.one")),
                    "{code}: {key}.one is missing (rule one-other)"
                );
            }
        }
    }
}

#[test]
fn a_catalog_never_carries_an_empty_string() {
    for code in catalog_codes() {
        for (key, value) in read_catalog(&code) {
            assert!(!value.trim().is_empty(), "{code}: {key} is empty");
        }
    }
}

#[test]
fn placeholders_match_the_source() {
    let source = read_catalog(SOURCE);
    for code in catalog_codes() {
        if code == SOURCE {
            continue;
        }
        let catalog = read_catalog(&code);
        for (key, value) in &catalog {
            let Some(source_value) = source.get(key) else {
                continue;
            };
            assert_eq!(
                placeholders(source_value),
                placeholders(value),
                "{code}: {key} has different placeholders\n  hu: {source_value}\n  {code}: {value}"
            );
        }
    }
}

/// `{name}`-style placeholders of a template, sorted (order is free, the set is not).
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let inner = &after[..end];
        // `{:.0}` and `{}` are positional; they still have to appear the same number of times.
        out.push(inner.to_string());
        rest = &after[end + 1..];
    }
    out.sort();
    out
}

#[test]
fn surrounding_whitespace_is_preserved() {
    // Several strings are glued to a link or to a value, so a lost leading or trailing space
    // changes the rendered sentence.
    // Keys where the source glues a Hungarian case suffix to the preceding link, so the
    // translation legitimately starts a new word (with a space) instead.
    const LEADING_SPACE_EXCEPTIONS: &[&str] =
        &["ui_dialogs.settings.ban_es_kapcsold_be_a_szamlazast_a_google_havi_ingyen"];
    let source = read_catalog(SOURCE);
    for code in catalog_codes() {
        if code == SOURCE {
            continue;
        }
        for (key, value) in read_catalog(&code) {
            let Some(source_value) = source.get(&key) else {
                continue;
            };
            if !LEADING_SPACE_EXCEPTIONS.contains(&key.as_str()) {
                assert_eq!(
                    value.starts_with(char::is_whitespace),
                    source_value.starts_with(char::is_whitespace),
                    "{code}: {key} lost or gained a leading space"
                );
            }
            assert_eq!(
                value.ends_with(char::is_whitespace),
                source_value.ends_with(char::is_whitespace),
                "{code}: {key} lost or gained a trailing space"
            );
        }
    }
}

#[test]
fn a_translation_is_never_left_in_hungarian() {
    // Values that are legitimately identical: proper nouns, unit templates, pure placeholder
    // templates and words Spanish or English share with Hungarian.
    const SAME: &[&str] = &[
        "model.workout.anaerob",
        "model.workout.sprint",
        "model.workout.sweet_spot",
        "route.bikerouter.name_with_km",
        "ui_dialogs.helpers.sprint",
        "ui_dialogs.settings.kep_utvonalhoz",
        "ui_dialogs.settings.trainer_csoport",
        "ui_dialogs.summary.energia",
        "ui_dialogs.workouts.lista",
        "ui_hud.energia",
        "ui_hud.energia_kcal",
        "ui_kit.palette.aerob",
        "ui_kit.palette.anaerob",
        "ui_kit.palette.vo_max",
        "ui_streetview.download.offline_street_view_kep",
    ];
    let source = read_catalog(SOURCE);
    for code in catalog_codes() {
        if code == SOURCE {
            continue;
        }
        for (key, value) in read_catalog(&code) {
            let Some(source_value) = source.get(&key) else {
                continue;
            };
            if &value != source_value {
                continue;
            }
            assert!(
                SAME.contains(&key.as_str()),
                "{code}: {key} is still the Hungarian text: {value}"
            );
        }
    }
}

#[test]
fn no_translation_carries_hungarian_letters() {
    // `ő` and `ű` exist in neither Spanish, English, German, Portuguese, Italian nor Chinese;
    // finding one means a word was copied from the source. A language whose orthography does use
    // them stays out.
    for code in ["es", "en", "de", "pt", "it", "zh"] {
        for (key, value) in read_catalog(code) {
            assert!(
                !value.contains(['ő', 'ű', 'Ő', 'Ű']),
                "{code}: {key} contains a Hungarian letter: {value}"
            );
        }
    }
}

#[test]
fn spanish_calls_a_rider_a_cyclist() {
    // "tekerő" is a rider on a bike, never a horse rider: `lovas` / `jinete` would be a mistranslation.
    for (key, value) in read_catalog("es") {
        let lower = value.to_lowercase();
        for word in ["lovas", "jinete", "equitaci"] {
            assert!(!lower.contains(word), "es: {key} uses {word}: {value}");
        }
    }
}

#[test]
fn english_calls_a_rider_a_rider() {
    // Same rule as the Spanish one above, in English: a "rider" is on a bike, not on a horse.
    for (key, value) in read_catalog("en") {
        let lower = value.to_lowercase();
        for word in ["horseback", "equestrian", "jockey", "horseman", "steed"] {
            assert!(!lower.contains(word), "en: {key} uses {word}: {value}");
        }
    }
}

#[test]
fn portuguese_calls_a_rider_a_cyclist() {
    // The same rule in Portuguese: a rider is on a bicycle, never on a horse (`cavaleiro`,
    // `equitação`), and the Spanish `jinete` is not Portuguese either.
    for (key, value) in read_catalog("pt") {
        for word in words(&value) {
            assert!(
                !matches!(
                    word.to_lowercase().as_str(),
                    "cavaleiro"
                        | "cavaleira"
                        | "cavaleiros"
                        | "cavaleiras"
                        | "equitação"
                        | "jinete"
                ),
                "pt: {key} uses a horse-rider word: {value}"
            );
        }
    }
}

#[test]
fn portuguese_keeps_the_european_forms() {
    // The catalog is European Portuguese: the Brazilian variants below are whole words here, so
    // a reviewer (or a later translator) gets a failing test instead of a mixed catalog.
    const BRAZILIAN: &[&str] = &[
        "você",
        "vocês",
        "arquivo",
        "arquivos",
        "salvar",
        "excluir",
        "tela",
        "telas",
        "configuração",
        "configurações",
        "registro",
        "registros",
        "celular",
        "planejar",
    ];
    for (key, value) in read_catalog("pt") {
        for word in words(&value) {
            assert!(
                !BRAZILIAN.contains(&word.to_lowercase().as_str()),
                "pt: {key} uses the Brazilian form {word}: {value}"
            );
        }
    }
}

/// Whole words of `text`, as written.
fn words(text: &str) -> impl Iterator<Item = &str> + '_ {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
}

#[test]
fn german_calls_a_rider_a_fahrer() {
    // "tekerő" is a rider on a bike, never a horse rider. Token comparison, not substring:
    // `breiter` contains `reiter`, and it is a perfectly good German word.
    for (key, value) in read_catalog("de") {
        for word in words(&value) {
            assert!(
                !matches!(
                    word.to_lowercase().as_str(),
                    "reiter" | "reiterin" | "reitern" | "pferd" | "pferde" | "pferden"
                ),
                "de: {key} uses a horse-rider word: {value}"
            );
        }
    }
}

#[test]
fn german_keeps_the_informal_address() {
    // The Hungarian source says "te", so German says "du". A capitalised "Sie" is the formal
    // register and reads wrong next to the rest of the UI; lowercase "sie" (they/she) is fine.
    for (key, value) in read_catalog("de") {
        for word in words(&value) {
            assert!(
                !matches!(
                    word,
                    "Sie" | "Ihnen" | "Ihre" | "Ihren" | "Ihrem" | "Ihrer" | "Ihres"
                ),
                "de: {key} uses the formal address: {value}"
            );
        }
    }
}

#[test]
fn chinese_calls_a_rider_a_cyclist() {
    // The same rule in Chinese: a rider is on a bicycle, never on a horse. 骑马 / 马术 / 赛马 are
    // whole-word terms for horse riding; a cyclist is 骑行者.
    for (key, value) in read_catalog("zh") {
        for phrase in ["骑马", "马术", "赛马"] {
            assert!(
                !value.contains(phrase),
                "zh: {key} uses a horse-riding phrase {phrase}: {value}"
            );
        }
    }
}

#[test]
fn chinese_keeps_the_informal_address() {
    // The Hungarian source addresses the rider as "te"; Chinese uses 你, never the formal 您.
    for (key, value) in read_catalog("zh") {
        assert!(
            !value.contains('您'),
            "zh: {key} uses the formal 您: {value}"
        );
    }
}

#[test]
fn chinese_uses_simplified_forms() {
    // A traditional character in a Simplified catalog is a copy-paste from a non-mainland source.
    // The list is limited to characters that are unambiguously traditional (their Simplified form
    // is a different code point), so a mainland term never trips it.
    const TRADITIONAL: &[char] = &[
        '騎', '車', '訓', '設', '開', '關', '這', '個', '們', '時', '間', '實', '現', '對', '為',
        '與', '從', '後', '進', '還', '過', '選', '單', '確', '認', '點', '數', '據', '圖', '標',
        '準', '階', '級', '態', '總', '結', '練', '習', '體', '驗', '證', '頁', '網', '線', '碼',
        '機', '動', '靜', '壓', '變', '檔', '風', '雲', '電', '腦', '軟', '應', '該', '讓', '覺',
    ];
    for (key, value) in read_catalog("zh") {
        for ch in TRADITIONAL {
            assert!(
                !value.contains(*ch),
                "zh: {key} contains the traditional character {ch}: {value}"
            );
        }
    }
}

#[test]
fn italian_calls_a_rider_a_ciclista() {
    // The same rule in Italian: a rider is on a bicycle, never on a horse. `cavallo` is left out
    // on purpose: it is also horsepower, a perfectly good word in a cycling app.
    for (key, value) in read_catalog("it") {
        for word in words(&value) {
            assert!(
                !matches!(
                    word.to_lowercase().as_str(),
                    "cavaliere" | "cavalieri" | "fantino" | "fantini" | "equitazione"
                ),
                "it: {key} uses a horse-rider word: {value}"
            );
        }
    }
}
