//! Heart rate straps (Heart Rate Service 0x180D, Measurement 0x2A37).
//!
//! Pattern notes (iron-heart, MIT, see NOTICE.md): subscribe to the measurement
//! characteristic only, treat a silent stream as a dead link (the `sensor_watchdog` of
//! `HubConfig`, checked in the dialogue loop of `multi.rs`), and tolerate straps that stop
//! notifying when they lose skin contact.

use std::time::Instant;

use crate::merge::Field;
use crate::parsers::{parse_heart_rate, ParseError, MAX_HR_BPM, MIN_HR_BPM};
use crate::sensor::Decoder;

#[derive(Debug, Clone, Default)]
pub struct HrDecoder;

impl Decoder for HrDecoder {
    fn feed(&mut self, bytes: &[u8], _now: Instant) -> Result<Vec<(Field, f64)>, ParseError> {
        let hr = parse_heart_rate(bytes)?;
        // 0 bpm is "not measured"; a strap that says "no skin contact" is not measuring
        // either. Both simply produce no value, so the merged sample goes stale after 3 s.
        if hr.contact == Some(false) {
            return Ok(Vec::new());
        }
        let bpm = f64::from(hr.bpm);
        if (MIN_HR_BPM..=MAX_HR_BPM).contains(&bpm) {
            Ok(vec![(Field::HeartRate, bpm)])
        } else {
            Ok(Vec::new())
        }
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_wide_formats() {
        let mut d = HrDecoder;
        let t = Instant::now();
        assert_eq!(
            d.feed(&[0x00, 72], t).unwrap(),
            vec![(Field::HeartRate, 72.0)]
        );
        // 16-bit value 300 is above the plausible window
        assert!(d.feed(&[0x01, 0x2C, 0x01], t).unwrap().is_empty());
        // 16-bit value 150
        assert_eq!(
            d.feed(&[0x01, 0x96, 0x00], t).unwrap(),
            vec![(Field::HeartRate, 150.0)]
        );
    }

    #[test]
    fn no_contact_and_zero_produce_nothing() {
        let mut d = HrDecoder;
        let t = Instant::now();
        // contact supported (bit 2) but not detected (bit 1 clear)
        assert!(d.feed(&[0x04, 80], t).unwrap().is_empty());
        // contact supported and detected
        assert_eq!(
            d.feed(&[0x06, 80], t).unwrap(),
            vec![(Field::HeartRate, 80.0)]
        );
        assert!(d.feed(&[0x00, 0], t).unwrap().is_empty());
    }

    #[test]
    fn rr_intervals_and_energy_do_not_disturb_the_value() {
        let mut d = HrDecoder;
        let t = Instant::now();
        // energy (bit 3) + RR (bit 4)
        let out = d
            .feed(&[0x18, 66, 0x0A, 0x00, 0x00, 0x04, 0x10, 0x03], t)
            .unwrap();
        assert_eq!(out, vec![(Field::HeartRate, 66.0)]);
    }

    #[test]
    fn garbage_is_an_error() {
        let mut d = HrDecoder;
        assert!(d.feed(&[], Instant::now()).is_err());
        assert!(d.feed(&[0x01, 0x10], Instant::now()).is_err());
    }
}
