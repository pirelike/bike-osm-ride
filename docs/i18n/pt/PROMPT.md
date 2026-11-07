# Translation brief — Portuguese (`pt`)

> **Target language:** European Portuguese (`pt`, `html_lang="pt"`, plural rule `one-other`, decimal
> comma, Latin script). This file is for a translator or a model doing the next pass; it is not part
> of the application. The review brief is [`REVIEW_PROMPT.md`](REVIEW_PROMPT.md).

## What you translate

The source language is Hungarian; English is the **pivot**. A sentence is written in Hungarian first
(`crates/ride-i18n/locales/hu.json`), then translated into English (`en.json`), and Portuguese is
translated from the English text while keeping the Hungarian meaning. When the two disagree, the
Hungarian wins; when the Hungarian is a suffix-heavy fragment, the English sentence shows the intent.

| Work | Source | Output |
|---|---|---|
| UI catalog | `crates/ride-i18n/locales/en.json` | `crates/ride-i18n/locales/pt.json` |
| Workout library | `crates/ride-model/assets/workouts.en.map.json` (2 065 strings) | `crates/ride-model/assets/workouts.pt.map.json`, then `workouts.pt.json` |
| Rider documents | `docs/i18n/en/{README,USER_GUIDE,PRIVACY}.md` | `docs/i18n/pt/` |

Read `crates/ride-i18n/GLOSSARY.tsv` (one row per concept, the `pt` column is the reference) and
`crates/ride-i18n/STYLE.tsv` before the first sentence. The glossary is the single place a term
decision lives; update it when a better word is found, then make the catalogs agree.

## How to work

1. **New source text first.** A changed Hungarian string goes through `node scripts/i18n/extract.mjs`;
   the new keys appear in `hu.json` and in `_meta.json` (the `ctx` field names the screen and the
   element). Translate the new keys in every complete catalog, not only in Portuguese.
2. **Group by screen.** Work in blocks of about 100 keys that share a dialog or a panel (Settings,
   Devices, Review, …), so the surrounding sentences use the same words.
3. **Check mechanically.** `cargo test -p ride-i18n --test catalogs` validates placeholders,
   leading/trailing spaces, leftover Hungarian, the forbidden words and the European forms. Fix the
   data, never the test.
4. **Look at the rendered page.** `node scripts/readme_shots.mjs --lang pt --text --port 9460 --out
   target/pt-text` writes the visible text of every screen; `node scripts/i18n/check_ui.mjs
   target/pt-text --lang pt` reports text that is still Hungarian. `node scripts/i18n/overflow.mjs
   --lang pt` reports labels that do not fit at 412 × 860 (and at 1440 × 900).
5. **Workouts.** `node scripts/i18n/workout_worklist.mjs pt` writes ordered chunks under
   `target/w-pt/`; the matching English lines are in `chunkNN.en.txt`. Translate the lines of a chunk
   into a JSON array in the same order (`chunkNN.pt.json`), then `node
   scripts/i18n/merge_workouts.mjs pt` and `node scripts/i18n/workouts.mjs pt`. A missing string
   stops the build on purpose: half a workout in two languages is worse than none.
6. **Documents.** Translate the English rider documents. Keep every link, image path, code fence
   and number. `README.md` must contain the `## Licença` section; licence names and SPDX identifiers
   stay unchanged.

## Voice and style

* European Portuguese: `tu`, never `você`; `ficheiro`, `ecrã`, `guardar`, `eliminar`, `definições`,
  `registo`, `telemóvel`, `planeia`.
* Sentence case for labels and headings; buttons in the infinitive (`Guardar`, `Abrir`, `Eliminar`).
* Short, factual, head-unit style. No filler, no marketing.
* Numbers: decimal comma, no thousands separator in the UI (`1234,5`), `55% FTP` without a space.
* Units and brands never translated: `W`, `W/kg`, `km`, `m`, `km/h`, `kcal`, `kJ`, `bpm`, `FTP`,
  `ERG`, `TCX`, `GPX`, `BRouter`, `bikerouter.de`, `Open-Meteo`, `Street View`, `Zwift`, `MyWhoosh`,
  `Barlow`, `bike-osm-ride`.
* `…` is one character, `“ ”` are the quotes, `–` is the en dash of the source.
* The vocabulary of the ride: *tekerő* → **ciclista**, *tekerés* → **pedalada**, *menet/túra* →
  **volta**, *edzés* → **treino**, *útvonal* → **percurso**, *okos trainer* → **rolo inteligente**,
  *szellem* → **fantasma**, *teljesítmény* → **potência**, *pulzus* → **pulso**, *küszöb* →
  **limiar**, *beállítások* → **definições**, *eszközök* → **dispositivos**, *összegzés* → **resumo**,
  *áttekintés* → **revisão**.

## Things that are easy to get wrong

* `szint` is the elevation shown in the HUD, not the accumulated climbing; climbing is
  `szintemelkedés` → **desnível** (or `desnível acumulado`).
* `teljes` in the profile means the whole profile → **completo**, not "cheio".
* `blokk` is a workout block → **bloco**, not a text block.
* `leadt` is the power actually delivered → **fornecida**; `megtett` is the distance done →
  **feita/percorrida**.
* `szabad` in the workout card means a free ride → **livre**.
* A simulated ride is `szimulált` → **simulada**; it is never a PR and never a ghost.
* The rider is a cyclist, never a horse rider: no `cavaleiro`, `equitação`, `jinete`.

## Definition of done

The catalog is complete (every source key), the catalog tests are green, the rendered pages have no
Hungarian line, no new horizontal overflow appears against the English run, the workout library is
complete and structurally identical to the source, and the three rider documents are in
`docs/i18n/pt/` with the licence section intact. Then ask a native speaker to read
[`REVIEW_PROMPT.md`](REVIEW_PROMPT.md) — machine translation alone is not a finished translation.
