//! The running workout: elapsed riding time → current step, target power and countdowns.
//!
//! Pure and clock-agnostic like the engine: the caller feeds the riding time and asks for a
//! [`WorkoutProgress`]. Targets are computed against the rider's **current** FTP, so changing
//! the FTP in the settings retargets the remaining workout immediately.
//!
//! A workout finishes when its last step is behind the rider; from then on
//! [`WorkoutSession::target_w`] is `None`, which the controller reads as "switch the trainer
//! back to simulation".

use std::sync::Arc;

use ride_model::{RiderSettings, StepKind, Workout, WorkoutProgress};

/// The running workout of one ride.
#[derive(Debug, Clone)]
pub struct WorkoutSession {
    workout: Arc<Workout>,
    /// Riding time since the workout started, seconds.
    elapsed_s: f64,
}

impl WorkoutSession {
    pub fn new(workout: Arc<Workout>) -> Self {
        Self {
            workout,
            elapsed_s: 0.0,
        }
    }

    pub fn workout(&self) -> &Workout {
        &self.workout
    }

    pub fn elapsed_s(&self) -> f64 {
        self.elapsed_s
    }

    pub fn total_s(&self) -> f64 {
        f64::from(self.workout.total_seconds())
    }

    pub fn is_finished(&self) -> bool {
        self.elapsed_s >= self.total_s() - 1e-9
    }

    /// Advances the workout by riding time (pauses never reach this).
    pub fn advance(&mut self, dt_s: f64) {
        if dt_s.is_finite() && dt_s > 0.0 {
            self.elapsed_s = (self.elapsed_s + dt_s).min(self.total_s());
        }
    }

    pub fn reset(&mut self) {
        self.elapsed_s = 0.0;
    }

    /// `(index, start_s, step)` of the step containing the current time. At the end the last
    /// step is returned (clamped).
    fn current(&self) -> (usize, f64, &ride_model::WorkoutStep) {
        let mut start = 0.0;
        let last = self.workout.steps.len().saturating_sub(1);
        for (i, step) in self.workout.steps.iter().enumerate() {
            let end = start + f64::from(step.duration_s);
            if self.elapsed_s < end || i == last {
                return (i, start, step);
            }
            start = end;
        }
        // only reachable for an empty workout, which validation rejects
        (0, 0.0, &self.workout.steps[last])
    }

    /// Power the trainer should hold right now; `None` in a free step or after the end.
    pub fn target_w(&self, rider: &RiderSettings) -> Option<f64> {
        if self.is_finished() {
            return None;
        }
        let (_, start, step) = self.current();
        let elapsed_in_step = (self.elapsed_s - start).clamp(0.0, f64::from(step.duration_s));
        step.target
            .watts_at(elapsed_in_step, f64::from(step.duration_s), rider.ftp_w)
    }

    /// Everything the HUD needs.
    pub fn progress(&self, rider: &RiderSettings) -> WorkoutProgress {
        let total = self.total_s().max(1.0);
        let finished = self.is_finished();
        let (index, start, step) = self.current();
        let step_len = f64::from(step.duration_s).max(1.0);
        let elapsed_in_step = if finished {
            step_len
        } else {
            (self.elapsed_s - start).clamp(0.0, step_len)
        };
        let target_w = if finished {
            None
        } else {
            step.target.watts_at(elapsed_in_step, step_len, rider.ftp_w)
        };
        let ftp_pct = target_w.and_then(|w| {
            (rider.ftp_w.is_finite() && rider.ftp_w > 0.0).then_some(w / rider.ftp_w * 100.0)
        });
        let next = self.workout.steps.get(index + 1);
        WorkoutProgress {
            workout_id: self.workout.id.clone(),
            workout_name: self.workout.name.clone(),
            step_index: index,
            step_count: self.workout.steps.len(),
            step_name: step.name.clone(),
            step_kind: step.kind,
            target_w,
            ftp_pct,
            cadence: step.cadence,
            step_elapsed_s: elapsed_in_step,
            step_remaining_s: (step_len - elapsed_in_step).max(0.0),
            step_progress: (elapsed_in_step / step_len).clamp(0.0, 1.0),
            total_elapsed_s: self.elapsed_s.min(total),
            total_remaining_s: (total - self.elapsed_s).max(0.0),
            progress: (self.elapsed_s / total).clamp(0.0, 1.0),
            finished,
            next_step_name: next.map(|s| s.name.clone()),
            next_target_w: next
                .and_then(|s| s.target.watts_at(0.0, f64::from(s.duration_s), rider.ftp_w)),
        }
    }

    /// One step of the profile for the picker's preview: `(step kind, duration, target %FTP)`.
    pub fn preview(&self, rider: &RiderSettings) -> Vec<(StepKind, f64, Option<f64>)> {
        self.workout
            .steps
            .iter()
            .map(|s| {
                (
                    s.kind,
                    f64::from(s.duration_s),
                    s.target.pct_at(
                        f64::from(s.duration_s) * 0.5,
                        f64::from(s.duration_s),
                        rider.ftp_w,
                    ),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{StepTarget, WorkoutCategory, WorkoutStep};

    fn rider(ftp: f64) -> RiderSettings {
        RiderSettings {
            ftp_w: ftp,
            ..RiderSettings::default()
        }
    }

    fn step(name: &str, duration: u32, target: StepTarget) -> WorkoutStep {
        WorkoutStep {
            kind: StepKind::Interval,
            name: name.into(),
            duration_s: duration,
            target,
            cadence: None,
            note: None,
        }
    }

    fn workout() -> Arc<Workout> {
        Arc::new(Workout {
            id: "w".into(),
            name: "Teszt edzés".into(),
            category: WorkoutCategory::Threshold,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 90,
            duration_min: 2,
            tags: vec![],
            sources: vec![],
            steps: vec![
                step("Melegítés", 60, StepTarget::PctFtp { value: 50.0 }),
                step(
                    "Küszöb",
                    60,
                    StepTarget::RampPct {
                        from: 90.0,
                        to: 100.0,
                    },
                ),
            ],
        })
    }

    #[test]
    fn steps_follow_elapsed_time_and_targets_use_the_current_ftp() {
        let mut s = WorkoutSession::new(workout());
        let r = rider(250.0);
        let p = s.progress(&r);
        assert_eq!((p.step_index, p.step_name.as_str()), (0, "Melegítés"));
        assert_eq!(p.target_w, Some(125.0));
        assert_eq!(p.total_remaining_s, 120.0);
        assert!(!p.finished);

        s.advance(30.0);
        assert_eq!(s.progress(&r).target_w, Some(125.0));
        s.advance(30.0);
        let p = s.progress(&r);
        assert_eq!((p.step_index, p.step_name.as_str()), (1, "Küszöb"));
        assert!((p.target_w.unwrap() - 225.0).abs() < 1e-9, "{p:?}");
        // half-way through the ramp
        s.advance(30.0);
        assert!((s.progress(&r).target_w.unwrap() - 237.5).abs() < 1e-9);
        // a new FTP retargets immediately
        assert!((s.progress(&rider(200.0)).target_w.unwrap() - 190.0).abs() < 1e-9);

        // the end: no more target, finished, clamped countdowns
        s.advance(60.0);
        let p = s.progress(&r);
        assert!(p.finished);
        assert_eq!(p.target_w, None);
        assert_eq!(p.total_remaining_s, 0.0);
        assert_eq!(p.step_remaining_s, 0.0);
        assert_eq!(s.target_w(&r), None);
        assert_eq!(p.progress, 1.0);
        // advancing past the end is harmless
        s.advance(1000.0);
        assert_eq!(s.elapsed_s(), 120.0);
    }

    #[test]
    fn step_watts_ramp_test_and_free_steps() {
        let mut s = WorkoutSession::new(Arc::new(Workout {
            id: "ramp".into(),
            name: "Ramp".into(),
            category: WorkoutCategory::Test,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 100,
            duration_min: 2,
            tags: vec![],
            sources: vec![],
            steps: vec![
                step(
                    "Ramp",
                    60,
                    StepTarget::StepWatts {
                        start: 100.0,
                        step: 20.0,
                        step_s: 20.0,
                        max: 500.0,
                    },
                ),
                step("Szabad", 60, StepTarget::Free),
            ],
        }));
        let r = rider(250.0);
        assert_eq!(s.target_w(&r), Some(100.0));
        s.advance(25.0);
        assert_eq!(s.target_w(&r), Some(120.0));
        s.advance(20.0);
        assert_eq!(s.target_w(&r), Some(140.0));
        s.advance(20.0);
        assert_eq!(s.target_w(&r), None, "free step has no ERG target");
        assert_eq!(s.progress(&r).target_w, None);
        assert_eq!(s.progress(&r).next_step_name, None);
    }

    #[test]
    fn reset_starts_over_and_preview_covers_every_step() {
        let mut s = WorkoutSession::new(workout());
        s.advance(90.0);
        s.reset();
        assert_eq!(s.elapsed_s(), 0.0);
        let p = s.progress(&rider(200.0));
        assert_eq!(p.step_index, 0);
        let preview = s.preview(&rider(200.0));
        assert_eq!(preview.len(), 2);
        assert_eq!(preview[0].0, StepKind::Interval);
        assert_eq!(preview[0].1, 60.0);
        assert_eq!(preview[1].2, Some(95.0));
    }

    #[test]
    fn zero_or_nan_steps_are_ignored() {
        let mut s = WorkoutSession::new(workout());
        s.advance(0.0);
        s.advance(f64::NAN);
        s.advance(-5.0);
        assert_eq!(s.elapsed_s(), 0.0);
    }
}
