# Translation brief — Italian (`it`)

> **Target language:** Italian (`it`, `html_lang="it"`, plural rule `one-other`, decimal comma, Latin
> script). This file is for a translator or a model doing the next pass; it is not part of the
> application. The review brief is [`REVIEW_PROMPT.md`](REVIEW_PROMPT.md).

## What you translate

The source language is Hungarian; English is the **pivot**. A sentence is written in Hungarian first
(`crates/ride-i18n/locales/hu.json`), then translated into English (`en.json`), and Italian is
translated from the English text while keeping the Hungarian meaning. When the two disagree, the
Hungarian wins; when the Hungarian is a suffix-heavy fragment, the English sentence shows the intent.

| Work | Source | Output |
|---|---|---|
| UI catalog | `crates/ride-i18n/locales/en.json` | `crates/ride-i18n/locales/it.json` |
| Workout library | `crates/ride-model/assets/workouts.en.map.json` (2 065 strings) | `crates/ride-model/assets/workouts.it.map.json`, then `workouts.it.json` |
| Rider documents | `docs/i18n/en/{README,USER_GUIDE,PRIVACY}.md` | `docs/i18n/it/` |

Read `crates/ride-i18n/GLOSSARY.tsv` (one row per concept, the `it` column is the reference) and
`crates/ride-i18n/STYLE.tsv` before the first sentence. The glossary is the single place a term
decision lives; update it when a better word is found, then make the catalogs agree.

## How to work

1. **New source text first.** A changed Hungarian string goes through `node scripts/i18n/extract.mjs`;
   the new keys appear in `hu.json` and in `_meta.json` (the `ctx` field names the screen and the
   element). Translate the new keys in every complete catalog, not only in Italian.
2. **Group by screen.** Work in blocks of about 100 keys that share a dialog or a panel (Settings,
   Devices, Review, …), so the surrounding sentences use the same words.
3. **Check mechanically.** `cargo test -p ride-i18n --test catalogs` validates placeholders,
   leading/trailing spaces, leftover Hungarian and the forbidden words. Fix the data, never the test.
4. **Look at the rendered page.** `node scripts/readme_shots.mjs --lang it --text --port 9460 --out
   target/it-text` writes the visible text of every screen; `node scripts/i18n/check_ui.mjs
   target/it-text --lang it` reports text that is still Hungarian. `node scripts/i18n/overflow.mjs
   --lang it` reports labels that do not fit at 412 × 860 (and at 1440 × 900).
5. **Workouts.** `node scripts/i18n/workout_worklist.mjs it` writes ordered chunks under
   `target/w-it/`; the matching English lines are in `chunkNN.en.json`. Translate the lines of a chunk
   into a JSON array in the same order (`chunkNN.it.json`), then `node
   scripts/i18n/merge_workouts.mjs it` and `node scripts/i18n/workouts.mjs it`. A missing string
   stops the build on purpose: half a workout in two languages is worse than none.
6. **Documents.** Translate the English rider documents. Keep every link, image path, code fence
   and number. `README.md` must contain the `## Licenza` section; licence names and SPDX identifiers
   stay unchanged.

## Voice and style

* Italian for a cyclist: `tu`, never the formal `Lei`; buttons in the infinitive (`Salva`, `Apri`,
  `Elimina`); sentence case for labels and headings.
* Short, factual, head-unit style. No filler, no marketing.
* Numbers: decimal comma, no thousands separator in the UI (`1234,5`), `55% FTP` without a space.
* Units and brands never translated: `W`, `W/kg`, `km`, `m`, `km/h`, `kcal`, `kJ`, `bpm`, `FTP`,
  `ERG`, `TCX`, `GPX`, `BRouter`, `bikerouter.de`, `Open-Meteo`, `Street View`, `Zwift`, `MyWhoosh`,
  `Barlow`, `bike-osm-ride`.
* `…` is one character, `“ ”` are the quotes, `–` is the en dash of the source.
* The vocabulary of the ride: *tekerő* → **ciclista**, *tekerés* → **pedalata**, *menet/túra* →
  **uscita**, *edzés* → **allenamento**, *útvonal* → **percorso**, *okos trainer* → **rullo
  intelligente**, *szellem* → **fantasma**, *teljesítmény* → **potenza**, *pulzus* → **frequenza
  cardiaca**, *küszöb* → **soglia**, *beállítások* → **impostazioni**, *eszközök* → **dispositivi**,
  *összegzés* → **riepilogo**, *áttekintés* → **Analisi**.

## Things that are easy to get wrong

* `szint` is the elevation shown in the HUD, not the accumulated climbing; climbing is
  `szintemelkedés` → **dislivello**.
* `magasság` on the profile is the real altitude → **altitudine**.
* `teljes` in the profile means the whole profile → **completo**.
* `menet`/`túra` is a recorded or finished ride → **uscita**, not "giro" (the demo loop is a *giro*).
* `blokk` is a workout block → **blocco**, not a text block.
* `pörgetés` is high-cadence easy pedalling → **frullata**.
* `fordulat` is cadence in rpm → **cadenza (rpm)**.
* `leadt` is the power actually delivered → **erogata**; `megtett` is the distance done → **percorsa**.
* `előretekintés` is the look-ahead mode → **anteprima**.
* `küszöb` is threshold → **soglia**, never “limite”.
* `terhelés` is both the trainer's resistance and the TSS load → **carico**; the context decides the
  sentence, not the word.
* `áttekintés` (the page that compares rides) is **Analisi**; “Revisione” sounds like an audit.
