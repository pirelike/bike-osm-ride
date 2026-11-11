//! Watts for a recording that has none: the power model run backwards.
//!
//! An imported GPX / TCX of a ride without a power meter has speed, position and time. The
//! forward model ([`crate::physics`]) turns pedal power into speed; here the speed series is
//! turned back into the power that would have produced it (an energy balance over a short window;
//! [`crate::physics::power_for`] is the same balance at one instant).
//!
//! It is an estimate, and the interface says so. Known limits (see `docs/SIMULATION.md`):
//! * no wind and no drafting, so a head wind reads as "slower, hence fewer watts";
//! * the climb is taken from the *route's* smoothed grade carried by each [`RecordPoint`], not
//!   from the file's noisy GPS altitude: on a climb gravity is most of the power;
//! * coasting and braking are `0` W, and a rider who stops pedalling for a moment on the flat is
//!   indistinguishable from one who is slowing down;
//! * the result depends on the weight, rolling resistance and CdA that are assumed.

use ride_model::{EstimateParams, RecordPoint, RiderSettings};

use crate::physics::{PhysicsParams, MAX_ESTIMATED_POWER_W};

/// Half width of the speed smoothing window, seconds (the whole window is 5 s).
const SPEED_HALF_WINDOW_S: f64 = 2.5;
/// The energy balance runs over this many samples each side of a sample (a 9 s window).
const WINDOW_SAMPLES: usize = 4;
/// A gap between two samples longer than this adds no work, seconds.
const MAX_STEP_S: f64 = 5.0;
/// Below this smoothed speed the bike counts as standing, m/s.
const STANDING_MS: f64 = 0.3;

/// The rider's own values as an estimate setup.
pub fn profile_params(rider: &RiderSettings) -> EstimateParams {
    let p = PhysicsParams::from_rider(rider);
    EstimateParams {
        rider_kg: rider.rider_kg,
        bike_kg: rider.bike_kg,
        crr: p.crr,
        cda: p.cda,
        drivetrain_eff: p.drivetrain_eff,
        air_density: p.air_density,
        from_profile: true,
    }
}

/// Somebody else's weight, bike and tyres; CdA, drivetrain and air density stay the rider's.
pub fn custom_params(
    rider: &RiderSettings,
    rider_kg: f64,
    bike_kg: f64,
    crr: f64,
) -> EstimateParams {
    EstimateParams {
        rider_kg,
        bike_kg,
        crr,
        from_profile: false,
        ..profile_params(rider)
    }
}

/// The model parameters of an estimate setup (every value is clamped like `from_rider` does).
pub fn physics_for(params: &EstimateParams) -> PhysicsParams {
    PhysicsParams::from_rider(&RiderSettings {
        rider_kg: params.rider_kg,
        bike_kg: params.bike_kg,
        crr: params.crr,
        cda: params.cda,
        drivetrain_eff: params.drivetrain_eff,
        air_density: params.air_density,
        ..RiderSettings::default()
    })
}

/// Estimated pedal power for every sample of a recording (same length as `points`).
///
/// An energy balance over a short window around each sample: the pedal energy is the change of
/// kinetic energy plus the work against gravity, rolling resistance, bearings and air along the
/// way, divided by the drivetrain efficiency and the time. Grade and inertia sit in the *same*
/// window, so a sharp change of grade (a crest, a bend in the projection) cannot show up as a
/// spike the way it does when the acceleration is smoothed and the grade is not. The speed is
/// smoothed over 5 s first; a standing bike gets `0` W, and a window in which the bike only lost
/// energy (coasting, braking) gives `0` W, like the forward model that never pedals backwards.
/// Samples out of time order are not expected (the recorder and the importer write them in
/// order); a zero or negative time step simply adds no work there.
pub fn estimate_power(points: &[RecordPoint], physics: &PhysicsParams) -> Vec<f64> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    let speed = smoothed_speed(points);
    let mass = physics.effective_mass_kg();
    let eta = if physics.drivetrain_eff.is_finite() {
        physics.drivetrain_eff.clamp(0.1, 1.0)
    } else {
        0.975
    };
    // cumulative work against the resistance, J (trapezoids on the 1 Hz samples)
    let mut work = vec![0.0; n];
    for i in 1..n {
        let dt = (points[i].t - points[i - 1].t).clamp(0.0, MAX_STEP_S);
        let before = physics.resistance_force(speed[i - 1], points[i - 1].grade) * speed[i - 1];
        let after = physics.resistance_force(speed[i], points[i].grade) * speed[i];
        work[i] = work[i - 1] + 0.5 * (before + after) * dt;
    }
    let kinetic: Vec<f64> = speed.iter().map(|v| 0.5 * mass * v * v).collect();
    (0..n)
        .map(|i| {
            if speed[i] < STANDING_MS {
                return 0.0;
            }
            let lo = i.saturating_sub(WINDOW_SAMPLES);
            let hi = (i + WINDOW_SAMPLES).min(n - 1);
            let dt = points[hi].t - points[lo].t;
            if dt <= 1e-6 {
                return 0.0;
            }
            let joules = (kinetic[hi] - kinetic[lo]) + (work[hi] - work[lo]);
            let watts = joules / (eta * dt);
            if watts.is_finite() {
                watts.clamp(0.0, MAX_ESTIMATED_POWER_W)
            } else {
                0.0
            }
        })
        .collect()
}

/// Writes the estimate into `points` (`power_w`) and returns it.
pub fn apply_estimate(points: &mut [RecordPoint], params: &EstimateParams) -> Vec<f64> {
    let watts = estimate_power(points, &physics_for(params));
    for (point, w) in points.iter_mut().zip(&watts) {
        point.power_w = Some(*w);
    }
    watts
}

/// Like [`apply_estimate`], but only the samples without a power value get one: measured watts
/// are never replaced. Returns how many samples were filled.
pub fn fill_estimate(points: &mut [RecordPoint], params: &EstimateParams) -> usize {
    let watts = estimate_power(points, &physics_for(params));
    let mut filled = 0;
    for (point, w) in points.iter_mut().zip(&watts) {
        if point.power_w.is_none() {
            point.power_w = Some(*w);
            filled += 1;
        }
    }
    filled
}

fn smoothed_speed(points: &[RecordPoint]) -> Vec<f64> {
    let n = points.len();
    let mut out = Vec::with_capacity(n);
    let (mut lo, mut hi) = (0usize, 0usize);
    for i in 0..n {
        let t = points[i].t;
        while lo < i && t - points[lo].t > SPEED_HALF_WINDOW_S {
            lo += 1;
        }
        hi = hi.max(i);
        while hi + 1 < n && points[hi + 1].t - t <= SPEED_HALF_WINDOW_S {
            hi += 1;
        }
        let window = &points[lo..=hi];
        let (sum, count) = window
            .iter()
            .map(|p| p.speed_ms)
            .filter(|v| v.is_finite())
            .fold((0.0, 0usize), |(s, c), v| (s + v.max(0.0), c + 1));
        out.push(if count > 0 { sum / count as f64 } else { 0.0 });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::step_speed;

    /// A ride at known power on a rolling profile, recorded at 1 Hz like the engine does.
    fn ride(
        power_at: impl Fn(f64) -> f64,
        grade_at: impl Fn(f64) -> f64,
        seconds: usize,
    ) -> Vec<RecordPoint> {
        let p = PhysicsParams::from_rider(&RiderSettings::default());
        let (mut v, mut dist) = (0.0, 0.0);
        let mut out = Vec::new();
        for i in 0..=seconds {
            let t = i as f64;
            let grade = grade_at(dist);
            out.push(RecordPoint {
                t,
                unix_s: 1_700_000_000.0 + t,
                dist,
                lat: 47.5,
                lon: 19.0,
                ele: 100.0,
                speed_ms: v,
                power_w: Some(power_at(t)),
                cadence_rpm: None,
                hr_bpm: None,
                grade,
            });
            let next = step_speed(v, power_at(t), grade, &p, 1.0);
            dist += 0.5 * (v + next);
            v = next;
        }
        out
    }

    fn rolling(dist: f64) -> f64 {
        // 0 % → +7 % → −4 % → 0 %, in blocks of 1.5 km
        match (dist / 1_500.0) as usize % 4 {
            1 => 7.0,
            2 => -4.0,
            _ => 0.0,
        }
    }

    #[test]
    fn it_gives_back_the_power_the_model_was_driven_with() {
        // steady blocks with steps between them: 180 W, 260 W, 140 W, 220 W
        let power = |t: f64| match (t / 150.0) as usize % 4 {
            0 => 180.0,
            1 => 260.0,
            2 => 140.0,
            _ => 220.0,
        };
        let points = ride(power, rolling, 1_200);
        let est = estimate_power(
            &points,
            &PhysicsParams::from_rider(&RiderSettings::default()),
        );
        assert_eq!(est.len(), points.len());

        // away from the power steps, the grade changes (the 5 s smoothing and the 4 s slope
        // reach across them) and the descents (where pedalling stops being visible) the
        // estimate is within 6 % of the truth
        let mut compared = 0;
        for (i, p) in points.iter().enumerate() {
            let near_step = (p.t % 150.0) < 12.0 || (p.t % 150.0) > 138.0;
            let around = &points[i.saturating_sub(6)..(i + 7).min(points.len())];
            let grade_changes = around.iter().any(|q| (q.grade - p.grade).abs() > 1e-9);
            let truth = power(p.t);
            if near_step || grade_changes || p.grade < 0.0 || p.t < 20.0 {
                continue;
            }
            compared += 1;
            let err = (est[i] - truth).abs() / truth;
            assert!(
                err < 0.06,
                "t={} grade={} truth={truth} est={:.1}",
                p.t,
                p.grade,
                est[i]
            );
        }
        assert!(compared > 500, "enough samples compared: {compared}");

        // over the whole ride the average is right to a few percent
        let (mut truth_sum, mut est_sum) = (0.0, 0.0);
        for (i, p) in points.iter().enumerate().skip(20) {
            if p.grade < 0.0 {
                continue;
            }
            truth_sum += power(p.t);
            est_sum += est[i];
        }
        assert!(
            (est_sum / truth_sum - 1.0).abs() < 0.03,
            "{est_sum} vs {truth_sum}"
        );
    }

    #[test]
    fn a_standing_bike_and_noise_never_make_negative_or_huge_watts() {
        let mut points = ride(|_| 0.0, |_| 0.0, 60);
        for (i, p) in points.iter_mut().enumerate() {
            p.speed_ms = if i % 2 == 0 { 0.0 } else { f64::NAN };
        }
        let est = estimate_power(
            &points,
            &PhysicsParams::from_rider(&RiderSettings::default()),
        );
        assert!(est
            .iter()
            .all(|w| w.is_finite() && (0.0..=2000.0).contains(w)));
        assert!(
            estimate_power(&[], &PhysicsParams::from_rider(&RiderSettings::default())).is_empty()
        );
    }

    #[test]
    fn heavier_riders_and_rougher_roads_need_more_watts() {
        let points = ride(|_| 220.0, |d| rolling(d).max(0.0), 600);
        let rider = RiderSettings::default();
        let own = profile_params(&rider);
        assert!(own.from_profile);
        let heavy = custom_params(&rider, rider.rider_kg + 20.0, rider.bike_kg, rider.crr);
        let rough = custom_params(&rider, rider.rider_kg, rider.bike_kg, 0.009);
        assert!(!heavy.from_profile);
        // same CdA and efficiency as the rider's own
        assert_eq!(heavy.cda, own.cda);
        let mean = |params: &EstimateParams| {
            let w = estimate_power(&points, &physics_for(params));
            w.iter().sum::<f64>() / w.len() as f64
        };
        assert!(mean(&heavy) > mean(&own) * 1.05);
        assert!(mean(&rough) > mean(&own) * 1.03);
    }

    #[test]
    fn apply_estimate_fills_the_watts_in() {
        let mut points = ride(|_| 200.0, |_| 0.0, 120);
        for p in &mut points {
            p.power_w = None;
        }
        let watts = apply_estimate(&mut points, &profile_params(&RiderSettings::default()));
        assert!(points.iter().all(|p| p.power_w.is_some()));
        assert_eq!(points[100].power_w, Some(watts[100]));
        assert!((watts[100] - 200.0).abs() < 8.0, "{:.1}", watts[100]);
    }
}
