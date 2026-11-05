# Prior art: what we reuse for the trainer / sensor layer

Researched 2026-10-31. **Rule: we do not write BLE codecs from scratch.** Permissively
licensed Rust code is used as a dependency or vendored (with attribution in `NOTICE`);
GPL/AGPL code is never copied (it would make the whole app copyleft) — only
*ideas/lessons* may be taken from it.

## Use / port (permissive)

| What | Where | License | Notes |
|---|---|---|---|
| FTMS codecs | crates.io `ftms` 0.2.x (github.com/deancochran/ftms) | MIT | `no_std`, no deps, `forbid(unsafe_code)`. Indoor Bike Data, Feature (8 B), 5 Supported Ranges, **all 21 Control Point requests + responses**, Training Status, 22 Machine Status opcodes. `normalized::normalize_control_request(NormalizedControlRequest::IndoorBikeSimulation { wind_speed_mps, grade_percent, crr, cw_kg_per_m })` = opcode 0x11 (wire units: wind ×1000, grade ×100, crr ×10000, cw ×100). **Values must already lie on the wire grid** (e.g. grade rounded to 0.01 %), otherwise `Error::InvalidRange` — quantise first. Very new (Sept 2026), hardware evidence = one passive KICKR CORE capture → **verify with our own test vectors**. Codec only: no BLE, no session/lifecycle. |
| CPS / CSC / HR parsers (+ FTMS Indoor Bike Data as a 2nd opinion, + Zwift Click) | github.com/friedrichwilken/cycling-ble (`src/{power,csc,heart_rate,ftms,reader,zwift_click}.rs`, `tests/contract/*`) | Apache-2.0 OR MIT | Parser-only, no BLE dep, hand-built byte tests only (no hardware yet). `reader.rs` has `revolutions_per_minute` with u16 event-time wraparound. Not on crates.io → **vendor** into `ride-ble/src/vendor/` with the licence text, keep tests. Use its `ftms` module for differential testing against the `ftms` crate. |
| Trainer connection + command safety | github.com/Parapeth962/OsmolControl : `crates/bike-ble/src/{lib,trainer}.rs`, `crates/bike-protocol/src/{lib,ftms_control}.rs` (allow-list, clamp to trainer limits, rate-limit, reset on stop), `docs/protocol/bluetooth.md`, `docs/devices/*.md` | MIT | btleplug based. Tested live with a ThinkRider X2 Max. Read it fully before writing `ride-ble/src/trainer.rs`; port the structure. |
| HR monitor scan / reconnect on Windows | github.com/nullstalgia/iron-heart : `src/scan.rs`, `src/heart_rate/ble.rs`, `src/heart_rate/measurement.rs` | MIT | btleplug, Windows-aware. |
| Spec cross-checks (read only) | pycycling (MIT), python-pyftms dudanov/python-pyftms (Apache-2.0), flutter_ftms Malte2036 (BSD-3), Zwack paixaop/zwack (MIT; FTMS *peripheral* simulator) | permissive | Use them to double check flag layouts / opcodes / byte vectors. |

## Never copy (copyleft) — ideas only

- TheElysium/opencycling (GPL-3, Rust, ERG only, Tauri). **Lesson: on Windows (WinRT) BLE scanning, filtering by service UUID is unreliable → scan unfiltered and classify devices by advertised service UUIDs / name prefix, then confirm after connect by discovering services.**
- frenata/suffice (AGPL-3, ERG only, no grade simulation), GoldenCheetah (GPL-2), qdomyos-zwift (GPL-3), FTMS.NET (GPL-3), ewoc (GPL-3), krishardy/smarttrainerui (GPL-3).

## Not useful
- `kondis` (MIT, iConsole+ non-BLE), `arctic` (Polar H7 only, old), `blehr` (non-standard licence, 2021).

## Open risk
Everything above is young and lightly hardware-tested. Mitigations: (1) differential
tests `ftms` crate vs `cycling-ble` vs hand-computed spec vectors; (2) raw hex logging of
every notification / control-point exchange (`RIDE_BLE_LOG=1` → `%LOCALAPPDATA%/bike-osm-ride/ble.log`);
(3) a fully simulated trainer backend so every other part of the app is testable without hardware.

## Added after the first research (2026-11-01 to 2026-11-03)

| What | Where | Licence | How it is used |
|---|---|---|---|
| Zwift Click / Play / Hub protocol | github.com/pymmog/virtual-cog (`VirtualCog/Devices/...`, `Physics/GearModel.swift`) | Unlicense (public domain) | **ported** to Rust: key exchange, packet framing, debouncer, protobuf subset: `ride-ble/src/zwift/` (see `NOTICE.md`) |
| Zwift protocol documentation | Makinolo's interoperability notes (makinolo.com) | documentation only | read as documentation, no code; unofficial work, not affiliated with Zwift |
| Cryptography for the Zwift handshake | RustCrypto `p256`, `hkdf`, `sha2`, `aes`, `ccm` | MIT OR Apache-2.0 | dependencies |
| bikerouter / brouter-web link formats | github.com/nrenner/brouter-web (`leaflet-fullHash.js`, `BRouter.js`) and BRouter's `RoutingParamCollector.java` | MIT | read to verify the accepted link formats (`ride-route/src/bikerouter.rs` documents the commit); no code copied |
| Martin et al. (1998) power model | J. Appl. Biomech. 14:276-291; the open-source `rouleur` package for a worked example | paper / package | the model is implemented from the paper; one worked example's number is a test value |
| Fonts | Barlow and Barlow Condensed (the Barlow Project) | SIL OFL 1.1 | embedded (subset, WOFF2): see `NOTICE.md` |
| Web upload form of Strava | StravaUploaderPy | GPL-3.0 | **idea only** (names of the form fields); no code copied; the app does not use the Strava API ([`STRAVA.md`](STRAVA.md)) |
| Map data | OpenStreetMap, BRouter, Open-Meteo, Google Maps Platform | ODbL, public services, CC BY 4.0, the user's own key | external services, attributed in the README |

Zwift Hub trainer messages are decoded by `zwift/proto.rs` but not wired into the trainer control.

## Status of the "open risk" (2026-11-04)

The risk above was that the reused trainer code is young and lightly hardware-tested. What has happened since:
all three mitigations exist (differential tests, an always-on hex log, a simulated trainer). A real trainer
(a generic FTMS "VE200") was captured for about 3.3 hours: it exposed behaviours no specification mentions (it
reports "Reset" four times a second, it sends Indoor Bike Data frames longer than their flags describe, it answers
Stop/Pause with "not supported"), and the control session now handles each of them ([`BLUETOOTH.md`](BLUETOOTH.md) §4 and §9). What the capture does **not** contain is a terrain simulation, so
the grade path is still proven only against the fake transport and the specification.
