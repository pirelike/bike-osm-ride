//! The ride loop without any UI: wiring between [`RideEngine`] and the sensor backend.
//!
//! `RideCore` owns the engine and the latest merged sensor sample. Every operation returns
//! an [`Output`]: the new snapshot, the commands to send to the sensor hub (trainer
//! start/pause, simulation parameters, neutral grade on stop), the new history when it
//! grew, and the state transition if there was one. The controller applies the output to
//! the signals and the hub; keeping this part free of Dioxus makes it unit-testable.
//!
//! All calls into the engine are panic-contained: if the engine fails, the core drops it,
//! reports `Output::failure` and continues as "no route".

use std::sync::Arc;
use std::time::{Duration, Instant};

use ride_model::{
    RecordPoint, RideSnapshot, RideState, RideSummary, RiderSettings, Route, SensorCommand,
    SensorSample, TrainerSim, Workout,
};
use ride_sim::RideEngine;

use crate::guard::guarded;

/// Longest wall-clock step fed to the engine in one tick (after a stall the engine
/// sub-steps internally; longer gaps are not simulated).
pub const MAX_DT_S: f64 = 1.0;
/// The trainer simulation parameters are re-sent at least this often (FTMS keep-alive).
pub const SIM_KEEPALIVE: Duration = Duration::from_secs(5);
/// ... and immediately when the grade moved by at least this many percentage points.
pub const SIM_GRADE_STEP: f64 = 0.1;
/// The ERG target is re-sent at least this often.
pub const TARGET_KEEPALIVE: Duration = Duration::from_secs(5);
/// ... and immediately when it moved by at least this many watts.
pub const TARGET_STEP_W: f64 = 1.0;
/// FTMS hard limit used when sanitising what is sent to the trainer.
const MAX_GRADE: f64 = 20.0;

/// Seconds of simulated time for one tick: real elapsed time, capped, times the (test) time scale.
pub fn step_dt(elapsed: Duration, time_scale: f64) -> f64 {
    elapsed.as_secs_f64().min(MAX_DT_S) * time_scale
}

/// Decides when new trainer simulation parameters must go out.
#[derive(Debug, Default)]
pub struct SimGate {
    last: Option<(TrainerSim, Instant)>,
}
impl SimGate {
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// `true` if `sim` has to be sent now: first time, grade moved >= 0.1 %, any other
    /// parameter changed, or the keep-alive interval elapsed.
    pub fn offer(&mut self, sim: TrainerSim, now: Instant) -> bool {
        let send = match &self.last {
            None => true,
            Some((prev, at)) => {
                (sim.grade_pct - prev.grade_pct).abs() >= SIM_GRADE_STEP - 1e-9
                    || (sim.crr - prev.crr).abs() > 1e-6
                    || (sim.cw - prev.cw).abs() > 1e-4
                    || (sim.wind_ms - prev.wind_ms).abs() > 1e-3
                    || now.saturating_duration_since(*at) >= SIM_KEEPALIVE
            }
        };
        if send {
            self.last = Some((sim, now));
        }
        send
    }
}

/// Decides when a new ERG target must be sent to the trainer.
#[derive(Debug, Default)]
pub struct TargetGate {
    last: Option<(f64, Instant)>,
}

impl TargetGate {
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// `true` if the target must be sent now: first time, moved >= 1 W, or keep-alive due.
    pub fn offer(&mut self, target: f64, now: Instant) -> bool {
        let send = match self.last {
            None => true,
            Some((prev, at)) => {
                (target - prev).abs() >= TARGET_STEP_W - 1e-9
                    || now.saturating_duration_since(at) >= TARGET_KEEPALIVE
            }
        };
        if send {
            self.last = Some((target, now));
        }
        send
    }
}

/// Finite values only, grade within the FTMS limits.
pub fn sanitize_sim(sim: TrainerSim) -> TrainerSim {
    let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
    TrainerSim {
        grade_pct: finite(sim.grade_pct, 0.0).clamp(-MAX_GRADE, MAX_GRADE),
        crr: finite(sim.crr, 0.004).clamp(0.0, 0.0255),
        cw: finite(sim.cw, 0.0).clamp(0.0, 2.55),
        wind_ms: finite(sim.wind_ms, 0.0).clamp(-32.0, 32.0),
    }
}

/// Flat road, no wind: what the trainer is told when the ride stops.
pub fn neutral_sim(rider: &RiderSettings) -> TrainerSim {
    sanitize_sim(TrainerSim {
        grade_pct: 0.0,
        crr: rider.crr,
        cw: 0.5 * rider.air_density * rider.cda,
        wind_ms: 0.0,
    })
}

/// Snapshot shown while no route is loaded: only the live sensor values.
pub fn live_snapshot(sample: &SensorSample) -> RideSnapshot {
    RideSnapshot {
        power_w: sample.power_w,
        cadence_rpm: sample.cadence_rpm,
        hr_bpm: sample.hr_bpm,
        resistance: sample.resistance,
        ..RideSnapshot::empty()
    }
}

/// Data kept after a ride ended, for the summary dialog and the TCX export.
#[derive(Debug, Clone)]
pub struct FinishedRide {
    pub summary: RideSummary,
    pub points: Vec<RecordPoint>,
}

/// Result of one core operation.
#[derive(Debug, Clone)]
pub struct Output {
    pub snapshot: RideSnapshot,
    /// To be sent to the sensor hub, in order.
    pub commands: Vec<SensorCommand>,
    /// The full 1 Hz history, present only when it changed.
    pub history: Option<Vec<RecordPoint>>,
    /// `(from, to)` when the ride state changed.
    pub transition: Option<(RideState, RideState)>,
    /// Present on the transition into `Finished`.
    pub finished: Option<FinishedRide>,
    /// The selected workout just ran out of steps (toast once).
    pub workout_finished: bool,
    /// The engine panicked and was dropped; the message is for the log.
    pub failure: Option<String>,
}

pub struct RideCore {
    engine: Option<RideEngine>,
    sample: SensorSample,
    rider: RiderSettings,
    gate: SimGate,
    target_gate: TargetGate,
    workout: Option<Arc<Workout>>,
    workout_finished_emitted: bool,
    state: RideState,
    history_len: usize,
    failure: Option<String>,
    /// Percent of grade the virtual gearbox adds to what the trainer is asked for.
    gear_offset_pct: f64,
}

impl Default for RideCore {
    fn default() -> Self {
        Self::new()
    }
}

impl RideCore {
    pub fn new() -> Self {
        Self {
            engine: None,
            sample: SensorSample::default(),
            rider: RiderSettings::default(),
            gate: SimGate::default(),
            target_gate: TargetGate::default(),
            workout: None,
            workout_finished_emitted: false,
            state: RideState::Empty,
            history_len: 0,
            failure: None,
            gear_offset_pct: 0.0,
        }
    }

    pub fn state(&self) -> RideState {
        self.state
    }

    /// Latest merged sample from the sensor hub.
    pub fn set_sample(&mut self, sample: SensorSample) {
        self.sample = sample;
    }

    /// The virtual gear changed: the trainer gets the shifted grade on the next tick.
    pub fn set_gear_offset(&mut self, percent: f64) {
        let percent = if percent.is_finite() { percent } else { 0.0 };
        if (percent - self.gear_offset_pct).abs() > 1e-9 {
            self.gear_offset_pct = percent;
            self.gate.reset();
        }
    }

    pub fn set_rider(&mut self, rider: RiderSettings) {
        self.rider = rider.clone();
        self.call(|e| e.set_rider(rider));
    }

    /// Selects (or clears) the workout. Ignored during a ride; the caller reports why.
    pub fn set_workout(&mut self, workout: Option<Arc<Workout>>, now: Instant) -> Output {
        if !matches!(self.state, RideState::Riding | RideState::Paused) {
            self.workout = workout;
            let selected = self.workout.clone();
            self.call(|e| e.set_workout(selected));
            self.target_gate.reset();
            self.gate.reset();
            self.workout_finished_emitted = false;
        }
        self.output_after_action(now)
    }

    pub fn workout(&self) -> Option<&Arc<Workout>> {
        self.workout.as_ref()
    }

    /// Sets what is replayed next to the rider: the ghost ride and the per-climb PR rides.
    /// Ignored while riding (the engine keeps the comparison of a running ride stable).
    pub fn set_ghost(&mut self, ghost: Option<ride_sim::ghost::GhostSet>) {
        self.call(|e| e.set_ghost(ghost));
    }

    /// Creates an engine for `route`. Active rides must be finished explicitly first.
    pub fn load(
        &mut self,
        route: Arc<Route>,
        rider: RiderSettings,
        now: Instant,
    ) -> Result<Output, String> {
        if matches!(self.state, RideState::Riding | RideState::Paused) {
            return Err(ride_i18n::t(
                "app.controller.tekeres_kozben_nem_lehet_utvonalat_valtani_elobb_fej",
            )
            .into());
        }
        self.rider = rider.clone();
        let engine = guarded("RideEngine::new", || RideEngine::new(route, rider))?;
        let mut engine = engine;
        engine.set_workout(self.workout.clone());
        self.engine = Some(engine);
        self.failure = None;
        self.gate.reset();
        self.target_gate.reset();
        self.workout_finished_emitted = false;
        self.history_len = 0;
        self.state = RideState::Empty;
        let snapshot = self.call(|e| e.snapshot());
        let mut out = self.output(snapshot, now);
        out.history = Some(Vec::new());
        Ok(out)
    }

    /// Forgets the route (back to the welcome screen).
    pub fn unload(&mut self, now: Instant) -> Output {
        let mut commands = Vec::new();
        if matches!(self.state, RideState::Riding | RideState::Paused) {
            commands.extend(self.stop_commands());
        }
        let from = self.state;
        self.engine = None;
        self.state = RideState::Empty;
        self.history_len = 0;
        self.gate.reset();
        self.target_gate.reset();
        self.workout_finished_emitted = false;
        let mut out = self.output(None, now);
        out.commands = commands;
        out.history = Some(Vec::new());
        out.transition = (from != RideState::Empty).then_some((from, RideState::Empty));
        out
    }

    /// Ready -> Riding or Paused -> Riding. Ignored in every other state.
    pub fn start(&mut self, unix_s: f64, now: Instant) -> Output {
        match self.state {
            RideState::Ready => {
                self.call(|e| e.start(unix_s));
                self.workout_finished_emitted = false;
            }
            RideState::Paused => {
                self.call(|e| e.resume());
            }
            _ => {}
        }
        self.output_after_action(now)
    }

    pub fn pause(&mut self, now: Instant) -> Output {
        if self.state == RideState::Riding {
            self.call(|e| e.pause());
        }
        self.output_after_action(now)
    }

    pub fn finish(&mut self, now: Instant) -> Output {
        if matches!(self.state, RideState::Riding | RideState::Paused) {
            self.call(|e| e.finish());
        }
        self.output_after_action(now)
    }

    /// Back to `Ready` at distance 0 with an empty history.
    pub fn reset(&mut self, now: Instant) -> Output {
        if self.engine.is_some() {
            self.call(|e| e.reset());
        }
        self.target_gate.reset();
        self.gate.reset();
        self.workout_finished_emitted = false;
        self.output_after_action(now)
    }

    /// Jump to `dist_m`; the engine ignores it while riding.
    pub fn seek(&mut self, dist_m: f64, now: Instant) -> Output {
        if dist_m.is_finite() && self.state != RideState::Riding {
            self.call(|e| e.seek(dist_m));
        }
        self.output_after_action(now)
    }

    /// One loop iteration: advance by `dt_s` with the latest sensor sample.
    pub fn tick(&mut self, dt_s: f64, unix_s: f64, now: Instant) -> Output {
        let sample = self.sample;
        let dt = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        let snapshot = self.call(|e| e.step(dt, &sample, unix_s));
        self.output(snapshot, now)
    }

    fn output_after_action(&mut self, now: Instant) -> Output {
        let snapshot = self.call(|e| e.snapshot());
        self.output(snapshot, now)
    }

    /// Runs `f` on the engine, containing panics. `None` = no engine or it just failed.
    fn call<R>(&mut self, f: impl FnOnce(&mut RideEngine) -> R) -> Option<R> {
        let engine = self.engine.as_mut()?;
        match guarded("RideEngine", || f(engine)) {
            Ok(r) => Some(r),
            Err(message) => {
                self.engine = None;
                self.failure = Some(message);
                None
            }
        }
    }

    /// Commands that put the trainer into a neutral state.
    fn stop_commands(&self) -> Vec<SensorCommand> {
        vec![
            SensorCommand::SetSimulation(neutral_sim(&self.rider)),
            SensorCommand::TrainerPause,
        ]
    }

    /// Builds the output for `snapshot` (the engine's answer, or `None` if there is no
    /// engine / it failed) and runs the state-transition logic.
    fn output(&mut self, snapshot: Option<RideSnapshot>, now: Instant) -> Output {
        let snapshot = snapshot.unwrap_or_else(|| live_snapshot(&self.sample));
        let failure = self.failure.take();
        let to = if self.engine.is_some() {
            snapshot.state
        } else {
            RideState::Empty
        };
        let from = self.state;
        let mut commands = Vec::new();
        let mut finished = None;
        let mut transition = None;

        if from != to {
            self.state = to;
            transition = Some((from, to));
            match to {
                RideState::Riding => {
                    commands.push(SensorCommand::TrainerStart);
                    self.gate.reset();
                    self.target_gate.reset();
                }
                RideState::Paused => commands.push(SensorCommand::TrainerPause),
                RideState::Finished => {
                    commands.extend(self.stop_commands());
                    finished = self.call(|e| FinishedRide {
                        summary: e.summary(),
                        points: e.history().to_vec(),
                    });
                }
                RideState::Ready | RideState::Empty => {
                    // After `Finished` the trainer was already neutralised.
                    if matches!(from, RideState::Riding | RideState::Paused) {
                        commands.extend(self.stop_commands());
                    }
                }
            }
        }

        if self.state == RideState::Riding {
            // A workout drives the trainer in ERG mode; without one (or in a free step) the
            // grade simulation takes over.
            let target = self.call(|e| e.workout_target()).flatten();
            if let Some(target) = target {
                if self.target_gate.offer(target, now) {
                    commands.push(SensorCommand::SetTargetPower(target));
                }
                // A workout can end any second: make the next simulation update immediate.
                self.gate.reset();
            } else if let Some(mut sim) = self.call(|e| e.trainer_sim()) {
                sim.grade_pct += self.gear_offset_pct;
                let sim = sanitize_sim(sim);
                if self.gate.offer(sim, now) {
                    commands.push(SensorCommand::SetSimulation(sim));
                }
            }
        }

        let previous_len = self.history_len;
        let history = self
            .call(|e| {
                let h = e.history();
                (h.len() != previous_len).then(|| h.to_vec())
            })
            .flatten();
        if let Some(h) = &history {
            self.history_len = h.len();
        }

        // The engine may have failed in one of the calls above.
        let failure = failure.or_else(|| self.failure.take());
        let (snapshot, transition) = if self.engine.is_none() && self.state != RideState::Empty {
            let from = self.state;
            commands.clear();
            commands.extend(self.stop_commands());
            self.state = RideState::Empty;
            (live_snapshot(&self.sample), Some((from, RideState::Empty)))
        } else {
            (snapshot, transition)
        };

        // The workout ran out of steps during this tick.
        let workout_finished =
            !self.workout_finished_emitted && snapshot.workout.as_ref().is_some_and(|w| w.finished);
        if workout_finished {
            self.workout_finished_emitted = true;
        }

        Output {
            snapshot,
            commands,
            history,
            transition,
            finished,
            workout_finished,
            failure,
        }
    }

    /// Re-sends the simulation parameters on the next tick (a trainer just connected).
    pub fn resend_simulation(&mut self) {
        self.gate.reset();
        self.target_gate.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;

    fn sim(grade: f64) -> TrainerSim {
        TrainerSim {
            grade_pct: grade,
            crr: 0.004,
            cw: 0.2,
            wind_ms: 0.0,
        }
    }

    #[test]
    fn dt_is_capped_and_scaled() {
        assert!((step_dt(Duration::from_millis(100), 1.0) - 0.1).abs() < 1e-12);
        assert_eq!(step_dt(Duration::from_secs(30), 1.0), 1.0);
        assert!((step_dt(Duration::from_millis(100), 60.0) - 6.0).abs() < 1e-9);
        assert_eq!(step_dt(Duration::ZERO, 1.0), 0.0);
    }

    #[test]
    fn gate_sends_first_then_only_on_change_or_keepalive() {
        let t0 = Instant::now();
        let mut gate = SimGate::default();
        assert!(gate.offer(sim(2.0), t0), "first offer is always sent");
        assert!(!gate.offer(sim(2.0), t0 + Duration::from_millis(100)));
        assert!(
            !gate.offer(sim(2.05), t0 + Duration::from_millis(200)),
            "0.05 % is below the step"
        );
        assert!(
            gate.offer(sim(2.1), t0 + Duration::from_millis(300)),
            "0.1 % step"
        );
        assert!(!gate.offer(sim(2.1), t0 + Duration::from_secs(4)));
        assert!(
            gate.offer(sim(2.1), t0 + Duration::from_millis(5_300)),
            "keep-alive after 5 s"
        );
        assert!(!gate.offer(sim(2.1), t0 + Duration::from_millis(5_400)));
    }

    #[test]
    fn gate_notices_other_parameters_and_reset() {
        let t0 = Instant::now();
        let mut gate = SimGate::default();
        assert!(gate.offer(sim(0.0), t0));
        let mut changed = sim(0.0);
        changed.crr = 0.006;
        assert!(gate.offer(changed, t0));
        let mut windy = changed;
        windy.cw = 0.25;
        assert!(gate.offer(windy, t0));
        assert!(!gate.offer(windy, t0));
        gate.reset();
        assert!(gate.offer(windy, t0));
    }

    #[test]
    fn sanitizing_clamps_and_replaces_non_finite_values() {
        let s = sanitize_sim(TrainerSim {
            grade_pct: 35.0,
            crr: f64::NAN,
            cw: -1.0,
            wind_ms: f64::INFINITY,
        });
        assert_eq!(s.grade_pct, 20.0);
        assert_eq!(s.crr, 0.004);
        assert_eq!(s.cw, 0.0);
        assert_eq!(s.wind_ms, 0.0);
        assert_eq!(sanitize_sim(sim(-40.0)).grade_pct, -20.0);
    }

    #[test]
    fn neutral_sim_is_flat_and_uses_the_rider_coefficients() {
        let rider = RiderSettings {
            crr: 0.005,
            cda: 0.4,
            air_density: 1.2,
            ..RiderSettings::default()
        };
        let n = neutral_sim(&rider);
        assert_eq!(n.grade_pct, 0.0);
        assert_eq!(n.crr, 0.005);
        assert!((n.cw - 0.24).abs() < 1e-12);
        assert_eq!(n.wind_ms, 0.0);
    }

    #[test]
    fn live_snapshot_carries_only_sensor_values() {
        let sample = SensorSample {
            power_w: Some(180.0),
            cadence_rpm: Some(85.0),
            hr_bpm: Some(140.0),
            ..Default::default()
        };
        let s = live_snapshot(&sample);
        assert_eq!(s.state, RideState::Empty);
        assert_eq!(
            (s.power_w, s.cadence_rpm, s.hr_bpm),
            (Some(180.0), Some(85.0), Some(140.0))
        );
        assert_eq!(s.dist, 0.0);
    }

    #[test]
    fn empty_core_is_inert() {
        let mut core = RideCore::new();
        let now = Instant::now();
        core.set_sample(SensorSample {
            power_w: Some(200.0),
            ..Default::default()
        });
        for out in [
            core.start(0.0, now),
            core.pause(now),
            core.finish(now),
            core.reset(now),
            core.seek(100.0, now),
            core.tick(0.1, 0.0, now),
        ] {
            assert_eq!(out.snapshot.state, RideState::Empty);
            assert_eq!(
                out.snapshot.power_w,
                Some(200.0),
                "live sensor values are shown without a route"
            );
            assert!(out.commands.is_empty() && out.transition.is_none() && out.finished.is_none());
        }
        assert_eq!(core.state(), RideState::Empty);
    }

    // ───── tests with the real engine (they need the SIM workstream's implementation) ─────

    fn rider() -> RiderSettings {
        RiderSettings::default()
    }

    fn loaded(segments: &[(f64, f64)]) -> (RideCore, Instant) {
        let now = Instant::now();
        let mut core = RideCore::new();
        let out = core
            .load(Arc::new(straight_route(segments, 100.0)), rider(), now)
            .expect("engine for a valid route");
        assert_eq!(out.snapshot.state, RideState::Ready);
        assert_eq!(out.transition, Some((RideState::Empty, RideState::Ready)));
        (core, now)
    }

    #[test]
    fn late_route_load_preserves_a_running_or_paused_ride() {
        let (mut core, now) = loaded(&[(2000.0, 5.0)]);
        core.start(1_700_000_000.0, now);
        for state in [RideState::Riding, RideState::Paused] {
            if state == RideState::Paused {
                core.pause(now);
            }
            let before = core.tick(0.1, 1_700_000_001.0, now).snapshot;
            assert!(core
                .load(
                    Arc::new(straight_route(&[(500.0, 0.0)], 100.0)),
                    rider(),
                    now
                )
                .is_err());
            assert_eq!(core.state(), state);
            let after = core.tick(0.0, 1_700_000_001.0, now).snapshot;
            assert_eq!(after.elapsed_s, before.elapsed_s);
            assert_eq!(after.dist, before.dist);
        }
    }

    #[test]
    fn shifting_changes_the_grade_the_trainer_is_asked_for() {
        let (mut core, t0) = loaded(&[(2000.0, 5.0)]);
        core.set_sample(SensorSample {
            power_w: Some(250.0),
            ..Default::default()
        });
        let grade_of = |out: &Output| {
            out.commands.iter().rev().find_map(|c| match c {
                SensorCommand::SetSimulation(s) => Some(s.grade_pct),
                _ => None,
            })
        };
        core.start(1_700_000_000.0, t0);
        let mut now = t0;
        let mut settled = None;
        for _ in 0..120 {
            now += Duration::from_millis(100);
            let out = core.tick(0.1, 1_700_000_000.0, now);
            settled = grade_of(&out).or(settled);
        }
        let base = settled.expect("a grade was sent");
        core.set_gear_offset(3.0);
        now += Duration::from_millis(100);
        let out = core.tick(0.1, 1_700_000_000.0, now);
        let harder = grade_of(&out).expect("the shift is sent at once");
        assert!((harder - base - 3.0).abs() < 0.3, "{base} -> {harder}");
        core.set_gear_offset(f64::NAN);
        now += Duration::from_millis(100);
        let out = core.tick(0.1, 1_700_000_000.0, now);
        assert!((grade_of(&out).expect("sent") - base).abs() < 0.3);
    }

    #[test]
    fn riding_starts_the_trainer_and_sends_the_grade() {
        let (mut core, t0) = loaded(&[(2000.0, 5.0)]);
        core.set_sample(SensorSample {
            power_w: Some(250.0),
            ..Default::default()
        });

        let out = core.start(1_700_000_000.0, t0);
        assert_eq!(out.snapshot.state, RideState::Riding);
        assert_eq!(out.transition, Some((RideState::Ready, RideState::Riding)));
        assert_eq!(out.commands.first(), Some(&SensorCommand::TrainerStart));
        let sent: Vec<_> = out
            .commands
            .iter()
            .filter(|c| matches!(c, SensorCommand::SetSimulation(_)))
            .collect();
        assert_eq!(
            sent.len(),
            1,
            "simulation parameters go out right at the start"
        );

        // 10 s of riding on a constant 5 % road: once the engine's grade smoothing has settled,
        // at most the keep-alive goes out (not one message per tick).
        let mut late_sims = 0;
        for i in 1..=100u64 {
            let out = core.tick(
                0.1,
                1_700_000_000.0 + i as f64 * 0.1,
                t0 + Duration::from_millis(100 * i),
            );
            if i > 80 {
                late_sims += out
                    .commands
                    .iter()
                    .filter(|c| matches!(c, SensorCommand::SetSimulation(_)))
                    .count();
            }
        }
        assert!(
            late_sims <= 1,
            "no redundant simulation updates on a constant grade, got {late_sims}"
        );
        assert!(
            core.tick(0.1, 0.0, t0 + Duration::from_secs(11))
                .snapshot
                .dist
                > 10.0,
            "250 W moves the rider"
        );
    }

    #[test]
    fn keepalive_resends_the_simulation_every_five_seconds() {
        let (mut core, t0) = loaded(&[(5000.0, 3.0)]);
        core.set_sample(SensorSample {
            power_w: Some(220.0),
            ..Default::default()
        });
        core.start(1.0, t0);
        let mut times = Vec::new();
        for i in 1..=300u64 {
            let out = core.tick(0.1, 1.0, t0 + Duration::from_millis(100 * i));
            if out
                .commands
                .iter()
                .any(|c| matches!(c, SensorCommand::SetSimulation(_)))
            {
                times.push(i);
            }
        }
        // From second 10 to 30 the grade is constant: only keep-alives (every 5 s) remain.
        let keepalives = times.iter().filter(|&&t| t > 100).count();
        assert!((3..=5).contains(&keepalives), "keep-alives: {times:?}");
    }

    #[test]
    fn pause_and_resume_talk_to_the_trainer() {
        let (mut core, t0) = loaded(&[(2000.0, 0.0)]);
        core.start(1.0, t0);
        let paused = core.pause(t0);
        assert_eq!(paused.snapshot.state, RideState::Paused);
        assert_eq!(paused.commands, vec![SensorCommand::TrainerPause]);
        let resumed = core.start(2.0, t0);
        assert_eq!(resumed.snapshot.state, RideState::Riding);
        assert_eq!(resumed.commands.first(), Some(&SensorCommand::TrainerStart));
        assert!(
            resumed
                .commands
                .iter()
                .any(|c| matches!(c, SensorCommand::SetSimulation(_))),
            "grade re-sent after a resume"
        );
    }

    #[test]
    fn finishing_returns_the_trainer_to_a_neutral_grade_and_keeps_the_ride() {
        let (mut core, t0) = loaded(&[(2000.0, 8.0)]);
        core.set_sample(SensorSample {
            power_w: Some(200.0),
            ..Default::default()
        });
        core.start(1_700_000_000.0, t0);
        for i in 1..=50u64 {
            core.tick(
                0.1,
                1_700_000_000.0 + i as f64 * 0.1,
                t0 + Duration::from_millis(100 * i),
            );
        }
        let out = core.finish(t0 + Duration::from_secs(6));
        assert_eq!(out.snapshot.state, RideState::Finished);
        assert_eq!(
            out.transition,
            Some((RideState::Riding, RideState::Finished))
        );
        assert!(
            matches!(out.commands.first(), Some(SensorCommand::SetSimulation(s)) if s.grade_pct == 0.0)
        );
        assert_eq!(out.commands.last(), Some(&SensorCommand::TrainerPause));
        let finished = out.finished.expect("finished ride captured");
        assert_eq!(finished.summary.route_name, "Tesztútvonal");
        assert!(finished.summary.dist > 0.0);
        // Ticking a finished ride changes nothing and emits no more transitions.
        let again = core.tick(0.1, 0.0, t0 + Duration::from_secs(7));
        assert!(again.transition.is_none() && again.finished.is_none());
    }

    #[test]
    fn reaching_the_end_finishes_automatically_exactly_once() {
        let (mut core, t0) = loaded(&[(300.0, 0.0)]);
        core.set_sample(SensorSample {
            power_w: Some(300.0),
            ..Default::default()
        });
        core.start(1_700_000_000.0, t0);
        let mut finished = 0;
        for i in 1..=600u64 {
            let out = core.tick(
                0.5,
                1_700_000_000.0 + i as f64 * 0.5,
                t0 + Duration::from_millis(500 * i),
            );
            if out.finished.is_some() {
                finished += 1;
                assert_eq!(
                    out.transition,
                    Some((RideState::Riding, RideState::Finished))
                );
            }
        }
        assert_eq!(finished, 1);
        assert_eq!(core.state(), RideState::Finished);
    }

    #[test]
    fn reset_returns_to_ready_with_an_empty_history() {
        let (mut core, t0) = loaded(&[(2000.0, 0.0)]);
        core.set_sample(SensorSample {
            power_w: Some(200.0),
            ..Default::default()
        });
        core.start(1.0, t0);
        let mut grew = false;
        for i in 1..=30u64 {
            grew |= core
                .tick(
                    0.1,
                    1.0 + i as f64 * 0.1,
                    t0 + Duration::from_millis(100 * i),
                )
                .history
                .is_some();
        }
        assert!(grew, "the 1 Hz history is published as it grows");
        core.finish(t0 + Duration::from_secs(4));
        let out = core.reset(t0 + Duration::from_secs(5));
        assert_eq!(out.snapshot.state, RideState::Ready);
        assert_eq!(out.history, Some(Vec::new()));
        assert_eq!(out.snapshot.dist, 0.0);
    }

    #[test]
    fn seek_works_only_when_not_riding() {
        let (mut core, t0) = loaded(&[(2000.0, 0.0)]);
        let out = core.seek(1000.0, t0);
        assert!((out.snapshot.dist - 1000.0).abs() < 1.0);
        core.start(1.0, t0);
        let out = core.seek(0.0, t0);
        assert!(out.snapshot.dist > 500.0, "ignored while riding");
        assert_eq!(core.seek(f64::NAN, t0).snapshot.state, RideState::Riding);
    }

    #[test]
    fn unloading_a_running_ride_resets_the_trainer() {
        let (mut core, t0) = loaded(&[(2000.0, 0.0)]);
        core.start(1.0, t0);
        let out = core.unload(t0);
        assert_eq!(out.snapshot.state, RideState::Empty);
        assert_eq!(out.transition, Some((RideState::Riding, RideState::Empty)));
        assert_eq!(out.commands.last(), Some(&SensorCommand::TrainerPause));
        assert_eq!(core.state(), RideState::Empty);
    }

    fn workout() -> Arc<Workout> {
        use ride_model::{StepKind, StepTarget, WorkoutCategory, WorkoutStep};
        Arc::new(Workout {
            id: "core-test".into(),
            name: "Két lépés".into(),
            category: WorkoutCategory::Threshold,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 90,
            duration_min: 2,
            tags: vec![],
            sources: vec![],
            steps: vec![
                WorkoutStep {
                    kind: StepKind::Warmup,
                    name: "Melegítés".into(),
                    duration_s: 60,
                    target: StepTarget::PctFtp { value: 60.0 },
                    cadence: None,
                    note: None,
                    messages: Vec::new(),
                },
                WorkoutStep {
                    kind: StepKind::Interval,
                    name: "Munka".into(),
                    duration_s: 60,
                    target: StepTarget::PctFtp { value: 90.0 },
                    cadence: None,
                    note: None,
                    messages: Vec::new(),
                },
            ],
        })
    }

    #[test]
    fn workout_selection_cannot_diverge_while_paused() {
        let (mut core, now) = loaded(&[(3000.0, 4.0)]);
        core.set_workout(Some(workout()), now);
        core.start(1.0, now);
        core.pause(now + Duration::from_secs(1));
        let out = core.set_workout(None, now + Duration::from_secs(2));
        assert!(core.workout().is_some());
        assert!(out.snapshot.workout.is_some());
    }

    #[test]
    fn a_workout_sends_erg_targets_instead_of_the_grade() {
        let (mut core, t0) = loaded(&[(3000.0, 4.0)]);
        let out = core.set_workout(Some(workout()), t0);
        assert_eq!(
            out.snapshot.workout.as_ref().map(|w| w.step_name.as_str()),
            Some("Melegítés")
        );
        assert!(out.commands.is_empty(), "nothing to send before the start");

        let out = core.start(1.0, t0);
        assert_eq!(out.commands.first(), Some(&SensorCommand::TrainerStart));
        assert!(
            out.commands.contains(&SensorCommand::SetTargetPower(120.0)),
            "the first ERG target goes out at the start: {:?}",
            out.commands
        );
        assert!(
            !out.commands
                .iter()
                .any(|c| matches!(c, SensorCommand::SetSimulation(_))),
            "no grade while a workout drives the trainer: {:?}",
            out.commands
        );

        // 65 s later the second step target is sent.
        let mut saw_second = false;
        for i in 1..=700u64 {
            let out = core.tick(
                0.1,
                1.0 + i as f64 * 0.1,
                t0 + Duration::from_millis(100 * i),
            );
            saw_second |= out.commands.contains(&SensorCommand::SetTargetPower(180.0));
            assert!(
                !out.commands
                    .iter()
                    .any(|c| matches!(c, SensorCommand::SetSimulation(_))),
                "no grade until the workout is over"
            );
        }
        assert!(saw_second, "second step target was sent");

        // A workout cannot be swapped during the ride (the engine ignores it).
        let out = core.set_workout(None, t0 + Duration::from_secs(71));
        assert!(out.snapshot.workout.is_some());
        assert!(core.workout().is_some());

        // Pausing stops the ERG effort exactly like a free ride.
        let out = core.pause(t0 + Duration::from_secs(71));
        assert_eq!(out.commands, vec![SensorCommand::TrainerPause]);
    }

    #[test]
    fn after_the_last_workout_step_the_grade_simulation_returns() {
        let (mut core, t0) = loaded(&[(3000.0, 4.0)]);
        core.set_workout(Some(workout()), t0);
        core.start(1.0, t0);
        let mut switched_back = false;
        let mut finished = false;
        for i in 1..=1400u64 {
            let out = core.tick(
                0.1,
                1.0 + i as f64 * 0.1,
                t0 + Duration::from_millis(100 * i),
            );
            finished |= out.workout_finished;
            switched_back |= out
                .commands
                .iter()
                .any(|c| matches!(c, SensorCommand::SetSimulation(_)));
        }
        assert!(finished, "the workout-finished output fires once");
        assert!(
            switched_back,
            "the trainer returns to grade simulation after the workout"
        );
        let out = core.tick(0.1, 200.0, t0 + Duration::from_secs(200));
        assert!(!out.workout_finished, "the finish is reported once");
        assert!(out.snapshot.workout.as_ref().is_some_and(|w| w.finished));
    }
}
