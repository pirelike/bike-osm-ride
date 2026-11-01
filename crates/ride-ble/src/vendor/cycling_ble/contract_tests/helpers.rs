// Vendored from cycling-ble 0.2.0 - https://github.com/friedrichwilken/cycling-ble
// Upstream file: tests/contract/helpers.rs at commit 06c34eaf410d4470bf7558d472bc50407f8307e4 (2026-10-01).
// Licence: MIT OR Apache-2.0 (SPDX: MIT OR Apache-2.0), Copyright (c) 2026 Jens Friedrich Wilken.
//   Licence texts: crates/ride-ble/third_party/cycling-ble/LICENSE-MIT and LICENSE-APACHE.
// Changes: ported from an integration test crate to an in-crate test module: `cycling_ble::` ->
//   `crate::vendor::cycling_ble::`, `crate::assert_truncations` -> `super::assert_truncations`;
//   Zwift Click parts removed. Assertions unchanged.
// ---------------------------------------------------------------------------
//! `revolutions_per_minute` and the public constants.

use crate::vendor::cycling_ble::{
    revolutions_per_minute, CP_WHEEL_EVENT_TIME_HZ, CRANK_EVENT_TIME_HZ, CRANK_REVOLUTIONS_WRAP_AT,
    CSC_WHEEL_EVENT_TIME_HZ, WHEEL_REVOLUTIONS_WRAP_AT,
};

#[test]
fn constant_values() {
    assert_eq!(CRANK_EVENT_TIME_HZ, 1024.0);
    assert_eq!(CSC_WHEEL_EVENT_TIME_HZ, 1024.0);
    assert_eq!(CP_WHEEL_EVENT_TIME_HZ, 2048.0);
    assert_eq!(CRANK_REVOLUTIONS_WRAP_AT, 65_536);
    assert_eq!(WHEEL_REVOLUTIONS_WRAP_AT, 4_294_967_296);
}

fn assert_rpm(actual: Option<f32>, expected: f32) {
    let rpm = actual.expect("expected Some(rpm)");
    assert!(
        (rpm - expected).abs() < 0.01,
        "rpm {rpm}, expected {expected}"
    );
}

#[test]
fn rpm_basic() {
    // 10 revolutions in 1 s = 600 rpm.
    assert_rpm(
        revolutions_per_minute(
            0,
            0,
            10,
            1024,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ,
        ),
        600.0,
    );
}

#[test]
fn rpm_event_time_wraparound() {
    assert_rpm(
        revolutions_per_minute(
            0,
            65_435,
            10,
            923,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ,
        ),
        600.0,
    );
}

#[test]
fn rpm_crank_revolution_wraparound() {
    assert_rpm(
        revolutions_per_minute(
            65_531,
            0,
            5,
            1024,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ,
        ),
        600.0,
    );
}

#[test]
fn rpm_wheel_revolution_wraparound() {
    assert_rpm(
        revolutions_per_minute(
            u32::MAX - 4,
            0,
            5,
            2048,
            WHEEL_REVOLUTIONS_WRAP_AT,
            CP_WHEEL_EVENT_TIME_HZ,
        ),
        600.0,
    );
}

#[test]
fn rpm_none_on_duplicate_notification() {
    assert_eq!(
        revolutions_per_minute(
            5,
            100,
            5,
            100,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ
        ),
        None
    );
}

#[test]
fn rpm_none_on_zero_elapsed_time() {
    assert_eq!(
        revolutions_per_minute(
            5,
            100,
            6,
            100,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ
        ),
        None
    );
}
