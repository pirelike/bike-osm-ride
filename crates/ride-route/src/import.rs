//! An activity file → a stored ride of a route.
//!
//! [`import_activity`] projects the recording onto the route ([`crate::matching`]), resamples it
//! to the 1 Hz timer-time recording every ride in the log has, and — when the file carries no
//! watts — estimates them with the power model run backwards ([`ride_sim::estimate`]). The
//! result is a [`RideRecord`] with `source = Imported`, ready for [`crate::rides::Store`].
//!
//! Position, elevation and grade of every sample come from the *route* at the matched distance,
//! not from the file: two rides of one route then share one elevation profile, and the grade the
//! estimate needs is the route's smoothed one rather than noisy GPS altitude.

use ride_model::{
    EstimateParams, RecordPoint, RideRecord, RideSource, RiderSettings, Route, RIDE_RECORD_VERSION,
};
use ride_sim::analysis::summarize;
use ride_sim::estimate::{apply_estimate, profile_params};

use crate::activity::{Activity, ActivitySample};
use crate::matching::{match_to_route, Matched};
use crate::{err, Result};

/// A gap between two samples longer than this may be a pause, seconds.
const PAUSE_GAP_S: f64 = 10.0;
/// Over such a gap an average speed below this means the rider stood still, m/s.
const PAUSE_SPEED_MS: f64 = 1.0;
/// A sensor value is used for a second only if it was measured this close to it, seconds.
const SENSOR_REACH_S: f64 = 3.0;
/// Speed is the slope of the distance over this many seconds each side.
const SPEED_SPAN_S: usize = 2;
/// Fastest speed the recording may report, m/s.
const MAX_SPEED_MS: f64 = 30.0;
/// More than this many seconds is not an activity but a log of days (or a broken time stamp).
const MAX_DURATION_S: f64 = 24.0 * 3600.0;

/// The result of an import.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub record: RideRecord,
    /// Things the user should know (partial coverage, a detour, estimated power), Hungarian.
    pub warnings: Vec<String>,
    /// The activity's own name, if the file had one.
    pub name: String,
    /// Share of the route the ride covers, `0..=1`.
    pub coverage: f64,
    /// Seconds in the file that were a stop (auto-pause) and are not part of the timer time.
    pub paused_s: f64,
}

/// Imports `activity` as a ride of `route`.
///
/// `estimate` is the rider / bike setup for the watt estimate; `None` uses the rider's own
/// settings. It is ignored when the file has power.
pub fn import_activity(
    activity: &Activity,
    route: &Route,
    rider: &RiderSettings,
    estimate: Option<EstimateParams>,
) -> Result<Imported> {
    let matched = match_to_route(&activity.samples, route)?;
    let frames = key_frames(&activity.samples, &matched);
    let Some((timeline, paused_s)) = timer_times(&frames) else {
        return err("Az edzés időbélyegei nem használhatók (nincs benne mozgás).");
    };
    let total_s = *timeline.last().unwrap_or(&0.0);
    if total_s < 1.0 {
        return err("Az edzés túl rövid az importáláshoz.");
    }
    if total_s > MAX_DURATION_S {
        return err("Az edzés 24 óránál hosszabb, valószínűleg hibás az időbélyeg.");
    }

    let mut points = resample(&frames, &timeline, route);
    let has_power = points.iter().any(|p| p.power_w.is_some());
    let mut warnings = Vec::new();
    let params = if has_power {
        None
    } else {
        let params = estimate.unwrap_or_else(|| profile_params(rider));
        apply_estimate(&mut points, &params);
        warnings.push(format!(
            "A fájlban nincs teljesítményadat, ezért a wattokat a szimuláció fizikájából becsültük ({} kg versenyző, {} kg kerékpár, Crr {}). Szél és szélárnyék nélkül ez közelítés.",
            hu(params.rider_kg, 1),
            hu(params.bike_kg, 1),
            hu(params.crr, 4)
        ));
        Some(params)
    };

    let Some(summary) = summarize(&route.name, &route.id, &points, rider) else {
        return err("Az edzésből nem készíthető összegzés.");
    };
    if matched.is_partial() {
        warnings.push(format!(
            "Az edzés az útvonal {:.0}%-át fedi le ({:.1}–{:.1} km), a többi szakaszt nem lehet összehasonlítani.",
            matched.coverage * 100.0,
            matched.start_dist / 1000.0,
            matched.end_dist / 1000.0
        ));
    }
    if matched.off_route > 0 {
        warnings.push(format!(
            "{} pont az útvonalon kívülre esett (kitérő vagy GPS-hiba), ezeket a távolságot a környező pontokból becsültük.",
            matched.off_route
        ));
    }
    if paused_s >= 1.0 {
        warnings.push(format!(
            "{:.0} másodpercnyi megállást (automatikus szünet) nem számoltunk az időbe.",
            paused_s
        ));
    }

    Ok(Imported {
        record: RideRecord {
            version: RIDE_RECORD_VERSION,
            route_id: route.id.clone(),
            route_name: route.name.clone(),
            simulated: false,
            summary,
            points,
            source: RideSource::Imported,
            power_estimated: params.is_some(),
            estimate: params,
        },
        warnings,
        name: activity.name.clone(),
        coverage: matched.coverage,
        paused_s,
    })
}

/// Estimates the watts of a stored ride again with another setup (someone else's weight, a rougher
/// road). Only a ride whose watts were estimated can be redone: measured power is never replaced.
pub fn reestimate(
    record: &RideRecord,
    params: EstimateParams,
    rider: &RiderSettings,
) -> Result<RideRecord> {
    if !record.power_estimated {
        return err(
            "Ennek az edzésnek mért teljesítményadata van, ezt nem írjuk felül becsléssel.",
        );
    }
    let mut points = record.points.clone();
    apply_estimate(&mut points, &params);
    let Some(mut summary) = summarize(&record.route_name, &record.route_id, &points, rider) else {
        return err("Az edzésből nem készíthető összegzés.");
    };
    // the stem of the stored file is the start time: it must stay as it was
    summary.started_unix_s = record.summary.started_unix_s;
    Ok(RideRecord {
        summary,
        points,
        estimate: Some(params),
        ..record.clone()
    })
}

/// A number with a decimal comma, trailing zeros of the fraction kept short (`75`, `9,5`, `0,004`).
fn hu(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    trimmed.replace('.', ",")
}

/// One on-route sample with its distance.
#[derive(Clone, Copy)]
struct Frame<'a> {
    sample: &'a ActivitySample,
    dist: f64,
}

fn key_frames<'a>(samples: &'a [ActivitySample], matched: &Matched) -> Vec<Frame<'a>> {
    samples
        .iter()
        .zip(&matched.dist)
        .filter_map(|(sample, dist)| dist.map(|dist| Frame { sample, dist }))
        .collect()
}

/// Timer time of every frame (a stop longer than [`PAUSE_GAP_S`] is left out), and the seconds
/// that were left out. `None` when the whole recording is a single instant.
fn timer_times(frames: &[Frame<'_>]) -> Option<(Vec<f64>, f64)> {
    let first = frames.first()?;
    let mut timeline = vec![0.0];
    let mut paused = 0.0;
    let mut t = 0.0;
    for pair in frames.windows(2) {
        let gap = pair[1].sample.time - pair[0].sample.time;
        let moved = pair[1].dist - pair[0].dist;
        if gap > PAUSE_GAP_S && moved / gap < PAUSE_SPEED_MS {
            paused += gap - 1.0;
            t += 1.0;
        } else {
            t += gap;
        }
        timeline.push(t);
    }
    let _ = first;
    (t > 0.0).then_some((timeline, paused))
}

/// 1 Hz recording in timer time.
fn resample(frames: &[Frame<'_>], timeline: &[f64], route: &Route) -> Vec<RecordPoint> {
    let seconds = timeline.last().copied().unwrap_or(0.0).floor() as usize;
    let mut dists = Vec::with_capacity(seconds + 1);
    let mut unix = Vec::with_capacity(seconds + 1);
    let mut j = 0usize; // frame at or before the second
    for s in 0..=seconds {
        let t = s as f64;
        while j + 2 < frames.len() && timeline[j + 1] <= t {
            j += 1;
        }
        let (a, b) = (j, (j + 1).min(frames.len() - 1));
        let span = timeline[b] - timeline[a];
        let f = if span > 1e-9 {
            ((t - timeline[a]) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        dists.push(frames[a].dist + (frames[b].dist - frames[a].dist) * f);
        unix.push(frames[a].sample.time + (frames[b].sample.time - frames[a].sample.time) * f);
    }

    let sensor = |pick: fn(&ActivitySample) -> Option<f64>| -> Vec<Option<f64>> {
        let values: Vec<(f64, f64)> = frames
            .iter()
            .zip(timeline)
            .filter_map(|(frame, &t)| pick(frame.sample).map(|v| (t, v)))
            .collect();
        let mut out = Vec::with_capacity(seconds + 1);
        let mut k = 0usize;
        for s in 0..=seconds {
            let t = s as f64;
            while k + 1 < values.len() && (values[k + 1].0 - t).abs() <= (values[k].0 - t).abs() {
                k += 1;
            }
            out.push(
                values
                    .get(k)
                    .filter(|(vt, _)| (vt - t).abs() <= SENSOR_REACH_S)
                    .map(|&(_, v)| v),
            );
        }
        out
    };
    let power = sensor(|s| s.power);
    let cadence = sensor(|s| s.cadence);
    let hr = sensor(|s| s.hr);

    (0..=seconds)
        .map(|s| {
            let lo = s.saturating_sub(SPEED_SPAN_S);
            let hi = (s + SPEED_SPAN_S).min(seconds);
            let speed = if hi > lo {
                ((dists[hi] - dists[lo]) / (hi - lo) as f64).clamp(0.0, MAX_SPEED_MS)
            } else {
                0.0
            };
            let at = route.locate(dists[s]);
            RecordPoint {
                t: s as f64,
                unix_s: unix[s],
                dist: dists[s],
                lat: at.lat,
                lon: at.lon,
                ele: at.ele,
                speed_ms: speed,
                power_w: power[s],
                cadence_rpm: cadence[s],
                hr_bpm: hr[s],
                grade: at.grade,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ActivityFormat;
    use ride_model::testing::straight_route;
    use ride_sim::physics::{step_speed, PhysicsParams};

    const M_PER_DEG_LON: f64 = 111_320.0 * 0.676_204_6;
    const START: f64 = 1_790_000_000.0;

    /// A recording of the forward model at `power(t)`, as another app would have written it.
    fn activity(
        route: &Route,
        seconds: usize,
        power: impl Fn(f64) -> f64,
        with_power: bool,
    ) -> (Activity, Vec<f64>) {
        let p = PhysicsParams::from_rider(&RiderSettings::default());
        let (mut v, mut dist) = (0.0, 0.0);
        let mut samples = Vec::new();
        let mut truth = Vec::new();
        for i in 0..seconds {
            let t = i as f64;
            let grade = route.locate(dist).grade;
            samples.push(ActivitySample {
                time: START + t,
                lat: 47.5 + 4.0 / 110_540.0,
                lon: 19.0 + dist / M_PER_DEG_LON,
                ele: Some(route.locate(dist).ele + 3.0),
                hr: Some(140.0 + (i % 7) as f64),
                cadence: Some(85.0),
                power: with_power.then(|| power(t)),
            });
            truth.push(power(t));
            let next = step_speed(v, power(t), grade, &p, 1.0);
            dist += 0.5 * (v + next);
            v = next;
        }
        (
            Activity {
                name: "Reggeli kör".into(),
                format: ActivityFormat::Gpx,
                samples,
                skipped: 0,
            },
            truth,
        )
    }

    fn route() -> Route {
        straight_route(&[(1_500.0, 0.0), (1_500.0, 4.0), (3_000.0, 0.0)], 120.0)
    }

    #[test]
    fn a_file_without_watts_gets_estimated_ones_and_the_flags_say_so() {
        let route = route();
        let (act, truth) = activity(
            &route,
            700,
            |t| if t < 350.0 { 190.0 } else { 240.0 },
            false,
        );
        let rider = RiderSettings::default();
        let done = import_activity(&act, &route, &rider, None).unwrap();
        let r = &done.record;
        assert_eq!(r.source, RideSource::Imported);
        assert!(r.power_estimated);
        let params = r.estimate.expect("the setup is kept");
        assert!(params.from_profile);
        assert!((params.rider_kg - rider.rider_kg).abs() < 1e-9);
        assert_eq!(r.route_id, route.id);
        assert!(!r.simulated);

        // 1 Hz timer time, one point per second
        assert!(r.points.len() >= 690, "{}", r.points.len());
        assert!(r
            .points
            .windows(2)
            .all(|w| (w[1].t - w[0].t - 1.0).abs() < 1e-9));
        assert!(r.points.windows(2).all(|w| w[1].dist >= w[0].dist));
        assert_eq!(r.points[10].hr_bpm.map(|h| h >= 140.0), Some(true));

        // the estimated average is close to what the model was driven with
        let n = r.points.len().min(truth.len());
        let est: f64 = r.points[60..n]
            .iter()
            .filter_map(|p| p.power_w)
            .sum::<f64>()
            / (n - 60) as f64;
        let real: f64 = truth[60..n].iter().sum::<f64>() / (n - 60) as f64;
        assert!(
            (est / real - 1.0).abs() < 0.06,
            "estimated {est:.1} W vs {real:.1} W"
        );
        assert_eq!(r.summary.avg_power_w.map(|w| w > 100.0), Some(true));
        assert!(
            done.warnings.iter().any(|w| w.contains("becsült")),
            "{:?}",
            done.warnings
        );
        assert_eq!(done.name, "Reggeli kör");
    }

    #[test]
    fn measured_watts_are_kept_as_they_are() {
        let route = route();
        let (act, _) = activity(&route, 400, |_| 205.0, true);
        let done = import_activity(&act, &route, &RiderSettings::default(), None).unwrap();
        assert!(!done.record.power_estimated);
        assert_eq!(done.record.estimate, None);
        assert_eq!(done.record.points[100].power_w, Some(205.0));
        assert!((done.record.summary.avg_power_w.unwrap() - 205.0).abs() < 0.5);
        assert!(done.warnings.iter().all(|w| !w.contains("becsült")));
    }

    #[test]
    fn a_stop_is_not_counted_and_a_partial_ride_is_flagged() {
        let route = route();
        let (mut act, _) = activity(&route, 400, |_| 220.0, true);
        // the recorder paused for 5 minutes at sample 200: the later samples are 300 s later
        for s in &mut act.samples[200..] {
            s.time += 300.0;
        }
        // standing: both samples around the gap are at the same place
        act.samples[200].lon = act.samples[199].lon;
        let done = import_activity(&act, &route, &RiderSettings::default(), None).unwrap();
        assert!(done.paused_s > 250.0, "{}", done.paused_s);
        assert!(
            done.record.summary.elapsed_s < 420.0,
            "{}",
            done.record.summary.elapsed_s
        );
        assert!(done.warnings.iter().any(|w| w.contains("megállást")));
        // ~400 s at 8 m/s covers about a third of the 6 km route
        assert!(done.coverage < 0.9);
        assert!(
            done.warnings.iter().any(|w| w.contains("fedi le")),
            "{:?}",
            done.warnings
        );
    }

    #[test]
    fn other_weights_and_tyres_give_other_watts_and_measured_rides_refuse_it() {
        let route = route();
        let (act, _) = activity(&route, 600, |_| 200.0, false);
        let rider = RiderSettings::default();
        let own = import_activity(&act, &route, &rider, None).unwrap().record;
        let heavy = ride_sim::estimate::custom_params(&rider, 95.0, 11.0, 0.007);
        let redone = reestimate(&own, heavy, &rider).unwrap();
        assert!(redone.power_estimated);
        assert_eq!(redone.estimate, Some(heavy));
        assert!(!redone.estimate.unwrap().from_profile);
        assert!(
            redone.summary.avg_power_w.unwrap() > own.summary.avg_power_w.unwrap() * 1.1,
            "{:?} vs {:?}",
            redone.summary.avg_power_w,
            own.summary.avg_power_w
        );
        assert_eq!(redone.summary.started_unix_s, own.summary.started_unix_s);
        assert_eq!(redone.points.len(), own.points.len());

        let (measured, _) = activity(&route, 400, |_| 205.0, true);
        let measured = import_activity(&measured, &route, &rider, None)
            .unwrap()
            .record;
        let e = reestimate(&measured, heavy, &rider)
            .unwrap_err()
            .to_string();
        assert!(e.contains("mért"), "{e}");
    }

    #[test]
    fn an_imported_ride_is_stored_listed_and_never_a_pr() {
        use crate::store::Store;
        let route = route();
        let (act, _) = activity(&route, 500, |_| 210.0, true);
        let done = import_activity(&act, &route, &RiderSettings::default(), None).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::at(tmp.path());
        let meta = store
            .save_ride(&done.record, &route)
            .unwrap()
            .expect("kept");
        assert_eq!(meta.source, RideSource::Imported);
        assert!(!meta.power_estimated);
        assert!((meta.avg_power_w.unwrap() - 210.0).abs() < 0.5);
        assert!(meta.avg_hr.is_some());
        let loaded = store.load_ride(&route.id, &meta.stem).expect("stored");
        assert_eq!(loaded.source, RideSource::Imported);
        assert_eq!(loaded.points.len(), done.record.points.len());
        assert!(ride_sim::segments::best_times(std::slice::from_ref(&meta)).is_empty());
        assert!(ride_sim::segments::choose_ghost(&[meta], ride_model::GhostMode::Best).is_none());
    }

    #[test]
    fn a_wrong_route_is_refused() {
        let route = straight_route(&[(3_000.0, 0.0)], 100.0);
        let (mut act, _) = activity(&route, 300, |_| 200.0, true);
        for s in &mut act.samples {
            s.lat += 0.01; // 1.1 km north
        }
        let e = import_activity(&act, &route, &RiderSettings::default(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("nem illik"), "{e}");
    }
}
