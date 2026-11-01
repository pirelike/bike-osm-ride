// Vendored from cycling-ble 0.2.0 - https://github.com/friedrichwilken/cycling-ble
// Upstream file: tests/contract/csc.rs at commit 06c34eaf410d4470bf7558d472bc50407f8307e4 (2026-10-01).
// Licence: MIT OR Apache-2.0 (SPDX: MIT OR Apache-2.0), Copyright (c) 2026 Jens Friedrich Wilken.
//   Licence texts: crates/ride-ble/third_party/cycling-ble/LICENSE-MIT and LICENSE-APACHE.
// Changes: ported from an integration test crate to an in-crate test module: `cycling_ble::` ->
//   `crate::vendor::cycling_ble::`, `crate::assert_truncations` -> `super::assert_truncations`;
//   Zwift Click parts removed. Assertions unchanged.
// ---------------------------------------------------------------------------
//! CSC Measurement (0x2A5B).

use super::assert_truncations;
use crate::vendor::cycling_ble::csc::{self, CscMeasurement};

/// Every field of [`CscMeasurement`], spelled out so a golden test asserts
/// the complete parsed value. Nested structs are flattened into
/// `(cumulative_revolutions, last_event_time_raw)` tuples.
#[derive(Debug, Default)]
struct Expected {
    wheel_revolutions: Option<(u32, u16)>,
    crank_revolutions: Option<(u16, u16)>,
}

fn assert_csc(actual: &CscMeasurement, e: &Expected) {
    assert_eq!(
        actual
            .wheel_revolutions
            .map(|w| (w.cumulative_revolutions, w.last_event_time_raw)),
        e.wheel_revolutions,
        "wheel_revolutions"
    );
    assert_eq!(
        actual
            .crank_revolutions
            .map(|c| (c.cumulative_revolutions, c.last_event_time_raw)),
        e.crank_revolutions,
        "crank_revolutions"
    );
}

/// Parses `data`, checks the complete value, then checks every truncation
/// errors.
fn golden(data: &[u8], expected: Expected) {
    let m = csc::parse(data).expect("golden payload must parse");
    assert_csc(&m, &expected);
    assert_truncations(data, csc::parse, &[]);
}

#[test]
fn no_flags_set() {
    golden(&[0x00], Expected::default());
}

#[test]
fn wheel_then_crank_offsets() {
    golden(
        &[
            0x03, // flags: wheel, crank
            0x39, 0x30, 0x00, 0x00, // wheel revolutions 12345
            0xD0, 0x07, // wheel event time 2000
            0x58, 0x02, // crank revolutions 600
            0xA0, 0x0F, // crank event time 4000
        ],
        Expected {
            wheel_revolutions: Some((12345, 2000)),
            crank_revolutions: Some((600, 4000)),
        },
    );
}
