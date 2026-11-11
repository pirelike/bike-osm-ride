//! Power over time: the mean-maximal power curve, the FTP it suggests and the weekly / monthly
//! training load.
//!
//! * The **mean-maximal power** (the "power curve") is, for a window length, the highest average
//!   power the rider held over any stretch of that length: [`DURATIONS`] are 5 s, 1, 5, 20 and 60
//!   minutes. A ride shorter than the window has no value for it (`None`), it is not padded with
//!   zeros.
//! * The **FTP estimate** is 95 % of the best 20 minutes ([`estimate_ftp`]), the usual field-test
//!   rule. It is a suggestion; the rider decides whether to apply it.
//! * The **trend** adds up the rides per week (Monday to Sunday, UTC) or calendar month.
//!
//! Only **measured** watts count ([`is_measured`]): a simulated rider's power comes from the
//! physics model and an estimated ride's watts were computed backwards from speed and grade, so an
//! FTP derived from either would only confirm the model's own assumptions.
//!
//! Pure functions: no clock (the caller passes `now`), no I/O. The index of the ride log keeps the
//! per-ride numbers ([`index_numbers`]), so [`analyze`] never opens a record.

use ride_model::{RecordPoint, RideMeta, RideRecord, RouteRides};

use crate::analysis::MAX_SAMPLE_GAP_S;

/// Window lengths of the curve, seconds. The index of a ride stores one value per entry, in this
/// order; a stored list of another length is treated as "not computed".
pub const DURATIONS: [u32; 5] = [5, 60, 300, 1200, 3600];

/// Index of the 20 minute window in [`DURATIONS`] (the FTP test length).
pub const FTP_WINDOW: usize = 3;

/// The FTP test rule: 95 % of the best 20 minutes.
pub const FTP_FACTOR: f64 = 0.95;

/// The setting range of the FTP field, watts.
pub const FTP_MIN_W: f64 = 50.0;
pub const FTP_MAX_W: f64 = 600.0;

/// A recording longer than this is not read any further (28 hours; a corrupt timestamp must not
/// allocate gigabytes).
const MAX_SLOTS: usize = 100_000;

/// Whether the watt values of a ride are the rider's own: neither simulated nor estimated.
pub fn is_measured(meta: &RideMeta) -> bool {
    !meta.simulated && !meta.power_estimated
}

/// The power of a recording on a one-second grid, indexed by timer time.
///
/// A second without a sample takes the previous sample's value when the gap is short
/// (`MAX_SAMPLE_GAP_S`, a late frame), and counts as 0 W when it is longer (a dropout); a sample
/// without power is 0 W too. Both are on the safe side: they can only lower an average.
pub fn dense_power(points: &[RecordPoint]) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    let mut previous: Option<(usize, f64)> = None;
    let mut end = 0usize;
    for point in points {
        if !point.t.is_finite() || point.t < 0.0 {
            continue;
        }
        let slot = (point.t + 1e-9).floor() as usize;
        end = end.max(slot.min(MAX_SLOTS));
        if slot >= MAX_SLOTS {
            break;
        }
        let watts = point
            .power_w
            .filter(|w| w.is_finite())
            .map_or(0.0, |w| w.max(0.0));
        if let Some((last, last_watts)) = previous {
            if slot <= last {
                // two samples in the same second: the later one wins
                if let Some(cell) = out.get_mut(slot) {
                    *cell = watts;
                }
                previous = Some((last, if slot == last { watts } else { last_watts }));
                continue;
            }
            let hold = if (slot - last) as f64 <= MAX_SAMPLE_GAP_S {
                last_watts
            } else {
                0.0
            };
            out.resize(slot, hold);
        } else {
            out.resize(slot, 0.0);
        }
        out.push(watts);
        previous = Some((slot, watts));
    }
    // A point at t=N is an endpoint, not evidence for an extra second after N.
    out.truncate(end);
    out
}

/// Whether any sample of the recording carries a power value.
pub fn has_power(points: &[RecordPoint]) -> bool {
    points.iter().any(|p| p.power_w.is_some_and(f64::is_finite))
}

/// The best average power for every window length in `durations`, watts. `None` for a window
/// longer than the ride, and for every window when the recording has no power at all.
pub fn mean_max(points: &[RecordPoint], durations: &[u32]) -> Vec<Option<f64>> {
    if !has_power(points) {
        return vec![None; durations.len()];
    }
    let power = dense_power(points);
    let mut prefix = Vec::with_capacity(power.len() + 1);
    prefix.push(0.0);
    let mut sum = 0.0;
    for watts in &power {
        sum += watts;
        prefix.push(sum);
    }
    durations
        .iter()
        .map(|&seconds| {
            let window = seconds as usize;
            if window == 0 || power.len() < window {
                return None;
            }
            (0..=power.len() - window)
                .map(|start| (prefix[start + window] - prefix[start]) / f64::from(seconds))
                .reduce(f64::max)
        })
        .collect()
}

/// FTP suggested by the best 20 minute power: 95 %, whole watts, inside the setting's range.
pub fn estimate_ftp(best_20min_w: Option<f64>) -> Option<f64> {
    best_20min_w
        .filter(|w| w.is_finite() && *w > 0.0)
        .map(|w| (w * FTP_FACTOR).round().clamp(FTP_MIN_W, FTP_MAX_W))
}

/// What the index of the ride log keeps about one ride: its TSS and its curve (one value per
/// [`DURATIONS`] entry, watts as `f32`). The curve of a ride whose watts are not the rider's own is
/// all `None`, so the "not computed yet" state (`None` in the index) is left only for old entries.
pub fn index_numbers(record: &RideRecord) -> (Option<f64>, Option<Vec<Option<f32>>>) {
    let tss = record.summary.tss.filter(|t| t.is_finite());
    let measured = !record.simulated && !record.power_estimated;
    let curve = if measured {
        mean_max(&record.points, &DURATIONS)
            .into_iter()
            .map(|w| w.map(|w| w as f32))
            .collect()
    } else {
        vec![None; DURATIONS.len()]
    };
    (tss, Some(curve))
}

/// The stored curve of an index entry, when it has one for the current [`DURATIONS`].
pub fn stored_curve(meta: &RideMeta) -> Option<&[Option<f32>]> {
    meta.mmp
        .as_deref()
        .filter(|curve| curve.len() == DURATIONS.len())
}

/// The ride that holds the best value of one window length.
#[derive(Debug, Clone, PartialEq)]
pub struct Best {
    pub watts: f64,
    pub route_id: String,
    pub route_name: String,
    pub stem: String,
    pub started_unix_s: f64,
}

/// FTP suggested by the best 20 minutes of all measured rides.
#[derive(Debug, Clone, PartialEq)]
pub struct FtpEstimate {
    pub ftp_w: f64,
    /// The 20 minute power it was derived from.
    pub best_20min_w: f64,
    pub started_unix_s: f64,
}

/// Calendar unit of the trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Monday to Sunday, UTC.
    Week,
    /// Calendar month, UTC.
    Month,
}

/// The rides of one week or month.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    /// First second of the period (UTC midnight of its first day).
    pub start_unix_s: f64,
    pub rides: usize,
    /// Riding time, seconds (every ride that is not simulated).
    pub time_s: f64,
    /// Training stress of the rides with measured watts.
    pub tss: f64,
}

/// Everything the "Teljesítmény" page shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    /// One entry per [`DURATIONS`] window; `None` when no measured ride is long enough.
    pub best: Vec<Option<Best>>,
    pub ftp_estimate: Option<FtpEstimate>,
    /// The last twelve weeks, oldest first, empty weeks included.
    pub weeks: Vec<Bucket>,
    /// The last twelve months, oldest first, empty months included.
    pub months: Vec<Bucket>,
    /// Measured rides that went into the curve.
    pub rides_counted: usize,
    /// Measured rides whose numbers are not in the index yet.
    pub rides_pending: usize,
}

/// Number of periods in the trend.
pub const TREND_LEN: usize = 12;

/// The curve, the FTP suggestion and the trend of every ride in `all`. `now_unix_s` fixes the
/// last week / month of the trend.
pub fn analyze(all: &[RouteRides], now_unix_s: f64) -> Analysis {
    let mut best: Vec<Option<Best>> = vec![None; DURATIONS.len()];
    let mut counted = 0;
    let mut pending = 0;
    let mut rides: Vec<&RideMeta> = Vec::new();
    for route in all {
        for meta in &route.rides {
            rides.push(meta);
            if !is_measured(meta) {
                continue;
            }
            let Some(curve) = stored_curve(meta) else {
                pending += 1;
                continue;
            };
            if curve.iter().any(Option::is_some) {
                counted += 1;
            }
            for (slot, watts) in best.iter_mut().zip(curve) {
                let Some(watts) = watts.map(f64::from).filter(|w| w.is_finite()) else {
                    continue;
                };
                if slot.as_ref().is_none_or(|b| watts > b.watts) {
                    *slot = Some(Best {
                        watts,
                        route_id: route.route_id.clone(),
                        route_name: route.route_name.clone(),
                        stem: meta.stem.clone(),
                        started_unix_s: meta.started_unix_s,
                    });
                }
            }
        }
    }
    let ftp_estimate = best.get(FTP_WINDOW).and_then(Option::as_ref).and_then(|b| {
        estimate_ftp(Some(b.watts)).map(|ftp_w| FtpEstimate {
            ftp_w,
            best_20min_w: b.watts,
            started_unix_s: b.started_unix_s,
        })
    });
    Analysis {
        best,
        ftp_estimate,
        weeks: bucket(&rides, Period::Week, TREND_LEN, now_unix_s),
        months: bucket(&rides, Period::Month, TREND_LEN, now_unix_s),
        rides_counted: counted,
        rides_pending: pending,
    }
}

/// The last `count` periods ending with the one that contains `until_unix_s`, oldest first, each
/// with what the rides of that period add up to. Simulated rides do not count at all; the TSS of a
/// ride with estimated watts is left out (the time stays).
pub fn bucket(rides: &[&RideMeta], period: Period, count: usize, until_unix_s: f64) -> Vec<Bucket> {
    if count == 0 || count > 1200 || !valid_timestamp(until_unix_s) {
        return Vec::new();
    }
    let mut starts: Vec<i64> = Vec::with_capacity(count);
    let mut current = period_start(day_of(until_unix_s), period);
    for _ in 0..count {
        starts.push(current);
        current = previous_start(current, period);
    }
    starts.reverse();
    let mut out: Vec<Bucket> = starts
        .iter()
        .map(|&day| Bucket {
            start_unix_s: day as f64 * 86_400.0,
            rides: 0,
            time_s: 0.0,
            tss: 0.0,
        })
        .collect();
    for meta in rides {
        if meta.simulated || !valid_timestamp(meta.started_unix_s) {
            continue;
        }
        let start = period_start(day_of(meta.started_unix_s), period);
        let Ok(index) = starts.binary_search(&start) else {
            continue;
        };
        let bucket = &mut out[index];
        bucket.rides += 1;
        bucket.time_s += meta.elapsed_s.max(0.0);
        if !meta.power_estimated {
            bucket.tss += meta.tss.filter(|t| t.is_finite()).unwrap_or(0.0);
        }
    }
    out
}

/// Supported calendar range: 0001-01-01 through 9999-12-31 (UTC).
pub fn valid_timestamp(value: f64) -> bool {
    value.is_finite() && (-62_135_596_800.0..253_402_300_800.0).contains(&value)
}

/// Days since the Unix epoch of the UTC day containing the instant.
fn day_of(unix_s: f64) -> i64 {
    (unix_s / 86_400.0).floor() as i64
}

/// The first day of the week (Monday) or month containing `day`.
fn period_start(day: i64, period: Period) -> i64 {
    match period {
        // 1970-01-01 was a Thursday: day 0 is 3 days after a Monday
        Period::Week => day - (day + 3).rem_euclid(7),
        Period::Month => {
            let (year, month, _) = civil_from_days(day);
            days_from_civil(year, month, 1)
        }
    }
}

/// The start of the period before the one starting on `start`.
fn previous_start(start: i64, period: Period) -> i64 {
    match period {
        Period::Week => start - 7,
        Period::Month => {
            let (year, month, _) = civil_from_days(start);
            if month == 1 {
                days_from_civil(year - 1, 12, 1)
            } else {
                days_from_civil(year, month - 1, 1)
            }
        }
    }
}

/// `(year, month, day)` of a day count since 1970-01-01 (proleptic Gregorian).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.clamp(-3_652_425, 3_652_425) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RideSource, RideSummary};

    fn point(t: f64, power: Option<f64>) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_700_000_000.0 + t,
            dist: t * 8.0,
            lat: 47.5,
            lon: 19.0,
            ele: 100.0,
            speed_ms: 8.0,
            power_w: power,
            cadence_rpm: None,
            hr_bpm: None,
            grade: 0.0,
        }
    }

    /// A ride of `seconds` seconds: one sample per second, `watts(t)` for `t` in `0..=seconds`
    /// (the last sample is the end of the ride, not another second of it).
    fn ride(seconds: u32, watts: impl Fn(u32) -> f64) -> Vec<RecordPoint> {
        (0..=seconds)
            .map(|t| point(f64::from(t), Some(watts(t))))
            .collect()
    }

    fn meta(stem: &str, started: f64, mmp: Option<Vec<Option<f32>>>) -> RideMeta {
        RideMeta {
            stem: stem.into(),
            started_unix_s: started,
            elapsed_s: 3_600.0,
            mmp,
            ..RideMeta::default()
        }
    }

    fn curve(values: [f32; 5]) -> Option<Vec<Option<f32>>> {
        Some(values.iter().map(|v| Some(*v)).collect())
    }

    fn route_of(id: &str, rides: Vec<RideMeta>) -> RouteRides {
        RouteRides {
            route_id: id.into(),
            route_name: format!("Út {id}"),
            rides,
        }
    }

    #[test]
    fn a_constant_ride_has_the_same_best_for_every_window_it_fills() {
        let points = ride(1_500, |_| 200.0);
        let best = mean_max(&points, &DURATIONS);
        assert_eq!(best.len(), 5);
        for value in &best[..4] {
            assert!((value.expect("filled") - 200.0).abs() < 1e-9);
        }
        assert_eq!(best[4], None, "a 25 minute ride has no 60 minute value");
    }

    #[test]
    fn the_best_window_is_found_wherever_it_sits() {
        // 10 min easy, 5 min at 300 W, 10 min easy, a 5 s sprint at 900 W in the middle of the effort
        let points = ride(1_500, |t| match t {
            750..=754 => 900.0,
            600..=899 => 300.0,
            _ => 150.0,
        });
        let best = mean_max(&points, &DURATIONS);
        assert!((best[0].expect("5 s") - 900.0).abs() < 1e-9);
        // 1 min: the sprint plus 55 s at 300 W
        assert!((best[1].expect("1 min") - (900.0 * 5.0 + 300.0 * 55.0) / 60.0).abs() < 1e-9);
        // 5 min: exactly the effort, with the sprint inside it
        assert!((best[2].expect("5 min") - (300.0 * 295.0 + 900.0 * 5.0) / 300.0).abs() < 1e-9);
        // 20 min: the effort plus 15 easy minutes, wherever they fall around it
        let twenty = (300.0 * 295.0 + 900.0 * 5.0 + 150.0 * 900.0) / 1_200.0;
        assert!((best[3].expect("20 min") - twenty).abs() < 1e-9);
    }

    #[test]
    fn a_ride_shorter_than_the_window_has_no_value() {
        let points = ride(59, |_| 250.0);
        let best = mean_max(&points, &DURATIONS);
        assert!(best[0].is_some());
        assert_eq!(&best[1..], &[None, None, None, None]);
    }

    #[test]
    fn a_ride_without_any_power_has_no_curve_at_all() {
        let points: Vec<_> = (0..600).map(|t| point(f64::from(t), None)).collect();
        assert_eq!(mean_max(&points, &DURATIONS), vec![None; 5]);
        assert_eq!(mean_max(&[], &DURATIONS), vec![None; 5]);
    }

    #[test]
    fn a_sensor_dropout_counts_as_zero_and_never_raises_an_average() {
        // 200 W with ten seconds of nothing in the middle
        let mut points = ride(120, |_| 200.0);
        for p in &mut points[50..60] {
            p.power_w = None;
        }
        let best = mean_max(&points, &[60]);
        // the best minute avoids the dropout entirely
        assert!((best[0].expect("1 min") - 200.0).abs() < 1e-9);
        // two minutes cannot: ten seconds of zero pull the average down
        let whole = mean_max(&points, &[120]);
        let expected = 200.0 * 110.0 / 120.0;
        assert!((whole[0].expect("2 min") - expected).abs() < 1e-9);
    }

    #[test]
    fn samples_are_placed_by_time_not_by_index() {
        // the engine can skip a second under load: 0, 1, 2, 4, 5 (3 s is missing, a short gap);
        // the ride is 5 s long, so it has 5 one-second cells
        let points: Vec<_> = [0.0, 1.0, 2.0, 4.0, 5.0]
            .into_iter()
            .map(|t| point(t, Some(100.0)))
            .collect();
        let dense = dense_power(&points);
        assert_eq!(dense.len(), 5);
        assert!(dense.iter().all(|w| (*w - 100.0).abs() < 1e-9), "{dense:?}");
        // a long gap is a dropout: zeros
        let gap: Vec<_> = [0.0, 1.0, 30.0, 31.0]
            .into_iter()
            .map(|t| point(t, Some(100.0)))
            .collect();
        let dense = dense_power(&gap);
        assert_eq!(dense.len(), 31);
        assert!((dense[1] - 100.0).abs() < 1e-9);
        assert!(dense[2..30].iter().all(|w| *w == 0.0));
        assert!((dense[30] - 100.0).abs() < 1e-9);
    }

    #[test]
    fn nonsense_samples_do_not_poison_the_curve() {
        let mut points = ride(120, |_| 200.0);
        points[10].power_w = Some(f64::NAN);
        points[11].power_w = Some(-50.0);
        points[12].t = f64::NAN;
        points.push(point(1.0e12, Some(1.0e6)));
        let best = mean_max(&points, &[60]);
        let value = best[0].expect("1 min");
        assert!(value.is_finite() && value <= 200.0, "{value}");
    }

    #[test]
    fn the_ftp_is_95_percent_of_the_best_20_minutes() {
        assert_eq!(estimate_ftp(Some(250.0)), Some(238.0));
        assert_eq!(estimate_ftp(Some(251.0)), Some(238.0)); // 238.45
        assert_eq!(
            estimate_ftp(Some(10.0)),
            Some(50.0),
            "clamped up to the setting's range"
        );
        assert_eq!(estimate_ftp(Some(2_000.0)), Some(600.0), "and down");
        assert_eq!(estimate_ftp(Some(f64::NAN)), None);
        assert_eq!(estimate_ftp(Some(0.0)), None);
        assert_eq!(estimate_ftp(None), None);
    }

    fn record(simulated: bool, estimated: bool) -> RideRecord {
        RideRecord {
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: "r".into(),
            route_name: "Út".into(),
            simulated,
            summary: RideSummary {
                route_name: "Út".into(),
                route_id: "r".into(),
                started_unix_s: 1_700_000_000.0,
                elapsed_s: 1_500.0,
                dist: 12_000.0,
                ascent: 0.0,
                avg_speed_ms: 8.0,
                max_speed_ms: 9.0,
                avg_power_w: Some(200.0),
                max_power_w: Some(200.0),
                normalized_power_w: Some(200.0),
                intensity_factor: Some(0.8),
                tss: Some(27.0),
                work_kj: 300.0,
                kcal: 300.0,
                avg_hr: None,
                max_hr: None,
                avg_cadence: None,
                power_zone_secs: [0.0; 7],
                hr_zone_secs: [0.0; 5],
            },
            points: ride(1_500, |_| 200.0),
            source: RideSource::Recorded,
            power_estimated: estimated,
            estimate: None,
            measured_power: Vec::new(),
        }
    }

    #[test]
    fn the_index_keeps_the_curve_of_a_measured_ride_only() {
        let (tss, curve) = index_numbers(&record(false, false));
        assert_eq!(tss, Some(27.0));
        let curve = curve.expect("computed");
        assert_eq!(curve.len(), DURATIONS.len());
        assert!((curve[3].expect("20 min") - 200.0).abs() < 1e-3);
        assert_eq!(curve[4], None);

        for (simulated, estimated) in [(true, false), (false, true)] {
            let (_, curve) = index_numbers(&record(simulated, estimated));
            assert_eq!(
                curve,
                Some(vec![None; DURATIONS.len()]),
                "computed, and empty: not the rider's own watts"
            );
        }
    }

    #[test]
    fn the_best_of_all_rides_and_the_ftp_come_from_the_index() {
        let low = meta(
            "1700000000",
            1_700_000_000.0,
            curve([500.0, 300.0, 250.0, 220.0, 200.0]),
        );
        let high = meta(
            "1700100000",
            1_700_100_000.0,
            curve([450.0, 320.0, 240.0, 240.0, 0.0]),
        );
        let all = [route_of("a", vec![low]), route_of("b", vec![high])];
        let analysis = analyze(&all, 1_700_200_000.0);
        assert_eq!(analysis.best.len(), 5);
        let best = |i: usize| analysis.best[i].as_ref().expect("best");
        assert_eq!(best(0).watts, 500.0);
        assert_eq!(best(0).route_id, "a");
        assert_eq!(best(1).watts, 320.0);
        assert_eq!(best(1).stem, "1700100000");
        assert_eq!(best(3).watts, 240.0);
        let ftp = analysis.ftp_estimate.expect("ftp");
        assert_eq!(ftp.ftp_w, 228.0);
        assert_eq!(ftp.best_20min_w, 240.0);
        assert_eq!(ftp.started_unix_s, 1_700_100_000.0);
        assert_eq!((analysis.rides_counted, analysis.rides_pending), (2, 0));
    }

    #[test]
    fn simulated_and_estimated_rides_never_reach_the_curve() {
        let mut simulated = meta("1", 1_700_000_000.0, curve([900.0; 5]));
        simulated.simulated = true;
        let mut estimated = meta("2", 1_700_010_000.0, curve([800.0; 5]));
        estimated.power_estimated = true;
        let measured = meta(
            "3",
            1_700_020_000.0,
            curve([200.0, 190.0, 180.0, 170.0, 160.0]),
        );
        let analysis = analyze(
            &[route_of("a", vec![simulated, estimated, measured])],
            1_700_030_000.0,
        );
        assert_eq!(analysis.best[0].as_ref().expect("best").watts, 200.0);
        assert_eq!(analysis.ftp_estimate.expect("ftp").best_20min_w, 170.0);
        assert_eq!(analysis.rides_counted, 1);
    }

    #[test]
    fn rides_without_numbers_are_pending_not_counted() {
        let old = meta("1", 1_700_000_000.0, None);
        let wrong_length = meta("2", 1_700_010_000.0, Some(vec![Some(300.0)]));
        let none_long_enough = meta("3", 1_700_020_000.0, Some(vec![None; 5]));
        let analysis = analyze(
            &[route_of("a", vec![old, wrong_length, none_long_enough])],
            1_700_030_000.0,
        );
        assert_eq!(analysis.rides_pending, 2);
        assert_eq!(analysis.rides_counted, 0);
        assert!(analysis.best.iter().all(Option::is_none));
        assert!(analysis.ftp_estimate.is_none());
        let empty = analyze(&[], 0.0);
        assert!(
            empty.best.iter().all(Option::is_none),
            "an empty log has no best"
        );
        assert_eq!(empty.weeks.len(), TREND_LEN);
    }

    #[test]
    fn weeks_start_on_monday_and_months_on_the_first() {
        // 2023-11-14 22:13:20 UTC (a Tuesday)
        let tuesday = 1_700_000_000.0;
        let monday = period_start(day_of(tuesday), Period::Week);
        assert_eq!(monday as f64 * 86_400.0, 1_699_833_600.0); // 2023-11-13 00:00 UTC
        assert_eq!(civil_from_days(monday), (2023, 11, 13));
        let first = period_start(day_of(tuesday), Period::Month);
        assert_eq!(civil_from_days(first), (2023, 11, 1));
        // the Sunday before still belongs to the previous week; the Monday to the next
        assert_eq!(period_start(monday - 1, Period::Week), monday - 7);
        assert_eq!(period_start(monday + 6, Period::Week), monday);
        // a Thursday on the epoch itself
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(period_start(0, Period::Week), -3);
    }

    #[test]
    fn civil_dates_round_trip_across_leap_years_and_the_new_year() {
        for (y, m, d) in [
            (1969, 12, 31),
            (1970, 1, 1),
            (2000, 2, 29),
            (2023, 12, 31),
            (2024, 2, 29),
            (2024, 3, 1),
            (2100, 3, 1),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        // a month before January is December of the year before
        let jan = days_from_civil(2024, 1, 1);
        assert_eq!(
            civil_from_days(previous_start(jan, Period::Month)),
            (2023, 12, 1)
        );
        let mar = days_from_civil(2024, 3, 1);
        assert_eq!(
            civil_from_days(previous_start(mar, Period::Month)),
            (2024, 2, 1)
        );
    }

    #[test]
    fn the_trend_fills_empty_periods_and_adds_up_the_rides() {
        let now = 1_700_000_000.0; // Tuesday 2023-11-14
        let day = 86_400.0;
        let mut this_week = meta("a", now - day, None);
        this_week.elapsed_s = 3_600.0;
        this_week.tss = Some(60.0);
        let mut also = meta("b", now - 2.0 * day, None); // Sunday, previous week
        also.elapsed_s = 1_800.0;
        also.tss = Some(30.0);
        let mut estimated = meta("c", now - day, None);
        estimated.power_estimated = true;
        estimated.elapsed_s = 600.0;
        estimated.tss = Some(500.0);
        let mut simulated = meta("d", now - day, None);
        simulated.simulated = true;
        simulated.elapsed_s = 9_999.0;
        simulated.tss = Some(999.0);
        let ancient = meta("e", now - 400.0 * day, None);
        let rides = [&this_week, &also, &estimated, &simulated, &ancient];

        let weeks = bucket(&rides, Period::Week, 12, now);
        assert_eq!(weeks.len(), 12);
        let last = weeks.last().expect("this week");
        assert_eq!(last.rides, 2, "the simulated one does not count");
        assert!((last.time_s - 4_200.0).abs() < 1e-9);
        assert!(
            (last.tss - 60.0).abs() < 1e-9,
            "the estimated TSS is left out"
        );
        let before = &weeks[10];
        assert_eq!((before.rides, before.tss), (1, 30.0));
        assert!(
            weeks[..10].iter().all(|w| w.rides == 0),
            "empty weeks are there, and empty"
        );
        assert!(weeks
            .windows(2)
            .all(|w| w[0].start_unix_s < w[1].start_unix_s));

        let months = bucket(&rides, Period::Month, 12, now);
        assert_eq!(months.len(), 12);
        assert_eq!(months.last().expect("november").rides, 3);
        assert_eq!(bucket(&rides, Period::Month, 0, now), Vec::<Bucket>::new());
        assert!(bucket(&rides, Period::Week, 3, f64::NAN).is_empty());
    }
}
