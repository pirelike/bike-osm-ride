// Vendored from cycling-ble 0.2.0 - https://github.com/friedrichwilken/cycling-ble
// Upstream file: tests/contract/main.rs at commit 06c34eaf410d4470bf7558d472bc50407f8307e4 (2026-10-01).
// Licence: MIT OR Apache-2.0 (SPDX: MIT OR Apache-2.0), Copyright (c) 2026 Jens Friedrich Wilken.
//   Licence texts: crates/ride-ble/third_party/cycling-ble/LICENSE-MIT and LICENSE-APACHE.
// Changes: ported from an integration test crate to an in-crate test module: `cycling_ble::` ->
//   `crate::vendor::cycling_ble::`, `crate::assert_truncations` -> `super::assert_truncations`;
//   Zwift Click parts removed. Assertions unchanged.
// ---------------------------------------------------------------------------
//! Contract tests: the behaviour of `cycling-ble`'s public API, pinned down
//! as golden vectors. These are integration tests, so they can only reach
//! the crate through its public API.
//!
//! Append-only within one breaking version — see `README.md` in this
//! directory before editing or deleting anything here.

mod csc;
mod ftms;
mod heart_rate;
mod helpers;
mod no_panic;
mod power;

use crate::vendor::cycling_ble::ParseError;
use std::fmt::Debug;

/// Error contract: parses every truncation of `full` (lengths `0` to
/// `full.len() - 1`). Lengths listed in `ok_lengths` are themselves valid
/// payloads and must parse `Ok`; every other length must return
/// `Err(ParseError { got, needed })` with `got` equal to the truncated
/// length and `needed` greater than it.
pub(crate) fn assert_truncations<T: Debug>(
    full: &[u8],
    parse: fn(&[u8]) -> Result<T, ParseError>,
    ok_lengths: &[usize],
) {
    for len in 0..full.len() {
        let payload = &full[..len];
        match parse(payload) {
            Ok(value) => assert!(
                ok_lengths.contains(&len),
                "truncation to {len} bytes {payload:02X?} parsed Ok({value:?}), expected Err"
            ),
            Err(err) => {
                assert!(
                    !ok_lengths.contains(&len),
                    "truncation to {len} bytes {payload:02X?} returned {err:?}, expected Ok"
                );
                assert_eq!(err.got, len, "ParseError::got for {payload:02X?}");
                assert!(
                    err.needed > len,
                    "ParseError::needed ({}) must exceed got ({len}) for {payload:02X?}",
                    err.needed
                );
            }
        }
    }
}
