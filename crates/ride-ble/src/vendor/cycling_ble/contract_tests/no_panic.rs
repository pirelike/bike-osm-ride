// Vendored from cycling-ble 0.2.0 - https://github.com/friedrichwilken/cycling-ble
// Upstream file: tests/contract/no_panic.rs at commit 06c34eaf410d4470bf7558d472bc50407f8307e4 (2026-10-01).
// Licence: MIT OR Apache-2.0 (SPDX: MIT OR Apache-2.0), Copyright (c) 2026 Jens Friedrich Wilken.
//   Licence texts: crates/ride-ble/third_party/cycling-ble/LICENSE-MIT and LICENSE-APACHE.
// Changes: ported from an integration test crate to an in-crate test module: `cycling_ble::` ->
//   `crate::vendor::cycling_ble::`, `crate::assert_truncations` -> `super::assert_truncations`;
//   Zwift Click parts removed. Assertions unchanged.
// ---------------------------------------------------------------------------
//! Behavioural guarantee: no parser panics, on any input. Passes if nothing
//! panics; the parse results themselves are not checked here.

use std::panic;

/// Runs every public parser on `data`, reporting the offending payload if
/// any of them panics.
fn feed_all(data: &[u8]) {
    let result = panic::catch_unwind(|| {
        let _ = crate::vendor::cycling_ble::power::parse(data);
        let _ = crate::vendor::cycling_ble::heart_rate::parse(data);
        let _ = crate::vendor::cycling_ble::csc::parse(data);
        let _ = crate::vendor::cycling_ble::ftms::parse(data);
    });
    if result.is_err() {
        panic!("a parser panicked on payload {data:02X?}");
    }
}

/// Every payload of length 0..=3, exhaustive over the first two bytes
/// (where every characteristic keeps its flags). The third byte, when
/// present, takes both extreme values.
#[test]
fn short_payloads_exhaustive_over_flags() {
    feed_all(&[]);
    for b0 in 0..=u8::MAX {
        feed_all(&[b0]);
        for b1 in 0..=u8::MAX {
            feed_all(&[b0, b1]);
            for b2 in [0x00, 0xFF] {
                feed_all(&[b0, b1, b2]);
            }
        }
    }
}

/// Fixed-seed 64-bit linear congruential generator (Knuth's MMIX
/// constants), inline so the test needs no extra dependency and is
/// reproducible.
struct Lcg(u64);

impl Lcg {
    fn next_u8(&mut self) -> u8 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // High bits of an LCG are the well-mixed ones.
        (self.0 >> 56) as u8
    }
}

/// 100 000 pseudo-random payloads of length 0..=40.
#[test]
fn random_payloads() {
    let mut rng = Lcg(0x00C0_FFEE_D00D_F00D);
    let mut buf = [0u8; 40];
    for _ in 0..100_000 {
        let len = usize::from(rng.next_u8()) % 41;
        for byte in &mut buf[..len] {
            *byte = rng.next_u8();
        }
        feed_all(&buf[..len]);
    }
}
