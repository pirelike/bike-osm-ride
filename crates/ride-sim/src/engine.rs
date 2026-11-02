//! The ride state machine: physics + statistics + 1 Hz recorder + trainer syncing.
//! Pure, synchronous, clock-agnostic: the app owns the wall clock and feeds [`RideEngine::step`].

use std::collections::VecDeque;
use std::sync::Arc;

use ride_model::{
    GhostLive, RecordPoint, RideSnapshot, RideState, RideSummary, RiderSettings, Route, RoutePoint,
    SegmentLive, SensorSample, TrainerSim, Workout,
};

use crate::ghost::{Ghost, GhostSet};
use crate::physics::{self, PhysicsParams, MAX_SPEED_MS};
use crate::workout::WorkoutSession;
use crate::zones;

/// Longest step accepted by one `step` call, s. The caller caps stalls per wall-clock tick;
/// a larger value is legal (it is split into 50 ms sub-steps) so test time-scaling can run a
/// whole ride through quickly.
pub const MAX_STEP_S: f64 = 60.0;
/// Integration sub-step, s.
const SUBSTEP_S: f64 = 0.05;
/// Window of the "3 s average power", s.
const POWER_AVG_WINDOW_S: f64 = 3.0;
/// Trainer grade limits, percent (FTMS inclination range for consumer trainers).
const MAX_TRAINER_GRADE: f64 = 20.0;
/// How fast the simulated grade may move, percentage points per second.
const SIM_SLEW_PCT_PER_S: f64 = 2.0;
/// Look-ahead used to pick the grade for the trainer: average over `[d + near, d + far]`.
const SIM_LOOKAHEAD_NEAR_M: f64 = 5.0;
const SIM_LOOKAHEAD_FAR_M: f64 = 45.0;

fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

fn quantise(v: f64, step: f64) -> f64 {
    if !v.is_finite() || !step.is_finite() || step <= 0.0 {
        return 0.0;
    }
    (v / step).round() * step
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Cumulative ascent/descent at every route point (scaled to the route's own filtered totals,
/// so `ascent_left` never goes negative because of a different smoothing rule).
fn cumulative(points: &[RoutePoint]) -> (Vec<f64>, Vec<f64>) {
    let mut up = Vec::with_capacity(points.len());
    let mut down = Vec::with_capacity(points.len());
    let (mut au, mut ad) = (0.0f64, 0.0f64);
    let mut prev: Option<f64> = None;
    for p in points {
        let ele = finite_or(p.ele, 0.0);
        if let Some(last) = prev {
            let d = ele - last;
            if d > 0.0 {
                au += d;
            } else {
                ad -= d;
            }
        }
        prev = Some(ele);
        up.push(au);
        down.push(ad);
    }
    (up, down)
}

fn scale_to(raw: &mut [f64], target: f64) {
    let total = raw.last().copied().unwrap_or(0.0);
    let factor = if total > 1e-6 && target.is_finite() && target > 0.0 {
        (target / total).clamp(0.0, 20.0)
    } else {
        0.0
    };
    for v in raw.iter_mut() {
        *v = (*v * factor).clamp(0.0, target.max(0.0));
    }
}

/// The ride state machine + physics + statistics + recorder.
pub struct RideEngine {
    route: Arc<Route>,
    rider: RiderSettings,
    params: PhysicsParams,

    state: RideState,
    /// Distance along the route, m.
    dist: f64,
    speed: f64,
    /// Timer time (excludes pauses), s.
    elapsed: f64,
    /// Engine clock, s (advances in every state; used for staleness and the power window).
    clock: f64,
    started_unix: f64,

    // Route profile, pre-scaled so the HUD can show done/left ascent.
    cum_up: Vec<f64>,
    cum_down: Vec<f64>,

    // Latest merged sensor values (sanitised).
    live: SensorSample,
    power_hist: VecDeque<(f64, f64)>,
    power_3s: Option<f64>,

    // Statistics.
    work_j: f64,
    power_time: f64,
    power_integral: f64,
    power_max: Option<f64>,
    hr_time: f64,
    hr_integral: f64,
    hr_max: Option<f64>,
    cadence_time: f64,
    cadence_integral: f64,
    max_speed: f64,
    zone_secs: [f64; 7],
    hr_zone_secs: [f64; 5],

    // Normalised power (Allen: mean of the 4th power of the 30 s rolling average).
    np_window: VecDeque<f64>,
    np_sum4: f64,
    np_count: u64,

    // 1 Hz recording.
    history: Vec<RecordPoint>,
    next_record_t: f64,

    // Trainer.
    sim_grade: f64,

    /// Selected workout (ERG targets) or `None` for a free ride.
    workout: Option<WorkoutSession>,

    // Ghost replay and the live segment (climb) comparison.
    ghost_set: Option<GhostSet>,
    /// The ghost aligned to this ride's start; `None` before the start or when there is none.
    ghost: Option<Ghost>,
    /// `(climb index, elapsed time when its foot was crossed)` while inside that climb.
    segment_entry: Option<(usize, f64)>,
}

impl RideEngine {
    pub fn new(route: Arc<Route>, rider: RiderSettings) -> Self {
        let params = PhysicsParams::from_rider(&rider);
        let (mut up, mut down) = cumulative(&route.points);
        scale_to(&mut up, finite_or(route.ascent, 0.0));
        scale_to(&mut down, finite_or(route.descent, 0.0));
        let mut engine = Self {
            route,
            rider,
            params,
            state: RideState::Ready,
            dist: 0.0,
            speed: 0.0,
            elapsed: 0.0,
            clock: 0.0,
            started_unix: 0.0,
            cum_up: up,
            cum_down: down,
            live: SensorSample::default(),
            power_hist: VecDeque::new(),
            power_3s: None,
            work_j: 0.0,
            power_time: 0.0,
            power_integral: 0.0,
            power_max: None,
            hr_time: 0.0,
            hr_integral: 0.0,
            hr_max: None,
            cadence_time: 0.0,
            cadence_integral: 0.0,
            max_speed: 0.0,
            zone_secs: [0.0; 7],
            hr_zone_secs: [0.0; 5],
            np_window: VecDeque::new(),
            np_sum4: 0.0,
            np_count: 0,
            history: Vec::new(),
            next_record_t: 0.0,
            sim_grade: 0.0,
            workout: None,
            ghost_set: None,
            ghost: None,
            segment_entry: None,
        };
        engine.sim_grade = engine
            .target_grade(0.0)
            .clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
        engine
    }

    pub fn set_rider(&mut self, rider: RiderSettings) {
        self.params = PhysicsParams::from_rider(&rider);
        self.rider = rider;
    }

    /// Selects (or clears) the workout. Ignored while riding: a running effort cannot be
    /// swapped under the rider.
    pub fn set_workout(&mut self, workout: Option<Arc<Workout>>) {
        if self.state == RideState::Riding {
            return;
        }
        self.workout = workout.map(WorkoutSession::new);
    }

    pub fn workout(&self) -> Option<&WorkoutSession> {
        self.workout.as_ref()
    }

    /// Sets (or clears) what is replayed next to the rider: the ghost ride and the per-climb PR
    /// rides. Ignored during a ride (`Riding` / `Paused`): the comparison must not change under
    /// the rider, exactly like the workout. After the finish the new set is stored for the next
    /// ride, but the summary keeps the ghost of the ride that just ended. In `Ready` the ghost
    /// waits on the start line, so it is visible before the ride begins.
    pub fn set_ghost(&mut self, ghost: Option<GhostSet>) {
        if matches!(self.state, RideState::Riding | RideState::Paused) {
            return;
        }
        self.ghost_set = ghost.filter(|set| !set.is_empty());
        self.segment_entry = None;
        if self.state == RideState::Finished {
            return;
        }
        self.align_ghost();
    }

    /// Places the ghost at the rider's current distance: zero difference on the start line.
    fn align_ghost(&mut self) {
        self.ghost = self
            .ghost_set
            .as_ref()
            .and_then(|set| set.main.clone())
            .and_then(|track| Ghost::align(track, self.dist));
    }

    /// The ghost of this ride (aligned), if there is one: it waits on the start line in `Ready`.
    pub fn ghost(&self) -> Option<&Ghost> {
        self.ghost.as_ref()
    }

    /// ERG target of the current workout step, `None` without a workout, before the start,
    /// in a free step or after the last step.
    pub fn workout_target(&self) -> Option<f64> {
        if self.state != RideState::Riding {
            return None;
        }
        self.workout.as_ref().and_then(|w| w.target_w(&self.rider))
    }

    pub fn state(&self) -> RideState {
        self.state
    }

    /// Ready → Riding. `unix_s` = wall clock (for the recording timestamps).
    pub fn start(&mut self, unix_s: f64) {
        if self.state != RideState::Ready {
            return;
        }
        self.state = RideState::Riding;
        self.started_unix = if unix_s.is_finite() { unix_s } else { 0.0 };
        self.elapsed = 0.0;
        self.next_record_t = 0.0;
        if let Some(w) = &mut self.workout {
            w.reset();
        }
        self.sim_grade = self
            .target_grade(self.dist)
            .clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
        // The ghost is aligned to where the ride starts: the difference is zero on the start line.
        self.align_ghost();
        self.segment_entry = None;
        self.record(self.started_unix);
        self.power_3s = self.compute_power_3s();
    }

    pub fn pause(&mut self) {
        if self.state == RideState::Riding {
            self.state = RideState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == RideState::Paused {
            self.state = RideState::Riding;
        }
    }

    /// Riding/Paused → Finished (also called automatically at the end of the route).
    pub fn finish(&mut self) {
        if matches!(self.state, RideState::Riding | RideState::Paused) {
            self.state = RideState::Finished;
        }
    }

    /// Back to Ready at distance 0, history cleared.
    pub fn reset(&mut self) {
        self.state = RideState::Ready;
        self.dist = 0.0;
        self.speed = 0.0;
        self.elapsed = 0.0;
        self.started_unix = 0.0;
        self.work_j = 0.0;
        self.power_time = 0.0;
        self.power_integral = 0.0;
        self.power_max = None;
        self.hr_time = 0.0;
        self.hr_integral = 0.0;
        self.hr_max = None;
        self.cadence_time = 0.0;
        self.cadence_integral = 0.0;
        self.max_speed = 0.0;
        self.zone_secs = [0.0; 7];
        self.hr_zone_secs = [0.0; 5];
        self.np_window.clear();
        self.np_sum4 = 0.0;
        self.np_count = 0;
        self.history.clear();
        self.next_record_t = 0.0;
        self.power_hist.clear();
        self.power_3s = None;
        self.align_ghost();
        self.segment_entry = None;
        if let Some(w) = &mut self.workout {
            w.reset();
        }
        self.sim_grade = self
            .target_grade(0.0)
            .clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
    }

    /// Jump to a distance (allowed in Ready/Paused/Finished; ignored while Riding).
    pub fn seek(&mut self, dist_m: f64) {
        if self.state == RideState::Riding || !dist_m.is_finite() {
            return;
        }
        let total = finite_or(self.route.total_dist, 0.0).max(0.0);
        self.dist = dist_m.clamp(0.0, total);
        self.speed = 0.0;
        self.sim_grade = self
            .target_grade(self.dist)
            .clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
        // A ride that has not started yet replays from wherever it will start.
        if self.state == RideState::Ready {
            self.align_ghost();
        }
    }

    /// Advance by `dt` seconds with the latest sensor values; returns the new snapshot.
    /// In Ready/Paused/Finished the clock and position do not advance but the live
    /// sensor values still appear in the snapshot.
    pub fn step(&mut self, dt_s: f64, input: &SensorSample, unix_s: f64) -> RideSnapshot {
        self.live = sanitize_sample(input);
        let dt = if dt_s.is_finite() {
            dt_s.clamp(0.0, MAX_STEP_S)
        } else {
            0.0
        };
        self.clock += dt;
        self.push_power(self.live.power_w);

        if self.state == RideState::Riding && dt > 0.0 {
            let total = finite_or(self.route.total_dist, 0.0).max(0.0);
            let mut left = dt;
            while left > 1e-9 {
                let h = left.min(SUBSTEP_S);
                let grade = self.route.locate(self.dist).grade;
                let power = self.live.power_w.unwrap_or(0.0).max(0.0);
                let next = if self.rider.use_trainer_speed {
                    finite_or(self.live.speed_ms.unwrap_or(self.speed), self.speed)
                        .clamp(0.0, MAX_SPEED_MS)
                } else {
                    physics::step_speed(self.speed, power, grade, &self.params, h)
                };
                self.speed = next;
                let before_dist = self.dist;
                let before_elapsed = self.elapsed;
                self.dist = (self.dist + next * h).clamp(0.0, total);
                self.elapsed += h;
                self.track_segment(before_dist, before_elapsed);
                if let Some(w) = &mut self.workout {
                    w.advance(h);
                }
                self.accumulate(h, power);
                left -= h;
            }
            self.update_sim_grade(dt);
            self.record(unix_s);
            if total > 0.0 && self.dist >= total - 1e-9 {
                self.dist = total;
                self.speed = 0.0;
                self.state = RideState::Finished;
            }
        }

        self.power_3s = self.compute_power_3s();
        self.snapshot()
    }

    pub fn snapshot(&self) -> RideSnapshot {
        let pos = self.route.locate(self.dist);
        let total = finite_or(self.route.total_dist, 0.0).max(0.0);
        let remaining = (total - self.dist).max(0.0);
        let ascent_done = self.cum_at(&self.cum_up, self.dist);
        let climb = self.route.climb_ahead(self.dist);
        let next_climb_in = climb.map(|i| {
            self.route
                .climbs
                .get(i)
                .map(|c| (c.start_dist - self.dist).max(0.0))
                .unwrap_or(0.0)
        });
        RideSnapshot {
            state: self.state,
            pos,
            speed_ms: self.speed,
            power_w: self.live.power_w,
            power_3s: self.power_3s,
            cadence_rpm: self.live.cadence_rpm,
            hr_bpm: self.live.hr_bpm,
            resistance: self.live.resistance,
            elapsed_s: self.elapsed,
            dist: self.dist,
            remaining_dist: remaining,
            eta_s: self.eta(),
            ascent_done,
            descent_done: self.cum_at(&self.cum_down, self.dist),
            ascent_left: (finite_or(self.route.ascent, 0.0) - ascent_done).max(0.0),
            avg_speed_ms: if self.elapsed > 1e-9 {
                self.dist / self.elapsed
            } else {
                0.0
            },
            max_speed_ms: self.max_speed,
            avg_power_w: self.avg_power(),
            max_power_w: self.power_max,
            normalized_power_w: self.normalized_power(),
            work_kj: self.work_j / 1000.0,
            kcal: self.work_j / 1000.0,
            avg_hr: if self.hr_time > 1e-9 {
                Some(self.hr_integral / self.hr_time)
            } else {
                None
            },
            max_hr: self.hr_max,
            power_zone: self
                .power_3s
                .map(|p| zones::power_zone(p, self.rider.ftp_w))
                .filter(|z| *z > 0),
            hr_zone: self
                .live
                .hr_bpm
                .map(|h| zones::hr_zone(h, self.rider.max_hr))
                .filter(|z| *z > 0),
            next_climb: climb,
            next_climb_in,
            trainer_grade: self.sim_grade,
            workout: self.workout.as_ref().map(|w| w.progress(&self.rider)),
            ghost: self.ghost_live(),
            segment: self.segment_live(),
        }
    }

    /// What the trainer should simulate right now (look-ahead smoothed, scaled, quantised).
    pub fn trainer_sim(&self) -> TrainerSim {
        TrainerSim {
            grade_pct: quantise(
                self.sim_grade.clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE),
                0.01,
            ),
            crr: quantise(finite_or(self.rider.crr, 0.004).clamp(0.0, 0.0255), 0.0001),
            cw: quantise(
                (0.5 * finite_or(self.rider.air_density, 1.225).clamp(0.5, 1.6)
                    * finite_or(self.rider.cda, 0.32).clamp(0.05, 1.5))
                .clamp(0.0, 2.55),
                0.01,
            ),
            wind_ms: 0.0,
        }
    }

    /// 1 Hz recording.
    pub fn history(&self) -> &[RecordPoint] {
        &self.history
    }

    pub fn summary(&self) -> RideSummary {
        let avg_power = self.avg_power();
        let np = self.normalized_power();
        let ftp = finite_or(self.rider.ftp_w, 0.0);
        let intensity_factor = match np {
            Some(v) if ftp > 0.0 => Some(v / ftp),
            _ => None,
        };
        let tss = match (intensity_factor, np) {
            (Some(if_), Some(_)) if ftp > 0.0 && self.elapsed > 0.0 => {
                Some((self.elapsed * if_ * if_ / 3600.0) * 100.0)
            }
            _ => None,
        };
        RideSummary {
            route_name: self.route.name.clone(),
            route_id: self.route.id.clone(),
            started_unix_s: self.started_unix,
            elapsed_s: self.elapsed,
            dist: self.dist,
            ascent: self.cum_at(&self.cum_up, self.dist),
            avg_speed_ms: if self.elapsed > 1e-9 {
                self.dist / self.elapsed
            } else {
                0.0
            },
            max_speed_ms: self.max_speed,
            avg_power_w: avg_power,
            max_power_w: self.power_max,
            normalized_power_w: np,
            intensity_factor,
            tss,
            work_kj: self.work_j / 1000.0,
            kcal: self.work_j / 1000.0,
            avg_hr: if self.hr_time > 1e-9 {
                Some(self.hr_integral / self.hr_time)
            } else {
                None
            },
            max_hr: self.hr_max,
            avg_cadence: if self.cadence_time > 1e-9 {
                Some(self.cadence_integral / self.cadence_time)
            } else {
                None
            },
            power_zone_secs: self.zone_secs,
            hr_zone_secs: self.hr_zone_secs,
        }
    }

    // ───────────────────────────── internals ─────────────────────────────

    fn accumulate(&mut self, h: f64, power: f64) {
        if let Some(raw) = self.live.power_w {
            let p = raw.max(0.0);
            self.power_time += h;
            self.power_integral += p * h;
            self.work_j += p * h;
            self.power_max = Some(self.power_max.map_or(p, |m| m.max(p)));
            let reference = self.power_3s.unwrap_or(p);
            let zone = zones::power_zone(reference, self.rider.ftp_w);
            if (1..=7).contains(&zone) {
                self.zone_secs[(zone - 1) as usize] += h;
            }
        }
        if let Some(hr) = self.live.hr_bpm {
            let hr = hr.max(0.0);
            self.hr_time += h;
            self.hr_integral += hr * h;
            self.hr_max = Some(self.hr_max.map_or(hr, |m| m.max(hr)));
            let zone = zones::hr_zone(hr, self.rider.max_hr);
            if (1..=5).contains(&zone) {
                self.hr_zone_secs[(zone - 1) as usize] += h;
            }
        }
        if let Some(c) = self.live.cadence_rpm {
            if c.is_finite() && c >= 0.0 {
                self.cadence_time += h;
                self.cadence_integral += c * h;
            }
        }
        if self.speed.is_finite() {
            self.max_speed = self.max_speed.max(self.speed);
        }
        let _ = power;
    }

    fn avg_power(&self) -> Option<f64> {
        if self.power_time > 1e-9 {
            Some(self.power_integral / self.power_time)
        } else {
            None
        }
    }

    fn normalized_power(&self) -> Option<f64> {
        if self.np_count == 0 {
            return None;
        }
        let mean4 = self.np_sum4 / self.np_count as f64;
        if mean4 > 0.0 {
            Some(mean4.powf(0.25))
        } else {
            Some(0.0)
        }
    }

    /// Time-weighted average of the samples inside the last 3 s of engine time.
    fn compute_power_3s(&self) -> Option<f64> {
        let now = self.clock;
        let mut sum = 0.0;
        let mut span = 0.0;
        let len = self.power_hist.len();
        for (i, (t, p)) in self.power_hist.iter().enumerate() {
            let next = if i + 1 < len {
                self.power_hist[i + 1].0
            } else {
                now
            };
            let w = (next - t).clamp(0.0, POWER_AVG_WINDOW_S);
            if w <= 0.0 {
                continue;
            }
            sum += p * w;
            span += w;
        }
        if span <= 1e-6 {
            None
        } else {
            Some(sum / span)
        }
    }

    fn push_power(&mut self, power: Option<f64>) {
        let now = self.clock;
        match power.filter(|p| p.is_finite()) {
            Some(p) => {
                let p = p.max(0.0);
                if let Some(back) = self.power_hist.back_mut() {
                    if (back.0 - now).abs() < 1e-9 {
                        back.1 = p;
                        return;
                    }
                }
                self.power_hist.push_back((now, p));
                while let Some(front) = self.power_hist.front() {
                    if now - front.0 > POWER_AVG_WINDOW_S {
                        self.power_hist.pop_front();
                    } else {
                        break;
                    }
                }
            }
            None => {
                // No power at all: drop the old window so the average goes stale.
                while let Some(front) = self.power_hist.front() {
                    if now - front.0 > POWER_AVG_WINDOW_S {
                        self.power_hist.pop_front();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    /// Writes the 1 Hz record (and feeds normalised power). Called at most once per call.
    fn record(&mut self, unix_s: f64) {
        if self.state != RideState::Riding {
            return;
        }
        if self.elapsed + 1e-9 < self.next_record_t {
            return;
        }
        let pos = self.route.locate(self.dist);
        self.history.push(RecordPoint {
            t: self.elapsed,
            unix_s: if unix_s.is_finite() { unix_s } else { 0.0 },
            dist: self.dist,
            lat: pos.lat,
            lon: pos.lon,
            ele: pos.ele,
            speed_ms: self.speed,
            power_w: self.live.power_w,
            cadence_rpm: self.live.cadence_rpm,
            hr_bpm: self.live.hr_bpm,
            grade: pos.grade,
        });
        if let Some(p) = self.live.power_w.filter(|v| v.is_finite()) {
            self.np_window.push_back(p.max(0.0));
            while self.np_window.len() > 30 {
                self.np_window.pop_front();
            }
            if !self.np_window.is_empty() {
                let mean = self.np_window.iter().sum::<f64>() / self.np_window.len() as f64;
                self.np_sum4 += mean.powi(4);
                self.np_count += 1;
            }
        }
        self.next_record_t += 1.0;
        if self.next_record_t <= self.elapsed {
            self.next_record_t = self.elapsed + 1.0;
        }
    }

    fn cum_at(&self, cum: &[f64], d: f64) -> f64 {
        let points = &self.route.points;
        let n = points.len().min(cum.len());
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            return cum[0];
        }
        let i = self.route.index_at(d).min(n - 2);
        let (a, b) = (points[i], points[i + 1]);
        let span = b.dist - a.dist;
        let t = if span > 1e-9 {
            ((d.clamp(a.dist, b.dist) - a.dist) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        lerp(cum[i], cum[i + 1], t)
    }

    /// Average grade over `[from, to]` (sampled every ~50 m; falls back to the end point).
    fn average_grade(&self, from: f64, to: f64) -> f64 {
        let total = finite_or(self.route.total_dist, 0.0);
        if total <= 0.0 {
            return 0.0;
        }
        let a = from.clamp(0.0, total);
        let b = to.clamp(a, total);
        if b - a < 1.0 {
            return self.route.locate(b).grade;
        }
        let n = (((b - a) / 50.0).ceil() as usize).clamp(1, 400);
        let mut sum = 0.0;
        for i in 0..=n {
            let d = a + (b - a) * (i as f64 / n as f64);
            sum += self.route.locate(d).grade;
        }
        sum / (n as f64 + 1.0)
    }

    /// The grade the trainer should be told: smoothed look-ahead, scaled by difficulty.
    fn target_grade(&self, dist: f64) -> f64 {
        let g = self.average_grade(dist + SIM_LOOKAHEAD_NEAR_M, dist + SIM_LOOKAHEAD_FAR_M);
        g * finite_or(self.rider.trainer_grade_scale, 1.0).clamp(0.0, 1.5)
    }

    fn update_sim_grade(&mut self, dt: f64) {
        let target = self
            .target_grade(self.dist)
            .clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
        let max_step = SIM_SLEW_PCT_PER_S * dt.max(0.0);
        let delta = (target - self.sim_grade).clamp(-max_step, max_step);
        self.sim_grade = (self.sim_grade + delta).clamp(-MAX_TRAINER_GRADE, MAX_TRAINER_GRADE);
    }

    /// Records the crossing of a climb foot so the live segment can count the time inside it.
    /// Entry and exit times are interpolated inside the sub-step that crossed the line.
    fn track_segment(&mut self, before_dist: f64, before_elapsed: f64) {
        if self.segment_entry.is_none() {
            let crossed = self.route.climbs.iter().enumerate().find(|(_, climb)| {
                before_dist < climb.start_dist
                    && self.dist >= climb.start_dist
                    && self.dist < climb.end_dist
            });
            if let Some((index, climb)) = crossed {
                let span = self.dist - before_dist;
                let frac = if span > 1e-9 {
                    ((climb.start_dist - before_dist) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                self.segment_entry = Some((
                    index,
                    before_elapsed + (self.elapsed - before_elapsed) * frac,
                ));
            }
        }
        if let Some((index, _)) = self.segment_entry {
            let still_inside = self
                .route
                .climbs
                .get(index)
                .is_some_and(|climb| self.dist < climb.end_dist);
            if !still_inside {
                self.segment_entry = None;
            }
        }
    }

    /// The ghost's position and the time difference, as the ride screen shows them. In `Ready`
    /// the ghost waits on the start line: it is drawn before the ride begins.
    fn ghost_live(&self) -> Option<GhostLive> {
        let ghost = self.ghost.as_ref()?;
        if !matches!(
            self.state,
            RideState::Ready | RideState::Riding | RideState::Paused | RideState::Finished
        ) {
            return None;
        }
        let total = finite_or(self.route.total_dist, 0.0).max(0.0);
        let dist = ghost.dist_at(self.elapsed).clamp(0.0, total);
        Some(GhostLive {
            dist,
            pos: self.route.locate(dist),
            delta_s: ghost.delta_at(self.elapsed, self.dist),
        })
    }

    /// The climb the rider is inside, measured against the PR ride at the same distance.
    fn segment_live(&self) -> Option<SegmentLive> {
        let (index, entry) = self.segment_entry?;
        let climb = self.route.climbs.get(index)?;
        if !(self.dist >= climb.start_dist && self.dist < climb.end_dist) {
            return None;
        }
        let elapsed_s = (self.elapsed - entry).max(0.0);
        let pride = self
            .ghost_set
            .as_ref()
            .and_then(|set| set.segments.get(index))
            .and_then(|value| value.as_ref());
        let (pr_s, delta_s) = match pride {
            Some((pr_s, track)) => {
                let comparable = track.covers(climb.start_dist) && track.covers(self.dist);
                let delta = comparable.then(|| {
                    elapsed_s - (track.time_at(self.dist) - track.time_at(climb.start_dist))
                });
                (Some(*pr_s), delta.filter(|d| d.is_finite()))
            }
            None => (None, None),
        };
        Some(SegmentLive {
            climb: index,
            elapsed_s,
            pr_s,
            delta_s,
        })
    }

    fn eta(&self) -> Option<f64> {
        let total = finite_or(self.route.total_dist, 0.0).max(0.0);
        let remaining = (total - self.dist).max(0.0);
        if remaining <= 0.0 {
            return Some(0.0);
        }
        let reference = self.power_3s.or(self.live.power_w).filter(|p| *p > 0.0);
        let speed = match reference {
            Some(p) => {
                let grade = self.average_grade(self.dist, (self.dist + 1000.0).min(total));
                physics::steady_state_speed(p, grade, &self.params)
            }
            None => self.speed,
        };
        if speed.is_finite() && speed > 0.5 {
            Some(remaining / speed.min(MAX_SPEED_MS))
        } else {
            None
        }
    }
}

/// Drops non-finite sensor values and clamps physically impossible ones.
fn sanitize_sample(s: &SensorSample) -> SensorSample {
    let sane = |v: Option<f64>, min: f64, max: f64| -> Option<f64> {
        v.filter(|x| x.is_finite()).map(|x| x.clamp(min, max))
    };
    SensorSample {
        power_w: sane(s.power_w, -2000.0, 3000.0),
        cadence_rpm: sane(s.cadence_rpm, 0.0, 260.0),
        hr_bpm: sane(s.hr_bpm, 0.0, 260.0),
        speed_ms: sane(s.speed_ms, 0.0, MAX_SPEED_MS),
        resistance: sane(s.resistance, 0.0, 100.0),
        power_balance_left_pct: sane(s.power_balance_left_pct, 0.0, 100.0),
        energy_kcal: sane(s.energy_kcal, 0.0, 100_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghost::Track;
    use ride_model::testing::straight_route;

    fn engine(segments: &[(f64, f64)]) -> RideEngine {
        RideEngine::new(
            Arc::new(straight_route(segments, 100.0)),
            RiderSettings::default(),
        )
    }

    fn sample(power: f64) -> SensorSample {
        SensorSample {
            power_w: Some(power),
            cadence_rpm: Some(90.0),
            hr_bpm: Some(140.0),
            ..SensorSample::default()
        }
    }

    #[test]
    fn starts_ready_and_only_start_moves() {
        let mut e = engine(&[(5000.0, 0.0)]);
        assert_eq!(e.state(), RideState::Ready);
        let snap = e.step(1.0, &sample(200.0), 0.0);
        assert_eq!(snap.state, RideState::Ready);
        assert_eq!(snap.dist, 0.0);
        assert_eq!(
            snap.power_w,
            Some(200.0),
            "live values still show while ready"
        );

        e.start(1_700_000_000.0);
        assert_eq!(e.state(), RideState::Riding);
        for _ in 0..30 {
            e.step(0.1, &sample(200.0), 1_700_000_001.0);
        }
        let snap = e.snapshot();
        assert!(snap.dist > 5.0 && snap.dist < 40.0, "dist {}", snap.dist);
        assert!(snap.speed_ms > 3.0, "speed {}", snap.speed_ms);
        assert!(e.history().len() >= 2, "1 Hz recording started");
    }

    #[test]
    fn pause_freezes_the_timer_and_position() {
        let mut e = engine(&[(5000.0, 0.0)]);
        e.start(0.0);
        for _ in 0..20 {
            e.step(0.1, &sample(250.0), 0.0);
        }
        let moving = e.snapshot().dist;
        assert!(moving > 0.0);
        e.pause();
        for _ in 0..20 {
            e.step(0.1, &sample(250.0), 0.0);
        }
        let paused = e.snapshot();
        assert_eq!(paused.dist, moving);
        assert!(paused.elapsed_s > 0.0);
        e.resume();
        e.step(0.1, &sample(250.0), 0.0);
        assert!(e.snapshot().dist > moving);
    }

    #[test]
    fn auto_finishes_at_the_end_of_the_route() {
        let mut e = engine(&[(300.0, 0.0)]);
        e.start(0.0);
        let mut last = e.snapshot();
        for _ in 0..2000 {
            last = e.step(0.1, &sample(400.0), 0.0);
            if last.state == RideState::Finished {
                break;
            }
        }
        assert_eq!(last.state, RideState::Finished);
        assert!((last.dist - 300.0).abs() < 1e-6);
        assert_eq!(last.remaining_dist, 0.0);
        assert_eq!(last.eta_s, Some(0.0));
        assert_eq!(last.speed_ms, 0.0);
    }

    #[test]
    fn records_at_one_hertz_and_counts_zones() {
        let mut e = engine(&[(2000.0, 2.0)]);
        e.start(1_000.0);
        for _ in 0..300 {
            e.step(0.1, &sample(250.0), 1_000.0);
        }
        let hist = e.history();
        assert!(hist.len() >= 25, "recorded {} points", hist.len());
        assert!(hist.windows(2).all(|w| w[1].t >= w[0].t));
        let s = e.summary();
        assert!((s.elapsed_s - 30.0).abs() < 0.5);
        assert!(s.avg_power_w.unwrap() > 240.0);
        assert_eq!(s.max_power_w, Some(250.0));
        let zone_total: f64 = s.power_zone_secs.iter().sum();
        assert!((zone_total - s.elapsed_s).abs() < 0.5);
        assert!(s.tss.unwrap() > 0.0);
        assert!(s.normalized_power_w.unwrap() > 200.0);
    }

    #[test]
    fn seek_is_ignored_while_riding() {
        let mut e = engine(&[(5000.0, 0.0)]);
        e.start(0.0);
        e.step(0.5, &sample(200.0), 0.0);
        let before = e.snapshot().dist;
        e.seek(4000.0);
        assert_eq!(e.snapshot().dist, before);

        e.pause();
        e.seek(4000.0);
        assert!((e.snapshot().dist - 4000.0).abs() < 1e-6);
        assert!((e.snapshot().remaining_dist - 1000.0).abs() < 1e-6);
        e.seek(-100.0);
        assert_eq!(e.snapshot().dist, 0.0);
        e.seek(1e9);
        assert!((e.snapshot().dist - 5000.0).abs() < 1e-6);
    }

    #[test]
    fn trainer_sim_is_quantised_and_slew_limited() {
        let mut e = RideEngine::new(
            Arc::new(straight_route(&[(2000.0, 12.0)], 100.0)),
            RiderSettings::default(),
        );
        e.start(0.0);
        let mut prev = e.trainer_sim().grade_pct;
        for _ in 0..300 {
            e.step(0.1, &sample(300.0), 0.0);
            let sim = e.trainer_sim();
            assert!(sim.grade_pct.abs() <= 20.0 + 1e-9);
            assert!((sim.grade_pct - (sim.grade_pct * 100.0).round() / 100.0).abs() < 1e-9);
            assert!((sim.grade_pct - prev).abs() <= 2.0 * 0.1 + 0.011, "slew");
            prev = sim.grade_pct;
        }
        assert!(prev > 5.0, "grade should have climbed, got {prev}");
    }

    #[test]
    fn trainer_grade_scale_and_clamp() {
        let rider = RiderSettings {
            trainer_grade_scale: 0.5,
            ..RiderSettings::default()
        };
        let mut e = RideEngine::new(Arc::new(straight_route(&[(3000.0, 10.0)], 0.0)), rider);
        e.start(0.0);
        for _ in 0..600 {
            e.step(0.1, &sample(250.0), 0.0);
        }
        let g = e.trainer_sim().grade_pct;
        assert!((g - 5.0).abs() < 1.0, "half of 10 % → {g}");
        assert!(e.snapshot().trainer_grade <= 5.0 + 0.5);
    }

    #[test]
    fn reset_clears_everything() {
        let mut e = engine(&[(2000.0, 1.0)]);
        e.start(0.0);
        for _ in 0..100 {
            e.step(0.1, &sample(220.0), 0.0);
        }
        e.reset();
        let s = e.snapshot();
        assert_eq!(s.state, RideState::Ready);
        assert_eq!(s.dist, 0.0);
        assert_eq!(s.elapsed_s, 0.0);
        assert!(e.history().is_empty());
        assert_eq!(s.avg_power_w, None);
        assert_eq!(s.work_kj, 0.0);
    }

    #[test]
    fn ascent_done_tracks_the_profile_and_never_exceeds_the_total() {
        let mut e = engine(&[(200.0, 0.0), (2000.0, 6.0)]);
        e.start(0.0);
        let mut last = 0.0;
        for _ in 0..900 {
            let s = e.step(0.1, &sample(250.0), 0.0);
            assert!(s.ascent_done >= last - 1e-9, "monotone ascent");
            assert!(s.ascent_left >= 0.0);
            last = s.ascent_done;
        }
        let s = e.snapshot();
        assert!(s.ascent_done > 10.0, "ascent {}", s.ascent_done);
        assert!(s.ascent_done <= 120.0 + 1e-6, "ascent {}", s.ascent_done);
        assert!((s.ascent_done + s.ascent_left - 120.0).abs() < 1e-6);
        assert!((e.summary().ascent - s.ascent_done).abs() < 1e-6);
    }

    // ───── ghost and segments ─────

    fn record_point(t: f64, dist: f64) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0,
            speed_ms: 10.0,
            power_w: Some(200.0),
            cadence_rpm: Some(85.0),
            hr_bpm: Some(140.0),
            grade: 0.0,
        }
    }

    /// A constant-speed trace up to `len_m` metres.
    fn trace(len_m: f64, speed: f64) -> Track {
        let n = (len_m / speed).ceil() as usize;
        let points: Vec<RecordPoint> = (0..=n)
            .map(|i| record_point(i as f64, (i as f64 * speed).min(len_m)))
            .collect();
        Track::from_points(&points).expect("a usable trace")
    }

    #[test]
    fn the_ghost_is_aligned_at_the_start_and_runs_ahead() {
        let mut e = engine(&[(2_000.0, 0.0)]);
        // 15 m/s (54 km/h): faster than the 300 W rider, so the rider falls behind
        e.set_ghost(Some(GhostSet {
            main: Some(trace(2_000.0, 15.0)),
            segments: vec![],
        }));
        // before the start the ghost waits on the start line, so the rider sees that it is there
        let waiting = e.snapshot().ghost.expect("waiting at the start");
        assert_eq!(waiting.dist, 0.0);
        assert_eq!(waiting.delta_s, Some(0.0));

        e.start(0.0);
        let first = e.snapshot().ghost.expect("aligned at the start line");
        assert_eq!(first.dist, 0.0);
        assert_eq!(first.delta_s, Some(0.0));

        for _ in 0..300 {
            e.step(0.1, &sample(300.0), 0.0);
        }
        let snapshot = e.snapshot();
        let ghost = snapshot.ghost.expect("still there");
        // 30 s at 15 m/s: the ghost is at 450 m
        assert!((ghost.dist - 450.0).abs() < 15.0, "ghost at {}", ghost.dist);
        let delta = ghost.delta_s.expect("covered");
        assert!(delta > 0.0, "the rider is behind: {delta}");
        assert!(delta < 60.0, "but not a whole ghost ride behind: {delta}");
        // the ghost marker is on the route
        assert!((ghost.pos.dist - ghost.dist).abs() < 1e-6);
    }

    #[test]
    fn the_ghost_freezes_while_paused_and_follows_a_time_scaled_step() {
        let mut e = engine(&[(5_000.0, 0.0)]);
        e.set_ghost(Some(GhostSet {
            main: Some(trace(5_000.0, 7.0)),
            segments: vec![],
        }));
        e.start(0.0);
        e.step(0.1, &sample(250.0), 0.0);
        let moving = e.snapshot().ghost.expect("ghost").dist;

        e.pause();
        for _ in 0..50 {
            let snapshot = e.step(0.1, &sample(250.0), 0.0);
            assert_eq!(
                snapshot.ghost.expect("ghost").dist,
                moving,
                "a pause stops the ghost too"
            );
        }

        e.resume();
        // one time-scaled step of a minute: the ghost moves 60 s × 7 m/s
        let snapshot = e.step(60.0, &sample(250.0), 0.0);
        let ghost = snapshot.ghost.expect("ghost");
        assert!(
            (ghost.dist - (moving + 420.0)).abs() < 1.0,
            "ghost at {}",
            ghost.dist
        );
    }

    #[test]
    fn a_ghost_cannot_be_swapped_under_a_running_ride() {
        let mut e = engine(&[(1_000.0, 0.0)]);
        e.set_ghost(Some(GhostSet {
            main: Some(trace(1_000.0, 5.0)),
            segments: vec![],
        }));
        e.start(0.0);
        e.step(0.1, &sample(200.0), 0.0);
        e.set_ghost(Some(GhostSet {
            main: Some(trace(1_000.0, 20.0)),
            segments: vec![],
        }));
        let running = e.snapshot().ghost.expect("kept while riding");
        assert!(running.dist > 0.0, "the running ghost is not replaced");
        e.pause();
        e.set_ghost(None);
        assert!(
            e.snapshot().ghost.is_some(),
            "a pause does not swap the ghost either"
        );
        assert!(e.snapshot().segment.is_none());
    }

    #[test]
    fn a_finished_ride_keeps_its_ghost() {
        let mut e = engine(&[(1_000.0, 0.0)]);
        e.set_ghost(Some(GhostSet {
            main: Some(trace(1_000.0, 10.0)),
            segments: vec![],
        }));
        e.start(0.0);
        for _ in 0..20 {
            e.step(0.1, &sample(250.0), 0.0);
        }
        e.finish();
        let finished = e
            .snapshot()
            .ghost
            .expect("the ghost stays after the finish");
        // the history is reloaded when the ride has been stored; the summary keeps its ghost
        e.set_ghost(None);
        let kept = e.snapshot().ghost.expect("kept after the finish");
        assert_eq!(kept.dist, finished.dist);
        // ... but the next ride starts without one, because the reloaded set was empty
        e.reset();
        assert!(e.snapshot().ghost.is_none());
    }

    #[test]
    fn a_ghost_waits_where_the_ride_will_start() {
        let mut e = engine(&[(2_000.0, 0.0)]);
        e.seek(400.0);
        e.set_ghost(Some(GhostSet {
            main: Some(trace(2_000.0, 10.0)),
            segments: vec![],
        }));
        let ghost = e.snapshot().ghost.expect("waiting where the ride starts");
        assert!((ghost.dist - 400.0).abs() < 1e-6, "ghost at {}", ghost.dist);
        assert_eq!(ghost.delta_s, Some(0.0));
        // the start aligns to the same place: zero difference on the start line
        e.start(0.0);
        assert_eq!(e.snapshot().ghost.expect("aligned").delta_s, Some(0.0));
    }

    #[test]
    fn the_live_segment_counts_time_inside_the_climb_against_the_pr() {
        let mut route = straight_route(&[(2_000.0, 0.0)], 100.0);
        route.climbs = vec![ride_model::Climb {
            start_dist: 500.0,
            end_dist: 1_500.0,
            length: 1_000.0,
            gain: 50.0,
            avg_grade: 5.0,
            max_grade: 5.0,
            start_ele: 100.0,
            end_ele: 150.0,
            category: None,
        }];
        let mut e = RideEngine::new(Arc::new(route), RiderSettings::default());
        // the PR ride rode the whole route at 5 m/s: 200 s for the climb
        e.set_ghost(Some(GhostSet {
            main: None,
            segments: vec![Some((200.0, trace(2_000.0, 5.0)))],
        }));
        e.start(0.0);

        // before the climb there is no segment
        let snapshot = e.step(0.1, &sample(300.0), 0.0);
        assert!(snapshot.segment.is_none());

        let mut seen = None;
        for _ in 0..600 {
            let snapshot = e.step(0.1, &sample(300.0), 0.0);
            if let Some(segment) = snapshot.segment {
                seen = Some((snapshot.dist, segment));
            }
            if snapshot.dist > 1_200.0 {
                break;
            }
        }
        let (dist, segment) = seen.expect("inside the climb");
        assert!(dist > 500.0 && dist < 1_500.0, "dist {dist}");
        assert_eq!(segment.climb, 0);
        assert_eq!(segment.pr_s, Some(200.0));
        // the rider is much faster than 5 m/s, so the difference is negative (ahead)
        let delta = segment.delta_s.expect("comparable");
        assert!(delta < 0.0, "ahead of the PR: {delta}");
        // the time inside the climb is about the time it took to cover (dist - 500) m
        assert!((segment.elapsed_s - (dist - 500.0) / e.snapshot().avg_speed_ms).abs() < 6.0);

        // after the summit the segment is gone again
        for _ in 0..600 {
            let snapshot = e.step(0.1, &sample(300.0), 0.0);
            if snapshot.dist > 1_600.0 {
                assert!(snapshot.segment.is_none(), "segment ended at the summit");
                break;
            }
        }
    }

    #[test]
    fn no_route_points_does_not_panic() {
        let mut route = straight_route(&[], 0.0);
        route.points.clear();
        route.total_dist = 0.0;
        let mut e = RideEngine::new(Arc::new(route), RiderSettings::default());
        e.start(0.0);
        for _ in 0..10 {
            e.step(0.1, &sample(200.0), 0.0);
        }
        let s = e.snapshot();
        assert_eq!(s.dist, 0.0);
        assert_eq!(s.ascent_done, 0.0);
        let _ = e.summary();
    }

    #[test]
    fn trainer_speed_mode_uses_the_sensor_speed() {
        let rider = RiderSettings {
            use_trainer_speed: true,
            ..RiderSettings::default()
        };
        let mut e = RideEngine::new(Arc::new(straight_route(&[(1000.0, 0.0)], 0.0)), rider);
        e.start(0.0);
        let s = e.step(
            1.0,
            &SensorSample {
                speed_ms: Some(8.0),
                ..SensorSample::default()
            },
            0.0,
        );
        assert!((s.speed_ms - 8.0).abs() < 1e-6);
        assert!((s.dist - 8.0).abs() < 0.5);
    }

    #[test]
    fn a_workout_advances_with_the_ride_and_resets() {
        use ride_model::{StepKind, StepTarget, Workout, WorkoutCategory, WorkoutStep};
        let workout = Arc::new(Workout {
            id: "engine-test".into(),
            name: "Motor teszt".into(),
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
        });
        let mut e = engine(&[(3000.0, 0.0)]);
        e.set_workout(Some(workout));
        assert!(e.workout().is_some());
        assert_eq!(e.workout_target(), None, "no target before the start");
        e.start(0.0);
        assert_eq!(e.workout_target(), Some(120.0));
        let s = e.step(0.1, &sample(150.0), 0.0);
        let p = s.workout.expect("progress in the snapshot");
        assert_eq!(p.step_index, 0);
        assert_eq!(p.workout_name, "Motor teszt");
        for _ in 0..650 {
            e.step(0.1, &sample(150.0), 0.0);
        }
        let p = e.snapshot().workout.expect("progress");
        assert_eq!(p.step_index, 1);
        assert_eq!(e.workout_target(), Some(180.0));
        // after the end there is no ERG target and the snapshot says it is done
        for _ in 0..800 {
            e.step(0.1, &sample(150.0), 0.0);
        }
        assert_eq!(e.workout_target(), None);
        assert!(e.snapshot().workout.expect("progress").finished);

        e.reset();
        let p = e.snapshot().workout.expect("progress");
        assert_eq!((p.total_elapsed_s, p.finished), (0.0, false));
        // a workout cannot be swapped under a running ride
        e.start(0.0);
        e.set_workout(None);
        assert!(e.workout().is_some(), "ignored while riding");
    }
}
