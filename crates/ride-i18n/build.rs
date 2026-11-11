//! Collects `locales/*.json` into `CATALOGS`, so adding a language is dropping a file into
//! `locales/` plus a row in `languages.tsv` — no Rust source has to change.
//!
//! It also compares every catalog with the source language (`hu.json`) and emits
//! `COMPLETE_LOCALES`: the languages whose UI text is finished. The language picker only offers
//! those, so a half-translated catalog can never reach a rider (it stays reachable for translators
//! through `RIDE_LANG`).

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The source language: every other catalog must cover its keys.
const SOURCE: &str = "hu";

fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=languages.tsv");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let locales = manifest.join("locales");

    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(read) = fs::read_dir(&locales) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // `_meta.json` documents the keys for translators; it is not a catalog.
            if stem.starts_with('_') {
                continue;
            }
            entries.push((stem.to_string(), slash(&path)));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::from("/// Every embedded catalog: language code and its JSON text.\n");
    out.push_str("pub static CATALOGS: &[(&str, &str)] = &[\n");
    for (code, path) in &entries {
        out.push_str(&format!("    ({code:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");

    // Completeness: every plain key of the source catalog, and for every counted key the plural
    // forms the language's own rule needs (`tn` looks up `<key>.one` / `<key>.other`): a
    // `one-other` catalog without its `.other` forms would show the source text for every count
    // but one.
    let source_keys = entries
        .iter()
        .find(|(code, _)| code == SOURCE)
        .map(|(_, path)| keys_of(path))
        .unwrap_or_default();
    let (plain, counted): (BTreeSet<String>, BTreeSet<String>) = {
        let mut plain = BTreeSet::new();
        let mut counted = BTreeSet::new();
        for key in &source_keys {
            match plural_head(key) {
                Some(head) => {
                    counted.insert(head.to_string());
                }
                None => {
                    plain.insert(key.clone());
                }
            }
        }
        (plain, counted)
    };
    let rules = plural_rules(&manifest.join("languages.tsv"));
    let mut complete: Vec<&str> = Vec::new();
    for (code, path) in &entries {
        if code == SOURCE {
            complete.push(code);
            continue;
        }
        let keys = keys_of(path);
        let forms: &[&str] = match rules.get(code.as_str()).map(String::as_str) {
            Some("one-other") => &["one", "other"],
            _ => &["other"],
        };
        let plurals_ok = counted.iter().all(|head| {
            forms
                .iter()
                .all(|form| keys.contains(&format!("{head}.{form}")))
        });
        if plain.is_subset(&keys) && plurals_ok {
            complete.push(code);
        }
    }
    out.push_str("\n/// Languages whose catalog covers every key of the source language.\n");
    out.push_str("pub static COMPLETE_LOCALES: &[&str] = &[");
    for (i, code) in complete.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{code:?}"));
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").expect("out dir")).join("catalogs.rs");
    fs::write(&dest, out).expect("write catalogs.rs");
}

/// `items` of `items.one` / `items.other`, `None` for a plain key.
fn plural_head(key: &str) -> Option<&str> {
    match key.rsplit_once('.') {
        Some((head, "one" | "other")) => Some(head),
        _ => None,
    }
}

/// `code` → plural rule from `languages.tsv` (the fourth column).
fn plural_rules(path: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(path)
        .unwrap_or_default()
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

/// Top-level keys of a catalog, plural suffixes kept. The `i18n.test.*` fixtures are skipped:
/// they exist to exercise fallback and plurals, so a catalog may legitimately leave one out.
fn keys_of(path: &str) -> BTreeSet<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let map: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(&text) {
        Ok(map) => map,
        Err(err) => {
            println!("cargo:warning={path}: {err}");
            return BTreeSet::new();
        }
    };
    map.keys()
        .filter(|key| !key.starts_with("i18n.test."))
        .cloned()
        .collect()
}

/// `include_str!` paths are relative to the source file on every platform; forward slashes keep
/// Windows paths valid Rust string literals.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
