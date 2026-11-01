// Vendored from cycling-ble 0.2.0 - https://github.com/friedrichwilken/cycling-ble
// Upstream file: tests/contract/heart_rate.rs at commit 06c34eaf410d4470bf7558d472bc50407f8307e4 (2026-10-01).
// Licence: MIT OR Apache-2.0 (SPDX: MIT OR Apache-2.0), Copyright (c) 2026 Jens Friedrich Wilken.
//   Licence texts: crates/ride-ble/third_party/cycling-ble/LICENSE-MIT and LICENSE-APACHE.
// Changes: ported from an integration test crate to an in-crate test module: `cycling_ble::` ->
//   `crate::vendor::cycling_ble::`, `crate::assert_truncations` -> `super::assert_truncations`;
//   Zwift Click parts removed. Assertions unchanged.
// ---------------------------------------------------------------------------
//! Heart Rate Measurement (0x2A37).

use super::assert_truncations;
use crate::vendor::cycling_ble::heart_rate::{self, HeartRateMeasurement};

/// Every field of [`HeartRateMeasurement`], spelled out so a golden test
/// asserts the complete parsed value.
#[derive(Debug, Default)]
struct Expected {
    bpm: u16,
    sensor_contact_detected: Option<bool>,
    energy_expended_kj: Option<u16>,
    rr_intervals_secs: Vec<f32>,
}

fn assert_heart_rate(actual: &HeartRateMeasurement, e: &Expected) {
    assert_eq!(actual.bpm, e.bpm, "bpm");
    assert_eq!(
        actual.sensor_contact_detected, e.sensor_contact_detected,
        "sensor_contact_detected"
    );
    assert_eq!(
        actual.energy_expended_kj, e.energy_expended_kj,
        "energy_expended_kj"
    );
    assert_eq!(
        actual.rr_intervals_secs, e.rr_intervals_secs,
        "rr_intervals_secs"
    );
}

/// Parses `data`, checks the complete value, then checks every truncation:
/// lengths in `ok_lengths` are valid shorter payloads, all others error.
fn golden(data: &[u8], expected: Expected, ok_lengths: &[usize]) {
    let m = heart_rate::parse(data).expect("golden payload must parse");
    assert_heart_rate(&m, &expected);
    assert_truncations(data, heart_rate::parse, ok_lengths);
}

#[test]
fn uint8_bpm_only() {
    golden(
        &[0x00, 0x41],
        Expected {
            bpm: 65,
            ..Default::default()
        },
        &[],
    );
}

#[test]
fn uint16_bpm_with_contact_and_energy() {
    golden(
        &[0x0F, 0x2C, 0x01, 0xF4, 0x01],
        Expected {
            bpm: 300,
            sensor_contact_detected: Some(true),
            energy_expended_kj: Some(500),
            ..Default::default()
        },
        &[],
    );
}

#[test]
fn contact_feature_supported_but_not_detected() {
    golden(
        &[0x04, 0x46],
        Expected {
            bpm: 70,
            sensor_contact_detected: Some(false),
            ..Default::default()
        },
        &[],
    );
}

#[test]
fn contact_bit_without_feature_support_is_none() {
    golden(
        &[0x02, 0x46],
        Expected {
            bpm: 70,
            sensor_contact_detected: None,
            ..Default::default()
        },
        &[],
    );
}

#[test]
fn rr_intervals() {
    // RR intervals fill the rest of the packet with no count field, so any
    // prefix that still holds flags + bpm is a valid (shorter) payload; a
    // trailing odd byte is ignored.
    golden(
        &[0x10, 0x50, 0x00, 0x02, 0x00, 0x04],
        Expected {
            bpm: 80,
            rr_intervals_secs: vec![0.5, 1.0],
            ..Default::default()
        },
        &[2, 3, 4, 5],
    );
}
