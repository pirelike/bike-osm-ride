# Development guide

How to build, run, debug and change bike-osm-ride. Everything here was checked against the repository
on 2026-11-10 (source references for version 0.5.0). For the big picture read [`ARCHITECTURE.md`](ARCHITECTURE.md) first;
for how to prove a change works read [`TESTING.md`](TESTING.md).

## 1. Prerequisites

| Platform | You need |
|---|---|
| Windows 10/11 | Rust stable (the project is developed on `stable-x86_64-pc-windows-gnu`, MinGW/WinLibs; MSVC works too and is what CI uses for the release exe). The [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 11). |
| Linux | Rust stable and `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libdbus-1-dev pkg-config` (the CI list). At run time BlueZ for Bluetooth. |
| macOS | Rust stable and the Xcode command line tools. |
| Android | Android SDK + NDK 27, JDK 17+, the Dioxus CLI 0.7.10: see [`ANDROID.md`](ANDROID.md). |
| Optional | Node 22+ (the `scripts/*.mjs` DevTools tools), Python 3 with Pillow (icons, workout library generator). |

On the GNU toolchain the exe imports `WebView2Loader.dll`; without it next to the exe the process
dies at once with `0xC0000135`. `crates/ride-app/build.rs` copies the DLL into `target/<profile>/`,
`deps/` and `examples/`, and only warns (never fails) when it cannot, for example because a running
app has the file locked. The MSVC release build links the loader statically and needs no DLL.

## 2. Build and run

```powershell
cargo run -p ride-app                       # debug build and start
.\scripts\run.ps1 -Demo -Sim 250 -Window 1600x900 -Theme light     # the same with hooks (see section 3)
cargo build --release -p ride-app           # release profile: opt-level 3, thin LTO, stripped
```

Linux and macOS: the same `cargo run -p ride-app`; the `scripts\*.ps1` helpers need PowerShell.

The dev profile optimises *dependencies* (`opt-level = 2`) and leaves our own crates at 0, so the UI
diffing and the 10 Hz ride loop stay smooth while our crates compile fast.

Things that bite:

- **A running app locks `target\debug\bike-osm-ride.exe`.** Close it before building, or build with
  another `CARGO_TARGET_DIR`.
- Do not `cargo clean` casually: a cold build of the Dioxus/WebView stack is slow and `target/` also holds
  `dx` (Android/Linux packaging) output and the screenshot data.
- If the PowerShell execution policy blocks the scripts:
  `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1`.

## 3. Environment hooks

Read once at start-up (`crates/ride-app/src/hooks.rs`); none is needed for normal use. They exist so a
scenario can be reproduced without clicking, and so screenshots can be scripted.

| Variable | Effect |
|---|---|
| `RIDE_DEMO=1` | load the built-in demo route (a synthetic 24 km route with one 4 km climb; its geometry is *near*, not on, real roads) |
| `RIDE_GPX=<path>` | load this GPX at start-up |
| `RIDE_SIM=<watts>` | connect the simulated rider at that power **and start the ride** (loads the demo route unless `RIDE_GPX` is set); 0–2000 |
| `RIDE_TIMESCALE=<n>` | run the ride clock *n* times faster, 1–200. Above about 20 the 3 s power average is empty (one 100 ms tick is longer than 3 s of ride time) |
| `RIDE_WORKOUT=<id>` | select a workout (`c-threshold-2x20`; unknown ids are ignored; not persisted) |
| `RIDE_THEME=light\|dark` | theme override, not written to `settings.json` unless the rider changes it |
| `RIDE_WINDOW=1600x900` | initial window size (never below 360 × 560) |
| `RIDE_DIALOG=route\|devices\|settings\|workouts\|about` | open a dialog at start-up |
| `RIDE_REVIEW=1` | open the review page at start-up (`RIDE_DATA_DIR` decides which rides it finds) |
| `RIDE_DATA_DIR=<dir>` | use another data folder (settings, rides, tiles, ...): tests, portable installs, screenshots; Android sets it to the app's private folder |
| `RIDE_TILE_BASE=<url>` | fetch tiles from `<url>/<style>/<z>/<x>/<y>.png` (`http://127.0.0.1:9` simulates "no network") |
| `RIDE_TILE_PROBE=1` | load one map tile through the `tiles` protocol at start-up and print the result |
| `RIDE_STREETVIEW_BASE=http://127.0.0.1:<port>` | send every Street View request to a local mock (loopback only; anything else is ignored): `node scripts/streetview_mock.mjs 9300` |
| `RIDE_DEBUG_PORT=<port>` | Windows only: open the WebView2 DevTools protocol on that port (>= 1024) |

`WEBKIT_DISABLE_DMABUF_RENDERER=1` is set by the app itself on Linux when it is unset (an empty window
on some NVIDIA/Wayland setups); an explicit value wins.

## 4. Looking into a running app

- **DevTools protocol (Windows).** Start with `RIDE_DEBUG_PORT=9333`, then
  `node scripts/cdp_eval.mjs 9333 "<javascript>"` evaluates in the page (prefix the expression with
  `b64:` to pass it base64-encoded and avoid quoting trouble), and
  `node scripts/cdp_probe.mjs out.png 9333 [--dom]` saves a reliable screenshot, prints console problems and
  optionally the layout tree. `CopyFromScreen` returns a white window for a WebView2 window; the page
  screenshot does not. `.\scripts\shot.ps1` wraps the start, the wait and the probe.
- **BLE traffic.** `<data>/ble.log`, one line per packet: `timestamp  NOTIFY|INDICATE|READ|WRITE  <characteristic>  <hex>`,
  plus `#` comment lines for what the session decided. See [`BLUETOOTH.md`](BLUETOOTH.md).
- **Crashes.** Every panic goes to `<data>/crash.log` (rotated at 256 KB), to stderr, to logcat on Android, and the
  shell shows the first line in a banner. The app survives a panic in an event handler by design.
- Debug builds keep the console; release builds on Windows are GUI-subsystem executables (no console).

## 5. Scripts and packaging files

| Path | What it is |
|---|---|
| `scripts/run.ps1` | build + start with hook switches |
| `scripts/screenshot.ps1`, `scripts/shot.ps1` | window screenshot (screen copy) / page screenshot through DevTools |
| `scripts/cdp_eval.mjs`, `scripts/cdp_probe.mjs` | DevTools helpers (Node 22+, built-in WebSocket) |
| `scripts/readme_shots.mjs` | regenerates `docs/screenshots/*.png` (section 6) |
| `scripts/streetview_mock.mjs` | local imitation of the three Google endpoints; `--delay`, `--gap`, special keys `BADKEY`, `QUOTAKEY`, `HTTP500KEY`, `GARBAGEKEY`; `/__stats` counts requests |
| `scripts/gen_workouts.py` | writes `crates/ride-model/assets/workouts.json` (`python -I scripts/gen_workouts.py crates/ride-model/assets/workouts.json`); merges `scripts/coach_messages.json`; its checks mirror the Rust tests |
| `scripts/third_party_licenses.ps1` | regenerates `THIRD_PARTY_LICENSES.md` from `cargo metadata` |
| `packaging/icons/` | window/app icons 32..1024 px, rendered by `cargo run -p ride-app --example export_icon` from `src/icon.rs` |
| `packaging/linux/`, `packaging/macos/`, `packaging/android/` | see [`RELEASING.md`](RELEASING.md) and [`ANDROID.md`](ANDROID.md) |
| `tests/fixtures/` | GPX fixtures with their origin and licence (`tests/fixtures/README.md`) |
| Example binaries | `cargo run -p ride-ui-kit --example kit_preview`, `ride-ui-hud --example hud_preview`, ... render crate UI to HTML (`target/preview/`) |

## 6. Screenshots for the README

```powershell
cargo build -p ride-app
node scripts/readme_shots.mjs                    # all scenarios, about 8 minutes
node scripts/readme_shots.mjs --only ride-dark   # one; --list shows them all
```

For every scenario the script starts the app with its own hooks and a throw-away data folder
(`target/readme-shots-data`, which keeps only its tile cache between runs), pins the page to 1440 × 900 at
1× (phone: 412 × 860 at 2×) with the DevTools `Emulation` domain, takes a PNG and kills the process tree.
No setting, route, ride, Google key of the person running it can end up in a picture.
The route is the last 12.4 km of the real BRouter fixture (Hűvösvölgy–Dobogókő, ODbL): the synthetic demo
route runs across fields and water on the map. The first run fetches a few dozen OSM tiles. Regenerate
rarely: every PNG stays in git history.

The `review` and `review-phone` scenarios seed the throw-away folder first with
`cargo run -p ride-route --example seed_review -- <folder> [route.gpx]`: two rides made by the ride engine at different
power, and a third that is the same kind of ride written as a Strava-style GPX *without watts* (2 s samples, 5 m off the line)
and imported, so the estimate has a known answer to be compared with. Useful for trying the page by hand too
(`RIDE_DATA_DIR=<folder> RIDE_REVIEW=1`).

## 7. Conventions

- **Language.** UI text, error messages and the workout library are Hungarian; code, comments, commit
  messages and developer docs are English; `README.md`, `CHANGELOG.md`, `docs/USER_GUIDE.md` and
  `docs/PRIVACY.md` are Hungarian.
- **No panics on external data.** Files, network answers, BLE bytes and settings never reach `unwrap()`,
  `expect()`, `panic!` or an index that can be out of range. Errors a rider can see are Hungarian.
- **Panic containment.** Calls from the shell into other crates go through `guard::guarded`;
  controller methods run in `Controller::with`. Never let a panic leave an event handler: on Android
  one escaped panic poisons dioxus-desktop's global mutex and every later tap panics too
  ([`ANDROID.md`](ANDROID.md)).
- **`ride-model` is a contract.** Add, never rename, remove or retype. A new field in a persisted type
  needs `#[serde(default)]` and a test that an old file still loads. A ride record of an unknown
  `version` is ignored.
- **Units** are SI plus percent for grades; display conversion is the UI's job (`ride_ui_kit::format`).
- **Pure logic, injected time.** `ride-sim` never reads a clock; BLE code talks to traits so the in-memory
  fake can drive it; UI crates take `ReadSignal` props.
- **CSS.** Tokens only, class names prefixed per crate (`map-`, `profile-`, `hud-`, `dlg-`, `shell-`, `dock-`,
  `sv`), validated by tests; see [`DESIGN.md`](DESIGN.md) and [`UI.md`](UI.md).
- **Commits.** `<version>: <summary>` for releases, otherwise a short imperative title; the body explains
  *why* (`git log` has good examples). A fix after a pushed tag is a new patch version, never a moved tag.

## 8. Recipes

**Add a setting.** Field with `#[serde(default)]` in `UiSettings`/`RiderSettings`
(`ride-model/src/lib.rs`), a row in `ride-ui-dialogs/src/settings.rs` (helpers in `helpers.rs` are pure
and tested), mirror it in the `Debug` impl if it is secret or noisy, add the old-file-still-loads test.

**Add a panel.** `PanelId` variant with `key`, `title`, `min_size`, `fills`, `has` in `ride-app/src/dock.rs`;
render it in `shell.rs` (desktop) and decide its place in the phone tabs; add it to the default tree and to
the dock tests ("the default layout tiles the screen"); document it in `USER_GUIDE.md`.

**Add a dialog.** `DialogKind` in `state.rs`, a component in `ride-ui-dialogs` using the kit's single dialog
frame (`foot` and `flush` props), the `RIDE_DIALOG` name in `hooks.rs`, a pure helper module with tests.

**Add a workout.** Edit `scripts/gen_workouts.py` (tags come from the fixed vocabulary of
`ride_model::WorkoutTag`; an unknown tag invalidates the entry), run it, keep the sources it cites, add
coach messages to `scripts/coach_messages.json`.

**Reuse third-party code.** Permissive licences only. Add the header comment (origin URL and commit,
licence, what changed), a `NOTICE.md` section and the upstream licence text under
`crates/<crate>/third_party/<name>/`; update the README `Licenc` table and `about.rs`.

## 9. Licence audit

```powershell
.\scripts\third_party_licenses.ps1               # THIRD_PARTY_LICENSES.md, all platforms' lock file
cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-gnu > meta.json
cargo tree --offline --locked -p ride-app --target aarch64-linux-android -i cssparser --prefix none
```

Reduce `meta.json` to the packages reachable from the workspace through normal and build dependencies and
look at their `license` expressions. Result on 2026-11-04 (`--filter-platform`): Windows (GNU) 396 external
crates, Linux 458, macOS (aarch64) 400, Android (aarch64) 449; no mandatory GPL/AGPL/LGPL terms (`r-efi` offers MIT/Apache-2.0 alternatives). The only
copyleft crates are MPL-2.0, used unmodified: `option-ext` (through `tray-icon` → `dirs`) on Linux and
macOS, `selectors`, `cssparser`, `cssparser-macros`, `dtoa-short` (through `wry` → `kuchikiki`) on
Android, none on Windows. `THIRD_PARTY_LICENSES.md` (all platforms together) contains the current package list; do not duplicate its changing count here.

## 10. Keeping the docs true

Docs rot when they describe how something was meant to work. When you change behaviour, change the doc in
the same commit (the table in `CLAUDE.md` says which). Prefer pointing at the file that is the source of
truth over copying numbers into prose, and put a date on anything measured.
