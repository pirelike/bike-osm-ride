//! The numbers of one stored ride, second by second: what the review page's ride view shows.
//!
//! * the recording on a one-second grid ([`Dense`]), with the workout target of every second;
//! * the ride's derived numbers ([`RideDetail`]): variability index, efficiency factor, aerobic
//!   decoupling (Pw:HR), W/kg, work above FTP, the lowest W′ balance, time in zones counted second
//!   by second, the cadence spread;
//! * its **laps**: the workout's steps with the target against what was ridden (compliance), or,
//!   without a workout, the hard efforts found automatically ([`auto_intervals`]).
//!
//! The definitions: VI = NP / average power; EF = NP / average heart rate; decoupling compares
//! the power-to-heart-rate ratio of the first and the second half of the ride (positive = the
//! heart rate drifted up for the same watts; under 5 % is a well-built aerobic base). W′ balance
//! is the differential model (Skiba 2015): above CP it drains by `(P − CP)·dt`, below CP it
//! recovers by `(CP − P)·dt · (W′ − W′bal) / W′`.

use ride_model::{RecordPoint, RideRecord, RideWorkout, StepKind};

use crate::analysis::{normalized_power, MAX_SAMPLE_GAP_S};
use crate::fitness::CpModel;
use crate::zones;

/// A recording longer than this many seconds is cut (28 hours).
const MAX_SLOTS: usize = 100_000;

/// Upper bounds (rpm) of the cadence bands; the last band is everything above.
pub const CADENCE_BANDS: [f64; 6] = [60.0, 70.0, 80.0, 90.0, 100.0, 110.0];

/// Automatic intervals: the 30 s centred mean must stay at or above this share of FTP ...
pub const AUTO_THRESHOLD: f64 = 0.88;
/// ... for at least this long (seconds); dips shorter than [`AUTO_MERGE_S`] do not split one.
pub const AUTO_MIN_S: usize = 60;
pub const AUTO_MERGE_S: usize = 20;
/// At most this many automatic intervals are listed.
pub const AUTO_MAX: usize = 40;

/// The recording on a one-second grid: slot `i` is timer second `i`. A gap of up to
/// `MAX_SAMPLE_GAP_S` holds the previous value; a longer one (a dropout) is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dense {
    pub power_w: Vec<Option<f64>>,
    pub hr_bpm: Vec<Option<f64>>,
    pub cadence_rpm: Vec<Option<f64>>,
    pub speed_ms: Vec<Option<f64>>,
    /// The workout target of each second (`None`: free riding, no ERG).
    pub target_w: Vec<Option<f64>>,
}

impl Dense {
    pub fn len(&self) -> usize {
        self.power_w.len()
    }

    pub fn is_empty(&self) -> bool {
        self.power_w.is_empty()
    }
}

/// One channel of the recording on the one-second grid.
pub fn dense_channel(
    points: &[RecordPoint],
    value: impl Fn(&RecordPoint) -> Option<f64>,
) -> Vec<Option<f64>> {
    let mut out: Vec<Option<f64>> = Vec::new();
    let mut previous: Option<(usize, Option<f64>)> = None;
    let mut end = 0usize;
    for point in points {
        if !point.t.is_finite() || point.t < 0.0 {
            continue;
        }
        let slot = (point.t + 1e-9).floor() as usize;
        if slot >= MAX_SLOTS {
            break;
        }
        end = end.max(slot);
        let v = value(point).filter(|v| v.is_finite()).map(|v| v.max(0.0));
        if let Some((last, last_value)) = previous {
            if slot <= last {
                if let Some(cell) = out.get_mut(slot) {
                    *cell = v;
                }
                continue;
            }
            let hold = if (slot - last) as f64 <= MAX_SAMPLE_GAP_S {
                last_value
            } else {
                None
            };
            out.resize(slot, hold);
        } else {
            out.resize(slot, None);
        }
        out.push(v);
        previous = Some((slot, v));
    }
    // the last point closes the ride: it is not one more second of it
    out.truncate(end);
    out
}

/// The workout target of timer second `t` (`None` in a free step and after the workout).
pub fn target_at(workout: &RideWorkout, t: f64) -> Option<f64> {
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    let mut start = 0.0;
    for step in &workout.steps {
        let length = f64::from(step.duration_s);
        if t < start + length {
            return step.target.watts_at(t - start, length, workout.ftp_w);
        }
        start += length;
    }
    None
}

/// The targets of the first `len` seconds.
pub fn targets(workout: Option<&RideWorkout>, len: usize) -> Vec<Option<f64>> {
    match workout {
        Some(w) => (0..len).map(|t| target_at(w, t as f64 + 0.5)).collect(),
        None => vec![None; len],
    }
}

/// The whole recording on the grid.
pub fn dense(record: &RideRecord) -> Dense {
    let power_w = dense_channel(&record.points, |p| p.power_w);
    let len = power_w.len();
    let fit = |mut v: Vec<Option<f64>>| {
        v.resize(len, None);
        v
    };
    Dense {
        hr_bpm: fit(dense_channel(&record.points, |p| p.hr_bpm)),
        cadence_rpm: fit(dense_channel(&record.points, |p| p.cadence_rpm)),
        speed_ms: fit(dense_channel(&record.points, |p| Some(p.speed_ms))),
        target_w: targets(record.workout.as_ref(), len),
        power_w,
    }
}

/// The power of a ride for the FTP basis: the seconds the trainer held a workout target count
/// as 0 W, so no window that contains them can be an effort of the rider's own choosing.
pub fn free_power(record: &RideRecord) -> Vec<f64> {
    let power = crate::power_curve::dense_power(&record.points);
    match record.workout.as_ref() {
        None => power,
        Some(workout) => power
            .iter()
            .enumerate()
            .map(|(t, w)| {
                if target_at(workout, t as f64 + 0.5).is_some() {
                    0.0
                } else {
                    *w
                }
            })
            .collect(),
    }
}

/// What a lap is.
#[derive(Debug, Clone, PartialEq)]
pub enum LapKind {
    /// A step of the workout.
    Step { kind: StepKind, name: String },
    /// A hard effort found in the ride.
    Effort,
}

/// One lap (a workout step or an automatic interval) with what was ridden in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lap {
    pub kind: LapKind,
    /// Timer seconds `[start, end)`.
    pub start_s: usize,
    pub end_s: usize,
    /// Average target, watts (`None` in a free step).
    pub target_w: Option<f64>,
    pub avg_power_w: Option<f64>,
    pub np_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    /// Average power over average target, percent.
    pub compliance_pct: Option<f64>,
}

impl Lap {
    pub fn duration_s(&self) -> usize {
        self.end_s.saturating_sub(self.start_s)
    }
}

fn mean_of(values: &[Option<f64>]) -> Option<f64> {
    let (sum, n) = values
        .iter()
        .flatten()
        .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

fn max_of(values: &[Option<f64>]) -> Option<f64> {
    values.iter().flatten().copied().reduce(f64::max)
}

/// The numbers of seconds `[start, end)`. Power is averaged over time (a dropout second is 0 W,
/// as everywhere else); heart rate and cadence over the seconds that have a value.
fn lap_of(dense: &Dense, kind: LapKind, start: usize, end: usize) -> Lap {
    let end = end.min(dense.len());
    let start = start.min(end);
    let range = start..end;
    let power = &dense.power_w[range.clone()];
    let has_power = power.iter().any(Option::is_some);
    let avg_power_w = has_power
        .then(|| power.iter().map(|w| w.unwrap_or(0.0)).sum::<f64>() / power.len().max(1) as f64);
    let np_w = if has_power && power.len() >= 30 {
        normalized_power(power.iter().map(|w| w.unwrap_or(0.0)))
    } else {
        None
    };
    let target_w = mean_of(&dense.target_w[range.clone()]).filter(|_| {
        // a step is a target step when most of it had one
        dense.target_w[range.clone()].iter().flatten().count() * 2 >= power.len()
    });
    let compliance_pct = match (avg_power_w, target_w) {
        (Some(p), Some(t)) if t > 0.0 => Some(p / t * 100.0),
        _ => None,
    };
    Lap {
        kind,
        start_s: start,
        end_s: end,
        target_w,
        avg_power_w,
        np_w,
        max_power_w: max_of(power),
        avg_hr: mean_of(&dense.hr_bpm[range.clone()]),
        max_hr: max_of(&dense.hr_bpm[range.clone()]),
        avg_cadence: mean_of(&dense.cadence_rpm[range]),
        compliance_pct,
    }
}

/// The workout's steps as laps, cut at the end of the ride. Empty without a workout.
pub fn workout_laps(workout: Option<&RideWorkout>, dense: &Dense) -> Vec<Lap> {
    let Some(workout) = workout else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut start = 0usize;
    for step in &workout.steps {
        if start >= dense.len() {
            break;
        }
        let end = (start + step.duration_s as usize).min(dense.len());
        if end > start {
            out.push(lap_of(
                dense,
                LapKind::Step {
                    kind: step.kind,
                    name: step.name.clone(),
                },
                start,
                end,
            ));
        }
        start += step.duration_s as usize;
    }
    out
}

/// Hard efforts: stretches whose 30 s centred mean stays at or above [`AUTO_THRESHOLD`] × FTP
/// for at least [`AUTO_MIN_S`], dips shorter than [`AUTO_MERGE_S`] bridged. Without power or a
/// usable FTP there are none.
pub fn auto_intervals(dense: &Dense, ftp_w: f64) -> Vec<Lap> {
    if !(ftp_w.is_finite() && ftp_w > 0.0) || dense.power_w.iter().all(Option::is_none) {
        return Vec::new();
    }
    let power: Vec<f64> = dense.power_w.iter().map(|w| w.unwrap_or(0.0)).collect();
    let n = power.len();
    let mut prefix = Vec::with_capacity(n + 1);
    prefix.push(0.0);
    for w in &power {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + w);
    }
    let threshold = AUTO_THRESHOLD * ftp_w;
    let above: Vec<bool> = (0..n)
        .map(|i| {
            let a = i.saturating_sub(15);
            let b = (i + 15).min(n);
            (prefix[b] - prefix[a]) / (b - a).max(1) as f64 >= threshold
        })
        .collect();
    // runs of `above`, merged over short dips
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !above[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && above[i] {
            i += 1;
        }
        match runs.last_mut() {
            Some(last) if start - last.1 < AUTO_MERGE_S => last.1 = i,
            _ => runs.push((start, i)),
        }
    }
    // the centred mean blurs the edges by up to half its width: they move to where the watts
    // themselves cross the threshold
    let refine = |(a0, b0): (usize, usize)| {
        let (mut a, mut b) = (a0, b0);
        while a > 0 && a + 15 > a0 && power[a - 1] >= threshold {
            a -= 1;
        }
        while a < b && power[a] < threshold {
            a += 1;
        }
        while b < n && b < b0 + 15 && power[b] >= threshold {
            b += 1;
        }
        while b > a && power[b - 1] < threshold {
            b -= 1;
        }
        (a, b)
    };
    runs.into_iter()
        .map(refine)
        .filter(|(a, b)| b - a >= AUTO_MIN_S)
        .take(AUTO_MAX)
        .map(|(a, b)| lap_of(dense, LapKind::Effort, a, b))
        .collect()
}

/// W′ balance second by second (joules), `None` without power or a model.
pub fn w_prime_balance(dense: &Dense, model: &CpModel) -> Option<Vec<f64>> {
    if dense.power_w.iter().all(Option::is_none) || model.w_prime_j <= 0.0 || model.cp_w <= 0.0 {
        return None;
    }
    let w0 = model.w_prime_j;
    let mut balance = w0;
    Some(
        dense
            .power_w
            .iter()
            .map(|w| {
                let p = w.unwrap_or(0.0);
                if p > model.cp_w {
                    balance -= p - model.cp_w;
                } else {
                    balance += (model.cp_w - p) * (w0 - balance) / w0;
                }
                balance = balance.min(w0);
                balance
            })
            .collect(),
    )
}

/// Everything the ride view shows besides the summary that was saved with the ride.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RideDetail {
    pub duration_s: usize,
    pub avg_power_w: Option<f64>,
    pub np_w: Option<f64>,
    /// NP / average power.
    pub variability_index: Option<f64>,
    /// NP / average heart rate.
    pub efficiency_factor: Option<f64>,
    /// Pw:HR drift of the second half against the first, percent (positive = drift up).
    pub decoupling_pct: Option<f64>,
    /// Average and normalised power per kilogram of the rider.
    pub avg_w_per_kg: Option<f64>,
    pub np_w_per_kg: Option<f64>,
    pub work_kj: Option<f64>,
    /// Work done above FTP, kJ.
    pub work_above_ftp_kj: Option<f64>,
    /// The lowest W′ balance of the ride, joules, and its share of W′ (percent).
    pub w_prime_min_j: Option<f64>,
    pub w_prime_min_pct: Option<f64>,
    /// Seconds per power zone Z1..Z7, one second at a time.
    pub power_zone_secs: [f64; 7],
    /// Seconds per heart-rate zone with the current model.
    pub hr_zone_secs: [f64; 5],
    /// Seconds of pedalling per cadence band (see [`CADENCE_BANDS`]; the last one is above them).
    pub cadence_secs: [f64; 7],
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    /// The seconds the trainer held a workout target.
    pub erg_s: usize,
}

/// The shortest ride (seconds, with both power and heart rate) whose decoupling means something.
pub const DECOUPLING_MIN_S: usize = 1_200;

/// The derived numbers of a ride. `ftp_w` and `rider_kg` are the ride's own (stored with it) or
/// else the current settings; `rider` gives the heart-rate zones; `model` the W′ balance.
pub fn ride_detail(
    dense: &Dense,
    ftp_w: f64,
    rider_kg: f64,
    rider: &ride_model::RiderSettings,
    model: Option<&CpModel>,
) -> RideDetail {
    let n = dense.len();
    let has_power = dense.power_w.iter().any(Option::is_some);
    let power: Vec<f64> = dense.power_w.iter().map(|w| w.unwrap_or(0.0)).collect();
    let avg_power_w = (has_power && n > 0).then(|| power.iter().sum::<f64>() / n as f64);
    let np_w = if has_power {
        normalized_power(power.iter().copied())
    } else {
        None
    };
    let avg_hr = mean_of(&dense.hr_bpm);
    let variability_index = match (np_w, avg_power_w) {
        (Some(np), Some(avg)) if avg > 0.0 => Some(np / avg),
        _ => None,
    };
    let efficiency_factor = match (np_w, avg_hr) {
        (Some(np), Some(hr)) if hr > 0.0 => Some(np / hr),
        _ => None,
    };
    let decoupling_pct = decoupling(dense);
    let per_kg = |w: Option<f64>| {
        w.filter(|_| rider_kg.is_finite() && rider_kg > 0.0)
            .map(|w| w / rider_kg)
    };
    let ftp_ok = ftp_w.is_finite() && ftp_w > 0.0;
    let work_kj = has_power.then(|| power.iter().sum::<f64>() / 1000.0);
    let work_above_ftp_kj = (has_power && ftp_ok)
        .then(|| power.iter().map(|w| (w - ftp_w).max(0.0)).sum::<f64>() / 1000.0);
    let balance = model.and_then(|m| w_prime_balance(dense, m));
    // below zero the effort beat the model (it is not maximal data yet): shown as empty
    let w_prime_min_j = balance
        .as_ref()
        .and_then(|b| b.iter().copied().reduce(f64::min))
        .map(|j| j.max(0.0));
    let w_prime_min_pct = w_prime_min_j
        .zip(model)
        .map(|(min, m)| min / m.w_prime_j * 100.0);

    let mut power_zone_secs = [0.0; 7];
    if has_power && ftp_ok {
        for w in dense.power_w.iter().flatten() {
            let zone = zones::power_zone(*w, ftp_w);
            if (1..=7).contains(&zone) {
                power_zone_secs[usize::from(zone - 1)] += 1.0;
            }
        }
    }
    let mut hr_zone_secs = [0.0; 5];
    for hr in dense.hr_bpm.iter().flatten() {
        let zone = zones::hr_zone_of(*hr, rider);
        if (1..=5).contains(&zone) {
            hr_zone_secs[usize::from(zone - 1)] += 1.0;
        }
    }
    let mut cadence_secs = [0.0; 7];
    for c in dense.cadence_rpm.iter().flatten().filter(|c| **c > 0.0) {
        let band = CADENCE_BANDS.iter().filter(|b| *c >= **b).count();
        cadence_secs[band] += 1.0;
    }
    RideDetail {
        duration_s: n,
        avg_power_w,
        np_w,
        variability_index,
        efficiency_factor,
        decoupling_pct,
        avg_w_per_kg: per_kg(avg_power_w),
        np_w_per_kg: per_kg(np_w),
        work_kj,
        work_above_ftp_kj,
        w_prime_min_j,
        w_prime_min_pct,
        power_zone_secs,
        hr_zone_secs,
        cadence_secs,
        avg_hr,
        max_hr: max_of(&dense.hr_bpm),
        avg_cadence: mean_of(
            &dense
                .cadence_rpm
                .iter()
                .map(|c| c.filter(|c| *c > 0.0))
                .collect::<Vec<_>>(),
        ),
        erg_s: dense.target_w.iter().flatten().count(),
    }
}

/// Pw:HR decoupling: `(EF₁ − EF₂) / EF₁` in percent, EF being average power over average heart
/// rate in each half of the seconds that have both. `None` when fewer than
/// [`DECOUPLING_MIN_S`] seconds have both.
pub fn decoupling(dense: &Dense) -> Option<f64> {
    let both: Vec<(f64, f64)> = dense
        .power_w
        .iter()
        .zip(&dense.hr_bpm)
        .filter_map(|(p, h)| Some((p.unwrap_or(0.0), (*h)?)))
        .filter(|(_, h)| *h > 0.0)
        .collect();
    if both.len() < DECOUPLING_MIN_S || dense.power_w.iter().all(Option::is_none) {
        return None;
    }
    let (first, second) = both.split_at(both.len() / 2);
    let ef = |half: &[(f64, f64)]| {
        let n = half.len().max(1) as f64;
        let p = half.iter().map(|(p, _)| p).sum::<f64>() / n;
        let h = half.iter().map(|(_, h)| h).sum::<f64>() / n;
        (h > 0.0).then(|| p / h)
    };
    let (a, b) = (ef(first)?, ef(second)?);
    (a > 0.0).then(|| (a - b) / a * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RideSource, RideSummary, StepTarget, WorkoutStep};

    fn point(t: f64, power: Option<f64>, hr: Option<f64>) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist: t * 8.0,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0,
            speed_ms: 8.0,
            power_w: power,
            cadence_rpm: Some(90.0),
            hr_bpm: hr,
            grade: 0.0,
        }
    }

    fn record(seconds: u32, watts: impl Fn(u32) -> f64, hr: impl Fn(u32) -> f64) -> RideRecord {
        RideRecord {
            completed_calendar_id: None,
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: "r".into(),
            route_name: "R".into(),
            simulated: false,
            summary: RideSummary::default(),
            points: (0..=seconds)
                .map(|t| point(f64::from(t), Some(watts(t)), Some(hr(t))))
                .collect(),
            source: RideSource::Recorded,
            power_estimated: false,
            estimate: None,
            measured_power: Vec::new(),
            ftp_w: Some(250.0),
            rider_kg: Some(70.0),
            workout: None,
            rpe: None,
            note: String::new(),
        }
    }

    fn step(kind: StepKind, seconds: u32, target: StepTarget) -> WorkoutStep {
        WorkoutStep {
            kind,
            name: format!("{kind:?}"),
            duration_s: seconds,
            target,
            cadence: None,
            note: None,
            messages: Vec::new(),
        }
    }

    fn workout() -> RideWorkout {
        RideWorkout {
            id: "w".into(),
            name: "Teszt".into(),
            ftp_w: 250.0,
            steps: vec![
                step(StepKind::Warmup, 60, StepTarget::Free),
                step(StepKind::Interval, 120, StepTarget::PctFtp { value: 120.0 }),
                step(StepKind::Recovery, 60, StepTarget::Watts { value: 100.0 }),
            ],
        }
    }

    #[test]
    fn channels_hold_short_gaps_and_drop_long_ones() {
        let points = vec![
            point(0.0, Some(100.0), None),
            point(1.0, Some(110.0), None),
            point(4.0, Some(120.0), None),
            point(20.0, Some(130.0), None),
            point(21.0, Some(130.0), None),
        ];
        let power = dense_channel(&points, |p| p.power_w);
        assert_eq!(power.len(), 21);
        assert_eq!(power[2], Some(110.0), "held");
        assert_eq!(power[4], Some(120.0));
        assert_eq!(power[10], None, "a dropout");
        assert_eq!(power[20], Some(130.0));
        assert!(dense_channel(&[], |p| p.power_w).is_empty());
    }

    #[test]
    fn workout_targets_follow_the_steps() {
        let w = workout();
        assert_eq!(target_at(&w, 30.0), None, "free warm-up");
        assert_eq!(target_at(&w, 61.0), Some(300.0));
        assert_eq!(target_at(&w, 200.0), Some(100.0));
        assert_eq!(target_at(&w, 240.0), None, "after the workout");
        assert_eq!(target_at(&w, f64::NAN), None);
    }

    #[test]
    fn the_ftp_basis_leaves_out_the_erg_seconds() {
        let mut r = record(300, |_| 280.0, |_| 150.0);
        r.workout = Some(workout());
        let free = free_power(&r);
        assert_eq!(free.len(), 300);
        assert_eq!(free[30], 280.0);
        assert_eq!(free[100], 0.0, "ERG");
        assert_eq!(free[250], 280.0, "after the workout");
        // no workout: all of it
        r.workout = None;
        assert!(free_power(&r).iter().all(|w| *w == 280.0));
    }

    #[test]
    fn laps_compare_what_was_ridden_with_the_target() {
        // 300 W asked in the interval, 285 ridden
        let mut r = record(
            300,
            |t| match t {
                60..=179 => 285.0,
                _ => 100.0,
            },
            |_| 150.0,
        );
        r.workout = Some(workout());
        let d = dense(&r);
        let laps = workout_laps(r.workout.as_ref(), &d);
        assert_eq!(laps.len(), 3);
        assert_eq!((laps[1].start_s, laps[1].end_s), (60, 180));
        assert_eq!(laps[0].target_w, None);
        assert_eq!(laps[0].compliance_pct, None);
        assert_eq!(laps[1].target_w, Some(300.0));
        assert!((laps[1].compliance_pct.unwrap() - 95.0).abs() < 1e-9);
        assert!((laps[2].compliance_pct.unwrap() - 100.0).abs() < 1e-9);
        // a ride that stopped inside the workout cuts the last lap
        let short = record(100, |_| 200.0, |_| 140.0);
        let laps = workout_laps(Some(&workout()), &dense(&short));
        assert_eq!(laps.len(), 2);
        assert_eq!(laps[1].end_s, 100);
    }

    #[test]
    fn hard_efforts_are_found_and_short_dips_bridged() {
        // 5 min easy, 4 min at 280 W (with a 10 s dip), 5 min easy, a 30 s burst
        let r = record(
            900,
            |t| match t {
                300..=419 | 430..=539 => 280.0,
                700..=729 => 400.0,
                _ => 120.0,
            },
            |_| 140.0,
        );
        let laps = auto_intervals(&dense(&r), 250.0);
        assert_eq!(laps.len(), 1, "{laps:?}");
        let lap = &laps[0];
        assert!(lap.start_s >= 285 && lap.start_s <= 300, "{lap:?}");
        assert!(lap.end_s >= 540 && lap.end_s <= 556, "{lap:?}");
        assert_eq!(lap.kind, LapKind::Effort);
        assert!(auto_intervals(&dense(&r), 0.0).is_empty());
    }

    #[test]
    fn the_derived_numbers_of_a_steady_ride() {
        // an hour at 200 W; the heart rate drifts from 140 to 154 over it
        let r = record(3_600, |_| 200.0, |t| 140.0 + 14.0 * f64::from(t) / 3_600.0);
        let rider = ride_model::RiderSettings::default();
        let d = ride_detail(&dense(&r), 250.0, 70.0, &rider, None);
        assert_eq!(d.duration_s, 3_600);
        assert!((d.avg_power_w.unwrap() - 200.0).abs() < 1e-9);
        assert!((d.variability_index.unwrap() - 1.0).abs() < 1e-6);
        assert!((d.avg_w_per_kg.unwrap() - 200.0 / 70.0).abs() < 1e-9);
        assert!((d.work_kj.unwrap() - 720.0).abs() < 1e-6);
        assert_eq!(d.work_above_ftp_kj, Some(0.0));
        let decoupling = d.decoupling_pct.unwrap();
        // first half ~143.5 bpm, second ~150.5: about 4.7 % drift
        assert!(decoupling > 4.0 && decoupling < 5.5, "{decoupling}");
        assert_eq!(d.power_zone_secs[2], 3_600.0, "80 % of FTP is Z3");
        assert_eq!(d.cadence_secs[4], 3_600.0, "90 rpm is the 90..100 band");
        assert_eq!(d.erg_s, 0);
        assert_eq!(d.w_prime_min_j, None);
    }

    #[test]
    fn w_prime_drains_above_cp_and_comes_back_below() {
        let model = CpModel {
            cp_w: 250.0,
            w_prime_j: 20_000.0,
            points: 4,
            r2: 1.0,
        };
        // 100 s at 350 W drains 10 kJ, then easy riding refills it
        let r = record(400, |t| if t < 100 { 350.0 } else { 100.0 }, |_| 150.0);
        let balance = w_prime_balance(&dense(&r), &model).unwrap();
        assert!((balance[99] - 10_000.0).abs() < 1e-6, "{}", balance[99]);
        assert!(balance[399] > 18_000.0 && balance[399] < 20_000.0);
        let d = ride_detail(
            &dense(&r),
            250.0,
            70.0,
            &ride_model::RiderSettings::default(),
            Some(&model),
        );
        assert!((d.w_prime_min_pct.unwrap() - 50.0).abs() < 1e-6);
    }

    #[test]
    fn a_short_ride_has_no_decoupling() {
        let r = record(600, |_| 200.0, |_| 140.0);
        assert_eq!(decoupling(&dense(&r)), None);
    }
}
