//! Merging the values of all connected sources into one `SensorSample`.
//!
//! Every value is remembered per (field, source) with the time it arrived. The merged
//! sample takes, per field, the first *fresh* source in priority order, so a lost
//! power meter falls back to the trainer and a lost strap to the trainer-reported heart
//! rate. A value that was not refreshed for [`STALE_AFTER`] counts as absent (`None`).
//!
//! Priorities (docs/ARCHITECTURE.md):
//! power: power meter > trainer; cadence: CSC sensor > power-meter cranks > trainer;
//! heart rate: strap > trainer; speed: trainer > CSC wheel sensor.

use std::time::{Duration, Instant};

use ride_model::SensorSample;

/// A value not refreshed for this long is dropped from the merged sample.
pub const STALE_AFTER: Duration = Duration::from_secs(3);

/// Where a value came from (the *role* of the device, not the physical device).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    HrStrap,
    PowerMeter,
    Csc,
    Trainer,
    /// The virtual shifter: it sends steps, never measurements, so no field maps to it.
    Shifter,
}

impl Source {
    const COUNT: usize = 5;
    fn index(self) -> usize {
        match self {
            Source::HrStrap => 0,
            Source::PowerMeter => 1,
            Source::Csc => 2,
            Source::Trainer => 3,
            Source::Shifter => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Power,
    Cadence,
    HeartRate,
    Speed,
    Resistance,
    PowerBalance,
    Energy,
}

impl Field {
    const COUNT: usize = 7;
    fn index(self) -> usize {
        match self {
            Field::Power => 0,
            Field::Cadence => 1,
            Field::HeartRate => 2,
            Field::Speed => 3,
            Field::Resistance => 4,
            Field::PowerBalance => 5,
            Field::Energy => 6,
        }
    }

    /// Sources that may provide this field, best first.
    pub fn priority(self) -> &'static [Source] {
        match self {
            Field::Power => &[Source::PowerMeter, Source::Trainer],
            Field::Cadence => &[Source::Csc, Source::PowerMeter, Source::Trainer],
            Field::HeartRate => &[Source::HrStrap, Source::Trainer],
            Field::Speed => &[Source::Trainer, Source::Csc],
            Field::Resistance => &[Source::Trainer],
            Field::PowerBalance => &[Source::PowerMeter],
            Field::Energy => &[Source::Trainer],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Merger {
    stale_after: Duration,
    cells: [[Option<(f64, Instant)>; Source::COUNT]; Field::COUNT],
}

impl Default for Merger {
    fn default() -> Self {
        Self::new(STALE_AFTER)
    }
}

impl Merger {
    pub fn new(stale_after: Duration) -> Self {
        Self {
            stale_after,
            cells: [[None; Source::COUNT]; Field::COUNT],
        }
    }

    /// Record a fresh value. Non-finite numbers are ignored.
    pub fn set(&mut self, source: Source, field: Field, value: f64, now: Instant) {
        if value.is_finite() {
            self.cells[field.index()][source.index()] = Some((value, now));
        }
    }

    /// Forget everything a source delivered (it disconnected).
    pub fn clear_source(&mut self, source: Source) {
        for row in &mut self.cells {
            row[source.index()] = None;
        }
    }

    pub fn clear_all(&mut self) {
        self.cells = [[None; Source::COUNT]; Field::COUNT];
    }

    fn pick(&self, field: Field, now: Instant) -> Option<f64> {
        field.priority().iter().find_map(|s| {
            self.cells[field.index()][s.index()].and_then(|(v, t)| {
                (now.saturating_duration_since(t) <= self.stale_after).then_some(v)
            })
        })
    }

    pub fn sample(&self, now: Instant) -> SensorSample {
        SensorSample {
            power_w: self.pick(Field::Power, now),
            cadence_rpm: self.pick(Field::Cadence, now),
            hr_bpm: self.pick(Field::HeartRate, now),
            speed_ms: self.pick(Field::Speed, now),
            resistance: self.pick(Field::Resistance, now),
            power_balance_left_pct: self.pick(Field::PowerBalance, now),
            energy_kcal: self.pick(Field::Energy, now),
        }
    }
}

/// Emits a sample only when it changed, and not more often than `min_interval`
/// (the hub ticks at 10 Hz, so there are at most ~10 `Sample` events per second).
#[derive(Debug, Clone)]
pub struct EmitGate {
    min_interval: Duration,
    last_emit: Option<Instant>,
    last: Option<SensorSample>,
}

impl EmitGate {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_emit: None,
            last: None,
        }
    }

    /// `Some(sample)` when it should be sent now.
    pub fn offer(&mut self, sample: SensorSample, now: Instant) -> Option<SensorSample> {
        if self.last == Some(sample) {
            return None;
        }
        if let Some(t) = self.last_emit {
            if now.saturating_duration_since(t) < self.min_interval {
                return None;
            }
        }
        self.last = Some(sample);
        self.last_emit = Some(now);
        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn power_meter_beats_trainer_and_falls_back_when_stale() {
        let base = t0();
        let mut m = Merger::default();
        m.set(Source::Trainer, Field::Power, 200.0, base);
        m.set(Source::PowerMeter, Field::Power, 210.0, base);
        assert_eq!(m.sample(base).power_w, Some(210.0));
        // power meter silent for 2 s, trainer keeps talking
        let later = base + Duration::from_secs(2);
        m.set(Source::Trainer, Field::Power, 205.0, later);
        assert_eq!(
            m.sample(later).power_w,
            Some(210.0),
            "2 s old is still fresh"
        );
        let much_later = base + Duration::from_millis(3500);
        m.set(Source::Trainer, Field::Power, 206.0, much_later);
        assert_eq!(
            m.sample(much_later).power_w,
            Some(206.0),
            "meter is stale, trainer wins"
        );
        // everything stale
        assert_eq!(m.sample(base + Duration::from_secs(10)).power_w, None);
    }

    #[test]
    fn cadence_priority_csc_then_power_meter_then_trainer() {
        let now = t0();
        let mut m = Merger::default();
        m.set(Source::Trainer, Field::Cadence, 80.0, now);
        assert_eq!(m.sample(now).cadence_rpm, Some(80.0));
        m.set(Source::PowerMeter, Field::Cadence, 82.0, now);
        assert_eq!(m.sample(now).cadence_rpm, Some(82.0));
        m.set(Source::Csc, Field::Cadence, 84.0, now);
        assert_eq!(m.sample(now).cadence_rpm, Some(84.0));
        m.clear_source(Source::Csc);
        assert_eq!(m.sample(now).cadence_rpm, Some(82.0));
    }

    #[test]
    fn heart_rate_strap_beats_trainer_hr_and_speed_prefers_trainer() {
        let now = t0();
        let mut m = Merger::default();
        m.set(Source::Trainer, Field::HeartRate, 120.0, now);
        m.set(Source::HrStrap, Field::HeartRate, 140.0, now);
        m.set(Source::Csc, Field::Speed, 8.0, now);
        m.set(Source::Trainer, Field::Speed, 9.0, now);
        let s = m.sample(now);
        assert_eq!(s.hr_bpm, Some(140.0));
        assert_eq!(s.speed_ms, Some(9.0));
        m.clear_source(Source::Trainer);
        assert_eq!(m.sample(now).speed_ms, Some(8.0));
        assert_eq!(m.sample(now).hr_bpm, Some(140.0));
    }

    #[test]
    fn staleness_boundary_is_three_seconds() {
        let base = t0();
        let mut m = Merger::default();
        m.set(Source::Trainer, Field::Resistance, 12.0, base);
        assert_eq!(
            m.sample(base + Duration::from_secs(3)).resistance,
            Some(12.0)
        );
        assert_eq!(
            m.sample(base + Duration::from_millis(3001)).resistance,
            None
        );
    }

    #[test]
    fn non_finite_values_are_ignored_and_fields_are_independent() {
        let now = t0();
        let mut m = Merger::default();
        m.set(Source::Trainer, Field::Power, f64::NAN, now);
        m.set(Source::Trainer, Field::Energy, f64::INFINITY, now);
        m.set(Source::PowerMeter, Field::PowerBalance, 51.0, now);
        let s = m.sample(now);
        assert_eq!(s.power_w, None);
        assert_eq!(s.energy_kcal, None);
        assert_eq!(s.power_balance_left_pct, Some(51.0));
        m.clear_all();
        assert_eq!(m.sample(now), SensorSample::default());
    }

    #[test]
    fn emit_gate_limits_rate_and_skips_unchanged() {
        let base = t0();
        let mut gate = EmitGate::new(Duration::from_millis(90));
        let a = SensorSample {
            power_w: Some(100.0),
            ..Default::default()
        };
        let b = SensorSample {
            power_w: Some(101.0),
            ..Default::default()
        };
        assert_eq!(gate.offer(a, base), Some(a));
        assert_eq!(
            gate.offer(a, base + Duration::from_millis(200)),
            None,
            "unchanged"
        );
        assert_eq!(
            gate.offer(b, base + Duration::from_millis(50)),
            None,
            "too soon"
        );
        assert_eq!(gate.offer(b, base + Duration::from_millis(100)), Some(b));
        // over one simulated second of constant change: never more than 11 events
        let mut count = 0;
        for i in 0..1000u64 {
            let s = SensorSample {
                power_w: Some(i as f64),
                ..Default::default()
            };
            if gate
                .offer(s, base + Duration::from_secs(1) + Duration::from_millis(i))
                .is_some()
            {
                count += 1;
            }
        }
        assert!(count <= 12, "{count} events in 1 s");
    }
}
