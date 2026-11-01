//! The decoder interface shared by the heart-rate, power-meter and cadence/speed streams.
//!
//! The sessions themselves live in [`crate::multi`]: one GATT connection can carry any
//! combination of FTMS, Cycling Power, CSC and Heart Rate, and each stream feeds its own
//! decoder (the vendored `cycling-ble` parsers behind `hr.rs`, `power.rs`, `csc.rs`).

use std::time::Instant;

use crate::merge::Field;
use crate::parsers::ParseError;

/// Turns raw measurement packets into merger input.
pub trait Decoder: Send + 'static {
    fn feed(&mut self, bytes: &[u8], now: Instant) -> Result<Vec<(Field, f64)>, ParseError>;
    /// Timer-driven values (e.g. cadence 0 while coasting).
    fn idle(&mut self, _now: Instant) -> Vec<(Field, f64)> {
        Vec::new()
    }
    /// Forget counters (a new connection starts from scratch).
    fn reset(&mut self);
}
