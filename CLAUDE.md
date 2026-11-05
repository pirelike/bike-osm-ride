# CLAUDE.md — bike-osm-ride

Virtual rides on GPX routes with a Bluetooth LE smart trainer (or a simulated rider). 2D only: map,
elevation profile, every number the trainer broadcasts. **Rust**, Dioxus 0.7 (webview UI), btleplug.
Runs on Windows, Linux, macOS and Android from one code base. Public repo
`github.com/pirelike/bike-osm-ride`, version in `[workspace.package]` of `Cargo.toml`.

## Language

- **Talk to the user in Hungarian.** All user-visible text in the app is Hungarian (UI, toasts, error
  messages that can reach a toast, workout library, coach messages).
- Code, identifiers, comments, commit messages and developer docs are **English**.
- Hungarian documents: `README.md`, `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/PRIVACY.md`.
  Everything else under `docs/` is English.

## Commands

```powershell
cargo run -p ride-app                    # debug build + start (Windows: the exe needs WebView2Loader.dll beside it, build.rs copies it)
cargo test --workspace                   # 954 tests, 3 ignored (network / Bluetooth); all green on Windows GNU, 2026-11-04
cargo clippy --workspace --all-targets   # clean on 2026-11-04; keep it that way
cargo fmt --all -- --check
node scripts/readme_shots.mjs            # regenerate docs/screenshots (needs a built debug exe, Node 22+)
.\scripts\third_party_licenses.ps1       # regenerate THIRD_PARTY_LICENSES.md after a dependency change
```

- **Tests in a sandboxed shell:** `tempfile` fails with `PermissionDenied` (50 tests across four
  crates) when `%TEMP%` is not writable. Point `TMP` and `TEMP` at a folder inside the repo first, e.g.
  `target\tmp-tests` (worked; a folder in the session scratchpad did not). That is an environment
  limit, not a bug.
- A running app locks `target\debug\bike-osm-ride.exe`: do not build while one is open, or build with
  another `CARGO_TARGET_DIR`.
- Do not `cargo clean`; the cold build of the Dioxus stack is slow and `target/` also holds `dx` output.
- Android cannot be built in this environment (no NDK clang for the sandboxed shell). Verify the
  phone layout with `RIDE_WINDOW=412x860` and the CDP scripts instead. See `docs/ANDROID.md`.
- Details: `docs/DEVELOPMENT.md`, `docs/TESTING.md`.

## Machine facts (the user's PC)

- Windows 11, Rust on **`stable-x86_64-pc-windows-gnu`** (MinGW/WinLibs). **No MSVC — never suggest
  installing it.** The *release* exe is built by CI with MSVC + `+crt-static` (no DLL needed); the local
  GNU exe is not shippable alone (it imports `WebView2Loader.dll`; exit `0xC0000135` without it).
- A BLE adapter exists. The user's trainer is a generic FTMS "VE200" with firmware quirks (repeated "Reset"
  statuses, long Indoor Bike Data frames, no Stop/Pause); the 0.1.1 commit says it also left Request Control
  unanswered at times, which the later capture does not show (see `docs/BLUETOOTH.md`). Other trainers, a Strava
  login and a Google key were never available here.
- `gh` is at `C:\Program Files\GitHub CLI\gh.exe` (not on PATH), logged in as `pirelike`.
- Python's `open(p, 'w')` writes CRLF here: use `newline=''` or write bytes when patching files.
  `perl -pi` with Hungarian text double-encodes: use `-CSD` and `use utf8`. Prefer the Write/Edit tools.
- Files stored with CRLF in git (keep their endings when editing): `THIRD_PARTY_LICENSES.md`,
  `ride-route/Cargo.toml`, `ride-route/src/{elevation,http}.rs`, `ride-ui-map/assets/map.css`
  (mixed), `ride-ui-map/src/scale.rs`, `tests/fixtures/bom_namespaced.gpx`.
  Never run a blanket CR-strip: it once destroyed the PNG signature of `packaging/icons/*.png`
  (`.gitattributes` now marks images and fonts binary; regenerate icons with
  `cargo run -p ride-app --example export_icon`).

## Repository map

```
crates/ride-model       shared data model = the contract (Route, Settings, RideSnapshot, Workout, ride log types). Depends on nothing of ours.
crates/ride-route       GPX / bikerouter link -> Route; climbs; persistence (store, ride log). Pure + async loaders.
crates/ride-sim         physics, RideEngine, stats, recorder, workouts runner/import, ghost, segments, TCX. No I/O, no clock.
crates/ride-ble         sensor hub: transport traits, FTMS/CPS/CSC/HR/Zwift, simulated rider. btleplug types live in btle.rs only.
crates/ride-ui-kit      design system: CSS tokens, palette, icons, formatting, base components, protocol URLs.
crates/ride-ui-map      slippy map (pure geometry + Dioxus component)       crates/ride-ui-profile  elevation profile, live charts, workout view
crates/ride-ui-hud      metric tiles, stats, climb/workout cards, controls  crates/ride-ui-dialogs   route, devices, settings, workouts, summary, about
crates/ride-ui-streetview  optional Street View panel (user's own Google key)
crates/ride-app         the shell: state, controller (ride loop), tiling dock, tile proxy, updater, Strava, Android glue
packaging/ scripts/ docs/ tests/fixtures/ .github/workflows/release.yml
```

Dependency rule: everything depends on `ride-model`; `ride-ui-map/profile/hud` add only `ride-ui-kit`;
`ride-sim`, `ride-ble` and `ride-ui-kit` depend only on `ride-model`; `ride-route` also uses `ride-sim`
(segment times must be computed exactly like the live engine); `ride-ui-streetview` also uses `ride-route`
and `ride-ui-dialogs` uses `ride-ui-streetview`; `ride-app` depends on all. Keep it acyclic.
Full table: `docs/ARCHITECTURE.md`.

Ride loop (`ride-app/src/controller.rs` + `ride_core.rs`): every 100 ms real elapsed time (capped 1 s,
times `RIDE_TIMESCALE`) goes into `RideEngine::step` with the latest merged `SensorSample`; the snapshot
(10 Hz) and history (1 Hz) go to Dioxus signals; trainer commands go to the hub through a channel
(grade simulation, or the ERG target while a workout step has one).

## Conventions

- **No `unwrap()`, `expect()`, `panic!` or panicking indexing on external data** (files, network, BLE
  bytes, settings). Return errors with Hungarian messages where a user can see them.
- **A panic must never leave an event handler.** On Android `dioxus-desktop` runs every event under one
  global mutex: one escaped panic poisons it and every later tap panics too. Use `guard::guarded` and
  `Controller::with`; wrap `document::eval`.
- **`ride-model` is the contract: change it additively** (new field with `#[serde(default)]`, new fn).
  Old settings files, route caches and ride records must keep loading; there are tests for it. A ride
  record of an unknown `version` is ignored, never an error.
- Units everywhere: metres, seconds, m/s, watts, grades in percent (`5.0` = 5 %), degrees. Display
  conversion lives in the UI layer only (`ride_ui_kit::format`). A missing sensor value shows "—".
- Logic crates are pure and unit-tested (`ride-sim` takes `dt` and the sensor sample, never reads a clock).
  BLE code is written against `transport.rs` traits and tested with the in-memory fake (`fake.rs`).
- UI crates take `ReadSignal` props and own no business state; heavy derived data uses `use_memo`.
- Tests assert behaviour, including house rules: the CSS lints fail on `blur(` / `backdrop-filter`,
  blurred shadows, hard-coded colours and undefined tokens (`docs/DESIGN.md`), and a test fails when a
  dialog footer gets a second close button. Tests must not hard-code `tiles.localhost` (that base is
  Windows-only): use `ride_ui_kit::protocol::base`.
- Hooks and platform differences are `cfg`-gated in one code base; there is no fork per platform.
- Commit messages: `<version>: <summary>` for releases, a body that explains *why* (see `git log`).

## Licensing (hard rules)

- Project: `MIT OR Apache-2.0`. **Only permissive third-party code** (MIT, Apache-2.0, BSD, ISC, Zlib,
  CC0, Unlicense) may be copied, ported or vendored. **Never GPL/AGPL/LGPL/unlicensed** (OpenCycling,
  suffice, GoldenCheetah, QDomyos-Zwift, StravaUploaderPy are idea-only). See `docs/PRIOR_ART.md`.
- Vendored/ported file = header comment (origin URL + commit, licence, changes) + a `NOTICE.md` section +
  the upstream licence text under `crates/<crate>/third_party/`.
- **`README.md` must always contain the `## Licenc` section** (the user insisted): project licence, reused
  code with licences, map/data attributions, the licence audit. `ride-ui-dialogs/src/about.rs` mirrors
  that section one to one: change both together.
- Dependencies: prefer permissive crates; rerun `scripts/third_party_licenses.ps1`; re-audit with
  `cargo metadata --filter-platform <triple>` (commands in `docs/DEVELOPMENT.md`). The only copyleft
  crates are MPL-2.0 (file level, unmodified): `option-ext` on Linux/macOS, the `cssparser`/`selectors`
  chain on Android, none on Windows.

## Product rules the user decided

- No 3D. Rust, not a web stack. UI language Hungarian. "Super modern" UI: the *head unit / printed race
  sheet* design, one accent colour, dark and light themes both first-class (`docs/DESIGN.md`).
- Tiling layout "like a window manager, sizes adjustable"; separate layout with and without a workout.
- **Strava: no API, ever** (it needs a paid subscription; a shared secret cannot ship in open source).
  Desktop uploads through a hidden WebView on strava.com, Android through the share intent. Never
  validated against the live site: if it breaks read `<data>/strava/strava-debug.log` (`docs/STRAVA.md`).
- Street View: the user's **own** Google key; imagery is only stored on an explicit download (30 days),
  panorama ids may be cached. The key is never logged, never in `Debug` output, only sent to Google.
- The virtual gearbox shows only when switched on **and** no workout (ERG) runs.
- The app must start with no Bluetooth adapter and no network. OSM tile policy: no bulk prefetching.
- Android `target_sdk` stays 34 until the edge-to-edge layout of 35+ has been checked on a phone.

## Releasing

Bump the version in `[workspace.package]`, commit, tag `vX.Y.Z`, run the `release` workflow by hand on
that tag. The updater compares the tag with the compiled version and looks for exact asset names; renaming
an asset means changing `updater.rs`. A pushed tag is never moved or deleted: a fix means a new patch
version. Procedure and CI lessons: `docs/RELEASING.md`.

## Verification state (be honest about it)

Verified by automated tests and by running the app: route pipeline, physics, engine, workouts, ghost/PRs,
TCX, the tiling dock, the UI crates, the FTMS control logic against the in-memory fake transport.
**One real trainer** (the user's VE200) was captured (`ble.log.1`, 2026-11-01/02, about 3.3 h): ERG targets
were accepted 928 times, Request Control and Start work, and the trainer answers Stop/Pause with "op code
not supported". The capture contains **no terrain simulation**: its only six Set Indoor Bike Simulation
writes are neutral (0 %) grades sent right before a pause. That the trainer follows hills is therefore
**not confirmed on hardware**. **Never verified:** real Strava upload, real Google Street View (only a
local mock), Android Bluetooth and the Strava share on a device, the macOS and Linux apps at runtime (CI
builds them and smoke-tests Linux for 20 s), any trainer other than the VE200. Do not claim otherwise.
Matrix: `docs/TESTING.md`.

## Docs: what to update when you change something

| You changed | Update |
|---|---|
| a user-visible feature, shortcut or setting | `README.md` (short), `docs/USER_GUIDE.md`, `CHANGELOG.md` |
| a screen | rerun `node scripts/readme_shots.mjs` (sparingly: every PNG stays in git history) |
| the BLE layer, a firmware quirk | `docs/BLUETOOTH.md` |
| physics, engine, workouts, ghost | `docs/SIMULATION.md` |
| a file format or the data folder | `docs/ROUTES_AND_DATA.md`, `docs/PRIVACY.md` if data leaves the machine |
| layout, panels, CSS rules | `docs/UI.md`, `docs/DESIGN.md` |
| packaging, CI, assets | `docs/RELEASING.md`, `docs/ANDROID.md`, the header of `release.yml` |
| a decision with a reason | `docs/DECISIONS.md` |
| a dependency or vendored code | `NOTICE.md`, `THIRD_PARTY_LICENSES.md`, README `Licenc` section, About dialog |

Index of all docs: `docs/README.md`.
