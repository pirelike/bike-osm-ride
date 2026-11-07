# Review brief — Portuguese (`pt`)

> **Target language:** European Portuguese. This file is for a native speaker (or a back-translation
> pass) reviewing the translation; it is not part of the application. The translation brief is
> [`PROMPT.md`](PROMPT.md).

## Your role

You are the Portuguese reviewer of bike-osm-ride. The text was translated by a model from the English
pivot and the Hungarian source, and **no native speaker has read it yet**. Do not retranslate
everything: find the places where a rider would misunderstand the screen, or where the sentence is
not Portuguese, and fix those. Record every decision that should outlive the fix.

## Inputs

| File | Content |
|---|---|
| `crates/ride-i18n/locales/pt.json` | UI labels, errors, hints (1 066 keys) |
| `crates/ride-model/assets/workouts.pt.json` | 93 workouts: name, goal, description, steps, coach messages (2 065 strings) |
| `crates/ride-i18n/GLOSSARY.tsv` | the term glossary; the `pt` column is the translation, `note` is the forbidden/required rule |
| `crates/ride-i18n/STYLE.tsv` | the style rules; the `pt` column is the translation |
| `docs/i18n/pt/README.md`, `USER_GUIDE.md`, `PRIVACY.md` | the rider documents |

Hungarian is the source language and English is the pivot. On an ambiguity the Hungarian wins, but do
not copy Hungarian case suffixes or word order into Portuguese.

## Must-check

1. **Placeholders** `{name}`, `{n}`, `{err}`, `{}`, `{:.0}` must match the source in spelling and
   count; the order may follow Portuguese word order.
2. **Leading/trailing spaces.** Several strings are glued to a link or a number; the space must stay.
   The one exception is `ui_dialogs.settings.ban_es_kapcsold_be_a_szamlazast_a_google_havi_ingyen`,
   which needs a leading space in Portuguese.
3. **Length.** Buttons, tabs, tile labels and chips are narrow: prefer 1–3 words. If a translation is
   much longer than the English, shorten it instead of relying on an ellipsis. The 412 × 860 run
   (`node scripts/i18n/overflow.mjs --lang pt --window 412x860`) is the check.
4. **Address.** `tu` throughout, never `você`; coach messages speak to the rider directly, in the
   imperative, short and encouraging.
5. **Rider.** **ciclista**, never `cavaleiro`, `equitação` or `jinete`.
6. **Terms.** Follow the `pt` column of `GLOSSARY.tsv`; the same concept must use the same word on
   every screen. When you change a term, change it in the glossary and in every catalog.
7. **Brands and units** stay: `bike-osm-ride`, `FTP`, `ERG`, `TCX`, `GPX`, `BRouter`, `bikerouter.de`,
   `Open-Meteo`, `Street View`, `Google`, `Zwift`, `MyWhoosh`, `FTMS`, `BLE`, `VO₂max`, `W`, `W/kg`,
   `km`, `m`, `%`, `kcal`, `kJ`, `bpm`, `Barlow`.
8. **Punctuation:** `…` one character, `“ ”`, `–` en dash. Sentence case in headings.
9. **Numbers:** decimal comma, no thousands separator in the UI (`1234,5`); do not convert units.
10. **European forms:** no `você`, `arquivo`, `salvar`, `excluir`, `tela`, `configuração`, `registro`,
    `celular`, `planejar`. The catalog test blocks this list.

## Risk list (look here first)

### A. Narrow labels

These live in tiles, tabs and buttons and are the first to overflow:

`Iniciar` `Pausa` `Terminar` `Continuar` `Guardar` `Cancelar` `Definições` `Dispositivos` `Percurso`
`Treino` `Revisão` `Resumo` `Mudança` `Limiar` `Aquecimento` `Arrefecimento` `Recuperação`
`Intervalos` `Sweet spot` `Sprint` `Subida` `Plano` `Livre` `Ligado` `A ligar…` `A procurar…`

### B. False friends and near-misses

| Key / place | Risk | Current choice |
|---|---|---|
| `szint` (HUD, welcome) | the source means elevation, but the HUD shows the accumulated climbing | elevação / desnível — check the context |
| `magasság` (profile) | the real altitude | altitude |
| `teljes` (profile mode) | the whole profile, not "full" | completo |
| `menet` / `túra` | a recorded ride, not a "turn" | volta |
| `blokk` (workout) | a workout block, not a text block | bloco |
| `pörgetés` | high-cadence easy pedalling | pedalada rápida |
| `fordulat` | cadence in rpm | cadência |
| `leadt` / `megtett` | power delivered vs distance done | fornecida / feita |
| `előretekintés` | look-ahead mode | antevisão |
| `összeállás` | a combined workout, not "assembly" | combinado |
| `tartósság` | durability, separate from endurance | resistência (if the context allows) |
| `küszöb` | threshold, never `limite` | limiar |

Watch for Portuguese words that look English but are not: `atualmente` means "currently", not
"actually"; `eventualmente` means "possibly"; `realizar` is "to carry out"; `assistir` is "to watch";
`pretender` is "to intend". Translate the English meaning, not the English shape.

### C. Gender and agreement

* **a volta** (feminine): *a volta foi guardada*, *uma volta simulada*; **o rolo** (masculine):
  *o rolo segue o terreno*; **a potência**, **a cadência**, **a elevação**, **a subida**, **a descida**,
  **o percurso**, **o treino**, **o bloco**, **o fantasma**, **o segmento**, **a definição** (pl.
  *definições*), **o dispositivo**. A wrong article is the most common machine-translation smell here.

### D. Long sentences and error messages

* Backup/restore, GPX/TCX import, Street View and BLE errors: calm and actionable, not a literal copy
  of the Hungarian clauses. Keep `{err}` / `{why}` / `{error}` exactly.
* `ui_hud.edzes_w_hatra_kesz` and `ui_profile.workout.edzes_kovetkezik_name_target` carry a
  `{current}` placeholder the source does not always fill; keep it, it is the source's bug, not the
  translation's.
* `ui_review.power.duration_seconds` / `duration_minutes` are built in code; check `5 s`, `20 min`,
  `1:12 h` after any change.

### E. Workout library

* Coach messages: one or two sentences, present tense, second person, concrete. Do not start every
  message with the same cheer.
* Step names: short. `Pörgetés` = high-cadence spinning, `Nyomaték` = torque, `Fal` = a steep wall,
  `Lead-out` = lead-out, `Álló rajt` = standing start.
* A workout name keeps its duration, zone and interval notation; `FTP`, `VO2max`, `Over/Under`,
  `30/30`, `Sweet spot` stay as they are.
* The generator (`node scripts/i18n/workouts.mjs pt`) refuses a missing string; a translated library
  must not change a duration, a target or a step count.

### F. Documents

* `docs/i18n/pt/README.md` must contain `## Licença`; licence names and SPDX identifiers
  (`MIT`, `Apache-2.0`, `MIT OR Apache-2.0`, `ODbL 1.0`, `CC BY 4.0`, `SIL OFL 1.1`, `MPL-2.0`,
  `Unlicense`) must not change, and the attributions may not gain or lose a row.
* Keep every link, image path, command and file name; `<version>` stays in the download commands.
* `docs/i18n/pt/README.md` says the Hungarian `README.md` is authoritative; keep that sentence.

## Recording a fix

1. Fix the value in `pt.json` (or the workout map, or the document).
2. If the mistake is a **word choice**, write the rule into `GLOSSARY.tsv`: the `note` column is the
   forbidden/required column (`never cavaleiro`, `use limiar, not limite`), and add the same rule to
   the `SAME`/forbidden lists of `crates/ride-i18n/tests/catalogs.rs` when it can be tested.
3. Re-run `cargo test -p ride-i18n --test catalogs`, `node scripts/i18n/check_ui.mjs target/pt-text
   --lang pt` and the two overflow runs.
4. Say in `CHANGELOG.md` that the language was reviewed by a native speaker only when that is true.
