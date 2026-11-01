//! Cadence / speed from cumulative revolution counters (CSC sensors, power-meter cranks).
//!
//! Two consecutive (revolutions, event-time) readings give a rate. The counters wrap
//! (crank revolutions at 2^16, wheel revolutions at 2^32, event time at 2^16 ticks), the
//! arithmetic is the vendored `cycling_ble::revolutions_per_minute`. On top of it this
//! module adds what a ride needs:
//!
//! * duplicate notifications (same counters) carry no information and never produce a rate;
//! * a counter reset (sensor restarted) shows up as an absurd rate and is discarded;
//! * no new revolution for [`COAST_AFTER`] means the rider stopped pedalling: 0 rpm.

use std::time::{Duration, Instant};

use crate::merge::Field;
use crate::parsers::{parse_csc, ParseError, RevSample, MAX_CADENCE_RPM, MAX_SPEED_MS};
use crate::sensor::Decoder;
use crate::vendor::cycling_ble::{
    revolutions_per_minute, CRANK_EVENT_TIME_HZ, CRANK_REVOLUTIONS_WRAP_AT,
    CSC_WHEEL_EVENT_TIME_HZ, WHEEL_REVOLUTIONS_WRAP_AT,
};

/// No new revolution for this long = coasting.
pub const COAST_AFTER: Duration = Duration::from_millis(2500);

/// Default wheel circumference for speed from a CSC wheel sensor: 700x25c, 2.105 m. (The app
/// has no tyre setting; trainer-reported speed has priority anyway.)
pub const DEFAULT_WHEEL_CIRCUMFERENCE_M: f64 = 2.105;

#[derive(Debug, Clone)]
pub struct RevTracker {
    wrap_at: u64,
    hz: f32,
    max_rpm: f64,
    prev: Option<RevSample>,
    last_change: Option<Instant>,
    last_rpm: Option<f64>,
}

impl RevTracker {
    fn new(wrap_at: u64, hz: f32, max_rpm: f64) -> Self {
        Self {
            wrap_at,
            hz,
            max_rpm,
            prev: None,
            last_change: None,
            last_rpm: None,
        }
    }

    /// Crank counter: u16 revolutions, 1/1024 s event time (CSC and Cycling Power).
    pub fn crank() -> Self {
        Self::new(
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ,
            MAX_CADENCE_RPM,
        )
    }

    /// CSC wheel counter: u32 revolutions, 1/1024 s event time. The rate is wheel rpm.
    pub fn csc_wheel(circumference_m: f64) -> Self {
        let max_wheel_rpm = MAX_SPEED_MS / circumference_m.max(0.1) * 60.0;
        Self::new(
            WHEEL_REVOLUTIONS_WRAP_AT,
            CSC_WHEEL_EVENT_TIME_HZ,
            max_wheel_rpm,
        )
    }

    pub fn reset(&mut self) {
        self.prev = None;
        self.last_change = None;
        self.last_rpm = None;
    }

    /// Feed one reading. Returns the current rate, `Some(0.0)` when coasting, or `None`
    /// when nothing is known yet (first reading, or a discarded glitch).
    pub fn update(&mut self, sample: RevSample, now: Instant) -> Option<f64> {
        let Some(prev) = self.prev else {
            self.prev = Some(sample);
            self.last_change = Some(now);
            return None;
        };
        if sample.revs == prev.revs {
            // duplicate / timestamp refresh without a new revolution
            return self.coast_value(now).or(self.last_rpm);
        }
        match revolutions_per_minute(
            prev.revs,
            prev.time,
            sample.revs,
            sample.time,
            self.wrap_at,
            self.hz,
        ) {
            Some(rpm) if rpm.is_finite() && f64::from(rpm) <= self.max_rpm => {
                self.prev = Some(sample);
                self.last_change = Some(now);
                self.last_rpm = Some(f64::from(rpm));
                self.last_rpm
            }
            _ => {
                // counter reset or zero elapsed time: start over from this reading
                self.prev = Some(sample);
                self.last_change = Some(now);
                self.last_rpm = None;
                None
            }
        }
    }

    fn coast_value(&self, now: Instant) -> Option<f64> {
        let last = self.last_change?;
        (self.prev.is_some() && now.saturating_duration_since(last) >= COAST_AFTER).then_some(0.0)
    }

    /// Timer-driven check: `Some(0.0)` once the rider has been coasting for [`COAST_AFTER`].
    pub fn idle(&self, now: Instant) -> Option<f64> {
        self.coast_value(now)
    }
}

/// Decoder for a CSC Measurement stream (cadence sensor, speed sensor or combined).
#[derive(Debug, Clone)]
pub struct CscDecoder {
    crank: RevTracker,
    wheel: RevTracker,
    circumference_m: f64,
}

impl Default for CscDecoder {
    fn default() -> Self {
        Self::new(DEFAULT_WHEEL_CIRCUMFERENCE_M)
    }
}

impl CscDecoder {
    pub fn new(circumference_m: f64) -> Self {
        let c = if circumference_m.is_finite() && circumference_m > 0.5 {
            circumference_m
        } else {
            DEFAULT_WHEEL_CIRCUMFERENCE_M
        };
        Self {
            crank: RevTracker::crank(),
            wheel: RevTracker::csc_wheel(c),
            circumference_m: c,
        }
    }

    fn speed_of(&self, wheel_rpm: f64) -> f64 {
        wheel_rpm / 60.0 * self.circumference_m
    }
}

impl Decoder for CscDecoder {
    fn feed(&mut self, bytes: &[u8], now: Instant) -> Result<Vec<(Field, f64)>, ParseError> {
        let m = parse_csc(bytes)?;
        let mut out = Vec::new();
        if let Some(c) = m.crank {
            if let Some(rpm) = self.crank.update(c, now) {
                out.push((Field::Cadence, rpm));
            }
        }
        if let Some(w) = m.wheel {
            if let Some(rpm) = self.wheel.update(w, now) {
                out.push((Field::Speed, self.speed_of(rpm)));
            }
        }
        Ok(out)
    }

    fn idle(&mut self, now: Instant) -> Vec<(Field, f64)> {
        let mut out = Vec::new();
        if let Some(c) = self.crank.idle(now) {
            out.push((Field::Cadence, c));
        }
        if let Some(w) = self.wheel.idle(now) {
            out.push((Field::Speed, self.speed_of(w)));
        }
        out
    }

    fn reset(&mut self) {
        self.crank.reset();
        self.wheel.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(revs: u32, time: u16) -> RevSample {
        RevSample { revs, time }
    }

    fn csc_packet(crank_revs: u16, crank_time: u16) -> Vec<u8> {
        let mut v = vec![0x02];
        v.extend_from_slice(&crank_revs.to_le_bytes());
        v.extend_from_slice(&crank_time.to_le_bytes());
        v
    }

    #[test]
    fn first_reading_gives_nothing_second_gives_rate() {
        let t = Instant::now();
        let mut tr = RevTracker::crank();
        assert_eq!(tr.update(s(100, 0), t), None);
        // one revolution in 0.75 s (768 ticks) = 80 rpm
        let rpm = tr
            .update(s(101, 768), t + Duration::from_millis(750))
            .unwrap();
        assert!((rpm - 80.0).abs() < 0.01, "{rpm}");
    }

    #[test]
    fn wraparound_of_revolutions_and_time_in_one_step() {
        let t = Instant::now();
        let mut tr = RevTracker::crank();
        tr.update(s(65_535, 65_000), t);
        // revolutions wrap 65535 -> 1 (two revolutions), time wraps 65000 -> 488 (1024 ticks)
        let rpm = tr.update(s(1, 488), t + Duration::from_secs(1)).unwrap();
        assert!((rpm - 120.0).abs() < 0.01, "{rpm}");
    }

    #[test]
    fn duplicates_are_ignored_and_coasting_becomes_zero() {
        let t = Instant::now();
        let mut tr = RevTracker::crank();
        tr.update(s(10, 0), t);
        let rpm = tr.update(s(11, 1024), t + Duration::from_secs(1)).unwrap();
        assert!((rpm - 60.0).abs() < 0.01);
        // the sensor repeats the last event: keep the last rate for a while
        assert_eq!(
            tr.update(s(11, 1024), t + Duration::from_millis(1500)),
            Some(rpm)
        );
        assert_eq!(tr.idle(t + Duration::from_millis(1500)), None);
        // after COAST_AFTER without a new revolution: 0 rpm
        assert_eq!(
            tr.update(s(11, 1024), t + Duration::from_millis(3600)),
            Some(0.0)
        );
        assert_eq!(tr.idle(t + Duration::from_millis(3600)), Some(0.0));
        // pedalling resumes
        let rpm = tr
            .update(s(12, 2560), t + Duration::from_millis(4000))
            .unwrap();
        assert!((rpm - 40.0).abs() < 0.01, "{rpm}");
    }

    #[test]
    fn counter_reset_is_discarded_not_reported() {
        let t = Instant::now();
        let mut tr = RevTracker::crank();
        tr.update(s(30_000, 100), t);
        // sensor restarted: counters jump back to a small value
        assert_eq!(tr.update(s(3, 5000), t + Duration::from_secs(1)), None);
        // the next reading re-bases cleanly
        let rpm = tr.update(s(4, 6024), t + Duration::from_secs(2)).unwrap();
        assert!((rpm - 60.0).abs() < 0.01);
    }

    #[test]
    fn zero_elapsed_time_with_new_revolutions_is_discarded() {
        let t = Instant::now();
        let mut tr = RevTracker::crank();
        tr.update(s(1, 500), t);
        assert_eq!(tr.update(s(2, 500), t + Duration::from_millis(10)), None);
    }

    #[test]
    fn csc_decoder_emits_cadence_and_zero_when_idle() {
        let t = Instant::now();
        let mut d = CscDecoder::default();
        assert!(d.feed(&csc_packet(10, 0), t).unwrap().is_empty());
        let out = d
            .feed(&csc_packet(11, 683), t + Duration::from_millis(667))
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, Field::Cadence);
        assert!((out[0].1 - 90.0).abs() < 0.2, "{:?}", out);
        assert!(d.idle(t + Duration::from_secs(2)).is_empty());
        assert_eq!(
            d.idle(t + Duration::from_secs(4)),
            vec![(Field::Cadence, 0.0)]
        );
        d.reset();
        assert!(
            d.idle(t + Duration::from_secs(60)).is_empty(),
            "after reset there is no baseline"
        );
    }

    #[test]
    fn csc_decoder_wheel_speed() {
        let t = Instant::now();
        let mut d = CscDecoder::new(2.0);
        let packet = |revs: u32, time: u16| {
            let mut v = vec![0x01];
            v.extend_from_slice(&revs.to_le_bytes());
            v.extend_from_slice(&time.to_le_bytes());
            v
        };
        assert!(d.feed(&packet(1000, 0), t).unwrap().is_empty());
        // 5 wheel revolutions in exactly 1 s -> 5 * 2.0 m = 10 m/s
        let out = d
            .feed(&packet(1005, 1024), t + Duration::from_secs(1))
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, Field::Speed);
        assert!((out[0].1 - 10.0).abs() < 0.01, "{:?}", out);
        // an absurd circumference falls back to the default
        let d2 = CscDecoder::new(f64::NAN);
        assert!((d2.circumference_m - DEFAULT_WHEEL_CIRCUMFERENCE_M).abs() < 1e-12);
    }

    #[test]
    fn truncated_packets_are_errors_not_panics() {
        let mut d = CscDecoder::default();
        let t = Instant::now();
        assert!(d.feed(&[], t).is_err());
        assert!(d.feed(&[0x03, 1, 2], t).is_err());
    }
}
