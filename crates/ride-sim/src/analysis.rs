//! Numbers of a finished recording that the live engine did not count itself: an imported ride, or
//! a stored one whose watt values were re-estimated.
//!
//! The same definitions as `RideEngine::summary` (see `docs/SIMULATION.md` §2): normalised power
//! is the fourth-power mean of the 30 s rolling mean, IF is NP / FTP, TSS is
//! `elapsed · IF² / 3600 · 100`, and the zone seconds follow the 3 s average power.

use ride_model::{RecordPoint, RideSummary, RiderSettings};

use crate::zones;

/// Normalised power uses a 30 s rolling mean of the 1 Hz samples.
const NP_WINDOW: usize = 30;
/// The zone of a sample follows the average of the last few seconds, like the live display.
const ZONE_WINDOW: usize = 3;
/// A gap longer than this between two samples is not counted as riding time, seconds.
pub(crate) const MAX_SAMPLE_GAP_S: f64 = 5.0;

/// Normalised power of a 1 Hz power series (negative samples count as 0). `None` when empty.
///
/// The engine feeds the same window sample by sample (`RideEngine::record`); a test below keeps
/// the two in step.
pub fn normalized_power(power: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut window: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut sum4 = 0.0;
    let mut count = 0usize;
    for p in power {
        if !p.is_finite() {
            continue;
        }
        window.push_back(p.max(0.0));
        while window.len() > NP_WINDOW {
            window.pop_front();
        }
        let mean = window.iter().sum::<f64>() / window.len() as f64;
        sum4 += mean.powi(4);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some((sum4 / count as f64).powf(0.25))
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values
        .filter(|v| v.is_finite())
        .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

fn max_of(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.filter(|v| v.is_finite()).reduce(f64::max)
}

/// The summary of a recording, or `None` when it has fewer than two samples.
///
/// `rider` only supplies FTP and maximum heart rate for IF, TSS and the zones; the watt values
/// themselves are taken from `points`.
pub fn summarize(
    route_name: &str,
    route_id: &str,
    points: &[RecordPoint],
    rider: &RiderSettings,
) -> Option<RideSummary> {
    let first = points.first()?;
    let last = points.last()?;
    if points.len() < 2 {
        return None;
    }
    let elapsed_s = (last.t - first.t).max(0.0);
    let ridden_m = (last.dist - first.dist).max(0.0);

    let mut ascent = 0.0;
    for pair in points.windows(2) {
        let up = pair[1].ele - pair[0].ele;
        if up.is_finite() && up > 0.0 {
            ascent += up;
        }
    }

    let ftp = if rider.ftp_w.is_finite() {
        rider.ftp_w
    } else {
        0.0
    };
    let np = normalized_power(points.iter().filter_map(|p| p.power_w));
    let intensity_factor = np.filter(|_| ftp > 0.0).map(|np| np / ftp);
    let tss = intensity_factor
        .filter(|_| elapsed_s > 0.0)
        .map(|if_| elapsed_s * if_ * if_ / 3600.0 * 100.0);

    let mut work_j = 0.0;
    let mut power_zone_secs = [0.0; 7];
    let mut hr_zone_secs = [0.0; 5];
    let mut recent: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    for (i, point) in points.iter().enumerate() {
        let dt = if i == 0 {
            0.0
        } else {
            (point.t - points[i - 1].t).clamp(0.0, MAX_SAMPLE_GAP_S)
        };
        if let Some(p) = point.power_w.filter(|p| p.is_finite()) {
            let p = p.max(0.0);
            work_j += p * dt;
            recent.push_back(p);
            while recent.len() > ZONE_WINDOW {
                recent.pop_front();
            }
            let avg = recent.iter().sum::<f64>() / recent.len() as f64;
            let zone = zones::power_zone(avg, ftp);
            if (1..=7).contains(&zone) {
                power_zone_secs[usize::from(zone) - 1] += dt;
            }
        }
        if let Some(hr) = point.hr_bpm.filter(|h| h.is_finite()) {
            let zone = zones::hr_zone(hr.max(0.0), rider.max_hr);
            if (1..=5).contains(&zone) {
                hr_zone_secs[usize::from(zone) - 1] += dt;
            }
        }
    }

    Some(RideSummary {
        route_name: route_name.to_string(),
        route_id: route_id.to_string(),
        started_unix_s: first.unix_s - first.t,
        elapsed_s,
        dist: last.dist,
        ascent,
        avg_speed_ms: if elapsed_s > 1e-9 {
            ridden_m / elapsed_s
        } else {
            0.0
        },
        max_speed_ms: max_of(points.iter().map(|p| p.speed_ms)).unwrap_or(0.0),
        avg_power_w: mean(points.iter().filter_map(|p| p.power_w).map(|p| p.max(0.0))),
        max_power_w: max_of(points.iter().filter_map(|p| p.power_w).map(|p| p.max(0.0))),
        normalized_power_w: np,
        intensity_factor,
        tss,
        work_kj: work_j / 1000.0,
        kcal: work_j / 1000.0,
        avg_hr: mean(points.iter().filter_map(|p| p.hr_bpm)),
        max_hr: max_of(points.iter().filter_map(|p| p.hr_bpm)),
        avg_cadence: mean(points.iter().filter_map(|p| p.cadence_rpm)),
        power_zone_secs,
        hr_zone_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(t: f64, dist: f64, power: Option<f64>) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0 + dist * 0.01,
            speed_ms: 8.0,
            power_w: power,
            cadence_rpm: Some(88.0),
            hr_bpm: Some(150.0),
            grade: 1.0,
        }
    }

    #[test]
    fn normalised_power_of_a_constant_is_that_constant_and_of_a_surge_is_higher() {
        let flat = normalized_power(std::iter::repeat_n(200.0, 120)).unwrap();
        assert!((flat - 200.0).abs() < 1e-9);
        let surging: Vec<f64> = (0..120)
            .map(|i| if (i / 30) % 2 == 0 { 100.0 } else { 300.0 })
            .collect();
        let mean = surging.iter().sum::<f64>() / surging.len() as f64;
        assert!(normalized_power(surging).unwrap() > mean);
        assert_eq!(normalized_power(std::iter::empty()), None);
        assert_eq!(normalized_power([f64::NAN]), None);
    }

    #[test]
    fn the_engine_and_the_free_function_agree() {
        use crate::RideEngine;
        use ride_model::testing::straight_route;
        use ride_model::{RideState, SensorSample};

        let route = std::sync::Arc::new(straight_route(&[(5_000.0, 2.0)], 100.0));
        let mut engine = RideEngine::new(route, RiderSettings::default());
        engine.start(1_700_000_000.0);
        assert_eq!(engine.state(), RideState::Riding);
        for i in 0..300 {
            let watts = 150.0 + 100.0 * ((i / 40) % 2) as f64;
            let sample = SensorSample {
                power_w: Some(watts),
                ..SensorSample::default()
            };
            engine.step(0.5, &sample, 1_700_000_000.0 + i as f64 * 0.5);
        }
        let summary = engine.summary();
        let from_points =
            summarize("x", "y", engine.history(), &RiderSettings::default()).expect("a recording");
        let a = summary.normalized_power_w.expect("engine NP");
        let b = from_points.normalized_power_w.expect("free NP");
        assert!((a - b).abs() < 0.5, "engine {a:.2} vs free {b:.2}");
        let (a, b) = (
            summary.avg_power_w.unwrap(),
            from_points.avg_power_w.unwrap(),
        );
        assert!((a - b).abs() < 2.0, "avg {a:.2} vs {b:.2}");
    }

    #[test]
    fn a_summary_counts_time_ascent_and_zones() {
        let points: Vec<RecordPoint> = (0..=600)
            .map(|i| point(i as f64, i as f64 * 8.0, Some(200.0)))
            .collect();
        let rider = RiderSettings {
            ftp_w: 200.0,
            ..RiderSettings::default()
        };
        let s = summarize("Kör", "id", &points, &rider).unwrap();
        assert_eq!(s.route_id, "id");
        assert!((s.elapsed_s - 600.0).abs() < 1e-9);
        assert!((s.dist - 4_800.0).abs() < 1e-9);
        assert!((s.avg_speed_ms - 8.0).abs() < 1e-9);
        assert!((s.ascent - 48.0).abs() < 1e-6);
        assert!((s.avg_power_w.unwrap() - 200.0).abs() < 1e-9);
        assert!((s.intensity_factor.unwrap() - 1.0).abs() < 1e-9);
        assert!((s.tss.unwrap() - 600.0 / 3600.0 * 100.0).abs() < 1e-6);
        assert!((s.work_kj - 120.0).abs() < 1e-6);
        assert_eq!(s.avg_hr, Some(150.0));
        assert_eq!(s.avg_cadence, Some(88.0));
        assert!((s.power_zone_secs.iter().sum::<f64>() - 600.0).abs() < 1e-6);
    }

    #[test]
    fn a_recording_without_power_has_no_power_numbers() {
        let points: Vec<RecordPoint> = (0..=60)
            .map(|i| point(i as f64, i as f64 * 8.0, None))
            .collect();
        let s = summarize("Kör", "id", &points, &RiderSettings::default()).unwrap();
        assert_eq!(s.avg_power_w, None);
        assert_eq!(s.normalized_power_w, None);
        assert_eq!(s.tss, None);
        assert_eq!(s.work_kj, 0.0);
        assert!(summarize("Kör", "id", &points[..1], &RiderSettings::default()).is_none());
    }
}
