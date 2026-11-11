//! Language registry and text catalogs.
//!
//! The source language is Hungarian; every other language is a JSON map in `locales/`. A key is
//! looked up in the current language and falls back to Hungarian when it is missing (a debug build
//! says so on stderr). The current language is process-wide; tests override it per thread with
//! [`with_lang`], so the Hungarian assertions of the existing tests keep working.
//!
//! * [`Lang`] — one supported language, identified by its code (`"hu"`, `"en"`, `"es"`, ...).
//! * [`t`] — a translated `&'static str` (the signature the label helpers already return).
//! * [`tf`] — a translated template with `{name}` placeholders.
//! * [`tn`] — a plural form (`<key>.one` / `<key>.other`) with placeholders.
//!
//! Adding a language: a row in `languages.tsv` and `locales/<code>.json`; no Rust changes.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{OnceLock, RwLock};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));
}

use generated::CATALOGS;

// ───────────────────────────── language registry ─────────────────────────────

/// How a language forms plurals. The catalog carries one entry per form; [`tn`] picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralRule {
    /// One form for every count (Hungarian does not mark the plural after a numeral).
    OtherOnly,
    /// `one` for exactly 1, `other` otherwise (English, Spanish, ...).
    OneOther,
}

#[derive(Debug)]
struct LangInfo {
    code: &'static str,
    self_name: &'static str,
    html_lang: &'static str,
    plural: PluralRule,
    decimal: char,
    script: &'static str,
    rtl: bool,
}

/// One supported language. `Copy` and cheap; the code is one of the registry's static strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lang {
    code: &'static str,
}

impl Lang {
    /// Hungarian, the source language.
    pub const HU: Lang = Lang { code: "hu" };
    /// English, the pivot language of the translation pipeline.
    pub const EN: Lang = Lang { code: "en" };
    /// Spanish.
    pub const ES: Lang = Lang { code: "es" };
    /// German.
    pub const DE: Lang = Lang { code: "de" };

    /// The language with this code (`"es"`, `"es-ES"`, `"es_MX"`), if it is supported. An exact
    /// registry code wins; otherwise the primary subtag is tried (`es-419` → `es`).
    pub fn from_code(code: &str) -> Option<Lang> {
        let normalized = code.trim().replace('_', "-");
        if normalized.is_empty() {
            return None;
        }
        if let Some(info) = registry()
            .iter()
            .find(|info| info.code.eq_ignore_ascii_case(&normalized))
        {
            return Some(Lang { code: info.code });
        }
        let primary = normalized.split('-').next().unwrap_or(&normalized);
        registry()
            .iter()
            .find(|info| info.code.eq_ignore_ascii_case(primary))
            .map(|info| Lang { code: info.code })
    }

    /// Every supported language, in registry order.
    pub fn supported() -> impl Iterator<Item = Lang> {
        registry().iter().map(|info| Lang { code: info.code })
    }

    /// Every language whose UI text is finished — what the language picker offers.
    ///
    /// A catalog that is missing source keys would show raw keys to a rider, so it is not offered
    /// until it is complete. Translators still reach it with `RIDE_LANG`.
    pub fn complete() -> impl Iterator<Item = Lang> {
        Lang::supported().filter(|lang| lang.has_full_catalog())
    }

    /// True when this language's catalog covers every key of the source language.
    pub fn has_full_catalog(self) -> bool {
        generated::COMPLETE_LOCALES.contains(&self.code)
    }

    pub fn code(self) -> &'static str {
        self.code
    }

    /// The language's own name (`Magyar`, `English`, `Español`), for the settings dialog.
    pub fn self_name(self) -> &'static str {
        self.info().self_name
    }

    /// The value of `<html lang>`.
    pub fn html_lang(self) -> &'static str {
        self.info().html_lang
    }

    /// Decimal separator of everyday numbers (the HUD keeps the dot for tabular alignment).
    pub fn decimal_sep(self) -> char {
        self.info().decimal
    }

    /// Script family (`latin`, `latin-ext`, `cyrillic`, `greek`, `cjk`, ...): the UI falls back to
    /// a system font for scripts the embedded Barlow does not cover.
    pub fn script(self) -> &'static str {
        self.info().script
    }

    pub fn is_rtl(self) -> bool {
        self.info().rtl
    }

    pub fn plural_rule(self) -> PluralRule {
        self.info().plural
    }

    /// The plural form a count selects.
    pub fn plural_form(self, n: i64) -> &'static str {
        match self.plural_rule() {
            PluralRule::OtherOnly => "other",
            PluralRule::OneOther if n == 1 => "one",
            PluralRule::OneOther => "other",
        }
    }

    fn info(self) -> &'static LangInfo {
        registry()
            .iter()
            .find(|info| info.code == self.code)
            .unwrap_or(&registry()[0])
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

fn registry() -> &'static [LangInfo] {
    static REGISTRY: OnceLock<Vec<LangInfo>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut out = Vec::new();
        for line in include_str!("../languages.tsv").lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t').map(str::trim);
            let (
                Some(code),
                Some(self_name),
                Some(html_lang),
                Some(plural),
                Some(decimal),
                Some(script),
                Some(rtl),
            ) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            )
            else {
                continue;
            };
            let plural = match plural {
                "one-other" => PluralRule::OneOther,
                _ => PluralRule::OtherOnly,
            };
            out.push(LangInfo {
                code: leak(code),
                self_name: leak(self_name),
                html_lang: leak(html_lang),
                plural,
                decimal: decimal.chars().next().unwrap_or('.'),
                script: leak(script),
                rtl: rtl.eq_ignore_ascii_case("yes"),
            });
        }
        if out.is_empty() {
            out.push(LangInfo {
                code: "hu",
                self_name: "Magyar",
                html_lang: "hu",
                plural: PluralRule::OtherOnly,
                decimal: ',',
                script: "latin-ext",
                rtl: false,
            });
        }
        out
    })
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// ───────────────────────────── current language ─────────────────────────────

static GLOBAL: RwLock<Lang> = RwLock::new(Lang::HU);

thread_local! {
    /// Test override; `None` means "use the process-wide language".
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

/// The language in force on this thread.
pub fn current() -> Lang {
    if let Some(lang) = OVERRIDE.with(Cell::get) {
        return lang;
    }
    *GLOBAL
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Sets the process-wide language (the app calls this once, from the persisted settings).
pub fn set_lang(lang: Lang) {
    *GLOBAL
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = lang;
}

/// Runs `f` with `lang` on this thread, restoring the previous language afterwards (even on a
/// panic). Tests use it; the app changes the language with [`set_lang`].
pub fn with_lang<R>(lang: Lang, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|cell| cell.set(self.0));
        }
    }
    let previous = OVERRIDE.with(|cell| cell.replace(Some(lang)));
    let _restore = Restore(previous);
    f()
}

/// Resolves the setting: `RIDE_LANG` (scripts, no completeness check) wins, then `"auto"` (the OS
/// language when its catalog is complete), then the stored code, then Hungarian.
pub fn init(setting: &str) -> Lang {
    let from_env = std::env::var("RIDE_LANG")
        .ok()
        .and_then(|code| Lang::from_code(&code));
    let lang = from_env
        .or_else(|| {
            if setting.eq_ignore_ascii_case("auto") {
                Some(detect())
            } else {
                Lang::from_code(setting).filter(|lang| lang.has_full_catalog())
            }
        })
        .unwrap_or(Lang::HU);
    set_lang(lang);
    lang
}

/// The operating system's language when its catalog is complete, Hungarian otherwise.
///
/// The fallback is the source language, not English: a rider whose language has no catalog yet
/// should see a finished UI in the project's language, never a half-translated one.
pub fn detect() -> Lang {
    sys_locale::get_locale()
        .and_then(|code| Lang::from_code(&code))
        .filter(|lang| lang.has_full_catalog())
        .unwrap_or(Lang::HU)
}

// ───────────────────────────── catalogs ─────────────────────────────

struct Catalog {
    map: HashMap<&'static str, &'static str>,
}

fn catalogs() -> &'static HashMap<&'static str, &'static Catalog> {
    static LOADED: OnceLock<HashMap<&'static str, &'static Catalog>> = OnceLock::new();
    LOADED.get_or_init(|| {
        let mut out = HashMap::new();
        for (code, json) in CATALOGS {
            let parsed: HashMap<String, String> = serde_json::from_str(json).unwrap_or_default();
            let mut map = HashMap::with_capacity(parsed.len());
            for (key, value) in parsed {
                map.insert(leak(&key), leak(&value));
            }
            out.insert(*code, &*Box::leak(Box::new(Catalog { map })));
        }
        out
    })
}

fn lookup(lang: Lang, key: &str) -> Option<&'static str> {
    catalogs()
        .get(lang.code)
        .and_then(|catalog| catalog.map.get(key).copied())
}

/// The translated text of `key` in the current language. A key that is missing in the current
/// language falls back to Hungarian (with a debug-build warning); a key missing everywhere is
/// returned as-is, so a missing translation is visible instead of an empty label.
pub fn t(key: &'static str) -> &'static str {
    let lang = current();
    if let Some(value) = lookup(lang, key) {
        return value;
    }
    if lang != Lang::HU {
        if let Some(value) = lookup(Lang::HU, key) {
            debug_missing(key, lang);
            return value;
        }
    }
    debug_missing(key, lang);
    key
}

/// Like [`t`], in an explicit language (for content that must not follow the UI, e.g. a saved
/// name's source text in a test).
pub fn t_in(lang: Lang, key: &'static str) -> &'static str {
    lookup(lang, key)
        .or_else(|| lookup(Lang::HU, key))
        .unwrap_or(key)
}

fn debug_missing(key: &str, lang: Lang) {
    #[cfg(debug_assertions)]
    eprintln!("[ride-i18n] missing key `{key}` for `{lang}`");
}

/// The translated template with `{name}` placeholders replaced by `args`. An unknown placeholder
/// is left in place (visible in the UI, never a silent empty string).
pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    interpolate(t(key), args)
}

/// The plural form of `key` for `n`, with placeholders. `<key>.one` / `<key>.other`; a catalog
/// without the suffixed form falls back to the bare key (Hungarian's single form).
pub fn tn(key: &'static str, n: i64, args: &[(&str, &str)]) -> String {
    let lang = current();
    let form = lang.plural_form(n);
    let suffixed = format!("{key}.{form}");
    if let Some(value) = lookup(lang, &suffixed) {
        return interpolate(value, args);
    }
    if let Some(value) = lookup(lang, key) {
        return interpolate(value, args);
    }
    if lang != Lang::HU {
        if let Some(value) = lookup(Lang::HU, &suffixed).or_else(|| lookup(Lang::HU, key)) {
            debug_missing(&suffixed, lang);
            return interpolate(value, args);
        }
    }
    debug_missing(&suffixed, lang);
    interpolate(&suffixed, args)
}

fn interpolate(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        match args.iter().find(|(arg, _)| *arg == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Whether `key` exists in the catalog of `lang` itself (no fallback to the source language).
pub fn has_in(lang: Lang, key: &str) -> bool {
    lookup(lang, key).is_some()
}

/// The embedded catalog codes, sorted (`hu`, `en`, ...).
pub fn catalog_codes() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = catalogs().keys().copied().collect();
    codes.sort_unstable();
    codes
}

// ───────────────────────────── numbers ─────────────────────────────

/// A number with the current language's decimal separator and exactly `decimals` fraction
/// digits: `1234,5` in Hungarian, `1234.5` in English. The HUD's tabular numerals keep their dot
/// through `ride_ui_kit::format`; this is for prose and labels.
pub fn decimal(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let text = format!("{value:.decimals$}");
    with_separator(text)
}

/// Like [`decimal`], but trailing zeros of the fraction are dropped: `75`, `9,5`, `0,004`.
pub fn decimal_trimmed(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let text = format!("{value:.decimals$}");
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    with_separator(trimmed)
}

fn with_separator(text: String) -> String {
    let separator = current().decimal_sep();
    if separator == '.' {
        text
    } else {
        text.replace('.', &separator.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_the_shipped_languages() {
        let codes: Vec<&str> = Lang::supported().map(Lang::code).collect();
        assert!(codes.contains(&"hu"), "source language: {codes:?}");
        assert!(codes.contains(&"en"), "pivot language: {codes:?}");
        assert!(codes.contains(&"es"), "target language: {codes:?}");
        assert!(codes.contains(&"de"), "target language: {codes:?}");
        assert_eq!(Lang::HU.self_name(), "Magyar");
        assert_eq!(Lang::ES.html_lang(), "es");
        assert_eq!(Lang::DE.self_name(), "Deutsch");
        assert_eq!(Lang::DE.decimal_sep(), ',');
        assert_eq!(Lang::HU.decimal_sep(), ',');
        assert_eq!(Lang::EN.decimal_sep(), '.');
    }

    #[test]
    fn codes_accept_regions_and_separators() {
        assert_eq!(Lang::from_code("es-ES"), Some(Lang::ES));
        assert_eq!(Lang::from_code("es_MX"), Some(Lang::ES));
        assert_eq!(Lang::from_code("EN"), Some(Lang::EN));
        assert_eq!(Lang::from_code("de-DE"), Some(Lang::DE));
        assert_eq!(Lang::from_code("xx-YY"), None);
        assert_eq!(Lang::from_code(""), None);
    }

    #[test]
    fn with_lang_overrides_only_this_thread() {
        assert_eq!(current(), Lang::HU);
        with_lang(Lang::EN, || {
            assert_eq!(current(), Lang::EN);
            assert_eq!(t("i18n.test.hello"), "Hello");
        });
        assert_eq!(current(), Lang::HU);
        assert_eq!(t("i18n.test.hello"), "Szia");
    }

    #[test]
    fn missing_keys_fall_back_to_hungarian_then_to_the_key() {
        with_lang(Lang::ES, || {
            assert_eq!(t("i18n.test.hello"), "Hola");
            assert_eq!(t("i18n.test.only_hu"), "Csak magyar");
            assert_eq!(t("i18n.test.nowhere"), "i18n.test.nowhere");
        });
    }

    #[test]
    fn templates_substitute_named_arguments() {
        with_lang(Lang::HU, || {
            assert_eq!(
                tf("i18n.test.greeting", &[("name", "Anna"), ("n", "3")]),
                "Szia Anna, 3 elem"
            );
            assert_eq!(tf("i18n.test.greeting", &[]), "Szia {name}, {n} elem");
        });
    }

    #[test]
    fn plurals_follow_the_language_rule() {
        with_lang(Lang::HU, || {
            assert_eq!(tn("i18n.test.items", 1, &[("n", "1")]), "1 elem");
            assert_eq!(tn("i18n.test.items", 3, &[("n", "3")]), "3 elem");
        });
        with_lang(Lang::EN, || {
            assert_eq!(tn("i18n.test.items", 1, &[("n", "1")]), "1 item");
            assert_eq!(tn("i18n.test.items", 3, &[("n", "3")]), "3 items");
        });
    }

    #[test]
    fn every_catalog_has_the_test_keys_and_valid_json() {
        for code in catalog_codes() {
            let lang = Lang::from_code(code).expect("registry row for catalog");
            assert!(has_in(lang, "i18n.test.hello"), "{code} parses");
        }
    }

    #[test]
    fn numbers_follow_the_language_separator() {
        with_lang(Lang::HU, || {
            assert_eq!(decimal(1234.5, 1), "1234,5");
            assert_eq!(decimal_trimmed(9.5, 1), "9,5");
            assert_eq!(decimal_trimmed(75.0, 1), "75");
        });
        with_lang(Lang::EN, || {
            assert_eq!(decimal(1234.5, 1), "1234.5");
            assert_eq!(decimal_trimmed(0.004, 4), "0.004");
        });
        with_lang(Lang::ES, || {
            assert_eq!(decimal(1234.5, 1), "1234,5");
        });
    }
}
