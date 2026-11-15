//! Power over time: the mean-maximal power curve, the FTP it suggests, the training load per
//! week / month and the performance management chart.
//!
//! * The **mean-maximal power** (the "power curve") is, for a window length, the highest average
//!   power the rider held over any stretch of that length: [`DURATIONS`] run from 1 s to 90
//!   minutes. A ride shorter than the window has no value for it (`None`), it is not padded with
//!   zeros. The page shows it for a chosen period ([`Window`]) next to the best ever.
//! * The **FTP estimate** always looks at the last [`FTP_LOOKBACK_DAYS`] days, so it follows
//!   the rider down after a break as well as up. Its basis is the curve *without* the seconds a
//!   workout held the trainer at a target (ERG): the watts a workout prescribed say nothing about
//!   what the rider could do, and would only return the FTP that set them. From that curve it
//!   fits a critical-power model and takes 95 % of the better of the measured and the modelled
//!   20 minutes (`fitness::estimate_ftp_from`). It is a suggestion; the rider decides.
//! * The **trend** adds up the rides per week (Monday to Sunday, UTC) or calendar month: time,
//!   distance, training load (TSS from measured watts, else hrTSS, else the TSS of estimated
//!   watts; see `fitness::ride_load`) and the time in zones.
//!
//! Only **measured** watts reach the curve and the FTP ([`is_measured`]): a simulated rider's
//! power comes from the physics model and an estimated ride's watts were computed backwards from
//! speed and grade, so an FTP derived from either would only confirm the model's own assumptions.
//!
//! Pure functions: no clock (the caller passes `now`), no I/O. The index of the ride log keeps
//! the per-ride numbers ([`fill_meta`]), so [`analyze`] never opens a record.

use ride_model::{RecordPoint, RideMeta, RideRecord, RiderSettings, RouteRides, HR_HIST_FROM};

use crate::analysis::MAX_SAMPLE_GAP_S;
use crate::fitness::{self, CpModel, FtpMethod, LoadSource, PmcDay};

/// Window lengths of the curve, seconds. The index of a ride stores one value per entry, in this
/// order; a stored list of another length is treated as "not computed".
pub const DURATIONS: [u32; 15] = [
    1, 5, 10, 30, 60, 120, 180, 300, 480, 600, 720, 1_200, 1_800, 3_600, 5_400,
];

/// Index of the 20 minute window in [`DURATIONS`] (the FTP test length).
pub const FTP_WINDOW: usize = 11;

/// The rows of the curve the page lists in its table (the chart has all of them).
pub const TABLE_ROWS: [usize; 9] = [1, 3, 4, 5, 7, 9, 11, 13, 14];

/// The FTP test rule: 95 % of the best 20 minutes.
pub const FTP_FACTOR: f64 = fitness::FTP_FACTOR;

/// The FTP suggestion looks back this many days.
pub const FTP_LOOKBACK_DAYS: u32 = 90;

/// The setting range of the FTP field, watts.
pub const FTP_MIN_W: f64 = 50.0;
pub const FTP_MAX_W: f64 = 600.0;

/// Version of the derived numbers in an index entry ([`fill_meta`]). Raising it makes every
/// entry recompute once from its record.
pub const ANALYSIS_VERSION: u32 = 1;

/// A recording longer than this is not read any further (28 hours; a corrupt timestamp must not
/// allocate gigabytes).
const MAX_SLOTS: usize = 100_000;

/// The highest heart rate kept in the histogram, bpm.
const HR_HIST_TO: u32 = 240;

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
    mean_max_dense(&dense_power(points), durations)
}

/// The best average power for every window length in `durations` of a power series on a
/// one-second grid. `None` for a window longer than the series.
pub fn mean_max_dense(power: &[f64], durations: &[u32]) -> Vec<Option<f64>> {
    let mut prefix = Vec::with_capacity(power.len() + 1);
    prefix.push(0.0);
    let mut sum = 0.0;
    for watts in power {
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

/// FTP suggested by the best 20 minute power alone: 95 %, whole watts, inside the setting's
/// range.
pub fn estimate_ftp(best_20min_w: Option<f64>) -> Option<f64> {
    fitness::estimate_ftp_from(best_20min_w, None, FTP_MIN_W, FTP_MAX_W).map(|(ftp, _)| ftp)
}

fn curve_f32(values: Vec<Option<f64>>) -> Vec<Option<f32>> {
    values.into_iter().map(|w| w.map(|w| w as f32)).collect()
}

/// Seconds per bpm (from [`HR_HIST_FROM`]) of a recording, trailing zeros cut.
pub fn hr_histogram(points: &[RecordPoint]) -> Vec<u32> {
    let hr = crate::ride_detail::dense_channel(points, |p| p.hr_bpm);
    let mut out: Vec<u32> = Vec::new();
    for bpm in hr.into_iter().flatten() {
        if !(bpm >= f64::from(HR_HIST_FROM) && bpm <= f64::from(HR_HIST_TO)) {
            continue;
        }
        let i = (bpm.round() as u32).saturating_sub(HR_HIST_FROM) as usize;
        if out.len() <= i {
            out.resize(i + 1, 0);
        }
        out[i] += 1;
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

/// Fills in everything an index entry derives from its record: the TSS, the curves, the
/// heart-rate histogram, the zone times and the rider's own entries (FTP, mass, workout, RPE,
/// note). The curve of a ride whose watts are not the rider's own is all `None`, so the
/// "not computed yet" state (`None`) is left only for old entries.
pub fn fill_meta(meta: &mut RideMeta, record: &RideRecord) {
    let summary = &record.summary;
    let measured = !record.simulated && !record.power_estimated;
    let has = has_power(&record.points);
    meta.tss = summary.tss.filter(|t| t.is_finite());
    if measured {
        meta.mmp = Some(curve_f32(mean_max(&record.points, &DURATIONS)));
        meta.free_mmp = Some(curve_f32(if has {
            mean_max_dense(&crate::ride_detail::free_power(record), &DURATIONS)
        } else {
            vec![None; DURATIONS.len()]
        }));
        meta.power_zone_secs = has.then_some(summary.power_zone_secs);
    } else {
        meta.mmp = Some(vec![None; DURATIONS.len()]);
        meta.free_mmp = Some(vec![None; DURATIONS.len()]);
        meta.power_zone_secs = None;
    }
    meta.np_w = summary.normalized_power_w.filter(|w| w.is_finite());
    meta.work_kj = has.then_some(summary.work_kj).filter(|w| w.is_finite());
    meta.max_hr = summary.max_hr.filter(|h| h.is_finite());
    meta.dist_m = if summary.dist.is_finite() {
        summary.dist.max(0.0)
    } else {
        0.0
    };
    meta.hr_secs = if record.simulated {
        Vec::new()
    } else {
        hr_histogram(&record.points)
    };
    meta.ftp_w = record.ftp_w.filter(|w| w.is_finite() && *w > 0.0);
    meta.rider_kg = record.rider_kg.filter(|w| w.is_finite() && *w > 0.0);
    meta.workout_name = record.workout.as_ref().map(|w| w.name.clone());
    meta.rpe = ride_model::valid_rpe(record.rpe);
    meta.note = ride_model::clean_note(&record.note);
    meta.analysis_version = ANALYSIS_VERSION;
}

/// The stored curve of an index entry, when it has one for the current [`DURATIONS`].
pub fn stored_curve(meta: &RideMeta) -> Option<&[Option<f32>]> {
    meta.mmp
        .as_deref()
        .filter(|curve| curve.len() == DURATIONS.len())
}

/// The curve the FTP estimate uses: the one without ERG seconds, else the whole one.
fn ftp_curve(meta: &RideMeta) -> Option<&[Option<f32>]> {
    meta.free_mmp
        .as_deref()
        .filter(|curve| curve.len() == DURATIONS.len())
        .or_else(|| stored_curve(meta))
}

/// The entry still has to be computed (or recomputed) from its record.
pub fn needs_fill(meta: &RideMeta) -> bool {
    meta.analysis_version < ANALYSIS_VERSION || stored_curve(meta).is_none()
}

/// The period the page looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The last `n` days, today included.
    Days(u32),
    /// Every ride.
    All,
}

impl Window {
    /// The windows the page offers, shortest first.
    pub const CHOICES: [Window; 5] = [
        Window::Days(28),
        Window::Days(42),
        Window::Days(90),
        Window::Days(365),
        Window::All,
    ];

    /// A short key for the UI (`"28"`, ..., `"all"`).
    pub fn key(self) -> String {
        match self {
            Window::Days(n) => n.to_string(),
            Window::All => "all".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Window> {
        Window::CHOICES.into_iter().find(|w| w.key() == key)
    }

    /// The first instant inside the window.
    fn first_unix_s(self, now_unix_s: f64) -> f64 {
        match self {
            Window::Days(n) => (day_of(now_unix_s) - i64::from(n) + 1) as f64 * 86_400.0,
            Window::All => f64::NEG_INFINITY,
        }
    }
}

/// The ride that holds the best value of one window length.
#[derive(Debug, Clone, PartialEq)]
pub struct Best {
    pub watts: f64,
    pub route_id: String,
    pub route_name: String,
    pub stem: String,
    pub started_unix_s: f64,
    /// The rider's mass on that day, if the ride kept it.
    pub rider_kg: Option<f64>,
}

/// FTP suggested by the rides of the last [`FTP_LOOKBACK_DAYS`] days.
#[derive(Debug, Clone, PartialEq)]
pub struct FtpEstimate {
    pub ftp_w: f64,
    pub method: FtpMethod,
    /// The best 20 minutes of the period (ERG seconds left out) and when it was ridden.
    pub best_20min_w: Option<f64>,
    pub best_20min_unix_s: Option<f64>,
    /// The critical-power model of the period, when one could be fitted.
    pub model: Option<CpModel>,
}

/// One change of the FTP a ride was analysed with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FtpChange {
    pub unix_s: f64,
    pub ftp_w: f64,
}

/// The estimated FTP at the end of one week.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeekFtp {
    pub start_unix_s: f64,
    pub ftp_w: Option<f64>,
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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bucket {
    /// First second of the period (UTC midnight of its first day).
    pub start_unix_s: f64,
    pub rides: usize,
    /// Riding time, seconds (every ride that is not simulated).
    pub time_s: f64,
    pub dist_m: f64,
    /// Training stress of the rides with measured watts.
    pub tss: f64,
    /// hrTSS of the rides without measured watts.
    pub hr_tss: f64,
    /// TSS of rides with estimated watts and no heart rate.
    pub estimated_tss: f64,
    /// Seconds per power zone (rides with measured watts) and heart-rate zone (current model).
    pub power_zone_secs: [f64; 7],
    pub hr_zone_secs: [f64; 5],
}

impl Bucket {
    /// The whole training load of the period.
    pub fn load(&self) -> f64 {
        self.tss + self.hr_tss + self.estimated_tss
    }
}

/// How many rides of the period got their load from which source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadCounts {
    pub power: usize,
    pub heart_rate: usize,
    pub estimated: usize,
    /// Rides that have no load at all (no power, no heart rate).
    pub none: usize,
}

/// Everything the "Teljesítmény" page shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    /// One entry per [`DURATIONS`] window inside the chosen period; `None` when no measured ride
    /// is long enough.
    pub best: Vec<Option<Best>>,
    /// The same over every ride.
    pub best_ever: Vec<Option<Best>>,
    pub ftp_estimate: Option<FtpEstimate>,
    /// The estimated FTP at the end of each of the last twelve weeks (each looking back
    /// [`FTP_LOOKBACK_DAYS`] days), oldest first.
    pub ftp_weeks: Vec<WeekFtp>,
    /// The FTP the rides were analysed with, every time it changed, oldest first.
    pub ftp_history: Vec<FtpChange>,
    /// The last twelve weeks, oldest first, empty weeks included.
    pub weeks: Vec<Bucket>,
    /// The last twelve months, oldest first, empty months included.
    pub months: Vec<Bucket>,
    /// Fitness, fatigue and form for every day of the period (a period of every ride starts at
    /// the first ride, at most two years back).
    pub pmc: Vec<PmcDay>,
    /// Fitness gained over the last seven days.
    pub ramp_7d: Option<f64>,
    /// The whole period: rides, time in zones, where the load came from.
    pub period: Bucket,
    pub load_counts: LoadCounts,
    /// The highest heart rate of the period and when.
    pub max_hr_seen: Option<(f64, f64)>,
    /// Measured rides that went into the curve (whole log).
    pub rides_counted: usize,
    /// Rides whose numbers are not in the index yet.
    pub rides_pending: usize,
}

/// Number of periods in the trend.
pub const TREND_LEN: usize = 12;

/// The best of each window over `rides`.
fn best_of<'a>(
    rides: impl Iterator<Item = (&'a RouteRides, &'a RideMeta)>,
    curve: impl Fn(&RideMeta) -> Option<&[Option<f32>]>,
) -> Vec<Option<Best>> {
    let mut best: Vec<Option<Best>> = vec![None; DURATIONS.len()];
    for (route, meta) in rides {
        if !is_measured(meta) {
            continue;
        }
        let Some(values) = curve(meta) else {
            continue;
        };
        for (slot, watts) in best.iter_mut().zip(values) {
            let Some(watts) = watts.map(f64::from).filter(|w| w.is_finite() && *w > 0.0) else {
                continue;
            };
            if slot.as_ref().is_none_or(|b| watts > b.watts) {
                *slot = Some(Best {
                    watts,
                    route_id: route.route_id.clone(),
                    route_name: route.route_name.clone(),
                    stem: meta.stem.clone(),
                    started_unix_s: meta.started_unix_s,
                    rider_kg: meta.rider_kg,
                });
            }
        }
    }
    best
}

/// The FTP suggestion from the rides that started in `[from, until)`.
fn ftp_between(all: &[RouteRides], from: f64, until: f64) -> Option<FtpEstimate> {
    let rides = all.iter().flat_map(|r| r.rides.iter().map(move |m| (r, m)));
    let inside = rides.filter(|(_, m)| m.started_unix_s >= from && m.started_unix_s < until);
    let best = best_of(inside, ftp_curve);
    let points: Vec<(u32, f64)> = DURATIONS
        .iter()
        .zip(&best)
        .filter_map(|(t, b)| b.as_ref().map(|b| (*t, b.watts)))
        .collect();
    let model = fitness::fit_cp(&points);
    let twenty = best.get(FTP_WINDOW).and_then(Option::as_ref);
    let (ftp_w, method) = fitness::estimate_ftp_from(
        twenty.map(|b| b.watts),
        model.as_ref(),
        FTP_MIN_W,
        FTP_MAX_W,
    )?;
    Some(FtpEstimate {
        ftp_w,
        method,
        best_20min_w: twenty.map(|b| b.watts),
        best_20min_unix_s: twenty.map(|b| b.started_unix_s),
        model,
    })
}

/// The curve, the FTP suggestion, the trend and the load of every ride in `all`. `now_unix_s`
/// fixes "today"; `window` is the period the curve, the zones and the chart cover; `rider` gives
/// the heart-rate zones and hrTSS.
pub fn analyze(
    all: &[RouteRides],
    now_unix_s: f64,
    window: Window,
    rider: &RiderSettings,
) -> Analysis {
    let mut counted = 0;
    let mut pending = 0;
    let mut rides: Vec<&RideMeta> = Vec::new();
    for route in all {
        for meta in &route.rides {
            rides.push(meta);
            if needs_fill(meta) && !meta.simulated {
                pending += 1;
                continue;
            }
            if is_measured(meta)
                && stored_curve(meta).is_some_and(|c| c.iter().any(Option::is_some))
            {
                counted += 1;
            }
        }
    }
    let now_ok = valid_timestamp(now_unix_s);
    let from = window.first_unix_s(now_unix_s);
    let pairs = || all.iter().flat_map(|r| r.rides.iter().map(move |m| (r, m)));
    let best = best_of(pairs().filter(|(_, m)| m.started_unix_s >= from), |m| {
        stored_curve(m)
    });
    let best_ever = best_of(pairs(), stored_curve);

    let lookback = f64::from(FTP_LOOKBACK_DAYS) * 86_400.0;
    let ftp_estimate = now_ok
        .then(|| ftp_between(all, now_unix_s - lookback, f64::INFINITY))
        .flatten();
    let this_week = period_start(day_of(now_unix_s), Period::Week);
    let ftp_weeks = if now_ok {
        (0..TREND_LEN as i64)
            .rev()
            .map(|back| {
                let start = (this_week - 7 * back) as f64 * 86_400.0;
                let end = start + 7.0 * 86_400.0;
                WeekFtp {
                    start_unix_s: start,
                    ftp_w: ftp_between(all, end - lookback, end).map(|e| e.ftp_w),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut chronological: Vec<&RideMeta> = rides
        .iter()
        .copied()
        .filter(|m| valid_timestamp(m.started_unix_s))
        .collect();
    chronological.sort_by(|a, b| a.started_unix_s.total_cmp(&b.started_unix_s));
    let mut ftp_history: Vec<FtpChange> = Vec::new();
    for meta in &chronological {
        let Some(ftp) = meta.ftp_w.filter(|w| w.is_finite() && *w > 0.0) else {
            continue;
        };
        if ftp_history
            .last()
            .is_none_or(|c| (c.ftp_w - ftp).abs() >= 0.5)
        {
            ftp_history.push(FtpChange {
                unix_s: meta.started_unix_s,
                ftp_w: ftp,
            });
        }
    }

    // the load chart: a whole-log period starts at the first ride, at most two years back
    let today = day_of(now_unix_s);
    let first_day = chronological
        .first()
        .map(|m| day_of(m.started_unix_s))
        .unwrap_or(today);
    let pmc_from = match window {
        Window::Days(n) => today - i64::from(n) + 1,
        Window::All => first_day.max(today - 730),
    };
    let loads: Vec<(i64, f64)> = chronological
        .iter()
        .filter_map(|m| fitness::ride_load(m, rider).map(|(l, _)| (day_of(m.started_unix_s), l)))
        .collect();
    let pmc_all = if now_ok {
        fitness::pmc(&loads, pmc_from.min(today - 7), today)
    } else {
        Vec::new()
    };
    let ramp_7d = match (pmc_all.last(), pmc_all.iter().rev().nth(7)) {
        (Some(now), Some(week_ago)) if !loads.is_empty() => Some(now.ctl - week_ago.ctl),
        _ => None,
    };
    let pmc: Vec<PmcDay> = pmc_all.into_iter().filter(|d| d.day >= pmc_from).collect();

    let in_window: Vec<&RideMeta> = rides
        .iter()
        .copied()
        .filter(|m| m.started_unix_s >= from && valid_timestamp(m.started_unix_s))
        .collect();
    let mut period = Bucket {
        start_unix_s: if from.is_finite() {
            from
        } else {
            chronological.first().map_or(0.0, |m| m.started_unix_s)
        },
        ..Bucket::default()
    };
    let mut load_counts = LoadCounts::default();
    let mut max_hr_seen: Option<(f64, f64)> = None;
    for meta in &in_window {
        if meta.simulated {
            continue;
        }
        add_to(&mut period, meta, rider);
        match fitness::ride_load(meta, rider).map(|l| l.1) {
            Some(LoadSource::Power) => load_counts.power += 1,
            Some(LoadSource::HeartRate) => load_counts.heart_rate += 1,
            Some(LoadSource::Estimated) => load_counts.estimated += 1,
            None => load_counts.none += 1,
        }
        if let Some(hr) = meta.max_hr.filter(|h| h.is_finite() && *h > 0.0) {
            if max_hr_seen.is_none_or(|(best, _)| hr > best) {
                max_hr_seen = Some((hr, meta.started_unix_s));
            }
        }
    }

    Analysis {
        best,
        best_ever,
        ftp_estimate,
        ftp_weeks,
        ftp_history,
        weeks: bucket(&rides, Period::Week, TREND_LEN, now_unix_s, rider),
        months: bucket(&rides, Period::Month, TREND_LEN, now_unix_s, rider),
        pmc,
        ramp_7d,
        period,
        load_counts,
        max_hr_seen,
        rides_counted: counted,
        rides_pending: pending,
    }
}

/// Adds one (not simulated) ride to a bucket.
fn add_to(bucket: &mut Bucket, meta: &RideMeta, rider: &RiderSettings) {
    bucket.rides += 1;
    bucket.time_s += finite(meta.elapsed_s);
    bucket.dist_m += finite(meta.dist_m);
    match fitness::ride_load(meta, rider) {
        Some((load, LoadSource::Power)) => bucket.tss += load,
        Some((load, LoadSource::HeartRate)) => bucket.hr_tss += load,
        Some((load, LoadSource::Estimated)) => bucket.estimated_tss += load,
        None => {}
    }
    if let Some(zones) = meta.power_zone_secs.filter(|_| is_measured(meta)) {
        for (sum, secs) in bucket.power_zone_secs.iter_mut().zip(zones) {
            *sum += finite(secs);
        }
    }
    for (sum, secs) in bucket
        .hr_zone_secs
        .iter_mut()
        .zip(fitness::hr_zone_secs(&meta.hr_secs, rider))
    {
        *sum += secs;
    }
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// The last `count` periods ending with the one that contains `until_unix_s`, oldest first, each
/// with what the rides of that period add up to. Simulated rides do not count at all.
pub fn bucket(
    rides: &[&RideMeta],
    period: Period,
    count: usize,
    until_unix_s: f64,
    rider: &RiderSettings,
) -> Vec<Bucket> {
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
            ..Bucket::default()
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
        add_to(&mut out[index], meta, rider);
    }
    out
}

/// Supported calendar range: 0001-01-01 through 9999-12-31 (UTC).
pub fn valid_timestamp(value: f64) -> bool {
    value.is_finite() && (-62_135_596_800.0..253_402_300_800.0).contains(&value)
}

/// Days since the Unix epoch of the UTC day containing the instant.
pub fn day_of(unix_s: f64) -> i64 {
    (unix_s / 86_400.0).floor() as i64
}

/// The first day of the week (Monday) or month containing `day`.
pub fn period_start(day: i64, period: Period) -> i64 {
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
    use ride_model::{RideSource, RideSummary, RideWorkout, StepKind, StepTarget, WorkoutStep};

    const NOW: f64 = 1_700_000_000.0; // Tuesday 2023-11-14 22:13:20 UTC
    const DAY: f64 = 86_400.0;

    fn point(t: f64, power: Option<f64>) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: NOW + t,
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

    /// An index entry whose curve has `cp + w' / t` watts (no value past `longest` seconds).
    fn meta(stem: &str, started: f64, cp: f64, w_prime: f64, longest: u32) -> RideMeta {
        let curve: Vec<Option<f32>> = DURATIONS
            .iter()
            .map(|&t| (t <= longest).then(|| (cp + w_prime / f64::from(t)) as f32))
            .collect();
        RideMeta {
            stem: stem.into(),
            started_unix_s: started,
            elapsed_s: 3_600.0,
            mmp: Some(curve.clone()),
            free_mmp: Some(curve),
            analysis_version: ANALYSIS_VERSION,
            ..RideMeta::default()
        }
    }

    fn route_of(id: &str, rides: Vec<RideMeta>) -> RouteRides {
        RouteRides {
            route_id: id.into(),
            route_name: format!("Út {id}"),
            rides,
        }
    }

    fn rider() -> RiderSettings {
        RiderSettings {
            max_hr: 190.0,
            rest_hr: 50.0,
            lthr: 170.0,
            ..RiderSettings::default()
        }
    }

    #[test]
    fn the_table_rows_and_the_ftp_window_point_at_the_right_durations() {
        assert_eq!(DURATIONS[FTP_WINDOW], 1_200);
        assert!(TABLE_ROWS.iter().all(|i| *i < DURATIONS.len()));
        assert!(DURATIONS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_constant_ride_has_the_same_best_for_every_window_it_fills() {
        let points = ride(1_500, |_| 200.0);
        let best = mean_max(&points, &DURATIONS);
        assert_eq!(best.len(), DURATIONS.len());
        for (t, value) in DURATIONS.iter().zip(&best) {
            if *t <= 1_500 {
                assert!((value.expect("filled") - 200.0).abs() < 1e-9);
            } else {
                assert_eq!(*value, None, "a 25 minute ride has no {t} s value");
            }
        }
    }

    #[test]
    fn the_best_window_is_found_wherever_it_sits() {
        // 10 min easy, 5 min at 300 W, 10 min easy, a 5 s sprint at 900 W in the middle of the effort
        let points = ride(1_500, |t| match t {
            750..=754 => 900.0,
            600..=899 => 300.0,
            _ => 150.0,
        });
        let best = mean_max(&points, &[5, 60, 300, 1_200]);
        assert!((best[0].expect("5 s") - 900.0).abs() < 1e-9);
        assert!((best[1].expect("1 min") - (900.0 * 5.0 + 300.0 * 55.0) / 60.0).abs() < 1e-9);
        assert!((best[2].expect("5 min") - (300.0 * 295.0 + 900.0 * 5.0) / 300.0).abs() < 1e-9);
        let twenty = (300.0 * 295.0 + 900.0 * 5.0 + 150.0 * 900.0) / 1_200.0;
        assert!((best[3].expect("20 min") - twenty).abs() < 1e-9);
    }

    #[test]
    fn a_ride_without_any_power_has_no_curve_at_all() {
        let points: Vec<_> = (0..600).map(|t| point(f64::from(t), None)).collect();
        assert_eq!(mean_max(&points, &DURATIONS), vec![None; DURATIONS.len()]);
        assert_eq!(mean_max(&[], &DURATIONS), vec![None; DURATIONS.len()]);
    }

    #[test]
    fn a_sensor_dropout_counts_as_zero_and_never_raises_an_average() {
        let mut points = ride(120, |_| 200.0);
        for p in &mut points[50..60] {
            p.power_w = None;
        }
        let best = mean_max(&points, &[60]);
        assert!((best[0].expect("1 min") - 200.0).abs() < 1e-9);
        let whole = mean_max(&points, &[120]);
        let expected = 200.0 * 110.0 / 120.0;
        assert!((whole[0].expect("2 min") - expected).abs() < 1e-9);
    }

    #[test]
    fn samples_are_placed_by_time_not_by_index() {
        let points: Vec<_> = [0.0, 1.0, 2.0, 4.0, 5.0]
            .into_iter()
            .map(|t| point(t, Some(100.0)))
            .collect();
        let dense = dense_power(&points);
        assert_eq!(dense.len(), 5);
        assert!(dense.iter().all(|w| (*w - 100.0).abs() < 1e-9), "{dense:?}");
        let gap: Vec<_> = [0.0, 1.0, 30.0, 31.0]
            .into_iter()
            .map(|t| point(t, Some(100.0)))
            .collect();
        let dense = dense_power(&gap);
        assert_eq!(dense.len(), 31);
        assert!(dense[2..30].iter().all(|w| *w == 0.0));
    }

    #[test]
    fn nonsense_samples_do_not_poison_the_curve() {
        let mut points = ride(120, |_| 200.0);
        points[10].power_w = Some(f64::NAN);
        points[11].power_w = Some(-50.0);
        points[12].t = f64::NAN;
        points.push(point(1.0e12, Some(1.0e6)));
        let value = mean_max(&points, &[60])[0].expect("1 min");
        assert!(value.is_finite() && value <= 200.0, "{value}");
    }

    #[test]
    fn the_ftp_is_95_percent_of_the_best_20_minutes() {
        assert_eq!(estimate_ftp(Some(250.0)), Some(238.0));
        assert_eq!(estimate_ftp(Some(10.0)), Some(50.0), "the setting's range");
        assert_eq!(estimate_ftp(Some(2_000.0)), Some(600.0));
        assert_eq!(estimate_ftp(Some(f64::NAN)), None);
        assert_eq!(estimate_ftp(None), None);
    }

    fn record(simulated: bool, estimated: bool) -> RideRecord {
        RideRecord {
            completed_calendar_id: None,
            version: ride_model::RIDE_RECORD_VERSION,
            route_id: "r".into(),
            route_name: "Út".into(),
            simulated,
            summary: RideSummary {
                route_name: "Út".into(),
                route_id: "r".into(),
                started_unix_s: NOW,
                elapsed_s: 1_500.0,
                dist: 12_000.0,
                avg_power_w: Some(200.0),
                normalized_power_w: Some(200.0),
                tss: Some(27.0),
                work_kj: 300.0,
                max_hr: Some(171.0),
                power_zone_secs: [0.0, 0.0, 1_500.0, 0.0, 0.0, 0.0, 0.0],
                ..RideSummary::default()
            },
            points: ride(1_500, |_| 200.0)
                .into_iter()
                .map(|mut p| {
                    p.hr_bpm = Some(150.0);
                    p
                })
                .collect(),
            source: RideSource::Recorded,
            power_estimated: estimated,
            estimate: None,
            measured_power: Vec::new(),
            ftp_w: Some(250.0),
            rider_kg: Some(72.5),
            workout: None,
            rpe: Some(7),
            note: "  jó volt  ".into(),
        }
    }

    #[test]
    fn the_index_keeps_the_numbers_of_a_ride() {
        let mut meta = RideMeta::default();
        fill_meta(&mut meta, &record(false, false));
        assert_eq!(meta.analysis_version, ANALYSIS_VERSION);
        assert_eq!(meta.tss, Some(27.0));
        let curve = meta.mmp.clone().expect("computed");
        assert_eq!(curve.len(), DURATIONS.len());
        assert!((curve[FTP_WINDOW].expect("20 min") - 200.0).abs() < 1e-3);
        assert_eq!(curve[12], None, "no 30 min value");
        assert_eq!(meta.free_mmp, meta.mmp, "no workout: nothing left out");
        assert_eq!(meta.hr_secs.len(), (150 - HR_HIST_FROM + 1) as usize);
        assert_eq!(meta.hr_secs.last(), Some(&1_500));
        assert_eq!(meta.ftp_w, Some(250.0));
        assert_eq!(meta.rider_kg, Some(72.5));
        assert_eq!(meta.rpe, Some(7));
        assert_eq!(meta.note, "jó volt");
        assert_eq!(meta.dist_m, 12_000.0);
        assert_eq!(meta.max_hr, Some(171.0));
        assert!(meta.power_zone_secs.is_some());
        assert!(!needs_fill(&meta));

        for (simulated, estimated) in [(true, false), (false, true)] {
            let mut meta = RideMeta::default();
            fill_meta(&mut meta, &record(simulated, estimated));
            assert_eq!(
                meta.mmp,
                Some(vec![None; DURATIONS.len()]),
                "computed, and empty: not the rider's own watts"
            );
            assert_eq!(meta.power_zone_secs, None);
        }
        let mut sim = RideMeta::default();
        fill_meta(&mut sim, &record(true, false));
        assert!(
            sim.hr_secs.is_empty(),
            "nobody's heart beat in a simulation"
        );
    }

    #[test]
    fn the_ftp_curve_leaves_out_the_workout_targets() {
        let mut r = record(false, false);
        // 20 min, of which the middle 10 were held at a target
        r.workout = Some(RideWorkout {
            id: "w".into(),
            name: "Sweet spot".into(),
            ftp_w: 250.0,
            steps: vec![
                WorkoutStep {
                    kind: StepKind::Warmup,
                    name: String::new(),
                    duration_s: 300,
                    target: StepTarget::Free,
                    cadence: None,
                    note: None,
                    messages: Vec::new(),
                },
                WorkoutStep {
                    kind: StepKind::Interval,
                    name: String::new(),
                    duration_s: 600,
                    target: StepTarget::PctFtp { value: 80.0 },
                    cadence: None,
                    note: None,
                    messages: Vec::new(),
                },
            ],
        });
        let mut meta = RideMeta::default();
        fill_meta(&mut meta, &r);
        let full = meta.mmp.clone().unwrap();
        let free = meta.free_mmp.clone().unwrap();
        assert!((full[FTP_WINDOW].unwrap() - 200.0).abs() < 1e-3);
        assert!(free[FTP_WINDOW].unwrap() < 160.0, "{:?}", free[FTP_WINDOW]);
        assert_eq!(meta.workout_name.as_deref(), Some("Sweet spot"));
    }

    #[test]
    fn the_ftp_comes_from_the_last_90_days_and_the_model() {
        // three months ago a strong ride; recently a weaker one with short maximal efforts
        let old = meta("1", NOW - 120.0 * DAY, 300.0, 20_000.0, 3_600);
        let recent = meta("2", NOW - 10.0 * DAY, 250.0, 18_000.0, 1_200);
        let all = [route_of("a", vec![old]), route_of("b", vec![recent])];
        let a = analyze(&all, NOW, Window::Days(90), &rider());
        let estimate = a.ftp_estimate.expect("ftp");
        let model = estimate.model.expect("model");
        assert!((model.cp_w - 250.0).abs() < 0.5, "{model:?}");
        // 0.95 * (250 + 18000 / 1200) = 251.75
        assert_eq!(estimate.ftp_w, 252.0);
        assert_eq!(estimate.best_20min_unix_s, Some(NOW - 10.0 * DAY));
        // the strong old ride is still the best ever, but not of the period
        assert!(
            (a.best_ever[FTP_WINDOW].as_ref().unwrap().watts - (300.0 + 20_000.0 / 1_200.0)).abs()
                < 0.01
        );
        assert_eq!(a.best[FTP_WINDOW].as_ref().unwrap().stem, "2");
        // a year back the old ride is in the window of the curve, never in the FTP's
        let year = analyze(&all, NOW, Window::Days(365), &rider());
        assert_eq!(year.best[FTP_WINDOW].as_ref().unwrap().stem, "1");
        assert_eq!(year.ftp_estimate.unwrap().ftp_w, 252.0);
    }

    #[test]
    fn the_weekly_ftp_follows_the_rides_and_the_history_lists_changes() {
        let mut early = meta("1", NOW - 60.0 * DAY, 220.0, 15_000.0, 1_200);
        early.ftp_w = Some(200.0);
        let mut later = meta("2", NOW - 0.5 * DAY, 260.0, 18_000.0, 1_200);
        later.ftp_w = Some(230.0);
        let mut same = meta("3", NOW - 0.2 * DAY, 200.0, 10_000.0, 600);
        same.ftp_w = Some(230.0);
        let a = analyze(
            &[route_of("a", vec![later, same, early])],
            NOW,
            Window::Days(90),
            &rider(),
        );
        assert_eq!(a.ftp_weeks.len(), TREND_LEN);
        let first = a.ftp_weeks.iter().position(|w| w.ftp_w.is_some()).unwrap();
        assert!(first > 0, "nothing before the first ride");
        let last = a.ftp_weeks.last().unwrap().ftp_w.unwrap();
        let before = a.ftp_weeks[TREND_LEN - 2].ftp_w.unwrap();
        assert!(last > before, "{before} -> {last}");
        assert_eq!(
            a.ftp_history.iter().map(|c| c.ftp_w).collect::<Vec<_>>(),
            vec![200.0, 230.0]
        );
    }

    #[test]
    fn simulated_and_estimated_rides_never_reach_the_curve() {
        let mut simulated = meta("1", NOW - DAY, 900.0, 50_000.0, 3_600);
        simulated.simulated = true;
        let mut estimated = meta("2", NOW - DAY, 800.0, 50_000.0, 3_600);
        estimated.power_estimated = true;
        let measured = meta("3", NOW - DAY, 200.0, 10_000.0, 3_600);
        let a = analyze(
            &[route_of("a", vec![simulated, estimated, measured])],
            NOW,
            Window::All,
            &rider(),
        );
        assert_eq!(a.best[FTP_WINDOW].as_ref().unwrap().stem, "3");
        assert_eq!(a.rides_counted, 1);
    }

    #[test]
    fn rides_without_numbers_are_pending_not_counted() {
        let mut old = meta("1", NOW - DAY, 200.0, 10_000.0, 3_600);
        old.analysis_version = 0;
        let mut wrong_length = meta("2", NOW - DAY, 200.0, 10_000.0, 3_600);
        wrong_length.mmp = Some(vec![Some(300.0)]);
        let a = analyze(
            &[route_of("a", vec![old, wrong_length])],
            NOW,
            Window::All,
            &rider(),
        );
        assert_eq!(a.rides_pending, 2);
        assert_eq!(a.rides_counted, 0);
        let empty = analyze(&[], NOW, Window::All, &rider());
        assert!(empty.best.iter().all(Option::is_none));
        assert!(empty.ftp_estimate.is_none());
        assert_eq!(empty.weeks.len(), TREND_LEN);
        assert_eq!(empty.ramp_7d, None);
        // a broken clock gives an empty page, not a panic
        let broken = analyze(&[], f64::NAN, Window::Days(90), &rider());
        assert!(broken.pmc.is_empty() && broken.weeks.is_empty());
    }

    #[test]
    fn the_load_chart_and_the_period_totals() {
        let mut with_power = meta("1", NOW - 3.0 * DAY, 200.0, 10_000.0, 3_600);
        with_power.tss = Some(80.0);
        with_power.dist_m = 30_000.0;
        with_power.max_hr = Some(181.0);
        with_power.power_zone_secs = Some([600.0, 1_200.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let only_hr = RideMeta {
            stem: "2".into(),
            started_unix_s: NOW - DAY,
            elapsed_s: 3_600.0,
            hr_secs: {
                let mut h = vec![0; (170 - HR_HIST_FROM + 1) as usize];
                h[(170 - HR_HIST_FROM) as usize] = 3_600;
                h
            },
            analysis_version: ANALYSIS_VERSION,
            mmp: Some(vec![None; DURATIONS.len()]),
            ..RideMeta::default()
        };
        let nothing = RideMeta {
            stem: "3".into(),
            started_unix_s: NOW - DAY,
            elapsed_s: 600.0,
            analysis_version: ANALYSIS_VERSION,
            mmp: Some(vec![None; DURATIONS.len()]),
            ..RideMeta::default()
        };
        let mut long_ago = meta("4", NOW - 200.0 * DAY, 200.0, 10_000.0, 3_600);
        long_ago.tss = Some(500.0);
        let all = [route_of("a", vec![with_power, only_hr, nothing, long_ago])];
        let a = analyze(&all, NOW, Window::Days(28), &rider());
        assert_eq!(a.pmc.len(), 28);
        let today = a.pmc.last().unwrap();
        assert!(today.ctl > 0.0 && today.atl > today.ctl);
        assert!(a.ramp_7d.unwrap() > 0.0);
        assert_eq!(a.period.rides, 3);
        assert!((a.period.tss - 80.0).abs() < 1e-9);
        assert!((a.period.hr_tss - 100.0).abs() < 1e-6);
        assert_eq!(a.period.dist_m, 30_000.0);
        assert_eq!(a.period.power_zone_secs[1], 1_200.0);
        assert_eq!(a.period.hr_zone_secs[3], 3_600.0, "170 of 190 is zone 4");
        assert_eq!(
            a.load_counts,
            LoadCounts {
                power: 1,
                heart_rate: 1,
                estimated: 0,
                none: 1
            }
        );
        assert_eq!(a.max_hr_seen, Some((181.0, NOW - 3.0 * DAY)));
        // every ride: the chart starts at the first one
        let all_time = analyze(&all, NOW, Window::All, &rider());
        assert_eq!(all_time.pmc.len(), 201);
    }

    #[test]
    fn windows_round_trip_through_their_keys() {
        for w in Window::CHOICES {
            assert_eq!(Window::from_key(&w.key()), Some(w));
        }
        assert_eq!(Window::from_key("7"), None);
    }

    #[test]
    fn weeks_start_on_monday_and_months_on_the_first() {
        let monday = period_start(day_of(NOW), Period::Week);
        assert_eq!(monday as f64 * 86_400.0, 1_699_833_600.0); // 2023-11-13 00:00 UTC
        assert_eq!(civil_from_days(monday), (2023, 11, 13));
        let first = period_start(day_of(NOW), Period::Month);
        assert_eq!(civil_from_days(first), (2023, 11, 1));
        assert_eq!(period_start(monday - 1, Period::Week), monday - 7);
        assert_eq!(period_start(monday + 6, Period::Week), monday);
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
        let jan = days_from_civil(2024, 1, 1);
        assert_eq!(
            civil_from_days(previous_start(jan, Period::Month)),
            (2023, 12, 1)
        );
    }

    #[test]
    fn the_trend_fills_empty_periods_and_adds_up_the_rides() {
        let mut this_week = meta("a", NOW - DAY, 200.0, 10_000.0, 3_600);
        this_week.tss = Some(60.0);
        let mut also = meta("b", NOW - 2.0 * DAY, 200.0, 10_000.0, 3_600); // Sunday, previous week
        also.elapsed_s = 1_800.0;
        also.tss = Some(30.0);
        let mut estimated = meta("c", NOW - DAY, 200.0, 10_000.0, 3_600);
        estimated.power_estimated = true;
        estimated.elapsed_s = 600.0;
        estimated.tss = Some(50.0);
        let mut simulated = meta("d", NOW - DAY, 200.0, 10_000.0, 3_600);
        simulated.simulated = true;
        simulated.tss = Some(999.0);
        let ancient = meta("e", NOW - 400.0 * DAY, 200.0, 10_000.0, 3_600);
        let rides = [&this_week, &also, &estimated, &simulated, &ancient];

        let weeks = bucket(&rides, Period::Week, 12, NOW, &rider());
        assert_eq!(weeks.len(), 12);
        let last = weeks.last().expect("this week");
        assert_eq!(last.rides, 2, "the simulated one does not count");
        assert!((last.time_s - 4_200.0).abs() < 1e-9);
        assert!((last.tss - 60.0).abs() < 1e-9);
        assert!((last.estimated_tss - 50.0).abs() < 1e-9, "kept apart");
        assert!((last.load() - 110.0).abs() < 1e-9);
        let before = &weeks[10];
        assert_eq!((before.rides, before.tss), (1, 30.0));
        assert!(weeks[..10].iter().all(|w| w.rides == 0));
        let months = bucket(&rides, Period::Month, 12, NOW, &rider());
        assert_eq!(months.last().expect("november").rides, 3);
        assert!(bucket(&rides, Period::Month, 0, NOW, &rider()).is_empty());
        assert!(bucket(&rides, Period::Week, 3, f64::NAN, &rider()).is_empty());
    }
}
