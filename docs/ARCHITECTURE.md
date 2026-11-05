# Architecture

bike-osm-ride rides a GPX route on a 2D map with an elevation profile while a Bluetooth LE smart trainer
follows the hills (or a simulated rider pedals). One Rust code base builds the Windows, Linux, macOS and
Android apps. Everything below was checked against the code on 2026-11-04 (version 0.3.0).

This document is the map. The details live next to it: [`BLUETOOTH.md`](BLUETOOTH.md),
[`SIMULATION.md`](SIMULATION.md), [`ROUTES_AND_DATA.md`](ROUTES_AND_DATA.md), [`UI.md`](UI.md),
[`ANDROID.md`](ANDROID.md), [`STREETVIEW.md`](STREETVIEW.md), and the reasons behind
the big choices in [`DECISIONS.md`](DECISIONS.md).

## 1. Shape of the system

```
                       ┌──────────────────────────── ride-app (the shell) ───────────────────────────┐
 GPX file / link ─────►│ route loader ─► Route                                                         │
                       │                    │                                                          │
 BLE sensors ◄──► ride-ble hub  ──SensorSample──► RideCore ──► RideEngine (ride-sim: physics, stats,    │
 (or simulated rider)    ▲                          │            recorder, workout, ghost, segments)    │
                         └──SensorCommand───────────┘                                                   │
                                                    ▼ snapshot 10 Hz, history 1 Hz                      │
                              Dioxus signals ──► ride-ui-* components ──► webview (HTML/CSS/SVG)        │
                       └──────────────────────────────────────────────────────────────────────────────┘
```

- **Webview UI.** Dioxus 0.7 drives the system webview (WebView2, WKWebView, WebKitGTK, Android WebView). The
  UI is components, CSS and inline SVG; there is no canvas and no 3D by design.
- **Pure core.** Anything with rules lives in crates without I/O or clocks (`ride-model`, `ride-sim`, the
  pure halves of `ride-route`, `ride-ble` and the UI geometry), so it is unit-tested to the number.
- **One shell.** `ride-app` owns the clock, the channels, the files and the windows, and is the only crate
  that knows about every other.

## 2. Workspace

Dependencies between workspace crates (from the `Cargo.toml` files; the graph is acyclic):

| Crate | Depends on |
|---|---|
| `ride-model` | none |
| `ride-sim`, `ride-ble`, `ride-ui-kit` | `ride-model` |
| `ride-route` | `ride-model`, `ride-sim` |
| `ride-ui-map`, `ride-ui-profile`, `ride-ui-hud` | `ride-model`, `ride-ui-kit` |
| `ride-ui-streetview` | `ride-model`, `ride-route` (data folder, HTTP client), `ride-ui-kit` |
| `ride-ui-dialogs` | `ride-model`, `ride-ui-kit`, `ride-ui-streetview` (the storage overview in the settings) |
| `ride-ui-review` | `ride-model`, `ride-sim` (distance grid, section statistics, estimate parameters: pure maths), `ride-ui-kit`, `ride-ui-map`, `ride-ui-profile` (geometry helpers) |
| `ride-app` | all of them |

| Crate | Responsibility | Where to look first |
|---|---|---|
| `ride-model` | The shared types: `Route`, `Settings`, `RideSnapshot`, `SensorSample`/`SensorCommand`/`SensorEvent`, workouts and their library, the ride log types. **A contract: change it additively.** | `src/lib.rs`, `workout.rs`, `history.rs` |
| `ride-route` | GPX and bikerouter link → processed `Route`; climb detection; the file store (settings, recents, route cache, ride log, imported workouts), the backup zip (`backup.rs`). Async loaders, pure pipeline. | `lib.rs`, `build.rs`, `bikerouter.rs`, `store.rs`, `rides.rs`, `backup.rs` |
| `ride-sim` | Physics, `RideEngine` (state machine, statistics, 1 Hz recorder, trainer grade), workout runner and importer, ghost, segments, zones, TCX, the power curve / FTP suggestion / weekly load (`power_curve.rs`). No I/O, no clock, no async. | `engine.rs`, `physics.rs`, `workout.rs`, `ghost.rs`, `power_curve.rs` |
| `ride-ble` | The sensor hub on its own thread: transport traits, scanning, one connection per device with any mix of roles, FTMS control session, CPS/CSC/HR decoders, Zwift Click/Play, simulated rider, hex log. | `lib.rs`, `hub.rs`, `multi.rs`, `control.rs` |
| `ride-ui-kit` | Design system: CSS tokens and primitives, palette, icons, number formatting, base components, protocol URLs, the workout chart. | `assets/kit.css`, `palette.rs`, `format.rs` |
| `ride-ui-map` | Slippy map: Web-Mercator maths, camera, route as per-grade SVG paths, markers, tile `<img>` layer. | `lib.rs`, `camera.rs`, `route_path.rs` |
| `ride-ui-profile` | Elevation profile (full or look-ahead), live charts, workout view, phone swipe pages. | `lib.rs`, `geom.rs`, `series.rs`, `workout.rs` |
| `ride-ui-hud` | Power hero and metric cells, statistics, next-climb card, workout card, transport controls, phone head unit. | `lib.rs`, `dash.rs` |
| `ride-ui-dialogs` | Route chooser, devices, settings, workout picker, ride summary, about, simulated-rider panel. | `lib.rs`, `helpers.rs` (pure) |
| `ride-ui-review` | The review page: ride list, map with the zoomed section, overview profile with drag-to-zoom, multi-ride distance chart, section table, estimate setup. View state only; the loaded records come from the app. | `lib.rs`, `chart.rs` (pure) |
| `ride-ui-streetview` | Optional Street View panel with the rider's own Google key: metadata client, pano-id cache, offline image store, planner, mock. | `lib.rs`, [`STREETVIEW.md`](STREETVIEW.md) |
| `ride-app` | State, controller and ride loop, tiling dock, shell layout, tile proxy, `svimg` protocol, updater, browser panel, wake lock, crash reporter, Android glue, test hooks. | `main.rs`, `controller.rs`, `ride_core.rs`, `dock.rs`, `shell.rs` |

Why `ride-route` depends on `ride-sim`: `rides::save_ride` stores the climb ("segment") times of a finished
ride with the very computation the engine uses live (`ride_sim::segments::segment_times`), so a stored PR and
the PR on screen can never disagree. The dependency is acyclic.

## 3. Runtime model

| Where | What runs there |
|---|---|
| **UI thread** | The Dioxus event loop, all components, and the whole `Controller`: the ride loop, event handlers, settings, file dialogs, toasts. |
| **`ride-ble-hub` thread** | `spawn_sensor_hub` starts its own multi-thread tokio runtime (2 workers, `ride-ble-rt`) running the hub: command loop, merger, simulated rider, one task per connected device and one scan task. If it dies, `hub_lost` tells the rider once; the app keeps working without sensors. |
| **tokio blocking pool** | Route building, the ghost/PR plan (`history.rs`), storing a finished ride and its comparison, tile and image cache work. |
| **webview protocol handlers** | `tiles` (map tiles) and `svimg` (downloaded Street View images): served by `ride-app` custom protocols, path-validated. |

**The controller.** `Controller` is a `Copy` handle to a leaked `&'static RefCell<Inner>`, so closures in
`rsx!` can use it freely. The rule that keeps the `RefCell` sound: a method borrows `Inner` exactly once for one
synchronous call and never across an `await`. Every entry goes through `Controller::with`, which contains a
panic (`guard::guarded`), records it (`crash.rs`) and shows a banner instead of unwinding into the event loop.

**The ride loop** (`Controller::run`): a 100 ms `tokio::time::interval` (missed ticks skipped) and the sensor
event stream share one `select!`.

1. `step_dt = min(real elapsed, 1 s) × RIDE_TIMESCALE` (the scale is 1 outside tests).
2. `RideCore::tick` feeds the latest merged `SensorSample` to `RideEngine::step(dt, sample, unix_now)`.
3. The `Output` carries the new snapshot, the commands for the hub, the history when it grew, a state
   transition and, on `Finished`, the finished ride.
4. `apply` writes the signals (`snapshot` only when it changed, `history` as an `Arc<Vec<_>>` at 1 Hz),
   sends the commands, toggles the wake lock and stores a finished ride.
5. While `Riding`: with a workout target the ERG target goes out (at once when it moves ≥ 1 W, otherwise
   every 5 s); otherwise the simulation parameters go out (when the grade moves ≥ 0.1 % or every 5 s).
   Pause and finish send the trainer a neutral grade first. Details: [`SIMULATION.md`](SIMULATION.md).

**State.** `AppState` is a bundle of Dioxus `Signal`s created once at the root. The controller is the only
writer of the ride-related ones. UI crates receive `ReadSignal`s as props, own no business state and
memoise derived data (route paths, tick lists) per route and zoom instead of recomputing at 10 Hz. The
settings signal is saved debounced (500 ms) and flushed on exit (`LoopDestroyed`).

## 4. Platforms

The same shell runs everywhere; the differences are `cfg`-gated, there is no fork.

| | Windows | macOS | Linux | Android |
|---|---|---|---|---|
| webview | WebView2 | WKWebView | WebKitGTK 4.1 | Android WebView |
| Bluetooth (btleplug) | WinRT | CoreBluetooth | BlueZ over D-Bus | the Android backend, initialised through JNI (`android_ble.rs`); the activity asks for the runtime permissions |
| file open | native dialog (`rfd`) | native dialog | native dialog (xdg-portal) | system picker through `MainActivity.kt` (JNI), copied to the cache first |
| backup zip (save / restore) | `rfd` save / open | same | same | the system "save as" / "open" dialogs through `MainActivity.kt` (JNI); unverified on a device |
| keep awake while riding | `SetThreadExecutionState` | IOKit power assertion | `org.freedesktop.ScreenSaver.Inhibit` | `FLAG_KEEP_SCREEN_ON` through JNI (unverified on a device) |
| app protocol base | `http://<scheme>.localhost` | `<scheme>://localhost` | `<scheme>://localhost` | `https://<scheme>.localhost` |
| browser panel (Firefox) | yes | no | no | no |
| data folder | `%LOCALAPPDATA%\bike-osm-ride` | `~/Library/Application Support/bike-osm-ride` | `$XDG_DATA_HOME/bike-osm-ride` or `~/.local/share/...` | `/data/data/<package>/files` |
| package | `.exe` | universal `.app` in a `.dmg` | AppImage and `.deb` | `.apk` |
| self-update | replaces the exe | opens the release page | replaces the AppImage; `.deb` opens the page | opens the APK in the browser |

- **App protocol URLs** (`ride_ui_kit::protocol::base`): one handler per protocol serves all four webviews
  because every handler reads only `uri().path()`.
- **Menu.** macOS keeps Dioxus' default application menu (WKWebView takes Cmd+C/V/Q from it, without it the
  rider could not paste a Google key). Elsewhere `with_menu(None)`.
- **Linux.** `main` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` when unset (empty window on some NVIDIA/Wayland setups).
- **Windows GUI subsystem.** Release builds have no console window; debug builds keep it.

## 5. Failure containment

| Failure | What happens |
|---|---|
| a panic anywhere in a handler or a call into another crate | caught (`guarded`, `Controller::with`), written to `crash.log`, banner in the UI, the app continues. A panic in the engine drops the engine ("the simulation stopped") and keeps the window. |
| no Bluetooth adapter, Bluetooth off, backend init failure | the hub reports a Hungarian notice; the simulated rider stays available; the adapter is created again on demand |
| no network | tiles: a transparent tile, expired cache preferred, an 8 s pause after a transport error; route link: a Hungarian error; elevation fallback: the route loads flat |
| a corrupt or foreign file in the data folder | ignored, defaults used; files larger than 64 MB are refused; writes are atomic (temp file + rename) |
| a record or setting from another version | unknown record versions are ignored, unknown fields default |
| a failed update check | silent |

## 6. Security and privacy boundaries

- **Secrets.** The only secret is the rider's Google Maps key. It sits in `settings.json` on that machine, is
  redacted in every `Debug` output (`UiSettings`), never appears in an error or a log, and is only sent to
  `maps.googleapis.com` / `www.google.com`, or to a loopback mock chosen with `RIDE_STREETVIEW_BASE`
  (a non-loopback value is ignored). A test (`key_safety.rs`) guards this.
- **No account, no telemetry.** What leaves the machine is listed in [`PRIVACY.md`](PRIVACY.md).
- **Local protocols** validate the request path (no traversal), allow `GET` only and answer from the cache folders only.
- **Control safety.** Everything written to a trainer's FTMS Control Point passes an allow-list filter that
  clamps targets (`BLUETOOTH.md`).

## 7. History

The first build (2026-10-31) was organised as parallel workstreams, one crate each, driven by a written
contract (the former contents of this file: workstream briefs, ownership and "definition of done" rules, a
glass-style UI brief that the "race sheet" design replaced). That text is in git history (commit `7264770`).
What was still true in it was moved into the topic documents above and into [`DEVELOPMENT.md`](DEVELOPMENT.md).
Some doc comments in the code still say "see `docs/ARCHITECTURE.md`" for facts that now live elsewhere: the zone
definitions are in [`SIMULATION.md`](SIMULATION.md) §2, the merge priorities and the reconnect policy in
[`BLUETOOTH.md`](BLUETOOTH.md) §2 and §5.
