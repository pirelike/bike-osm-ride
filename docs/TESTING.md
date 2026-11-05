# Testing and verification

What the automated tests cover, how to run them, and, just as important, what has **not** been verified on real
hardware or against real services. Numbers measured on 2026-11-04 on the maintainer's Windows 11 machine
(`stable-x86_64-pc-windows-gnu`, Rust 1.99.1).

## 1. Running

```powershell
cargo test --workspace            # 954 passed, 0 failed, 3 ignored
cargo clippy --workspace --all-targets   # clean, no warnings
cargo fmt --all -- --check        # clean
```

**In a sandboxed shell, point `TEMP`/`TMP` at a folder inside the repository** (for example
`$env:TMP = "$PWD\target\tmp-tests"`; `$env:TEMP = $env:TMP`). Otherwise `tempfile` fails with `PermissionDenied`
and 50 tests across four crates go red although nothing is wrong (`ride-app` 10, `ride-route` 21, `ride-ble` 1,
`ride-ui-streetview` 18). A folder in the session scratchpad did *not* help; one under `target/` did.

| Crate | Passed | Of which in `tests/` |
|---|---|---|
| `ride-app` | 132 | |
| `ride-ble` | 286 (+1 ignored) | |
| `ride-model` | 19 | |
| `ride-route` | 101 + 10 (+2 ignored) | `fixtures_pipeline.rs` |
| `ride-sim` | 61 | |
| `ride-ui-dialogs` | 64 | |
| `ride-ui-hud` | 30 | |
| `ride-ui-kit` | 52 | |
| `ride-ui-map` | 39 + 6 | `render.rs` |
| `ride-ui-profile` | 68 | |
| `ride-ui-streetview` | 83 + 3 | `key_safety.rs`, `offline_flow.rs`, `panel_flow.rs` |

**Ignored tests** (they need something the CI runner does not have; run them by hand with `-- --ignored`):
`ride-route`'s `live_bikerouter_share_link` (public BRouter server) and `live_open_meteo_fills_missing_elevation`
(public Open-Meteo API), and `ride-ble`'s `btle::tests::smoke_adapter_count` (the machine's Bluetooth stack).

CI runs `cargo test --workspace` on `windows-latest`, `ubuntu-22.04` and `macos-14`, but only inside the manually
started release workflow ([`RELEASING.md`](RELEASING.md)); there is no CI on push or pull request.

## 2. Kinds of tests

| Kind | Where | What it catches |
|---|---|---|
| Reference values | `ride-sim/physics.rs`, `engine.rs` | the Martin model against the `rouleur` example, 250 W on the flat and at 8 %; NP, TSS, zone boundaries; the ride finishing exactly once |
| Real fixtures | `tests/fixtures/*.gpx`, `ride-route` | BOM and namespaces, comma decimals, multiple segments, `<rte>`, no elevation, a truncated file that must give a clear error; real BRouter output against its own header numbers |
| **Differential** | `ride-ble/src/diff_tests.rs` | the same bytes decoded by the `ftms` crate, by the vendored `cycling-ble` parser and by hand-built spec vectors must agree (every relevant Indoor Bike Data flag combination, Control Point vectors, CSC/CPS wrap-around) |
| **Behaviour on a fake transport** | `ride-ble/src/fake.rs`, `hub_tests.rs`, `control.rs` | scan, connect, the FTMS handshake, keep-alive, status storms, reclaim limit, pause and neutral hand-back, reconnects and replay, merge priorities: the real code paths with failures injected, in milliseconds (`HubConfig::fast`) |
| Protocol vectors | `ride-ble/src/zwift/` | NIST SP 800-38C AES-CCM vectors, the key exchange, the debouncer |
| Persistence | `ride-route/store.rs`, `rides.rs`, `ride-model` | round trips in a temp folder, corrupt and foreign files, path traversal ids, an old settings file / route cache / ride summary still loading |
| Pure UI helpers | every `ride-ui-*` crate | geometry, formatting, filtering, zone-bar percentages, the dock's tiling invariants after every operation |
| **House rules as tests** | `ride-ui-kit`, `ride-ui-dialogs`, `ride-app/shell.rs` | CSS lints (no blur, no hard-coded colours, only defined tokens, spacing grid, class prefixes), palette contrast, WCAG and colour-difference gates, "no second close button in a dialog footer", the font CSS in sync with the `.woff2` files, palette constants in sync with the CSS tokens |
| Packaging | `ride-app/main.rs` `packaging_tests` | the pinned Android manifest and activity match `Dioxus.toml` (permissions, `configChanges`, identifier) |
| Updater logic | `ride-app/updater.rs` | version comparison, asset choice per platform, install mode, magic bytes, ignoring drafts and garbage |
| Server-side rendering | `ride-ui-map/tests/render.rs`, the `examples/*_preview.rs` | markup of the map and the components in both themes |
| Mock servers | `ride-ui-streetview/src/mock.rs`, `scripts/streetview_mock.mjs`, `ride-route/src/http.rs` test server | the HTTP clients, quota and key errors, caching, "a repeat ride makes no requests" |

## 3. Looking at the real app

There are no end-to-end UI tests. The app is checked by running it: `RIDE_DEMO`, `RIDE_SIM`, `RIDE_TIMESCALE` and the
other hooks reproduce a ride without clicking, and the DevTools tools take page screenshots
([`DEVELOPMENT.md`](DEVELOPMENT.md) §3, §4, §6). `node scripts/readme_shots.mjs` is, as a side effect, a smoke test
of eleven screens in two layouts: it fails when the app dies or a screen does not appear.

## 4. Verification matrix

"Real" means: observed on actual hardware or against the actual service.

| Area | Automated | Real |
|---|---|---|
| Route pipeline, climbs, ghost, segments, TCX | yes | the app was ridden with the simulated rider on Windows; a real BRouter route is in the fixtures |
| Physics | yes | no riding data from a real trainer was compared; the model is cross-checked against published values |
| FTMS decoding and control logic | yes (differential, fake transport) | **one trainer** (VE200): Indoor Bike Data, CSC and CPS streams decoded; Request Control, Start and **ERG targets** accepted (928); Stop/Pause answered "not supported". **No terrain-grade simulation was ever captured on hardware** ([`BLUETOOTH.md`](BLUETOOTH.md) §9) |
| HR strap, separate power meter, cadence sensor | decoders yes | **never** |
| Zwift Click / Play | crypto and state machine yes | **never** |
| Windows app | yes | daily use |
| macOS app | CI build and bundle checks (Info.plist Bluetooth text, universal binary) | **no run on a real machine on record** |
| Linux app (AppImage, `.deb`) | CI build, a 20 s smoke test under `xvfb` | **no run on a real machine on record** |
| Android app | packaging tests; CI build | UI exercised during development (the 0.3.0 commit message records the crashes found and fixed); **Bluetooth, Strava share, long rides: never** |
| In-app updater | pure logic | the check runs at every start; a full self-replace on a published release is not documented as exercised |
| Strava upload | script generation, word parsing | **never against strava.com** ([`STRAVA.md`](STRAVA.md)) |
| Street View | mock-based flows, key-safety test | **never against Google** ([`STREETVIEW.md`](STREETVIEW.md)) |
| Browser panel (Firefox) | geometry helpers | used on Windows |

## 4a. Known gaps, ranked by how much they could hurt

1. Terrain simulation on a real trainer is unproven. Do one ride on a hilly route with `ble.log` kept, then look
   for `WRITE 2ad9 11...` with non-zero grade bytes and `80 11 01` answers.
2. No CI on push: a red test can reach `main`.
3. Strava and Google paths depend on pages and endpoints nobody has run since they were written.
4. Mac and Linux users are probably the first to exercise their builds on real machines.
5. One unexplained observation: once in about fifteen captures the look-ahead elevation profile showed the last
   3 km of the route while the rider was at 3.2 km (it did not repeat). If it reappears, note what changed in the
   layout just before.

## 5. Adding tests: what is expected

Logic crates: a test per rule, including the awkward input (non-finite numbers, empty route, zero FTP). Anything
that parses external data: a corrupt-input test proving it does not panic. A new persisted field: the
"old file still loads" test. BLE code: a fake-transport scenario, plus a byte-level vector from the spec or a
capture. UI crates: test the pure helper, not the markup, unless the markup is the contract (CSS rules, class
prefixes). Never hard-code the Windows-only tile base `http://tiles.localhost`; use `ride_ui_kit::protocol::base`.
