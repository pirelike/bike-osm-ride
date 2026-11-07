# Documentation index

Everything about bike-osm-ride that is not in the code. Written and checked against version 0.3.0 on 2026-11-04.
Hungarian, English, Spanish, Portuguese, German, Italian and Chinese documents are for riders; the English
developer docs are for people working on the code. Start with the [README](../README.md) for what the app is and how
to get it.

## For riders (Hungarian and its translations)

| Document | Content |
|---|---|
| [`USER_GUIDE.md`](USER_GUIDE.md) | the user manual: routes, devices, riding, layout, workouts, ghost, summary and TCX, Street View, phone, settings, keys, troubleshooting |
| [`PRIVACY.md`](PRIVACY.md) | every network call, what is stored locally and where, how secrets are handled |
| [`../CHANGELOG.md`](../CHANGELOG.md) | what changed in every release |
| [`i18n/review/README.md`](i18n/review/README.md) | the machine review of the six UI catalogs (one flag list per language, what was fixed, what waits for a native reviewer) |
| [`i18n/en/README.md`](i18n/en/README.md) | the English README — the app speaks English too, this is the rider-facing page for it |
| [`i18n/en/USER_GUIDE.md`](i18n/en/USER_GUIDE.md), [`i18n/en/PRIVACY.md`](i18n/en/PRIVACY.md) | the English user manual and privacy page (translations of the two above) |
| [`i18n/es/README.md`](i18n/es/README.md) | the Spanish README |
| [`i18n/es/USER_GUIDE.md`](i18n/es/USER_GUIDE.md), [`i18n/es/PRIVACY.md`](i18n/es/PRIVACY.md) | the Spanish user manual and privacy page (translations of the two above) |
| [`i18n/pt/README.md`](i18n/pt/README.md) | the Portuguese README |
| [`i18n/pt/USER_GUIDE.md`](i18n/pt/USER_GUIDE.md), [`i18n/pt/PRIVACY.md`](i18n/pt/PRIVACY.md) | the Portuguese user manual and privacy page (translations of the two above) |
| [`i18n/it/README.md`](i18n/it/README.md) | the Italian README |
| [`i18n/it/USER_GUIDE.md`](i18n/it/USER_GUIDE.md), [`i18n/it/PRIVACY.md`](i18n/it/PRIVACY.md) | the Italian user manual and privacy page (translations of the two above) |

## For developers (English)

| Document | Read it when you want to know |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | how the pieces fit: crates, runtime model, ride loop, platforms, failure containment |
| [`DEVELOPMENT.md`](DEVELOPMENT.md) | how to build, run, debug, add a setting/panel/dialog, reuse third-party code, audit licences, regenerate screenshots |
| [`TESTING.md`](TESTING.md) | what the 1076 tests cover, the sandbox `TEMP` caveat, and what has *not* been verified on hardware |
| [`RELEASING.md`](RELEASING.md) | the checklist, the CI jobs, the asset names the updater depends on, the failures so far |
| [`DECISIONS.md`](DECISIONS.md) | why things are the way they are, and what was deliberately not done |
| [`ROADMAP.md`](ROADMAP.md) | features that were asked for and are not built yet (the data overlay over full-screen video), with the risks to check first |

### By topic

| Document | Covers |
|---|---|
| [`BLUETOOTH.md`](BLUETOOTH.md) | `ride-ble`: protocols, the hub, multi-role connections, the FTMS safety filter and session, firmware quirks, Zwift, the hex log, what a real trainer showed |
| [`SIMULATION.md`](SIMULATION.md) | `ride-sim`: the physics, the ride state machine, the trainer grade, workouts and ERG, the gearbox, ghost and segments, TCX |
| [`ROUTES_AND_DATA.md`](ROUTES_AND_DATA.md) | `ride-route`: GPX and link import, the pipeline, climbs, the data folder, file formats and compatibility rules, the settings reference |
| [`UI.md`](UI.md) | the shell: state and signals, the tiling dock, the phone layout, dialogs, map and profile specifics |
| [`I18N.md`](I18N.md) | the translation pipeline: catalogs, keys, `t/tf/tn`, the language registry, the workout library map, the quality gates |
| [`DESIGN.md`](DESIGN.md) | the visual language: tokens, colours and their validation, type, primitives |
| [`ANDROID.md`](ANDROID.md) | the pinned manifest and activity, JNI, panic containment, building and signing |
| [`STREETVIEW.md`](STREETVIEW.md) | the Street View panel, Google's rules, the offline download, the mock |
| [`PRIOR_ART.md`](PRIOR_ART.md) | which open-source projects were used, ported or only read, and their licences |

## Also in the repository

| File | |
|---|---|
| [`../CLAUDE.md`](../CLAUDE.md) | the project guide for Claude Code sessions: commands, conventions, rules, machine facts |
| [`../NOTICE.md`](../NOTICE.md), [`../THIRD_PARTY_LICENSES.md`](../THIRD_PARTY_LICENSES.md) | vendored and ported code; every dependency with its licence |
| [`../tests/fixtures/README.md`](../tests/fixtures/README.md) | the GPX fixtures, their origin and licence |
| `screenshots/` | the pictures used in the README and the user guide (`node scripts/readme_shots.mjs`) |

## Conventions for these documents

Every number that was measured carries a date. Prefer linking to the source of truth over copying it. A document
that describes behaviour is updated in the same commit that changes the behaviour; the table in
[`CLAUDE.md`](../CLAUDE.md) lists which document follows which change. When a statement could not be verified, the
text says so instead of guessing.
