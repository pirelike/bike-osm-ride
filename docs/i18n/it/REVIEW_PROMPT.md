# Review brief — Italian (`it`)

> **Target language:** Italian. This file is for a native speaker (or a back-translation pass)
> reviewing the translation; it is not part of the application. The translation brief is
> [`PROMPT.md`](PROMPT.md).

## Your role

You are the Italian reviewer of bike-osm-ride. The text was translated by a model from the English
pivot and the Hungarian source, and **no native speaker has read it yet**. Do not retranslate
everything: find the places where a rider would misunderstand the screen, or where the sentence is
not Italian, and fix those. Record every decision that should outlive the fix.

## Inputs

| File | Content |
|---|---|
| `crates/ride-i18n/locales/it.json` | UI labels, errors, hints (1 061 keys) |
| `crates/ride-model/assets/workouts.it.json` | 93 workouts: name, goal, description, steps, coach messages (2 065 strings) |
| `crates/ride-i18n/GLOSSARY.tsv` | the term glossary; the `it` column is the translation, `note` is the forbidden/required rule |
| `crates/ride-i18n/STYLE.tsv` | the style rules; the `it` column is the translation |
| `docs/i18n/it/README.md`, `USER_GUIDE.md`, `PRIVACY.md` | the rider documents |

Hungarian is the source language and English is the pivot. On an ambiguity the Hungarian wins, but do
not copy Hungarian case suffixes or word order into Italian.

## Must-check

1. **Placeholders** `{name}`, `{n}`, `{err}`, `{}`, `{:.0}` must match the source in spelling and
   count; the order may follow Italian word order.
2. **Leading/trailing spaces.** Several strings are glued to a link or a number; the space must stay.
   The one exception is `ui_dialogs.settings.ban_es_kapcsold_be_a_szamlazast_a_google_havi_ingyen`,
   which needs a leading space in Italian.
3. **Length.** Buttons, tabs, tile labels and chips are narrow: prefer 1–3 words. If a translation is
   much longer than the English, shorten it instead of relying on an ellipsis. The 412 × 860 run
   (`node scripts/i18n/overflow.mjs --lang it --window 412x860`) is the check.
4. **Address.** `tu` throughout, never the formal `Lei`; coach messages speak to the rider directly,
   in the imperative, short and encouraging.
5. **Rider.** **ciclista**, never `cavaliere`, `fantino` or `equitazione`. (`cavallo` is allowed: it
   is also horsepower.)
6. **Terms.** Follow the `it` column of `GLOSSARY.tsv`; the same concept must use the same word on
   every screen. When you change a term, change it in the glossary and in every catalog.
7. **Brands and units** stay: `bike-osm-ride`, `FTP`, `ERG`, `TCX`, `GPX`, `BRouter`, `bikerouter.de`,
   `Open-Meteo`, `Street View`, `Google`, `Zwift`, `MyWhoosh`, `FTMS`, `BLE`, `VO₂max`, `W`, `W/kg`,
   `km`, `m`, `%`, `kcal`, `kJ`, `bpm`, `Barlow`.
8. **Punctuation:** `…` one character, `“ ”`, `–` en dash. Sentence case in headings and buttons
   (`Salva`, `Apri`, `Elimina`).
9. **Numbers:** decimal comma, no thousands separator in the UI (`1234,5`); do not convert units.
10. **Agreement and articles.** Check every article: *l'uscita* (f), *il rullo* (m), *la potenza*,
    *la cadenza*, *la quota*, *la salita*, *la discesa*, *il percorso*, *l'allenamento*, *il blocco*,
    *il fantasma*, *il segmento*, *l'impostazione* (pl. *impostazioni*), *il dispositivo*, *la marcia*,
    *il cambio*. `un'uscita` and `un'ora` need the apostrophe; `il uscita` and `la rullo` are the
    commonest machine-translation smells here.

## Risk list (look here first)

### A. Narrow labels

These live in tiles, tabs and buttons and are the first to overflow:

`Avvia` `Pausa` `Termina` `Riprendi` `Salva` `Annulla` `Impostazioni` `Dispositivi` `Percorso`
`Allenamento` `Analisi` `Riepilogo` `Marcia` `Soglia` `Riscaldamento` `Defaticamento` `Recupero`
`Intervallo` `Sweet spot` `Sprint` `Salita` `Pianura` `Libero` `Connessione…` `Ricerca…`
`Frequenza cardiaca` `Frequenza cardiaca max` `Potenza di picco`

The heart-rate labels are the longest in the HUD (`Frequenza cardiaca di picco`, 27 characters). If a
native review prefers `FC` on the narrowest tile, change the tile keys only, not the prose.

### B. False friends and near-misses

| Key / place | Risk | Current choice |
|---|---|---|
| `szint` (HUD, welcome) | the source means elevation, but the HUD shows the accumulated climbing | quota / dislivello — check the context |
| `magasság` (profile) | the real altitude | altitudine |
| `teljes` (profile mode) | the whole profile, not "full" | completo |
| `menet` / `túra` | a recorded ride, not a "turn" | uscita |
| `blokk` (workout) | a workout block, not a text block | blocco |
| `pörgetés` | high-cadence easy pedalling | frullata |
| `fordulat` | cadence in rpm | cadenza (rpm) |
| `leadt` / `megtett` | power delivered vs distance done | erogata / percorsa |
| `előretekintés` | look-ahead mode | anteprima |
| `összeállás` | a combined workout, not "assembly" | combinato |
| `küszöb` | threshold, never “limite” | soglia |
| `terhelés` | trainer resistance and TSS load share the word | carico |
| `áttekintés` | the page that compares rides, not an audit | Analisi |

Watch for Italian words that look English but are not: *eventualmente* means "possibly", not
"eventually"; *attualmente* means "currently", not "actually"; *realizzare* is "to build", not "to
realise"; *assistere* is "to attend", not "to assist". Translate the English meaning, not the English
shape.

### C. Gender and agreement

A wrong article or participle is the most visible mistake. *L'uscita è stata eliminata*, *il rullo è
collegato*, *la potenza è stata ricalcolata*, *i watt sono stimati*. Watch the pronouns: *la bici*,
*il cambio*, *la chiave*, *lo schermo*, *l'allenamento*.

### D. Long sentences and error messages

* Backup/restore, GPX/TCX import, Street View and BLE errors: calm and actionable, not a literal copy
  of the Hungarian clauses. Keep `{err}` / `{why}` / `{error}` exactly.
* `ui_hud.edzes_w_hatra_kesz` carries a `{current}` placeholder the source does not always fill; keep
  it, it is the source's bug, not the translation's.
* `ui_review.power.duration_seconds` / `duration_minutes` are built in code; check `5 s`, `20 min`,
  `1:12 h` after any change.

### E. Workout library

* Coach messages: one or two sentences, present tense, second person, concrete. Do not start every
  message with the same cheer.
* Step names: short. `Pörgetés` = frullata, `Nyomaték` = coppia, `Fal` = muro, `Lead-out` = lead-out,
  `Álló rajt` = partenza da fermo.
* A workout name keeps its duration, zone and interval notation; `FTP`, `VO2max`, `Over/Under`,
  `30/30`, `Sweet spot` stay as they are.
* The generator (`node scripts/i18n/workouts.mjs it`) refuses a missing string; a translated library
  must not change a duration, a target or a step count.

### F. Documents

* `docs/i18n/it/README.md` must contain `## Licenza`; licence names and SPDX identifiers
  (`MIT`, `Apache-2.0`, `MIT OR Apache-2.0`, `ODbL 1.0`, `CC BY 4.0`, `SIL OFL 1.1`, `MPL-2.0`,
  `Unlicense`) must not change, and the attributions may not gain or lose a row.
* Keep every link, image path, command and file name; `<version>` stays in the download commands.
* `docs/i18n/it/README.md` says the Hungarian `README.md` is authoritative; keep that sentence.

## What was already checked (2026-11-06)

A blind back-translation pass took 99 strings from this risk list, translated them Italian → English
without the source in view, and compared the result with the English pivot. Placeholders, leading and
trailing spaces and units matched everywhere; one label was changed afterwards (`ui_review.
kinek_az_adataival_szamoljunk` → *I dati di chi usare*). The pass confirmed two things a reviewer
should not "fix":

* `ui_hud.km_mulva` (*km più avanti*) is the distance **to** the next climb, not the distance covered;
  the same tile switches to `km rimanenti` inside the climb (`ui_hud.km_hatra`).
* `ui_hud.szint` is *quota* (elevation) while `ui_profile.magassag` is *altitudine* (the altitude on
  the profile); the difference is intentional and mirrors the Hungarian keys.

The rendered screens were scanned for Hungarian leftovers (`check_ui.mjs`: 0 untranslated lines, only
the GPX route name as data) and for labels that do not fit at 412 × 860 and 1440 × 900. One element
overflowed only in Italian — the heart-rate header of the data table — and the short tile label became
`FC`; every other clipped element already clips in Hungarian.

## Recording a fix

1. Fix the value in `it.json` (or the workout map, or the document).
2. If the mistake is a **word choice**, write the rule into `GLOSSARY.tsv`: the `note` column is the
   forbidden/required column (`never cavaliere`, `use soglia, not limite`), and add the same rule to
   the `SAME`/forbidden lists of `crates/ride-i18n/tests/catalogs.rs` when it can be tested.
3. Re-run `cargo test -p ride-i18n --test catalogs`, `node scripts/i18n/check_ui.mjs target/it-text
   --lang it` and the two overflow runs.
4. Say in `CHANGELOG.md` that the language was reviewed by a native speaker only when that is true.
