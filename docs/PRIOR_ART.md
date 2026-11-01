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
