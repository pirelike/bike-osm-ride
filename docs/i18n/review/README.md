# Machine review of the UI catalogs (2026-11-06)

One Claude Haiku agent per language read the whole UI catalog of that language (1061 keys, from
`crates/ride-i18n/locales/`) next to the Hungarian source, the English pivot, `GLOSSARY.tsv` and
`STYLE.tsv`, and **flagged** the translations it found questionable. Nothing was rewritten by the
agents. The reports are the working list for a native reviewer; **no native speaker has looked at
them yet**, and a flag is a question, not a verdict.

| Language | Report | Flags |
|---|---|---|
| English | [`en.md`](en.md) | 27 |
| Spanish | [`es.md`](es.md) | 65 |
| Portuguese (pt-PT) | [`pt.md`](pt.md) | 37 |
| German | [`de.md`](de.md) | 27 |
| Italian | [`it.md`](it.md) | 23 |
| Simplified Chinese | [`zh.md`](zh.md) | 45 |

Scope: the UI catalog only. The workout library (`crates/ride-model/assets/workouts.<code>.json`,
2065 strings per language) and the rider documents under `docs/i18n/<code>/` were not part of
this pass.

## Known false positives

- **"Only `.other` exists, add a `.one` form"** (en, es, pt, de). The agents saw a table built from
  the Hungarian keys, and Hungarian has no singular form, so they never saw the `.one` keys. The
  English, Spanish, Portuguese and German catalogs do carry every `.one` form. (Italian really
  lacked them, see below.)
- **`i18n.test.*`** (rows 259-262 in every report): test fixtures of `ride-i18n`, not shipped
  text; `i18n.test.only_hu` is empty on purpose.
- **The `tight` column was empty** in the agents' input, so the "length" flags are judgement,
  not a measured limit.

## Fixed right away (clear errors)

| Language | Key | Was | Now |
|---|---|---|---|
| it | the five counted keys (`kep_count`, `utvonal_count`, `korabbi_menet`, both offline Street View counts) | only `.other`: "1 immagini", "1 percorsi" | `.one` added; `catalogs.rs` now checks every `one-other` language |
| pt | `ui_hud.km_mulva` | "km percorridos" (km covered: the wrong direction) | "km à frente" |
| pt | `ui_review.power.az_utolso_…` | the second sentence was missing | translated in full ("Resumo de …") |
| en | `ui_review.power.het` / `honap` | "The last 12 week" | "weeks" / "months" (the only use is after the count) |
| de | `ui_review.power.het` / `honap` | "Die letzten 12 Woche" | "Wochen" / "Monate" |
| es | `ui_review.power.az_utolso_…`, `het` / `honap` | "Las últimas 12 mes" (number and gender) | "Resumen de 12 semanas / meses" |
| de | `model.workout.versenydinamika` (and `GLOSSARY.tsv`) | "Rennynamik" | "Renndynamik" |

`cargo test -p ride-i18n --test catalogs` also gates all six complete catalogs now (it listed only
`en` and `es` before).

## Open decisions (recurring in several reports)

- **Percent spacing.** `STYLE.tsv` says "55%" for en/es/it/zh and "55 %" for de; a handful of
  strings in each catalog copy the Hungarian spacing. One pass per catalog.
- **"edzés" means two things in the source:** a planned workout and, on the review page and in
  the import, a recorded ride. The translations split them correctly (workout / ride, Workout /
  Fahrt, treino / volta), but the glossary knows only the first sense.
- **Glossary drift** on recurring terms (es: tempó, regeneráció; zh: 区块 vs 段, 模拟骑行者 vs
  模拟骑手; it: resistenza vs carico for the trainer's resistance). The glossary or the catalog
  has to give way, once per term.
- **Sentence fragments.** `ui_dialogs.settings.ban_es_kapcsold_be_…` and a few About strings are
  built from separate keys; the house rule is one key per full sentence. That is a source change.

## Decided

- **"Szimulált tekerő" vs "Szimulált kerékpáros" is intentional** (the user, 2026-11-07): *tekerő*
  names the device in the device list (`ble.hub.szimulalt_tekero`), *kerékpáros* the rider riding
  through the map (the panel, `app.dock.szimulalt_kerekparos`,
  `ui_dialogs.virtual_rider.szimulalt_kerekparos`). The Hungarian stays as it is. The other
  catalogs use one word for both ("Simulated rider", "Ciclista simulado", …); that reads naturally
  in every one of them and is not a flag.
