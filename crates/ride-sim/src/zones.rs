//! Training zones. Power zones follow Coggan's seven levels as % of FTP, heart rate
//! zones are the five-level model as % of maximum HR (see `docs/ARCHITECTURE.md`).

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
