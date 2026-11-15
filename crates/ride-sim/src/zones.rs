//! Training zones. Power zones follow Coggan's seven levels as % of FTP. Heart-rate zones have
//! five levels in one of three models ([`HrZoneModel`]): % of maximum HR (the default), % of the
//! heart-rate reserve (Karvonen) or % of the lactate threshold heart rate (Coggan).

use ride_model::{HrZoneModel, RiderSettings};

/// Coggan power zone 1..=7 from watts and FTP; `0` when FTP is unusable.
pub fn power_zone(power_w: f64, ftp_w: f64) -> u8 {
    if !ftp_w.is_finite() || ftp_w <= 0.0 || !power_w.is_finite() {
        return 0;
    }
    ride_model::power_zone_of_pct((power_w.max(0.0) / ftp_w) * 100.0)
}

/// 5-zone HR model (% of max HR) 1..=5; `0` when max HR is unusable.
pub fn hr_zone(hr_bpm: f64, max_hr: f64) -> u8 {
    if !max_hr.is_finite() || max_hr <= 0.0 || !hr_bpm.is_finite() {
        return 0;
    }
    let pct = (hr_bpm.max(0.0) / max_hr) * 100.0;
    if pct < 60.0 {
        1
    } else if pct < 70.0 {
        2
    } else if pct < 80.0 {
        3
    } else if pct < 90.0 {
        4
    } else {
        5
    }
}

/// Lower bounds (bpm) of zones 2..=5 for the rider's settings, in the chosen model. The values
/// are not rounded: the zone of a reading is decided against them.
pub fn hr_zone_bounds(rider: &RiderSettings) -> [f64; 4] {
    let max = if rider.max_hr.is_finite() && rider.max_hr > 0.0 {
        rider.max_hr
    } else {
        190.0
    };
    match rider.hr_zone_model {
        HrZoneModel::MaxHr => [0.6, 0.7, 0.8, 0.9].map(|f| f * max),
        HrZoneModel::Reserve => {
            let rest = rider.effective_rest_hr();
            [0.6, 0.7, 0.8, 0.9].map(|f| rest + f * (max - rest))
        }
        HrZoneModel::Lthr => {
            let lthr = rider.effective_lthr();
            [0.69, 0.84, 0.95, 1.06].map(|f| f * lthr)
        }
    }
}

/// Heart-rate zone 1..=5 of a reading with the rider's zone model; `0` for an unusable reading.
///
/// In the reserve model zone 1 starts at rest (50 % of the reserve is the usual bottom of
/// zone 1, but a reading below it is still easy riding, not "no zone").
pub fn hr_zone_of(hr_bpm: f64, rider: &RiderSettings) -> u8 {
    if !hr_bpm.is_finite() {
        return 0;
    }
    let bounds = hr_zone_bounds(rider);
    let hr = hr_bpm.max(0.0);
    1 + bounds.iter().filter(|b| hr >= **b).count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_zone_boundaries() {
        let ftp = 200.0;
        assert_eq!(power_zone(0.0, ftp), 1);
        assert_eq!(power_zone(109.0, ftp), 1); // 54.5 %
        assert_eq!(power_zone(110.0, ftp), 2); // 55 %
        assert_eq!(power_zone(151.9, ftp), 2); // 75.95 %
        assert_eq!(power_zone(152.0, ftp), 3); // 76 %
        assert_eq!(power_zone(182.0, ftp), 4); // 91 %
        assert_eq!(power_zone(212.0, ftp), 5); // 106 %
        assert_eq!(power_zone(242.0, ftp), 6); // 121 %
        assert_eq!(power_zone(302.0, ftp), 7); // 151 %
    }

    #[test]
    fn power_zone_handles_bad_input() {
        assert_eq!(power_zone(250.0, 0.0), 0);
        assert_eq!(power_zone(250.0, -10.0), 0);
        assert_eq!(power_zone(f64::NAN, 200.0), 0);
        assert_eq!(power_zone(-30.0, 200.0), 1);
    }

    #[test]
    fn the_three_heart_rate_models() {
        let mut rider = RiderSettings {
            max_hr: 190.0,
            rest_hr: 50.0,
            lthr: 170.0,
            ..RiderSettings::default()
        };
        // % of max: the same as `hr_zone`
        for hr in [100.0, 113.0, 114.0, 133.0, 152.0, 171.0, 185.0] {
            assert_eq!(hr_zone_of(hr, &rider), hr_zone(hr, 190.0), "{hr}");
        }
        // reserve: 50 + 0.6 * 140 = 134
        rider.hr_zone_model = HrZoneModel::Reserve;
        assert_eq!(hr_zone_of(133.0, &rider), 1);
        assert_eq!(hr_zone_of(134.0, &rider), 2);
        assert_eq!(hr_zone_of(176.0, &rider), 5); // 50 + 0.9 * 140
                                                  // threshold: 0.95 * 170 = 161.5
        rider.hr_zone_model = HrZoneModel::Lthr;
        assert_eq!(hr_zone_of(161.0, &rider), 3);
        assert_eq!(hr_zone_of(162.0, &rider), 4);
        assert_eq!(hr_zone_of(181.0, &rider), 5); // 1.06 * 170 = 180.2
        assert_eq!(hr_zone_of(f64::NAN, &rider), 0);
        // an LTHR that was never set is 90 % of the maximum
        rider.lthr = 0.0;
        assert_eq!(rider.effective_lthr(), 171.0);
    }

    #[test]
    fn hr_zone_boundaries() {
        let max = 190.0;
        assert_eq!(hr_zone(113.0, max), 1); // 59.5 %
        assert_eq!(hr_zone(114.0, max), 2); // 60 %
        assert_eq!(hr_zone(133.0, max), 3); // 70 %
        assert_eq!(hr_zone(152.0, max), 4); // 80 %
        assert_eq!(hr_zone(171.0, max), 5); // 90 %
        assert_eq!(hr_zone(60.0, 0.0), 0);
    }
}
