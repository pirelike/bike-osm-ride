# bike-osm-ride — architecture & workstream contract

Virtual rides on GPX routes (bikerouter.de exports or any GPX) with a BLE smart trainer
or a simulated rider. **2D only** (map + elevation profile + all trainer data). Rust,
Dioxus 0.7 desktop (WebView2), btleplug. **All user-facing text is Hungarian.**
UI quality bar: "super modern" — see *UI brief*.

Read first: `docs/PRIOR_ART.md` (what we reuse, licences), `crates/ride-model/src/lib.rs`
(the shared model — the contract), the `lib.rs` stub of the crate you own.

## Toolchain facts (verified on this machine)

- Windows 11, Rust 1.99 **`stable-x86_64-pc-windows-gnu`** (MinGW via WinLibs). **No MSVC.** Do not install anything.
- Dioxus 0.7.10 `desktop` builds and runs on GNU; btleplug 0.13.5 finds the BT adapter.
- The GNU exe imports `WebView2Loader.dll` dynamically → it must sit next to the exe or the process dies with `0xC0000135`. `ride-app/build.rs` copies it (from `target/<profile>/build/webview2-com-sys-*/out/x64/`).
- One shared `target/` for the workspace. Builds block on the cargo lock — be patient, never `cargo clean`, never delete `target/`.
- Run only commands scoped to your crate: `cargo check -p <crate>`, `cargo test -p <crate>`, `cargo clippy -p <crate>` (if clippy is installed), `cargo fmt -p <crate>`. Never reformat other crates.
- The pre-warmed dev profile optimises dependencies (`opt-level = 2`), our crates are `opt-level = 0`.
- Shell: PowerShell/Bash both available. Write files with the Write/Edit tools (shell heredocs are flaky here).

## Crates and ownership

```
ride-model  (architect — read-only for you; ask via your report if a type must change)
ride-route  GPX/bikerouter → Route, climbs, persistence            [workstream ROUTE]
ride-sim    physics, RideEngine, stats, recorder, TCX, workouts    [workstream SIM]
ride-ble    BLE hub + simulated rider, FTMS/HR/CPS/CSC (multi-role)[workstream BLE]
ride-ui-kit design system: CSS tokens/primitives, palette, icons   [workstream KIT]
ride-ui-map slippy map                                             [workstream MAP]
ride-ui-profile elevation profile + live charts                    [workstream PROFILE]
ride-ui-hud metric tiles, stats, climb/workout cards, controls     [workstream HUD]
ride-ui-dialogs welcome, devices, settings, workouts, summary, about [workstream DIALOGS]
ride-app    shell: state, controller loop, tile proxy, layout      [workstream SHELL]
```

Dependency graph: `ride-model` ← everything; `ride-ui-*` ← `ride-ui-kit`; `ride-app` ← all.
`ride-sim`, `ride-route`, `ride-ble` do **not** depend on each other (only on `ride-model`).
Tests in other crates use `ride_model::testing::straight_route(&[(len_m, grade_pct)], start_ele)`.

**Ownership rules (parallel work):**
1. Only edit files inside your own crate(s) (+ docs you own). Never touch another workstream's files. If a contract change is needed, do the minimum **additive** change to `ride-model` (new field/fn/impl, never rename/remove/retype), re-run `cargo check --workspace`, and report it.
2. The public signatures in the stub `lib.rs` of your crate are the contract: keep them. You may add items.
3. Write each source file fully in one Write call, then compile immediately. If `cargo check` fails in a crate you do not own, someone is mid-edit: wait ~60 s and retry (do not "fix" their files).
4. Cargo.toml: you may add dependencies you need (re-read the file right before editing, minimal edit, no reformatting). Prefer permissive-licensed crates only.
5. Tests are mandatory for logic crates (route, sim, ble parsers). UI crates: unit-test pure helpers (geometry, path building, formatting).
6. No `unwrap()/expect()/panic!/indexing-that-can-panic` on external data (files, network, BLE bytes). Return errors with Hungarian messages where a user can see them.

## Licensing rules (the README must always contain the "Licenc" section)

- Project code: `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`). New crates inherit via `license.workspace = true`.
- Only permissive third-party code may be copied/ported/vendored (MIT, Apache-2.0, BSD, ISC, Zlib, CC0, Unlicense). **Never** copy GPL/AGPL/LGPL/unlicensed code (see PRIOR_ART.md).
- Every vendored/ported file starts with a header comment: origin URL + commit/version, licence (SPDX), what changed. Also append a section to `NOTICE.md` (re-read right before editing). Copy the upstream licence text into `crates/<crate>/third_party/<name>/LICENSE*` for vendored code.
- The map must display attribution (`ride_ui_map::attribution`). The `AboutDialog` and README "Licenc" list the same sources. Tell the architect in your report if you add a new external data source/service.

## Data flow (ride-app)

```
 sensors (BLE / simulated) ──SensorEvent──► controller task ──► Signals ──► UI crates (props)
        ▲                                        │
        └────────SensorCommand──────────────────┘  (grade/sim params, start/pause, scan/connect)

 controller: every 100 ms  dt = real elapsed (cap 1 s, substeps ok)
    sample = latest merged SensorSample (stale values already cleared by the hub)
    snapshot = engine.step(dt, &sample, unix_now)          // pure
    signals: snapshot (10 Hz), history (1 Hz, Arc<Vec<RecordPoint>>)
    if ride is Riding: if workout target Some → SetTargetPower (≥1 W or every 5 s)
                       else if trainer_sim() changed (≥0.1 % or every 5 s) → SetSimulation
    on RideState::Finished → summary dialog
```
- UI components get `ReadSignal<T>` props; they must not own business state. Heavy derived data (route paths, tick lists) is computed once per route/zoom with `use_memo`, not every 100 ms.
- Persistence (`ride_route::store`): settings on every change (debounced 500 ms), recent route on load.
- File open: `rfd` async dialog + drag&drop onto the window. Save TCX: `rfd` save dialog, default name `ride_sim::tcx::suggested_filename`.
- Keep the PC awake while riding (`SetThreadExecutionState` via the `windows` crate or equivalent).
- Keyboard: `Space` start/pause, `M` map follow, `P` profile mode, `W` workouts, `F11` fullscreen, `T` theme, `Esc` closes dialogs, `↑/↓` ±10 W for the simulated rider.
- Test hooks (env vars, read in `ride-app`): `RIDE_DEMO=1` load the demo route; `RIDE_SIM=<watts>` auto-connect the simulated rider at that power and auto-start the ride; `RIDE_WINDOW=1600x900`; `RIDE_THEME=light|dark`; `RIDE_WORKOUT=<id>` select a workout; `RIDE_GPX=<path>` load a file; `RIDE_BLE_LOG=1` hex log; `RIDE_STREETVIEW_BASE=http://127.0.0.1:<port>` points the Street View metadata, image and embed requests at a local mock (`scripts/streetview_mock.mjs`, loopback only). These are used by screenshot-based verification.

## Tile proxy (ride-app/src/tiles.rs)

Registered as a Dioxus custom protocol `tiles` (on Windows the webview URL is `http://tiles.localhost/...`). Handler: `ride_ui_map::parse_tile_path` → disk cache hit (`%LOCALAPPDATA%/bike-osm-ride/tiles/<style>/<z>/<x>/<y>.png`, 30 days) → else `reqwest` GET `ride_ui_map::upstream_url` with `User-Agent: bike-osm-ride/0.1 (desktop app)`, ≤ 6 concurrent, timeout 10 s, write to cache, return PNG with `Cache-Control`. On failure return a transparent 256×256 PNG (and 200) so the map never shows broken-image icons. Respect the OSM tile policy (no bulk prefetch).

## Street View offline cache (ride-ui-streetview)

Two stores below `%LOCALAPPDATA%/bike-osm-ride/streetview/`: `<route id>.json` holds panorama ids per 25 m bucket (30 days; "no panorama" 7 days) and `img/<route id>/` holds the opt-in downloaded images plus `manifest.json`. Image spacing is chosen per route (`step_for`: 75–250 m, max 900 images), the files are served by the `svimg` protocol (`http://svimg.localhost/streetview/<id>/<bucket>.jpg`, path validated, `no-store`) and expire after 30 days. Downloads only start from the panel's explicit button, use the Static API image endpoint with the user's key, 4 concurrent + ~8 starts/s, and are cancellable; progress goes through a `watch` channel. A cached image always wins over the iframe, so a downloaded route needs no key and no request at all; a missing bucket falls back to the live embed (or to the "not downloaded" message without a key).

## Domain knowledge (verified facts)

**Physics** (`ride-sim`): `P·η = (F_grav + F_roll + F_aero)·v + m_eff·v·dv/dt`, `θ = atan(grade/100)`, `F_grav = m g sinθ`, `F_roll = Crr m g cosθ`, `F_aero = ½ ρ CdA (v + v_wind)²` (sign-aware), `m_eff = m + rotational_mass`. Integrate kinetic energy (stable at v≈0), clamp `v ≥ 0` (no rolling backwards), cap ~30 m/s, substeps ≤ 50 ms. Reference value: 75 kg rider + 9 kg bike, CdA 0.32, Crr 0.004, η 0.975, ρ 1.225, 250 W flat → steady ≈ 36.7 km/h (10.2 m/s); 250 W at 8 % → ≈ 12.4 km/h. Coggan zones (% FTP): <55, 55–75, 76–90, 91–105, 106–120, 121–150, >150. HR zones (% max): <60, 60–70, 70–80, 80–90, >90. NP = 4th-root of the mean of 4th powers of the 30 s rolling average. kcal ≈ kJ work (≈24 % efficiency). TSS = (s·NP·IF)/(FTP·3600)·100, IF = NP/FTP.

**BRouter / bikerouter**: API `https://brouter.de/brouter?lonlats=<lon,lat>|<lon,lat>|…&profile=trekking&alternativeidx=0&format=gpx` (CORS `*`; verified GPX 1.1 `<trk><trkseg><trkpt lon lat><ele>`, elevation in 0.25 m steps and noisy → smooth; a comment header carries `track-length`, `filtered ascend`). A bikerouter.de share link keeps its state in the URL fragment (`#map=z/lat/lon/layer&lonlats=lon,lat;lon,lat&nogos=…&profile=…&alternativeidx=…&straight=…`): parse the fragment, convert `;`→`|`, keep `profile`, `alternativeidx`, `nogos`, `polylines`, `polygons`, `straight` if present (verify against github.com/nrenner/brouter-web, MIT). Real sample: `tests/fixtures/brouter_budapest_short.gpx`.
Route pipeline: parse (trk/trkseg, rte/rtept, wpt; several segments joined; missing/zero `ele` handled; namespaces/extensions ignored; BOM) → drop duplicate/jittery points → cumulative haversine distance → resample ~10 m → smooth elevation by a ~50 m distance window (no single-point spikes) → grade over ~50–100 m, clamp ±25 % → ascent/descent with ~3 m hysteresis → climbs (≥ 500 m long, avg ≥ 3 %, gain ≥ 25 m; merge dips < 150 m; category by length[m]×grade[%]: ≥8 000 cat 4, ≥16 000 cat 3, ≥32 000 cat 2, ≥64 000 cat 1, ≥80 000 HC) → bounds, id = stable hash (FNV-1a) of the geometry. `has_elevation=false` when < 80 % of points have non-zero distinct elevation.
Missing elevation fallback: Open-Meteo `https://api.open-meteo.com/v1/elevation?latitude=a,b,…&longitude=…` (≤ 100 coords per request, CC BY 4.0 — attribution already in README).

**BLE / FTMS** (`ride-ble`; confirm every number against the sources in PRIOR_ART.md before trusting this summary):
- Services: FTMS `0x1826` (Indoor Bike Data `0x2AD2` notify; Control Point `0x2AD9` write+indicate; Machine Status `0x2ADA` notify; Feature `0x2ACC` read; Supported Power Range `0x2AD8`; Training Status `0x2AD3`), Cycling Power `0x1818` (Measurement `0x2A63`, Feature `0x2A65`), CSC `0x1816` (`0x2A5B`), Heart Rate `0x180D` (`0x2A37`).
- Indoor Bike Data flags: bit0 is inverted (instantaneous speed present when bit0 = 0); bits: 1 avg speed, 2 inst cadence (0.5 rpm), 3 avg cadence, 4 total distance (u24 m), 5 resistance level, 6 inst power (s16 W), 7 avg power, 8 expended energy, 9 heart rate, 10 MET, 11 elapsed time, 12 remaining time. Speed unit 0.01 km/h.
- Control handshake: subscribe to Control Point indications + Machine Status + Indoor Bike Data **first**; write Request Control (0x00), wait for the response indication `[0x80, 0x00, 0x01]`; Start/Resume (0x07); then Set Indoor Bike Simulation Parameters (0x11: wind s16 0.001 m/s, grade s16 0.01 %, Crr u8 0.0001, Cw u8 0.01 kg/m). One outstanding Control Point procedure at a time (wait for its indication, timeout 3 s). Re-send the simulation parameters at least every 5 s as keep-alive; clamp grade to the trainer's supported inclination range and ±20 %; rate-limit; send Stop/Pause (0x08) on pause and a neutral grade on disconnect/finish (port the OsmolControl safety filter).
- **The `ftms` crate rejects values that are not on the wire grid** (`Error::InvalidRange`) → quantise (grade to 0.01 %, crr to 0.0001, cw to 0.01 kg/m, wind to 0.001 m/s) before encoding.
- CPS: cadence from crank revolution deltas (event time 1/1024 s, u16 wrap), CSC speed/cadence likewise (wheel 1/1024 s in CSC, 1/2048 s in CPS); pedal balance when present.
- Priorities for the merged `SensorSample`: power: power meter > trainer; cadence: CSC sensor > power-meter cranks > trainer; HR: HR strap > trainer-reported HR; speed: trainer-reported only as an optional source. Any value not refreshed for 3 s becomes `None`. Emit at most ~10 `Sample` events per second.
- **One physical device may serve several roles on one connection** (`multi.rs`): a trainer that also exposes CPS, CSC or HR is claimed as Trainer + PowerMeter + Cadence + HeartRate, subscribes to every measurement it finds and routes each stream to the matching `Source`; a role it does not expose shows "nincs … szolgáltatás" in its slot while the others stay connected. Service discovery claims every **free** role it finds, even when the advertisement was silent about it. `slots` is keyed by device id, `role_owner` maps role → device. A dedicated device (or the simulated rider) connecting later takes over only that role (the previous owner keeps its link and remaining roles; its data for the moved role is dropped by the hub). Connecting any role of a multi-role device and pressing "Bontás" disconnects the whole device.
- **Windows (WinRT) scanning: service-UUID scan filters are unreliable → scan unfiltered, classify by advertised service UUIDs and name prefix, then verify services after connecting.** Stop scanning before connecting. The same role set is also derived from the peripheral's advertisement when no scan ran. Auto-reconnect (3 tries, backoff) with `SensorEvent::Notice` messages.
- The simulated rider is a normal "device" (`sim:rider`): `SetVirtualPower(w)` drives power; cadence ≈ f(power, grade); HR follows a lagged first-order response; it honours `SetSimulation`/`TrainerStart/Pause` as no-ops. Used for tests and for riding without hardware.
- `HubOptions::hex_log`: append `timestamp  direction  characteristic  hex` for every notification/write/indication.
- **Zwift Click / Play shifting** (`ride-ble/src/zwift/`, unofficial interoperability, see `NOTICE.md`). A device that advertises the Zwift service `00000001-19ca-4651-86e5-fa29dcdd09d1`, manufacturer id `0x094A` with controller type 2/3/9, or a `Zwift Click`/`Zwift Play` name is `DeviceKind::Shifter`, unless it also advertises FTMS (a Zwift Hub is a trainer). The Shifter role uses the same GATT link as the other roles (`multi.rs`): it subscribes `0002` and `0004`, writes the `RideOn` hello to `0003`, and feeds the notifications to `zwift::ControllerSession`, a pure state machine with the 180 ms per-button debouncer. Shifts go `Ctx::shift` → `Internal::Shift` (live owner only) → `SensorEvent::Shift(±1)`; the app switches the virtual gearbox on at the first step. The layers below the session are pure: `proto` (protobuf2 subset: Click/Play key pads, Hub riding data and commands, codec only), `crypto` (ECDH P-256 → HKDF-SHA256 → AES-256-CCM, 4-byte tag, 8-byte nonce; a bare `RideOn` answer means plain packets). The Play button mapping is the single function `controller::play_shift_buttons`.

**Workouts** (`ride-model::workout`, `ride-sim::workout`): the library is a JSON asset compiled into the model (`assets/workouts.json`, 93 entries generated by `scripts/gen_workouts.py`, each validated on load; tags come from the fixed `WorkoutTag` vocabulary and an unknown tag invalidates the entry); a step has a kind, a duration and a target (`pct_ftp`, `watts`, ramps, `step_watts` for ramp tests, or free). The runner advances with riding time, computes targets from the **current** FTP and reports `WorkoutProgress` in the snapshot. `RideCore` sends `SetTargetPower` while a workout target exists (ERG) and falls back to `SetSimulation` in free steps or after the last block; the workout cannot be swapped while `Riding`, and pausing only sends `TrainerPause`.

**Adjustable shell** (shell/state): `rail_width`, `profile_height` and the small-pane rectangle (`pip_*`, fractions) live in `UiSettings`; during a drag `AppState.layout` holds the live geometry and `AppState.drag` says what is being changed. `DragCatcher` is a full-screen mouse catcher rendered by the root while a drag is active; it clamps and applies the new value and `Controller::save_layout` writes the settings synchronously (a debounced task spawned from the catcher's scope would be cancelled when the catcher unmounts). `streetview_large` is the focus flag: the focused pane fills the stage and the other one moves into the shared rectangle. `MapView` is the same component in both roles; its `compact` mode uses a fixed `COMPACT_ZOOM` rider minimap instead of fitting the route, so the small map never upscales coarse overview tiles.

## UI brief (applies to every `ride-ui-*` crate and the shell)

- **Look:** dark-first, premium "cockpit": near-black blue background, translucent glass panels (`backdrop-filter: blur`), 1 px hairline borders, soft shadows, generous spacing, 12–24 px radii. Big **tabular numerals** (`font-variant-numeric: tabular-nums`) with small dim unit labels; the power tile is the hero (zone colour ring/bar, 3 s average, W/kg, zone name). Smooth value transitions, hover/focus-visible states, `prefers-reduced-motion` respected. Light theme via `:root[data-theme="light"]`. No emoji; icons from `ride-ui-kit::Icon`.
- **Tokens (names are a contract; KIT owns values):** `--bg --bg-1 --bg-2 --bg-3 --glass --glass-strong --border --border-strong --text --text-dim --text-faint --accent --accent-ink --accent-soft --ok --warn --crit --z1…--z7 --h1…--h5 --grade-down --grade-flat --grade-up-1…--grade-up-4 --radius-sm/md/lg/xl --shadow-1 --shadow-2 --space-1…--space-7 --ease --dur-fast --dur --dur-slow --font-ui --font-display --font-num`. Primitive classes from the kit: `.btn .btn--primary .btn--ghost .btn--icon .chip .card .glass .kbd .num .seg .switch .slider .field .dialog .dialog-backdrop .toasts .toast`. Each crate adds its own component CSS (BEM-ish, prefixed: `map-`, `profile-`, `hud-`, `dlg-`, `shell-`) via its `pub const CSS`, using tokens only (no hard-coded colours except data colours from `ride_ui_kit::palette`).
- **Colour rules (dataviz):** grade colours are a *diverging* scale (cool = descent, neutral grey ≈ 0 %, warm → dark red = climb); zone colours are an ordered ramp; every colour that carries meaning also has a text/number label; validate palettes with the dataviz `validate_palette.js` for both themes (KIT does this).
- **Layout (shell, ≥ 1280 px wide):** top bar (logo · route name chip · `Útvonal` · `Eszközök` with 3 status dots · `Beállítások` · theme · about) / main area = map full-bleed with floating `ClimbCard` (top-left) and map controls (top-right) + right rail 380 px (`MetricTiles`, `StatsPanel`, `LiveChart`s) / bottom = `ElevationProfile` (full width, ~230 px) with `RideControls` docked over its left edge. < 1100 px: rail collapses under the map. Window default 1440×900, min 1024×640. Welcome screen (no route) replaces the map area.
- Every number the user sees comes from `ride_ui_kit::format`. Missing sensor → "—".
- Accessibility: contrast ≥ 4.5:1 for text, visible focus, buttons have `aria-label`s, dialogs trap Esc.

## Workstream briefs

**ROUTE** (`ride-route`): implement the stub API + the pipeline above (modules `gpx`, `geo`, `build`, `climbs`, `bikerouter`, `elevation`, `demo`, `store`). Fixtures in `tests/fixtures/` (add a longer hilly BRouter GPX fetched with curl, a GPX with `<rte>`/`<wpt>`, one without elevation, one with multiple segments, a broken file). Tests: parsing, resampling invariants (monotonic dist, spacing), smoothing removes spikes, ascent hysteresis, climb detection on a synthetic profile, bikerouter link parsing (several real-looking links), store round-trip in a temp dir (store functions take the dir from a `RIDE_DATA_DIR` override for tests). 100 km / 100 k-point GPX processes in < 2 s. Demo route: ≈ 24 km with rolling hills and one 4 km ~7 % climb, deterministic, plausible Budapest-area coordinates.

**SIM** (`ride-sim`): physics, zones, stats (rolling 3 s power, NP, TSS, kcal, ascent/descent done/left with hysteresis, ETA from steady-state speed ahead), recorder (1 Hz `RecordPoint`), `RideEngine` state machine (Ready → Riding ⇄ Paused → Finished; auto-finish at route end; timer excludes pauses; `seek` rules; `trainer_sim()` = grade at the position ~20 m ahead, smoothed, scaled by `trainer_grade_scale`, slew-limited, quantised, clamped ±20 %, crr/cw from settings), `RideSummary` incl. zone seconds, TCX writer (Garmin schema: Activity Sport="Biking", Lap, Track, Trackpoint with Time/Position/AltitudeMeters/DistanceMeters/HeartRateBpm/Cadence/Extensions TPX Watts+Speed; valid XML escaping; ISO-8601 UTC without external crates). Property/regression tests with the reference values above.

**BLE** (`ride-ble`): follow `docs/PRIOR_ART.md` strictly: depend on `ftms`; vendor `cycling-ble` parsers (CPS/CSC/HR + their tests) with licence text; port OsmolControl's connection/safety logic and iron-heart's scan/reconnect patterns (MIT, with headers + NOTICE entries). Modules: `parsers` (thin adapters → `SensorSample` fields), `merge` (priorities + staleness), `trainer` (handshake, control queue, keep-alive, safety filter), `hr`, `power`, `csc`, `scan` (unfiltered, classification), `sim` (simulated rider), `hub` (command loop, event stream, 10 Hz sample emission), `hexlog`. **Differential tests**: the `ftms` crate vs the vendored `cycling-ble` FTMS parser vs hand-built vectors from the spec for Indoor Bike Data (every flag combination that matters, bit0 inversion), plus Control Point encoding vectors (Request Control, Start, Stop/Pause, Set Indoor Bike Simulation incl. negative grade and quantisation edges) and CPS/CSC cadence-delta wraparound. Everything that can run without hardware must be unit-tested; define a small `GattTransport` trait if that makes the hub testable with a fake.

**KIT** (`ride-ui-kit`): full `kit.css` (tokens, dark+light, type scale, all primitive classes, dialog/seg/switch/slider/field/toast styling, scrollbars, focus rings, reduced motion), real inline-SVG icons for every `IconName` (consistent 1.75 px stroke, 24 px grid), validated palettes in `palette.rs` + `docs/DESIGN.md` (include the validator output), embedded Inter or Segoe-based type decisions (if a font is bundled it must be OFL/Apache and listed in NOTICE.md), working `Dialog` (Esc, backdrop, focus), `Segmented`, `Switch`, `Slider`, `NumberField`, `ToastHost`. Keep `format.rs` API (add, don't change).

**MAP** (`ride-ui-map`): pure-Rust slippy map: Web-Mercator math (tested), tile grid as absolutely positioned `<img>` using CSS transforms (no per-frame re-layout of all tiles), 256 px tiles with retina handling, zoom 3–19, wheel zoom around the cursor, drag pan, smooth follow mode (rider kept at ~60 % height, heading-up is optional), route as one SVG path per grade colour bucket (or gradient) with a darker casing, start/finish markers, rider marker with heading arrow and pulse, fit-to-route on load, style switcher (Sötét/Világos/OSM), follow/recenter buttons, attribution control. Handles missing/failed tiles gracefully (blank placeholder).

**PROFILE** (`ride-ui-profile`): SVG elevation profile with grade-coloured fill (per-segment or gradient stops, ≤ 400 segments), light grid, y-axis auto-range with padding, x ticks in km, climb bands with labels, rider marker + vertical guide + progress shading (done vs ahead), hover read-out, click-to-seek, look-ahead window mode with smooth scrolling, Full/Look-ahead segmented control. `LiveChart`: last N seconds area/line chart, zone-coloured, with value read-out and hover; both must stay smooth at 10 Hz (memoise static geometry).

**HUD** (`ride-ui-hud`): `MetricTiles` (hero power tile with zone ring/bar and zone name, W/kg, 3 s avg; cadence; HR with zone; speed; grade chip), `StatsPanel` (compact grid), `ClimbCard` (next climb with distance countdown, length/avg grade/gain; in-climb progress bar; hidden when none), `RideControls` (state-aware transport with keyboard hints). Values animate without layout shift (fixed-width numerals).

**DIALOGS** (`ride-ui-dialogs`): `WelcomeScreen` (hero, drop zone, file button, bikerouter-link field with paste button and validation hint, recents with metadata + delete, demo button, busy state, error line), `DevicesDialog` (slot cards for Trainer/Pulzus/Teljesítménymérő/Cadence-szenzor with state badges, scan button, discovered list with RSSI + suggested kind, connect/disconnect, the simulated rider listed first), `VirtualRiderPanel`, `SettingsDialog` (grouped, immediate apply), `SummaryDialog` (hero stats + zone bars), `AboutDialog` (licences, mirrors README).

**SHELL** (`ride-app`): `main.rs` (Dioxus desktop `Config`: title "bike-osm-ride", 1440×900, min size, no menu bar, background colour = `--bg`, custom protocol for tiles, window icon optional), `build.rs` (copy `WebView2Loader.dll`), `state.rs` (signals + context), `controller.rs` (ride loop, engine/hub wiring, persistence, toasts, summary), `tiles.rs`, `shell.rs` (layout above, theme attribute on `<html>`, CSS injection: kit first, then crate CSS), file open/save/drag-drop, keyboard shortcuts, wake lock, env test hooks. Error paths → toasts (Hungarian). Must start without Bluetooth hardware/adapter and without network.

## Definition of done (per workstream)

`cargo check -p <crate>` clean (no warnings from your code), `cargo test -p <crate>` green, public stub signatures unchanged, no `todo!()` left, licence headers + NOTICE entries for anything vendored, final report listing: files created, public API additions, deviations from this document (with reason), verification commands run + results, open issues.
