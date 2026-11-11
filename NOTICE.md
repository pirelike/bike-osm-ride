# NOTICE — third-party code that is vendored or ported into this repository

Dependencies pulled in through Cargo are listed (generated) in
`THIRD_PARTY_LICENSES.md`. This file lists only code that was **copied or translated**
into our source tree. Every such file also carries a header comment naming the origin,
the licence and what was changed.

Rules for contributors / agents: only permissive licences (MIT, Apache-2.0, BSD, ISC,
Zlib, CC0, Unlicense). Never copy GPL / AGPL / LGPL / unlicensed code — see
`docs/PRIOR_ART.md`. Append one section per vendored/ported source below (re-read the
file right before editing; do not reformat other sections).

<!-- entries are appended below -->

## cycling-ble — vendored (crate `ride-ble`)

- Origin: <https://github.com/friedrichwilken/cycling-ble>, version 0.2.0, commit
  `06c34eaf410d4470bf7558d472bc50407f8307e4` (2026-10-01).
- Licence: `MIT OR Apache-2.0`, Copyright (c) 2026 Jens Friedrich Wilken. Texts:
  `crates/ride-ble/third_party/cycling-ble/LICENSE-MIT` and `LICENSE-APACHE`.
- Files: `crates/ride-ble/src/vendor/cycling_ble/{mod,reader,power,csc,heart_rate,ftms}.rs`
  (Cycling Power, CSC and Heart Rate Measurement parsers, `revolutions_per_minute`; the FTMS
  Indoor Bike Data parser is compiled for tests only as a second opinion) and the upstream
  contract tests, ported to `crates/ride-ble/src/vendor/cycling_ble/contract_tests/`.
- Changes: module paths only (`crate::` to `super::`), headers added, Zwift Click module not
  vendored, `#![deny(missing_docs)]` dropped. Parsing code and test assertions are unchanged.

## OsmolControl — ported (crate `ride-ble`)

- Origin: <https://github.com/Parapeth962/OsmolControl>, commit
  `f51d14a036c2c9bb44f849f3793afecc3d800fd7` (2026-10-29). Licence: `MIT`, Copyright (c) 2026
  OsmolControl contributors. Text: `crates/ride-ble/third_party/OsmolControl/LICENSE`.
- Ported (translated and adapted, not copied verbatim): the FTMS Control Point allow-list /
  clamp / bounded coalescing command queue from `crates/bike-protocol/src/ftms_control.rs`
  and the control-replay + bounded-IO structure from `crates/bike-ble/src/{lib,trainer}.rs`
  into `crates/ride-ble/src/control.rs` and `crates/ride-ble/src/trainer.rs`. The captured
  ThinkRider X2 Max frames from `crates/bike-protocol/src/lib.rs` and
  `docs/devices/x2-max.md` are reused as test vectors.
- Changes: the filter now covers the simulation parameters (grade clamp tied to the trainer's
  inclination range and +/-20 %), the queue carries a procedure timeout and keep-alive, the
  code is independent of `anyhow`/`tracing`, and everything is driven through a transport trait.

## iron-heart — pattern port (crate `ride-ble`)

- Origin: <https://github.com/nullstalgia/iron-heart>, commit
  `8dec53e0ff35a0e243f0b093a877f2f23780d01f` (2026-10-27). Licence: `MIT`, Copyright (c) 2024
  nullstalgia. Text: `crates/ride-ble/third_party/iron-heart/LICENSE`.
- Patterns reimplemented in `crates/ride-ble/src/scan.rs` and `crates/ride-ble/src/hr.rs`
  (no source lines copied): unfiltered scan driven by the adapter event stream, re-starting a
  scan that went silent for 30 s, treating `NotConnected` / `DeviceNotFound` as a reason to
  restart scanning, a no-packet watchdog on the heart rate notification stream, and
  battery-level reads. Its `unwrap()`/`expect()` calls were deliberately not carried over.

## Barlow and Barlow Condensed — bundled fonts (crate `ride-ui-kit`)

- Origin: <https://github.com/jpt/barlow> (the Barlow Project, by Jeremy Tribby), taken from the
  Google Fonts distribution <https://github.com/google/fonts/tree/main/ofl/barlow> and
  `.../ofl/barlowcondensed` (TTF, 2017 release). Licence: SIL Open Font License 1.1, Copyright
  2017 The Barlow Project Authors. The font has no Reserved Font Name. Text:
  `crates/ride-ui-kit/assets/fonts/OFL.txt`.
- Files: `crates/ride-ui-kit/assets/fonts/*.woff2` (Barlow Regular / Medium / SemiBold, Barlow
  Condensed Medium / SemiBold / Bold) and `crates/ride-ui-kit/assets/fonts.css`, which embeds the
  same files as base64 data URIs so the app works offline.
- Changes: subset with `pyftsubset` (fontTools) to Basic Latin, Latin-1, Latin Extended-A
  (U+0100-017F, which holds the Hungarian o-double-acute and u-double-acute) and common
  punctuation / maths signs (U+2013-2014, U+2018-201E, U+2020-2022, U+2026, U+2030, U+2032-2033,
  U+20AC, U+2122, U+2190-2193, U+2212, U+2248, U+2260, U+2264-2265 ...); layout features kept:
  kern, liga, tnum, pnum, lnum, case, ccmp, locl, mark, mkmk, frac, zero; hinting dropped;
  converted to WOFF2. `crates/ride-ui-kit/tools/gen_fonts_css.mjs` regenerates `fonts.css`. The
  glyph shapes are unchanged.

## Google Maps Platform / Street View — external service, nothing vendored (crate `ride-ui-streetview`)

- No Google code, imagery or data is included in this repository or the binary. The optional
  Street View panel calls two Google endpoints with the **user's own API key**: the Street View
  Static API *metadata* endpoint (JSON only) and the Maps Embed API Street View iframe. Use is
  governed by the Google Maps Platform Terms of Service
  (<https://cloud.google.com/maps-platform/terms>) and the Street View policies
  (<https://developers.google.com/maps/documentation/streetview/policies>).
- Street View imagery is downloaded only after an explicit offline-download request, using the
  rider's own API key. Downloaded images are retained for at most 30 days (see `docs/STREETVIEW.md`).
  Panorama IDs are saved per route, together with our own route
  coordinates, the heading and a timestamp (`<data dir>/streetview/<route id>.json`). The key is
  kept in the local `settings.json` only, is redacted from every `Debug` output and is only ever
  sent to `maps.googleapis.com` / `www.google.com` (or, for tests, a loopback mock).
- `scripts/streetview_mock.mjs` is our own test double for those two endpoints; it contains no
  Google material.

## Zwift Click / Play and Zwift Hub BLE protocol — ported (crate `ride-ble`, module `zwift`)

- Origin: <https://github.com/pymmog/virtual-cog>, commit
  `1e75f4b8a1654308324dab51095ed84de60b2c79`. Licence: Unlicense (public domain dedication,
  "This is free and unencumbered software released into the public domain"), text in the
  upstream `LICENSE`.
- Protocol documentation (facts only, no code): Makinolo's interoperability notes, e.g.
  <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/> and
  <https://www.makinolo.com/blog/2023/10/08/connecting-to-zwift-play-controllers/>.
- Ported (translated, not copied verbatim): the Click/Play key exchange and packet framing
  (`VirtualCog/Devices/Click/ZwiftClickCrypto.swift`, `ClickClient.swift`), the button
  debouncer (`VirtualCog/Physics/GearModel.swift`), the protobuf subset and the Hub message
  codec (`VirtualCog/Devices/Shared/ZwiftProtobuf.swift`, `zwift.proto`, `BleUUIDs.swift`).
  Into `crates/ride-ble/src/zwift/{proto,crypto,controller,mod}.rs`.
- Changes: Rust instead of Swift; the Apple frameworks and CryptoSwift are replaced by the
  RustCrypto crates `p256`, `hkdf`, `sha2`, `aes`, `ccm` and `rand_core` (MIT OR Apache-2.0,
  listed in `THIRD_PARTY_LICENSES.md`). The Hub trainer messages are a codec only; they are not
  wired into the trainer control. The golden vectors of `protocol_golden.json` are not used
  (the AES-CCM check uses the NIST SP 800-38C vectors instead).
- This is unofficial interoperability work. It is not affiliated with, endorsed by or
  supported by Zwift, Inc.
