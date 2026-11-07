#!/usr/bin/env node
// Looks for text that is still in the source language (Hungarian) in a rendered UI.
//
// The input is the output of `node scripts/readme_shots.mjs --lang <code> --text`, which writes the
// visible text of every scenario next to its PNG (`<out>/<scenario>.txt`). This script reads those
// files and reports every line that looks Hungarian:
//
//   * a Hungarian accented letter (á é í ó ö ő ú ü ű) — the strong signal, and
//   * a short list of common Hungarian words that carry no accent (`Ki`, `kép`, `hátra`, …), which
//     the extractor's accented heuristic cannot see.
//
// Some text is *data*, not UI: a route name from the GPX file, a saved ride's name, a font name.
// Those lines are reported as `data` and do not fail the run; the allowlist below is the list of
// such sources. Everything else fails: an untranslated label is a bug, not a warning.
//
// Usage:
//   node scripts/i18n/check_ui.mjs target/es-text            # one folder of dumps
//   node scripts/i18n/check_ui.mjs target/es-text --lang es  # explicit target language (docs only)
//
// Run from the repository root.

import fs from "node:fs";
import path from "node:path";

// The letters Hungarian has and Spanish does not. `á é í ó ú ü` are shared with Spanish, so they
// say nothing; `ő` and `ű` are the ones a Spanish page must never show.
const HUNGARIAN_LETTER = /[őűŐŰ]/;

// Whole Hungarian words that a leftover string would contain. The accented ones are distinctive on
// their own; the accent-free ones (`Ki`, `hatra`, `kesz`) are what the accented heuristic misses.
const HUNGARIAN_WORDS = [
  "Ki",
  "kep",
  "kép",
  "kepek",
  "képek",
  "hatra",
  "hátra",
  "kesz",
  "kész",
  "utolso",
  "utolsó",
  "besorolatlan",
  "jelenleg",
  "mulva",
  "múlva",
  "elott",
  "előtt",
  "kozott",
  "között",
  "szerint",
  "valamint",
  "azonban",
  "illetve",
  "mintegy",
  "darab",
  "perc",
  "masodperc",
  "másodperc",
  "atlags",
  "átlag",
  "átlagos",
  "maximalis",
  "maximális",
  "megnyitas",
  "megnyitás",
  "bezar",
  "bezár",
  "torles",
  "törlés",
  "mentes",
  "mentés",
  "vissza",
  "elore",
  "előre",
  "emelkedő",
  "emelkedo",
  "edzés",
  "edzes",
  "útvonal",
  "utvonal",
  "beállítás",
  "beallitas",
  "szellem",
  "sebesség",
  "sebesseg",
  "terhelés",
  "terheles",
  "küszöb",
  "kuszob",
  "váltó",
  "valto",
  "eszköz",
  "eszkoz",
  "szenzor",
  "kulcs",
  "kadencia",
  "térkép",
  "terkep",
  "magasság",
  "magassag",
  "szöveg",
  "szoveg",
  "méret",
  "meret",
  "téma",
  "tema",
  "nyelv",
  "nyelven",
  "magyar",
  "szintemelkedés",
  "szintemelkedes",
  "hátralévő",
  "hatralevo",
  "következő",
  "kovetkezo",
  "pillanatkép",
  "pillanatkep",
  "műszerek",
  "muszerek",
  "vezérlés",
  "vezerles",
  "grafikonok",
  "adatok",
  "beállítások",
  "beallitasok",
  "eszközök",
  "eszkozok",
  "keresés",
  "kereses",
  "indítása",
  "inditasa",
  "csatlakozás",
  "csatlakozas",
  "szerepek",
  "jelenlegi",
  "menet",
  "menetek",
  "korábbi",
  "korabbi",
  "szimulált",
  "szimulalt",
  "rajtvonalon",
  "legutóbbi",
  "legutobbi",
  "legjobb",
  "teszt",
  "összegzés",
  "osszegzes",
  "áttekintés",
  "attekintes",
];

// Text that comes from the data, not from the UI: the GPX fixture's own route name, the
// file name of an import, a font family. A line carrying one of these is reported but passes.
const DATA_SOURCES = [
  "brouter_budapest_dobogoko",
  "Dobogókő",
  "BRouter",
  "Barlow",
  "OpenStreetMap",
  "Zwift",
  "MyWhoosh",
  "Garmin",
  "Strava",
];

const args = process.argv.slice(2);
const dir = args.find((a) => !a.startsWith("--"));
if (!dir) {
  console.error("usage: node scripts/i18n/check_ui.mjs <folder-with-txt-dumps>");
  process.exit(2);
}
const langArg = args.indexOf("--lang");
const target = langArg >= 0 ? args[langArg + 1] : "es";
if (target === "hu") {
  console.error("the source language is Hungarian: there is nothing to check");
  process.exit(2);
}

const files = fs
  .readdirSync(dir)
  .filter((name) => name.endsWith(".txt"))
  .sort();
if (!files.length) {
  console.error(`no .txt dumps in ${dir} - run readme_shots.mjs with --text first`);
  process.exit(2);
}

let problems = 0;
let dataLines = 0;
for (const file of files) {
  const text = fs.readFileSync(path.join(dir, file), "utf8");
  const lines = text.split(/\r?\n/);
  const hits = [];
  lines.forEach((line, index) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    const letter = HUNGARIAN_LETTER.exec(trimmed);
    const word = HUNGARIAN_WORDS.find(
      (w) => trimmed === w || new RegExp(`(^|[^\\p{L}])${escapeRe(w)}([^\\p{L}]|$)`, "u").test(trimmed),
    );
    if (!letter && !word) return;
    const data = DATA_SOURCES.find((d) => trimmed.includes(d));
    hits.push({ line: index + 1, text: trimmed.slice(0, 140), why: letter ? `letter ${letter[0]}` : `word ${word}`, data });
  });
  if (!hits.length) continue;
  console.log(`\n${file}`);
  for (const hit of hits) {
    if (hit.data) {
      dataLines += 1;
      console.log(`  ${String(hit.line).padStart(4)}  data (${hit.data}): ${hit.text}`);
    } else {
      problems += 1;
      console.log(`  ${String(hit.line).padStart(4)}  ${hit.why}: ${hit.text}`);
    }
  }
}

function escapeRe(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

console.log(
  `\n${files.length} dump(s) in ${dir}: ${problems} untranslated line(s), ${dataLines} data line(s)`,
);
process.exit(problems ? 1 : 0);
