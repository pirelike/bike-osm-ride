//! Power → speed model.
//!
//! The road-cycling power model of Martin et al. (1998, J. Appl. Biomech. 14:276–291, validated
//! against SRM power on the road: R² = 0.97, SE 2.7 W):
//! `P·η = (F_grav + F_roll + F_bearing + F_aero)·v + m_eff·v·dv/dt`, with the wheel-bearing
//! friction `(91 + 8.7·v)·10⁻³ N` and the spoke drag area `F_w = 0.0044 m²` added to CdA.
//! Integrated in kinetic energy: that form stays stable around `v ≈ 0`, where the force form
//! would divide by zero. Cross-checked against the open-source `rouleur` implementation of the
//! same paper (see the tests). Reference values: 75 kg rider + 9 kg bike, CdA 0.32, Crr 0.004,
//! η 0.975, ρ 1.225 → 250 W on the flat ≈ 36.3 km/h, at 8 % ≈ 12.3 km/h.

use ride_model::RiderSettings;

/// Hard speed cap, m/s (108 km/h).
pub const MAX_SPEED_MS: f64 = 30.0;
/// Rotating-mass equivalent of the wheels, kg (adds inertia, not weight).
pub const ROTATIONAL_MASS_KG: f64 = 1.5;
/// Gravity, m/s².
pub const GRAVITY: f64 = 9.806_65;
/// Largest integration step, s (the engine sub-steps internally).
const SUBSTEP_S: f64 = 0.05;
/// Wheel-bearing friction of Martin et al. (1998): `BEARING_N + BEARING_NS_PER_M · v` newtons.
pub const BEARING_N: f64 = 0.091;
pub const BEARING_NS_PER_M: f64 = 0.0087;
/// Drag area of the spinning spokes (Martin et al. 1998), m², added to the rider's CdA.
pub const SPOKE_DRAG_M2: f64 = 0.0044;
/// Grades beyond this magnitude are meaningless (and numerically hostile), percent.
const MAX_GRADE_PCT: f64 = 30.0;

/// Parameters of the power → speed model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsParams {
    /// Rider + bike, kg.
    pub total_mass_kg: f64,
    /// Equivalent mass of the rotating wheels, kg (adds inertia).
    pub rotational_mass_kg: f64,
    pub cda: f64,
    pub crr: f64,
    pub drivetrain_eff: f64,
    pub air_density: f64,
    /// Head wind positive, m/s.
    pub wind_ms: f64,
}

fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Speed clamped to the physically meaningful range, NaN treated as 0.
pub fn sanitize_speed(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, MAX_SPEED_MS)
    } else {
        0.0
    }
}

impl PhysicsParams {
    /// Builds the parameters from the rider profile, clamping nonsense values.
    pub fn from_rider(rider: &RiderSettings) -> Self {
        Self {
            total_mass_kg: finite_or(rider.rider_kg, 75.0).clamp(20.0, 250.0)
                + finite_or(rider.bike_kg, 9.0).clamp(0.0, 40.0),
            rotational_mass_kg: ROTATIONAL_MASS_KG,
            cda: finite_or(rider.cda, 0.32).clamp(0.05, 1.5),
            crr: finite_or(rider.crr, 0.004).clamp(0.0, 0.05),
            drivetrain_eff: finite_or(rider.drivetrain_eff, 0.975).clamp(0.5, 1.0),
            air_density: finite_or(rider.air_density, 1.225).clamp(0.5, 1.6),
            wind_ms: 0.0,
        }
    }

    /// Mass the acceleration term sees (rider + bike + wheels).
    pub fn effective_mass_kg(&self) -> f64 {
        (finite_or(self.total_mass_kg, 84.0) + finite_or(self.rotational_mass_kg, 0.0))
            .clamp(1.0, 500.0)
    }

    fn grade_angle(&self, grade_pct: f64) -> f64 {
        (finite_or(grade_pct, 0.0).clamp(-MAX_GRADE_PCT, MAX_GRADE_PCT) / 100.0).atan()
    }

    /// Downhill-component of gravity, N (negative when the road descends).
    pub fn gravity_force(&self, grade_pct: f64) -> f64 {
        let m = finite_or(self.total_mass_kg, 84.0).clamp(0.0, 500.0);
        m * GRAVITY * self.grade_angle(grade_pct).sin()
    }

    /// Rolling resistance, N (always opposing motion, magnitude ≥ 0).
    pub fn rolling_force(&self, grade_pct: f64) -> f64 {
        let m = finite_or(self.total_mass_kg, 84.0).clamp(0.0, 500.0);
        let crr = finite_or(self.crr, 0.0).clamp(0.0, 0.1);
        crr * m * GRAVITY * self.grade_angle(grade_pct).cos()
    }

    /// Aerodynamic force, N, positive = pushes the rider backwards.
    /// A tailwind stronger than the riding speed (`v + wind < 0`) pushes forward.
    pub fn aero_force(&self, v_ms: f64) -> f64 {
        let v = if v_ms.is_finite() { v_ms } else { 0.0 };
        let rel = v + finite_or(self.wind_ms, 0.0).clamp(-40.0, 40.0);
        let rho = finite_or(self.air_density, 1.225).clamp(0.5, 1.6);
        let cda = finite_or(self.cda, 0.32).clamp(0.0, 2.0) + SPOKE_DRAG_M2;
        0.5 * rho * cda * rel * rel.abs()
    }

    /// Wheel-bearing friction, N (only while rolling: it never pushes a standing bike).
    pub fn bearing_force(&self, v_ms: f64) -> f64 {
        if v_ms.is_finite() && v_ms > 0.0 {
            BEARING_N + BEARING_NS_PER_M * v_ms
        } else {
            0.0
        }
    }

    /// Total force opposing motion at `v` on `grade_pct`, N (may be negative downhill).
    pub fn resistance_force(&self, v_ms: f64, grade_pct: f64) -> f64 {
        self.gravity_force(grade_pct)
            + self.rolling_force(grade_pct)
            + self.bearing_force(v_ms)
            + self.aero_force(v_ms)
    }
}

/// Advance the rider speed by `dt` seconds (energy based, stable at v≈0, never negative).
pub fn step_speed(v_ms: f64, power_w: f64, grade_pct: f64, p: &PhysicsParams, dt: f64) -> f64 {
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 60.0)
    } else {
        0.0
    };
    let mut v = sanitize_speed(v_ms);
    if dt <= 0.0 {
        return v;
    }
    let steps = ((dt / SUBSTEP_S).ceil() as usize).clamp(1, 4000);
    let h = dt / steps as f64;
    for _ in 0..steps {
        v = substep(v, power_w, grade_pct, p, h);
    }
    v
}

fn substep(v: f64, power_w: f64, grade_pct: f64, p: &PhysicsParams, h: f64) -> f64 {
    let m = p.effective_mass_kg();
    // Gravity, rolling resistance and air drag act as forces, so a standstill on a
    // descent starts rolling (a pure power balance would multiply them by v = 0).
    let resist = p.resistance_force(v, grade_pct);
    let v = (v - resist / m * h).clamp(0.0, MAX_SPEED_MS);

    // Pedalled power is injected as kinetic energy: exact for constant power and stable
    // around v ≈ 0, where P / v would blow up.
    let p_in = if power_w.is_finite() { power_w } else { 0.0 };
    let traction = if p_in >= 0.0 {
        p_in * finite_or(p.drivetrain_eff, 0.975).clamp(0.1, 1.0)
    } else {
        p_in
    };
    if traction == 0.0 {
        return v;
    }
    let energy = 0.5 * m * v * v + traction * h;
    if !energy.is_finite() || energy <= 0.0 {
        return v;
    }
    (2.0 * energy / m).sqrt().clamp(0.0, MAX_SPEED_MS)
}

/// Highest power an estimate may report, W. Beyond it the speed series is noise (a GPS jump), not a
/// sprint.
pub const MAX_ESTIMATED_POWER_W: f64 = 2000.0;

/// The inverse of [`step_speed`]: the pedal power that keeps the bike at speed `v_ms` while it
/// accelerates with `accel_ms2` on `grade_pct`.
///
/// `P = (F_resist(v) · v + m_eff · v · a) / η`, the same balance the forward model integrates
/// (gravity and rolling see the plain mass, the acceleration term the mass plus the wheels).
/// Coasting and braking give a negative balance: they are reported as `0` W, like the forward
/// model that never pedals backwards.
pub fn power_for(v_ms: f64, accel_ms2: f64, grade_pct: f64, p: &PhysicsParams) -> f64 {
    let v = sanitize_speed(v_ms);
    if v <= 0.0 {
        return 0.0;
    }
    let accel = if accel_ms2.is_finite() {
        accel_ms2
    } else {
        0.0
    };
    let wheel_w = p.resistance_force(v, grade_pct) * v + p.effective_mass_kg() * v * accel;
    let eta = finite_or(p.drivetrain_eff, 0.975).clamp(0.1, 1.0);
    let pedal_w = wheel_w / eta;
    if pedal_w.is_finite() {
        pedal_w.clamp(0.0, MAX_ESTIMATED_POWER_W)
    } else {
        0.0
    }
}

/// Steady-state speed (m/s) for constant power and grade (used by ETA and tests).
pub fn steady_state_speed(power_w: f64, grade_pct: f64, p: &PhysicsParams) -> f64 {
    let p_in = if power_w.is_finite() {
        power_w.max(0.0)
    } else {
        0.0
    };
    let p_eff = p_in * finite_or(p.drivetrain_eff, 0.975).clamp(0.1, 1.0);
    if p_eff <= 0.0 {
        return 0.0;
    }
    // Net power at the wheel at speed v: positive = accelerating.
    let net = |v: f64| p_eff - p.resistance_force(v, grade_pct) * v;
    if net(0.0) <= 0.0 {
        return 0.0;
    }
    let mut lo = 0.0f64;
    let mut hi = MAX_SPEED_MS;
    if net(hi) > 0.0 {
        return hi;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if net(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> PhysicsParams {
        PhysicsParams::from_rider(&RiderSettings::default())
    }

    #[test]
    fn from_rider_uses_defaults_and_clamps() {
        let p = params();
        assert!((p.total_mass_kg - 84.0).abs() < 1e-9);
        assert!((p.effective_mass_kg() - 85.5).abs() < 1e-9);
        let bad = RiderSettings {
            rider_kg: f64::NAN,
            bike_kg: 1e9,
            cda: -1.0,
            drivetrain_eff: 0.0,
            air_density: f64::INFINITY,
            ..RiderSettings::default()
        };
        let p = PhysicsParams::from_rider(&bad);
        assert!(p.total_mass_kg.is_finite());
        assert!(p.cda >= 0.05 && p.drivetrain_eff >= 0.5);
    }

    /// The reference points from the architecture document.
    #[test]
    fn reference_steady_state() {
        let p = params();
        let flat = steady_state_speed(250.0, 0.0, &p);
        let kmh = flat * 3.6;
        assert!((kmh - 36.3).abs() < 0.5, "flat 250 W → {kmh:.1} km/h");

        let climb = steady_state_speed(250.0, 8.0, &p) * 3.6;
        assert!((climb - 12.3).abs() < 0.5, "8 % at 250 W → {climb:.1} km/h");
    }

    /// The worked example of the open-source `rouleur` package (an implementation of Martin et
    /// al. 1998): Wiggins' hour record, 54.526 km/h at CdA 0.19, Crr 0.0025, η 0.98, 82 kg and
    /// ρ(30 °C, 777 mmHg, 60 %) ≈ 1.1793 kg/m³ needs 440.96 W. The same speed must come out of
    /// that power here.
    #[test]
    fn agrees_with_the_rouleur_implementation_of_martin_1998() {
        let p = PhysicsParams {
            total_mass_kg: 82.0,
            rotational_mass_kg: 0.0,
            cda: 0.19,
            crr: 0.0025,
            drivetrain_eff: 0.98,
            air_density: 1.1793,
            wind_ms: 0.0,
        };
        let v = 54.526 / 3.6;
        let needed = p.resistance_force(v, 0.0) * v / p.drivetrain_eff;
        assert!((needed - 440.96).abs() < 1.0, "{needed:.2} W");
        let kmh = steady_state_speed(440.96, 0.0, &p) * 3.6;
        assert!((kmh - 54.526).abs() < 0.05, "{kmh:.3} km/h");
    }

    /// Ferrari's rule of thumb for steady climbs, VAM ≈ W/kg · (200 + 10·grade) on the rider's
    /// weight, holds within about 10 % on a 6–10 % grade.
    #[test]
    fn climbing_speed_matches_the_vam_rule_of_thumb() {
        let p = params();
        for grade in [6.0, 8.0, 10.0] {
            let v = steady_state_speed(300.0, grade, &p);
            let vam = v * 3600.0 * (grade / 100.0_f64).atan().sin();
            let rule = 300.0 / 75.0 * (200.0 + 10.0 * grade);
            assert!(
                (vam / rule - 1.0).abs() < 0.12,
                "{grade} %: VAM {vam:.0} vs {rule:.0}"
            );
        }
    }

    /// `power_for` is the inverse of the steady state, and of one forward step.
    #[test]
    fn power_for_inverts_the_forward_model() {
        let p = params();
        for (watts, grade) in [(120.0, 0.0), (250.0, 0.0), (300.0, 7.0), (180.0, -2.0)] {
            let v = steady_state_speed(watts, grade, &p);
            let back = power_for(v, 0.0, grade, &p);
            assert!(
                (back - watts).abs() < 0.5,
                "{watts} W at {grade} % → {v:.2} m/s → {back:.1} W"
            );
        }
        // one forward second from a standing start: the same power explains the speed gain
        let v1 = step_speed(5.0, 400.0, 3.0, &p, 1.0);
        let accel = v1 - 5.0;
        let mid = 0.5 * (5.0 + v1);
        let back = power_for(mid, accel, 3.0, &p);
        assert!((back - 400.0).abs() < 8.0, "{back:.1} W");
    }

    #[test]
    fn power_for_never_goes_negative_or_wild() {
        let p = params();
        assert_eq!(power_for(0.0, 1.0, 0.0, &p), 0.0, "standing still");
        assert_eq!(power_for(12.0, 0.0, -8.0, &p), 0.0, "coasting downhill");
        assert_eq!(power_for(10.0, -3.0, 0.0, &p), 0.0, "braking");
        assert_eq!(power_for(f64::NAN, 0.0, 0.0, &p), 0.0);
        assert_eq!(
            power_for(10.0, f64::NAN, 0.0, &p),
            power_for(10.0, 0.0, 0.0, &p)
        );
        assert_eq!(power_for(25.0, 5.0, 25.0, &p), MAX_ESTIMATED_POWER_W);
    }

    #[test]
    fn more_mass_or_rolling_resistance_needs_more_power() {
        let light = params();
        let heavy = PhysicsParams {
            total_mass_kg: light.total_mass_kg + 15.0,
            ..light
        };
        let rough = PhysicsParams {
            crr: 0.009,
            ..light
        };
        let base = power_for(6.0, 0.0, 5.0, &light);
        assert!(power_for(6.0, 0.0, 5.0, &heavy) > base + 40.0);
        assert!(power_for(6.0, 0.0, 5.0, &rough) > base + 20.0);
    }

    #[test]
    fn steady_state_is_monotone_in_power_and_grade() {
        let p = params();
        let a = steady_state_speed(150.0, 0.0, &p);
        let b = steady_state_speed(300.0, 0.0, &p);
        assert!(b > a);
        let up = steady_state_speed(250.0, 6.0, &p);
        let down = steady_state_speed(250.0, -6.0, &p);
        assert!(down > up);
        assert_eq!(steady_state_speed(0.0, 0.0, &p), 0.0);
        assert_eq!(steady_state_speed(f64::NAN, 0.0, &p), 0.0);
    }

    #[test]
    fn step_speed_reaches_steady_state_and_never_goes_negative() {
        let p = params();
        let target = steady_state_speed(250.0, 0.0, &p);
        let mut v = 0.0;
        for _ in 0..600 {
            v = step_speed(v, 250.0, 0.0, &p, 0.1);
        }
        assert!((v - target).abs() < 0.3, "v={v:.3} target={target:.3}");

        // Coasting on a climb decays to a standstill and stops there.
        let mut v = 8.0;
        for _ in 0..600 {
            v = step_speed(v, 0.0, 10.0, &p, 0.1);
        }
        assert_eq!(v, 0.0);

        // Free rolling downhill accelerates, but never past the cap.
        let mut v = 0.0;
        for _ in 0..2000 {
            v = step_speed(v, 0.0, -12.0, &p, 0.1);
        }
        assert!(v > 5.0 && v <= MAX_SPEED_MS);
    }

    #[test]
    fn step_speed_is_dt_stable() {
        let p = params();
        let coarse = step_speed(0.0, 250.0, 5.0, &p, 1.0);
        let mut fine = 0.0;
        for _ in 0..100 {
            fine = step_speed(fine, 250.0, 5.0, &p, 0.01);
        }
        assert!((coarse - fine).abs() < 0.25, "{coarse:.3} vs {fine:.3}");
    }

    #[test]
    fn headwind_slows_tailwind_helps() {
        let mut head = params();
        head.wind_ms = 5.0;
        let mut tail = params();
        tail.wind_ms = -5.0;
        let base = steady_state_speed(250.0, 0.0, &params());
        assert!(steady_state_speed(250.0, 0.0, &head) < base);
        assert!(steady_state_speed(250.0, 0.0, &tail) > base);
    }

    #[test]
    fn aero_force_sign_follows_relative_air_speed() {
        let mut p = params();
        assert!(p.aero_force(5.0) > 0.0);
        p.wind_ms = -10.0;
        assert!(p.aero_force(5.0) < 0.0, "strong tailwind pushes forward");
    }
}
