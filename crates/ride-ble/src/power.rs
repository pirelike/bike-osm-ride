//! Standalone power meters (Cycling Power Service 0x1818, Measurement 0x2A63): instantaneous
//! power, left/right balance when the meter reports it, and cadence from the crank
//! revolution counters. The wheel revolution data is ignored — the trainer reports speed.

use std::time::Instant;

use crate::csc::RevTracker;
use crate::merge::Field;
use crate::parsers::{parse_cycling_power, ParseError, MAX_POWER_W};
use crate::sensor::Decoder;

#[derive(Debug, Clone)]
pub struct PowerDecoder {
    crank: RevTracker,
}

impl Default for PowerDecoder {
    fn default() -> Self {
        Self {
            crank: RevTracker::crank(),
        }
    }
}

impl Decoder for PowerDecoder {
    fn feed(&mut self, bytes: &[u8], now: Instant) -> Result<Vec<(Field, f64)>, ParseError> {
        let m = parse_cycling_power(bytes)?;
        let mut out = Vec::with_capacity(3);
        let watts = f64::from(m.power_w);
        if watts <= MAX_POWER_W {
            out.push((Field::Power, watts.max(0.0)));
        }
        if let Some(b) = m.balance_left_pct.filter(|b| (0.0..=100.0).contains(b)) {
            out.push((Field::PowerBalance, b));
        }
        if let Some(c) = m.crank {
            if let Some(rpm) = self.crank.update(c, now) {
                out.push((Field::Cadence, rpm));
            }
        }
        Ok(out)
    }

    fn idle(&mut self, now: Instant) -> Vec<(Field, f64)> {
        self.crank
            .idle(now)
            .map(|c| vec![(Field::Cadence, c)])
            .unwrap_or_default()
    }

    fn reset(&mut self) {
        self.crank.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn packet(power: i16, crank_revs: u16, crank_time: u16) -> Vec<u8> {
        let mut v = vec![0x20, 0x00];
        v.extend_from_slice(&power.to_le_bytes());
        v.extend_from_slice(&crank_revs.to_le_bytes());
        v.extend_from_slice(&crank_time.to_le_bytes());
        v
    }

    #[test]
    fn power_and_cadence() {
        let t = Instant::now();
        let mut d = PowerDecoder::default();
        let first = d.feed(&packet(250, 100, 0), t).unwrap();
        assert_eq!(first, vec![(Field::Power, 250.0)]);
        let second = d
            .feed(&packet(260, 101, 683), t + Duration::from_millis(667))
            .unwrap();
        assert_eq!(second.len(), 2);
        assert_eq!(second[0], (Field::Power, 260.0));
        assert_eq!(second[1].0, Field::Cadence);
        assert!((second[1].1 - 90.0).abs() < 0.2);
    }

    #[test]
    fn negative_power_is_zero_and_balance_needs_left_reference() {
        let t = Instant::now();
        let mut d = PowerDecoder::default();
        let out = d.feed(&[0x00, 0x00, 0xFB, 0xFF], t).unwrap();
        assert_eq!(out, vec![(Field::Power, 0.0)]);
        let out = d.feed(&[0x03, 0x00, 0xC8, 0x00, 110], t).unwrap();
        assert!(out.contains(&(Field::PowerBalance, 55.0)));
        let out = d.feed(&[0x01, 0x00, 0xC8, 0x00, 110], t).unwrap();
        assert!(!out.iter().any(|(f, _)| *f == Field::PowerBalance));
    }

    #[test]
    fn coasting_cadence_goes_to_zero_on_idle() {
        let t = Instant::now();
        let mut d = PowerDecoder::default();
        d.feed(&packet(100, 5, 0), t).unwrap();
        d.feed(&packet(100, 6, 1024), t + Duration::from_secs(1))
            .unwrap();
        assert!(d.idle(t + Duration::from_secs(2)).is_empty());
        assert_eq!(
            d.idle(t + Duration::from_secs(5)),
            vec![(Field::Cadence, 0.0)]
        );
    }

    #[test]
    fn garbage_is_an_error() {
        let mut d = PowerDecoder::default();
        assert!(d.feed(&[], Instant::now()).is_err());
        assert!(d.feed(&[0x20, 0x00, 0x00], Instant::now()).is_err());
    }
}
