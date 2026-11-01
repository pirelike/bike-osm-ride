//! The simulated rider (`sim:rider`, "Szimulált tekerő"): a believable human on a smart
//! trainer, good enough to demo the whole app without hardware.
//!
//! * **Power** follows the virtual power the user dials in (↑/↓ keys, `SetVirtualPower`)
//!   with a short lag and a slow wobble of a few percent.
//! * **Cadence** is a function of power and grade (harder effort → higher cadence, climbs
//!   → lower), lags a little, and winds down to 0 when the rider stops pedalling.
//! * **Heart rate** is a lagged first-order response (~25 s up, ~40 s down) towards a
//!   steady-state value that depends on power, plus a slow cardiac drift.
//! * **Speed** comes from the same physics as the ride engine (kinetic-energy integration
//!   with grade, rolling resistance and aero drag from the last `SetSimulation`), so
//!   "use trainer speed" behaves sensibly too.
//!
//! The model is pure: `step(dt)` advances it, nothing here touches time or I/O.

use ride_model::TrainerSim;

/// Rider + bike mass assumed by the simulated trainer, kg.
pub const SIM_MASS_KG: f64 = 84.0;
const DRIVETRAIN_EFF: f64 = 0.975;
const G: f64 = 9.806_65;
const MAX_SPEED_MS: f64 = 30.0;
/// Highest virtual power accepted from the UI, watts.
pub const MAX_VIRTUAL_POWER_W: f64 = 2000.0;

const HR_REST: f64 = 62.0;
const HR_SPAN: f64 = 126.0;
const HR_POWER_SCALE: f64 = 220.0;
const HR_TAU_UP_S: f64 = 25.0;
const HR_TAU_DOWN_S: f64 = 40.0;
const HR_START: f64 = 68.0;

/// What a trainer would report right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimOutput {
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub hr_bpm: f64,
    pub speed_ms: f64,
}

/// xorshift64* — tiny, deterministic, good enough for sensor noise.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // xorshift must not start from zero; every other seed is used as-is so that
        // neighbouring seeds (42, 43) stay distinguishable instead of collapsing to 43.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Approximately N(0, 1) (Irwin–Hall, 12 uniforms).
    fn gauss(&mut self) -> f64 {
        (0..12).map(|_| self.uniform()).sum::<f64>() - 6.0
    }
}

/// Ornstein–Uhlenbeck noise: unit stationary deviation, correlation time `tau`.
#[derive(Debug, Clone, Copy, Default)]
struct Ou(f64);

impl Ou {
    fn step(&mut self, dt: f64, tau: f64, rng: &mut Rng) {
        let a = (-dt / tau).exp();
        self.0 = a * self.0 + (1.0 - a * a).sqrt() * rng.gauss();
    }
}

#[derive(Debug, Clone)]
pub struct SimRider {
    rng: Rng,
    // inputs
    target_power: f64,
    active: bool,
    terrain: TrainerSim,
    // state
    t: f64,
    power: f64,
    power_noise: Ou,
    cadence: f64,
    cad_noise: Ou,
    hr: f64,
    hr_noise: Ou,
    drift: f64,
    kinetic_j: f64,
}

impl SimRider {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            target_power: 0.0,
            active: true,
            terrain: TrainerSim {
                grade_pct: 0.0,
                crr: 0.004,
                cw: 0.5 * 1.225 * 0.32,
                wind_ms: 0.0,
            },
            t: 0.0,
            power: 0.0,
            power_noise: Ou::default(),
            cadence: 0.0,
            cad_noise: Ou::default(),
            hr: HR_START,
            hr_noise: Ou::default(),
            drift: 0.0,
            kinetic_j: 0.0,
        }
    }

    /// The power the virtual rider tries to hold. Non-finite values are ignored, the rest is
    /// clamped to 0..=2000 W.
    pub fn set_power(&mut self, watts: f64) {
        if watts.is_finite() {
            self.target_power = watts.clamp(0.0, MAX_VIRTUAL_POWER_W);
        }
    }

    pub fn target_power(&self) -> f64 {
        self.target_power
    }

    /// `false` while the ride is paused: the rider stops pedalling (power and cadence wind
    /// down to 0), heart rate recovers. The dialled-in power is remembered.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Grade / rolling resistance / wind drag as sent by the ride (`SetSimulation`).
    /// Non-finite values are ignored; the rest is clamped to sane physical ranges.
    pub fn set_terrain(&mut self, sim: TrainerSim) {
        if sim.grade_pct.is_finite()
            && sim.crr.is_finite()
            && sim.cw.is_finite()
            && sim.wind_ms.is_finite()
        {
            self.terrain = TrainerSim {
                grade_pct: sim.grade_pct.clamp(-40.0, 40.0),
                crr: sim.crr.clamp(0.0, 0.05),
                cw: sim.cw.clamp(0.0, 2.55),
                wind_ms: sim.wind_ms.clamp(-30.0, 30.0),
            };
        }
    }

    pub fn terrain(&self) -> TrainerSim {
        self.terrain
    }

    fn effective_target(&self) -> f64 {
        if self.active {
            self.target_power
        } else {
            0.0
        }
    }

    /// Advance by `dt` seconds (clamped to 0..=5 s, integrated in ≤ 50 ms slices).
    pub fn step(&mut self, dt: f64) -> SimOutput {
        let mut remaining = if dt.is_finite() {
            dt.clamp(0.0, 5.0)
        } else {
            0.0
        };
        while remaining > 1e-9 {
            let h = remaining.min(0.05);
            self.substep(h);
            remaining -= h;
        }
        self.output()
    }

    fn substep(&mut self, dt: f64) {
        self.t += dt;
        let target = self.effective_target();

        // power: asymmetric first-order lag towards the target
        let tau = if target > self.power { 0.8 } else { 0.5 };
        self.power += (target - self.power) * (1.0 - (-dt / tau).exp());
        if target <= 0.0 && self.power < 0.5 {
            self.power = 0.0;
        }
        self.power_noise.step(dt, 1.5, &mut self.rng);

        // cadence: steady state from power and grade
        let cad_ss = if target < 8.0 && self.power < 8.0 {
            0.0
        } else {
            let grade = self.terrain.grade_pct;
            let climb = grade.clamp(0.0, 12.0) * 1.2;
            let descent = (-grade).clamp(0.0, 6.0) * 0.5;
            (55.0 + 2.0 * self.power.max(0.0).sqrt() - climb + descent).clamp(40.0, 115.0)
        };
        let cad_tau = if cad_ss > self.cadence { 1.8 } else { 3.0 };
        self.cadence += (cad_ss - self.cadence) * (1.0 - (-dt / cad_tau).exp());
        if cad_ss == 0.0 && self.cadence < 0.5 {
            self.cadence = 0.0;
        }
        self.cad_noise.step(dt, 1.2, &mut self.rng);

        // heart rate: lagged response towards a power-dependent level, plus slow drift
        let hr_ss = HR_REST + HR_SPAN * (1.0 - (-self.power / HR_POWER_SCALE).exp());
        if self.power > 120.0 {
            self.drift = (self.drift + 0.004 * dt).min(10.0);
        } else {
            self.drift = (self.drift - 0.002 * dt).max(0.0);
        }
        let hr_target = hr_ss + self.drift;
        let hr_tau = if hr_target > self.hr {
            HR_TAU_UP_S
        } else {
            HR_TAU_DOWN_S
        };
        self.hr += (hr_target - self.hr) * (1.0 - (-dt / hr_tau).exp());
        self.hr_noise.step(dt, 5.0, &mut self.rng);

        // speed: energy balance, stable at standstill
        let t = &self.terrain;
        let v = (2.0 * self.kinetic_j / SIM_MASS_KG).sqrt();
        let theta = (t.grade_pct / 100.0).atan();
        let f_grav = SIM_MASS_KG * G * theta.sin();
        let f_roll = if v > 0.01 {
            t.crr * SIM_MASS_KG * G * theta.cos()
        } else {
            0.0
        };
        let air = v + t.wind_ms;
        let f_aero = t.cw * air * air.abs();
        let p_wheel = self.power * DRIVETRAIN_EFF;
        self.kinetic_j += (p_wheel - v * (f_grav + f_roll + f_aero)) * dt;
        let e_max = 0.5 * SIM_MASS_KG * MAX_SPEED_MS * MAX_SPEED_MS;
        self.kinetic_j = self.kinetic_j.clamp(0.0, e_max);
    }

    /// The current values with sensor-like noise and resolution (1 W, 0.5 rpm, 1 bpm).
    pub fn output(&self) -> SimOutput {
        let power = if self.power <= 0.0 {
            0.0
        } else {
            (self.power + (0.025 * self.power + 1.5) * self.power_noise.0)
                .max(0.0)
                .round()
        };
        let cadence = if self.cadence < 3.0 {
            0.0
        } else {
            let stroke = 1.2 * (std::f64::consts::TAU * 0.21 * self.t).sin();
            let c = (self.cadence + stroke + 0.5 * self.cad_noise.0).clamp(0.0, 130.0);
            (c * 2.0).round() / 2.0
        };
        SimOutput {
            power_w: power,
            cadence_rpm: cadence,
            hr_bpm: (self.hr + 0.8 * self.hr_noise.0).clamp(45.0, 200.0).round(),
            speed_ms: (2.0 * self.kinetic_j / SIM_MASS_KG).sqrt(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(r: &mut SimRider, seconds: f64) -> SimOutput {
        let mut out = r.output();
        let steps = (seconds / 0.1).round() as usize;
        for _ in 0..steps {
            out = r.step(0.1);
        }
        out
    }

    /// Mean of a field over `seconds` of simulated time.
    fn mean(r: &mut SimRider, seconds: f64, f: impl Fn(&SimOutput) -> f64) -> f64 {
        let n = (seconds / 0.1).round() as usize;
        (0..n).map(|_| f(&r.step(0.1))).sum::<f64>() / n as f64
    }

    #[test]
    fn starts_at_rest() {
        let r = SimRider::new(1);
        let o = r.output();
        assert_eq!(o.power_w, 0.0);
        assert_eq!(o.cadence_rpm, 0.0);
        assert_eq!(o.speed_ms, 0.0);
        assert!((60.0..=75.0).contains(&o.hr_bpm), "{}", o.hr_bpm);
    }

    #[test]
    fn power_follows_a_step_quickly_and_holds_it() {
        let mut r = SimRider::new(7);
        r.set_power(100.0);
        run(&mut r, 8.0);
        let before = mean(&mut r, 10.0, |o| o.power_w);
        assert!((92.0..=108.0).contains(&before), "{before}");
        r.set_power(300.0);
        let after_1s = run(&mut r, 1.0).power_w;
        assert!(
            after_1s > 180.0 && after_1s < 300.0,
            "mid-transition {after_1s}"
        );
        run(&mut r, 5.0);
        let after = mean(&mut r, 10.0, |o| o.power_w);
        assert!((285.0..=315.0).contains(&after), "{after}");
        // the wobble exists but is small
        let samples: Vec<f64> = (0..200).map(|_| r.step(0.1).power_w).collect();
        let (lo, hi) = samples
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(hi - lo >= 4.0, "a rider is not a robot: {lo}..{hi}");
        assert!(hi - lo <= 60.0, "wobble too large: {lo}..{hi}");
    }

    #[test]
    fn cadence_depends_on_power_and_grade() {
        let mut flat = SimRider::new(3);
        flat.set_power(200.0);
        run(&mut flat, 20.0);
        let c_flat = mean(&mut flat, 10.0, |o| o.cadence_rpm);
        assert!((78.0..=92.0).contains(&c_flat), "200 W flat: {c_flat}");

        let mut hard = SimRider::new(3);
        hard.set_power(350.0);
        run(&mut hard, 20.0);
        let c_hard = mean(&mut hard, 10.0, |o| o.cadence_rpm);
        assert!(
            c_hard > c_flat + 5.0,
            "harder effort spins faster: {c_hard} vs {c_flat}"
        );

        let mut climb = SimRider::new(3);
        climb.set_terrain(TrainerSim {
            grade_pct: 8.0,
            crr: 0.004,
            cw: 0.196,
            wind_ms: 0.0,
        });
        climb.set_power(200.0);
        run(&mut climb, 20.0);
        let c_climb = mean(&mut climb, 10.0, |o| o.cadence_rpm);
        assert!(
            c_climb < c_flat - 6.0,
            "8 % climb is ground out slower: {c_climb} vs {c_flat}"
        );
        assert!(c_climb > 55.0, "{c_climb}");
    }

    #[test]
    fn coasting_winds_everything_down() {
        let mut r = SimRider::new(5);
        r.set_power(250.0);
        run(&mut r, 30.0);
        r.set_power(0.0);
        let o = run(&mut r, 20.0);
        assert_eq!(o.power_w, 0.0);
        assert_eq!(o.cadence_rpm, 0.0, "stopped pedalling");
        // somewhere in between right after the drop
        let mut r2 = SimRider::new(5);
        r2.set_power(250.0);
        run(&mut r2, 30.0);
        r2.set_power(0.0);
        let early = run(&mut r2, 1.0);
        assert!(
            early.power_w < 60.0,
            "power falls within a second: {}",
            early.power_w
        );
        assert!(
            early.cadence_rpm > 20.0,
            "cadence lags: {}",
            early.cadence_rpm
        );
    }

    #[test]
    fn pausing_stops_the_rider_and_resuming_restores_the_dialled_power() {
        let mut r = SimRider::new(9);
        r.set_power(200.0);
        run(&mut r, 15.0);
        r.set_active(false);
        let paused = run(&mut r, 15.0);
        assert_eq!(paused.power_w, 0.0);
        assert_eq!(paused.cadence_rpm, 0.0);
        assert_eq!(r.target_power(), 200.0);
        r.set_active(true);
        run(&mut r, 8.0);
        let back = mean(&mut r, 5.0, |o| o.power_w);
        assert!((185.0..=215.0).contains(&back), "{back}");
    }

    #[test]
    fn heart_rate_lags_rises_and_recovers() {
        let mut r = SimRider::new(11);
        let start = r.output().hr_bpm;
        r.set_power(250.0);
        let at_10s = run(&mut r, 10.0).hr_bpm;
        assert!(
            at_10s < start + 40.0,
            "HR must lag behind the effort: {at_10s}"
        );
        assert!(at_10s > start, "but start rising: {at_10s} vs {start}");
        let at_2min = run(&mut r, 110.0).hr_bpm;
        assert!(
            (135.0..=165.0).contains(&at_2min),
            "after 2 min at 250 W: {at_2min}"
        );
        let at_10min = run(&mut r, 480.0).hr_bpm;
        assert!(
            at_10min >= at_2min - 3.0,
            "drift keeps it from falling: {at_10min}"
        );
        assert!(at_10min < 175.0, "{at_10min}");
        r.set_power(0.0);
        let rec_20s = run(&mut r, 20.0).hr_bpm;
        assert!(
            rec_20s > 110.0,
            "recovery is slower than the rise: {rec_20s}"
        );
        let rec_4min = run(&mut r, 220.0).hr_bpm;
        assert!(rec_4min < rec_20s - 20.0, "{rec_4min} vs {rec_20s}");
        assert!(rec_4min >= 58.0, "never far below resting: {rec_4min}");
    }

    #[test]
    fn harder_efforts_mean_higher_steady_heart_rate() {
        let hr_at = |watts: f64| {
            let mut r = SimRider::new(2);
            r.set_power(watts);
            run(&mut r, 300.0);
            mean(&mut r, 5.0, |o| o.hr_bpm)
        };
        let (a, b, c) = (hr_at(100.0), hr_at(200.0), hr_at(350.0));
        assert!(a < b && b < c, "{a} {b} {c}");
        assert!(a > 90.0 && c < 190.0, "{a} {c}");
    }

    #[test]
    fn speed_matches_the_reference_physics() {
        // docs/ARCHITECTURE.md: 84 kg, CdA 0.32, Crr 0.004, eta 0.975, 250 W flat ≈ 36.7 km/h
        let mut flat = SimRider::new(4);
        flat.set_power(250.0);
        run(&mut flat, 240.0);
        let v = mean(&mut flat, 20.0, |o| o.speed_ms);
        assert!((9.6..=10.9).contains(&v), "250 W flat: {} km/h", v * 3.6);
        // 250 W at 8 % ≈ 12.4 km/h
        let mut hill = SimRider::new(4);
        hill.set_terrain(TrainerSim {
            grade_pct: 8.0,
            crr: 0.004,
            cw: 0.196,
            wind_ms: 0.0,
        });
        hill.set_power(250.0);
        run(&mut hill, 240.0);
        let v = mean(&mut hill, 20.0, |o| o.speed_ms);
        assert!((3.2..=3.8).contains(&v), "250 W at 8 %: {} km/h", v * 3.6);
    }

    #[test]
    fn coasting_downhill_accelerates_and_uphill_stops() {
        let mut down = SimRider::new(6);
        down.set_terrain(TrainerSim {
            grade_pct: -8.0,
            crr: 0.004,
            cw: 0.196,
            wind_ms: 0.0,
        });
        down.set_power(0.0);
        // push it off first
        down.set_power(300.0);
        run(&mut down, 30.0);
        down.set_power(0.0);
        let v = run(&mut down, 120.0).speed_ms;
        assert!(
            v > 8.0,
            "free-wheeling down an 8 % descent keeps rolling: {v}"
        );
        let mut up = SimRider::new(6);
        up.set_terrain(TrainerSim {
            grade_pct: 10.0,
            crr: 0.004,
            cw: 0.196,
            wind_ms: 0.0,
        });
        up.set_power(250.0);
        run(&mut up, 60.0);
        up.set_power(0.0);
        assert_eq!(run(&mut up, 60.0).speed_ms, 0.0, "no rolling backwards");
    }

    #[test]
    fn bad_inputs_are_harmless() {
        let mut r = SimRider::new(8);
        r.set_power(f64::NAN);
        assert_eq!(r.target_power(), 0.0);
        r.set_power(-50.0);
        assert_eq!(r.target_power(), 0.0);
        r.set_power(1e9);
        assert_eq!(r.target_power(), MAX_VIRTUAL_POWER_W);
        r.set_terrain(TrainerSim {
            grade_pct: f64::NAN,
            crr: 0.004,
            cw: 0.2,
            wind_ms: 0.0,
        });
        assert_eq!(r.terrain().grade_pct, 0.0, "invalid terrain is ignored");
        r.set_terrain(TrainerSim {
            grade_pct: 500.0,
            crr: 9.0,
            cw: 99.0,
            wind_ms: 1e6,
        });
        let t = r.terrain();
        assert!(t.grade_pct <= 40.0 && t.crr <= 0.05 && t.cw <= 2.55 && t.wind_ms <= 30.0);
        for dt in [0.0, -1.0, f64::NAN, f64::INFINITY, 100.0, 1e-12] {
            let o = r.step(dt);
            assert!(
                o.power_w.is_finite()
                    && o.cadence_rpm.is_finite()
                    && o.hr_bpm.is_finite()
                    && o.speed_ms.is_finite()
            );
            assert!(o.speed_ms <= MAX_SPEED_MS + 1e-9);
        }
    }

    #[test]
    fn deterministic_per_seed() {
        let trace = |seed: u64| {
            let mut r = SimRider::new(seed);
            r.set_power(220.0);
            (0..300).map(|_| r.step(0.1)).collect::<Vec<_>>()
        };
        assert_eq!(trace(42), trace(42));
        assert_ne!(trace(42), trace(43));
    }

    #[test]
    fn output_resolution_matches_real_sensors() {
        let mut r = SimRider::new(12);
        r.set_power(180.0);
        for _ in 0..400 {
            let o = r.step(0.1);
            assert_eq!(o.power_w.fract(), 0.0);
            assert_eq!(o.hr_bpm.fract(), 0.0);
            assert_eq!((o.cadence_rpm * 2.0).fract(), 0.0);
        }
    }
}
