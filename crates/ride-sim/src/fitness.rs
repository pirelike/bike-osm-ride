//! Training load and threshold models, from the numbers the ride log index keeps.
//!
//! * **Critical power** ([`fit_cp`]): the two-parameter model `P(t) = CP + W′ / t`, fitted by least
//!   squares to the work `P · t` of the best efforts between 2 and 20 minutes. CP is the power the
//!   rider can hold for a long time, W′ the energy above it that runs out (joules).
//! * **FTP estimate** ([`estimate_ftp_from`]): the field-test rule (95 % of an all-out 20 minutes)
//!   applied to the better of the measured best 20 minutes and the 20 minutes the CP model
//!   predicts. A rider who never rode a maximal 20 minutes still gets a fair number from shorter
//!   maximal efforts; without a usable model it falls back to the measured 20 minutes alone.
//! * **hrTSS** ([`hr_tss`]): Banister's TRIMP of the ride divided by the TRIMP of one hour at the
//!   threshold heart rate, times 100, so an hour at LTHR is 100 like an hour at FTP. It gives a
//!   ride without (measured) power a training load.
//! * **Performance management** ([`pmc`]): fitness (CTL, a 42 day exponentially weighted mean of
//!   the daily load), fatigue (ATL, 7 days) and form (TSB = yesterday's CTL − ATL), the usual
//!   TrainingPeaks definitions.
//!
//! Pure functions, no clock and no I/O; days are UTC days since 1970-01-01.

use ride_model::{RideMeta, RiderSettings, HR_HIST_FROM};

/// Shortest and longest effort that goes into the CP fit, seconds.
pub const CP_MIN_S: u32 = 120;
pub const CP_MAX_S: u32 = 1_200;

/// The fit needs this many durations, and its longest one at least [`CP_MIN_LONGEST_S`].
pub const CP_MIN_POINTS: usize = 3;
pub const CP_MIN_LONGEST_S: u32 = 480;

/// The field-test rule: 95 % of the best 20 minutes.
pub const FTP_FACTOR: f64 = 0.95;

/// Time constants of the performance management chart, days.
pub const CTL_DAYS: f64 = 42.0;
pub const ATL_DAYS: f64 = 7.0;

/// Exponent of Banister's TRIMP weighting (the men's value; the women's 1.67 gives a few per cent
/// less at threshold).
const TRIMP_B: f64 = 1.92;

/// A fitted critical-power model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpModel {
    /// Critical power, watts.
    pub cp_w: f64,
    /// Work capacity above CP, joules.
    pub w_prime_j: f64,
    /// Durations the fit used.
    pub points: usize,
    /// Coefficient of determination of the work–time line.
    pub r2: f64,
}

impl CpModel {
    /// The power the model predicts for an all-out effort of `seconds`.
    pub fn power_at(&self, seconds: f64) -> f64 {
        if seconds > 0.0 {
            self.cp_w + self.w_prime_j / seconds
        } else {
            f64::NAN
        }
    }
}

/// Least-squares fit of `work = CP · t + W′` to `(seconds, watts)` best efforts. Only the
/// durations in [`CP_MIN_S`]..=[`CP_MAX_S`] count. `None` when there are too few of them, or the
/// result is not physiological (CP outside 50..=700 W, W′ outside 2..=60 kJ, a poor fit): a model
/// that only fits noise must not become an FTP.
pub fn fit_cp(points: &[(u32, f64)]) -> Option<CpModel> {
    let used: Vec<(f64, f64)> = points
        .iter()
        .filter(|(t, w)| (CP_MIN_S..=CP_MAX_S).contains(t) && w.is_finite() && *w > 0.0)
        .map(|&(t, w)| (f64::from(t), w * f64::from(t)))
        .collect();
    let longest = points
        .iter()
        .filter(|(t, w)| (CP_MIN_S..=CP_MAX_S).contains(t) && w.is_finite() && *w > 0.0)
        .map(|(t, _)| *t)
        .max()?;
    if used.len() < CP_MIN_POINTS || longest < CP_MIN_LONGEST_S {
        return None;
    }
    let n = used.len() as f64;
    let mean_t = used.iter().map(|(t, _)| t).sum::<f64>() / n;
    let mean_w = used.iter().map(|(_, w)| w).sum::<f64>() / n;
    let sxx: f64 = used.iter().map(|(t, _)| (t - mean_t).powi(2)).sum();
    let sxy: f64 = used.iter().map(|(t, w)| (t - mean_t) * (w - mean_w)).sum();
    let syy: f64 = used.iter().map(|(_, w)| (w - mean_w).powi(2)).sum();
    if sxx <= 0.0 || syy <= 0.0 {
        return None;
    }
    let cp = sxy / sxx;
    let w_prime = mean_w - cp * mean_t;
    let r2 = (sxy * sxy) / (sxx * syy);
    let plausible = (50.0..=700.0).contains(&cp)
        && (2_000.0..=60_000.0).contains(&w_prime)
        && r2 >= 0.95
        && cp.is_finite()
        && w_prime.is_finite();
    plausible.then_some(CpModel {
        cp_w: cp,
        w_prime_j: w_prime,
        points: used.len(),
        r2,
    })
}

/// How an FTP estimate was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtpMethod {
    /// 95 % of the 20 minutes the CP model predicts (it was higher than the measured 20 minutes).
    Model,
    /// 95 % of the measured best 20 minutes.
    TwentyMinutes,
}

/// The FTP suggestion: 95 % of the better of the measured best 20 minutes and the model's
/// 20 minutes, whole watts, inside `min..=max`. `None` when neither exists.
pub fn estimate_ftp_from(
    best_20min_w: Option<f64>,
    model: Option<&CpModel>,
    min: f64,
    max: f64,
) -> Option<(f64, FtpMethod)> {
    let measured = best_20min_w.filter(|w| w.is_finite() && *w > 0.0);
    let modelled = model
        .map(|m| m.power_at(1_200.0))
        .filter(|w| w.is_finite() && *w > 0.0);
    let (watts, method) = match (measured, modelled) {
        (Some(a), Some(b)) if b > a => (b, FtpMethod::Model),
        (Some(a), _) => (a, FtpMethod::TwentyMinutes),
        (None, Some(b)) => (b, FtpMethod::Model),
        (None, None) => return None,
    };
    Some(((watts * FTP_FACTOR).round().clamp(min, max), method))
}

/// Banister's TRIMP weight of one minute at heart-rate reserve fraction `x` (0..=1).
fn trimp_weight(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * 0.64 * (TRIMP_B * x).exp()
}

/// The heart-rate training load of a ride from its seconds-per-bpm histogram
/// ([`RideMeta::hr_secs`]): its TRIMP over the TRIMP of an hour at threshold, × 100. `None`
/// without heart rate or with unusable settings.
pub fn hr_tss(hr_secs: &[u32], rider: &RiderSettings) -> Option<f64> {
    if hr_secs.iter().all(|s| *s == 0) {
        return None;
    }
    let max = rider.max_hr;
    let rest = rider.effective_rest_hr();
    if !max.is_finite() || max <= rest + 10.0 {
        return None;
    }
    let reserve = |bpm: f64| (bpm - rest) / (max - rest);
    let trimp: f64 = hr_secs
        .iter()
        .enumerate()
        .map(|(i, secs)| {
            let bpm = f64::from(HR_HIST_FROM) + i as f64;
            f64::from(*secs) / 60.0 * trimp_weight(reserve(bpm))
        })
        .sum();
    let hour_at_threshold = 60.0 * trimp_weight(reserve(rider.effective_lthr()));
    (hour_at_threshold > 0.0).then(|| trimp / hour_at_threshold * 100.0)
}

/// Where a ride's training load comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSource {
    /// TSS from measured watts.
    Power,
    /// hrTSS (no measured watts).
    HeartRate,
    /// TSS from estimated watts, without heart rate to do better.
    Estimated,
}

/// The training load of one stored ride: TSS from measured power, else hrTSS, else the TSS of
/// estimated watts. A simulated ride has none (nobody rode it).
pub fn ride_load(meta: &RideMeta, rider: &RiderSettings) -> Option<(f64, LoadSource)> {
    if meta.simulated {
        return None;
    }
    let tss = meta.tss.filter(|t| t.is_finite() && *t >= 0.0);
    if !meta.power_estimated {
        if let Some(tss) = tss {
            return Some((tss, LoadSource::Power));
        }
    }
    if let Some(hr) = hr_tss(&meta.hr_secs, rider) {
        return Some((hr, LoadSource::HeartRate));
    }
    tss.filter(|_| meta.power_estimated)
        .map(|t| (t, LoadSource::Estimated))
}

/// Seconds per heart-rate zone 1..=5 of a histogram, with the rider's current zone model.
pub fn hr_zone_secs(hr_secs: &[u32], rider: &RiderSettings) -> [f64; 5] {
    let mut out = [0.0; 5];
    for (i, secs) in hr_secs.iter().enumerate() {
        let bpm = f64::from(HR_HIST_FROM) + i as f64;
        let zone = crate::zones::hr_zone_of(bpm, rider);
        if (1..=5).contains(&zone) {
            out[usize::from(zone - 1)] += f64::from(*secs);
        }
    }
    out
}

/// One day of the performance management chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PmcDay {
    /// Days since 1970-01-01 (UTC).
    pub day: i64,
    /// The load ridden that day.
    pub load: f64,
    /// Fitness at the end of the day.
    pub ctl: f64,
    /// Fatigue at the end of the day.
    pub atl: f64,
    /// Form at the start of the day (yesterday's CTL − ATL).
    pub tsb: f64,
}

/// The chart from `from_day` to `to_day` (inclusive) for the daily loads `(day, load)` (several
/// entries of one day add up). The history before `from_day` is run through as well, so the
/// first day shown already carries the fitness built before it; the run starts at the first
/// load and at most 3 years before `to_day`.
pub fn pmc(loads: &[(i64, f64)], from_day: i64, to_day: i64) -> Vec<PmcDay> {
    if to_day < from_day || to_day - from_day > 3_700 {
        return Vec::new();
    }
    let floor = to_day - 3 * 366;
    let mut daily: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    for &(day, load) in loads {
        if load.is_finite() && load > 0.0 && day >= floor && day <= to_day {
            *daily.entry(day).or_insert(0.0) += load;
        }
    }
    let start = daily
        .keys()
        .next()
        .copied()
        .unwrap_or(from_day)
        .min(from_day);
    let (mut ctl, mut atl) = (0.0_f64, 0.0_f64);
    let mut out = Vec::with_capacity((to_day - from_day + 1) as usize);
    for day in start..=to_day {
        let load = daily.get(&day).copied().unwrap_or(0.0);
        let tsb = ctl - atl;
        ctl += (load - ctl) / CTL_DAYS;
        atl += (load - atl) / ATL_DAYS;
        if day >= from_day {
            out.push(PmcDay {
                day,
                load,
                ctl,
                atl,
                tsb,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn efforts(cp: f64, w_prime: f64, durations: &[u32]) -> Vec<(u32, f64)> {
        durations
            .iter()
            .map(|&t| (t, cp + w_prime / f64::from(t)))
            .collect()
    }

    #[test]
    fn an_exact_cp_curve_is_recovered() {
        let points = efforts(
            260.0,
            18_000.0,
            &[60, 180, 300, 480, 600, 720, 1_200, 3_600],
        );
        let model = fit_cp(&points).expect("model");
        assert!((model.cp_w - 260.0).abs() < 1e-6, "{model:?}");
        assert!((model.w_prime_j - 18_000.0).abs() < 1e-3, "{model:?}");
        // the 1 min and the 60 min points are outside the fit window
        assert_eq!(model.points, 6);
        assert!(model.r2 > 0.999);
        assert!((model.power_at(1_200.0) - 275.0).abs() < 1e-6);
    }

    #[test]
    fn too_few_or_too_short_efforts_give_no_model() {
        assert!(fit_cp(&efforts(260.0, 18_000.0, &[180, 300])).is_none());
        // three points, but none of 8 minutes or more
        assert!(fit_cp(&efforts(260.0, 18_000.0, &[120, 180, 300])).is_none());
        assert!(fit_cp(&[]).is_none());
    }

    #[test]
    fn a_model_that_is_not_physiological_is_refused() {
        // flat: every duration the same power, W' = 0
        let flat: Vec<(u32, f64)> = [180, 300, 600, 1_200].iter().map(|&t| (t, 250.0)).collect();
        assert!(fit_cp(&flat).is_none());
        // rising with duration: negative W'
        let rising: Vec<(u32, f64)> = [(180, 200.0), (300, 220.0), (600, 240.0), (1_200, 250.0)]
            .into_iter()
            .collect();
        assert!(fit_cp(&rising).is_none());
        // nonsense values are skipped, not used
        let mut noisy = efforts(260.0, 18_000.0, &[180, 300, 600, 1_200]);
        noisy.push((900, f64::NAN));
        noisy.push((720, -5.0));
        assert_eq!(fit_cp(&noisy).expect("model").points, 4);
    }

    #[test]
    fn the_ftp_takes_the_better_of_measured_and_modelled_twenty_minutes() {
        let model = fit_cp(&efforts(260.0, 18_000.0, &[180, 300, 600, 1_200])).expect("model");
        // the model says 275 W for 20 min; a submaximal 20 min of 240 W does not drag it down
        let (ftp, method) = estimate_ftp_from(Some(240.0), Some(&model), 50.0, 600.0).unwrap();
        assert_eq!(method, FtpMethod::Model);
        assert_eq!(ftp, (275.0_f64 * 0.95).round());
        // a real 20 min test above the model wins
        let (ftp, method) = estimate_ftp_from(Some(290.0), Some(&model), 50.0, 600.0).unwrap();
        assert_eq!(method, FtpMethod::TwentyMinutes);
        assert_eq!(ftp, (290.0_f64 * 0.95).round());
        // no model: the classic rule
        assert_eq!(
            estimate_ftp_from(Some(250.0), None, 50.0, 600.0),
            Some((238.0, FtpMethod::TwentyMinutes))
        );
        assert_eq!(estimate_ftp_from(None, None, 50.0, 600.0), None);
        assert_eq!(estimate_ftp_from(Some(f64::NAN), None, 50.0, 600.0), None);
        // the setting's range
        assert_eq!(
            estimate_ftp_from(Some(2_000.0), None, 50.0, 600.0).map(|r| r.0),
            Some(600.0)
        );
    }

    fn rider() -> RiderSettings {
        RiderSettings {
            max_hr: 190.0,
            rest_hr: 50.0,
            lthr: 170.0,
            ..RiderSettings::default()
        }
    }

    fn histogram(bpm: u32, secs: u32) -> Vec<u32> {
        let mut out = vec![0; (bpm - HR_HIST_FROM + 1) as usize];
        if let Some(last) = out.last_mut() {
            *last = secs;
        }
        out
    }

    #[test]
    fn an_hour_at_threshold_heart_rate_is_100() {
        let tss = hr_tss(&histogram(170, 3_600), &rider()).expect("hrTSS");
        assert!((tss - 100.0).abs() < 1e-9, "{tss}");
        // half an hour is half of it, an easier hour far less
        let half = hr_tss(&histogram(170, 1_800), &rider()).unwrap();
        assert!((half - 50.0).abs() < 1e-9);
        let easy = hr_tss(&histogram(130, 3_600), &rider()).unwrap();
        assert!(easy > 20.0 && easy < 50.0, "{easy}");
        assert_eq!(hr_tss(&[], &rider()), None);
        assert_eq!(hr_tss(&[0, 0, 0], &rider()), None);
    }

    #[test]
    fn the_load_prefers_measured_power_then_heart_rate_then_an_estimate() {
        let rider = rider();
        let mut meta = RideMeta {
            tss: Some(80.0),
            hr_secs: histogram(170, 3_600),
            ..RideMeta::default()
        };
        assert_eq!(ride_load(&meta, &rider), Some((80.0, LoadSource::Power)));
        meta.power_estimated = true;
        let (load, source) = ride_load(&meta, &rider).unwrap();
        assert_eq!(source, LoadSource::HeartRate);
        assert!((load - 100.0).abs() < 1e-9);
        meta.hr_secs.clear();
        assert_eq!(
            ride_load(&meta, &rider),
            Some((80.0, LoadSource::Estimated))
        );
        meta.simulated = true;
        assert_eq!(ride_load(&meta, &rider), None);
        // measured watts, no TSS (no power at all): heart rate
        let only_hr = RideMeta {
            hr_secs: histogram(170, 1_800),
            ..RideMeta::default()
        };
        assert_eq!(
            ride_load(&only_hr, &rider).map(|l| l.1),
            Some(LoadSource::HeartRate)
        );
    }

    #[test]
    fn zones_of_a_histogram_follow_the_current_model() {
        let mut rider = rider();
        let secs = histogram(160, 600);
        assert_eq!(hr_zone_secs(&secs, &rider), [0.0, 0.0, 0.0, 600.0, 0.0]);
        rider.hr_zone_model = ride_model::HrZoneModel::Lthr;
        // 160 / 170 = 94 %: zone 3 of the threshold model
        assert_eq!(hr_zone_secs(&secs, &rider), [0.0, 0.0, 600.0, 0.0, 0.0]);
    }

    #[test]
    fn fitness_fatigue_and_form() {
        // 100 every day for 42 days, then nothing
        let loads: Vec<(i64, f64)> = (0..42).map(|d| (d, 100.0)).collect();
        let chart = pmc(&loads, 0, 60);
        assert_eq!(chart.len(), 61);
        let day1 = chart[0];
        assert!((day1.ctl - 100.0 / 42.0).abs() < 1e-9);
        assert!((day1.atl - 100.0 / 7.0).abs() < 1e-9);
        assert_eq!(day1.tsb, 0.0, "form is yesterday's");
        // after six weeks fatigue is near the daily load, fitness well below it
        let d41 = chart[41];
        assert!(
            d41.atl > 99.0 && d41.ctl > 60.0 && d41.ctl < 66.0,
            "{d41:?}"
        );
        assert!(chart[42].tsb < -30.0, "tired the day after");
        // rest: fatigue drops faster than fitness, form turns positive
        assert!(chart[60].tsb > 0.0, "{:?}", chart[60]);
        assert!(chart[60].ctl > chart[60].atl);
    }

    #[test]
    fn the_history_before_the_window_still_counts() {
        let loads: Vec<(i64, f64)> = (0..100).map(|d| (d, 50.0)).collect();
        let chart = pmc(&loads, 90, 99);
        assert_eq!(chart.len(), 10);
        assert!(chart[0].ctl > 40.0, "{:?}", chart[0]);
        // several rides on one day add up, junk is ignored
        let chart = pmc(&[(5, 30.0), (5, 20.0), (5, f64::NAN), (5, -10.0)], 5, 5);
        assert!((chart[0].load - 50.0).abs() < 1e-9);
        assert!(pmc(&loads, 10, 5).is_empty());
    }
}
