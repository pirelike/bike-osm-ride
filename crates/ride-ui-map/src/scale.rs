//! The printed scale bar in the lower-left corner of the map: ground resolution of the camera
//! and a "nice" round length (1, 2, 5 x 10^n metres) that fits in a given pixel budget.

use crate::mercator;

/// Equatorial circumference of the Web-Mercator sphere, metres.
const EARTH_CIRCUMFERENCE_M: f64 = 40_075_016.685_578_5;

/// Ground metres covered by one CSS pixel at `lat` for a (fractional) camera `zoom`.
/// Returns `None` for non-finite input.
pub fn meters_per_pixel(lat: f64, zoom: f64) -> Option<f64> {
    if !lat.is_finite() || !zoom.is_finite() {
        return None;
    }
    let world = mercator::world_size(mercator::clamp_zoom(zoom));
    if !world.is_finite() || world <= 0.0 {
        return None;
    }
    let value = EARTH_CIRCUMFERENCE_M * mercator::clamp_lat(lat).to_radians().cos() / world;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// A scale bar: its length on screen and the label printed next to it.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleBar {
    /// Real-world length in metres.
    pub meters: f64,
    /// Length on screen in CSS pixels.
    pub px: f64,
    /// `"500 m"`, `"2 km"`.
    pub label: String,
}

/// The longest 1-2-5 length that stays within `max_px` pixels. `None` when the input is
/// unusable or even the shortest candidate (1 m) would be wider than `max_px`.
pub fn scale_bar(meters_per_px: f64, max_px: f64) -> Option<ScaleBar> {
    if !meters_per_px.is_finite() || meters_per_px <= 0.0 || !max_px.is_finite() || max_px < 8.0 {
        return None;
    }
    let budget_m = meters_per_px * max_px;
    let mut exponent = budget_m.log10().floor();
    if !exponent.is_finite() {
        return None;
    }
    // Walk down from the budget's decade: 5, 2, 1 x 10^n, then the next decade down.
    for _ in 0..3 {
        let base = 10f64.powf(exponent);
        for factor in [5.0, 2.0, 1.0] {
            let meters = factor * base;
            if meters <= budget_m && meters >= 1.0 {
                let px = meters / meters_per_px;
                return Some(ScaleBar {
                    meters,
                    px,
                    label: scale_label(meters),
                });
            }
        }
        exponent -= 1.0;
    }
    None
}

/// `"50 m"`, `"1 km"`, `"500 km"`.
pub fn scale_label(meters: f64) -> String {
    if meters >= 1000.0 {
        format!("{} km", (meters / 1000.0).round() as u64)
    } else {
        format!("{} m", meters.round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_resolution_matches_the_textbook_values() {
        // zoom 3 (the lowest the map allows), equator: 156 543 m / 8
        let z3 = meters_per_pixel(0.0, 3.0).expect("value");
        assert!((z3 - 19_567.88).abs() < 0.1, "{z3}");
        // below the minimum zoom the value is clamped, never extrapolated
        assert_eq!(meters_per_pixel(0.0, 0.0), Some(z3));
        // zoom 15 at Budapest (47.5 N): ~3.2 m / px
        let z15 = meters_per_pixel(47.5, 15.0).expect("value");
        assert!((z15 - 3.22).abs() < 0.05, "{z15}");
        // every zoom step halves the resolution
        let z16 = meters_per_pixel(47.5, 16.0).expect("value");
        assert!((z15 / z16 - 2.0).abs() < 1e-9);
        assert_eq!(meters_per_pixel(f64::NAN, 3.0), None);
        assert_eq!(meters_per_pixel(10.0, f64::INFINITY), None);
    }

    #[test]
    fn bar_lengths_are_round_and_fit_the_budget() {
        for (mpp, max_px) in [(3.22, 96.0), (0.5, 120.0), (610.0, 100.0), (9.55, 80.0)] {
            let bar = scale_bar(mpp, max_px).expect("bar");
            assert!(bar.px <= max_px + 1e-9, "{bar:?}");
            assert!(bar.px >= max_px * 0.19, "{bar:?} is much too short");
            let mantissa = bar.meters / 10f64.powf(bar.meters.log10().floor());
            assert!(
                [1.0, 2.0, 5.0].iter().any(|m| (mantissa - m).abs() < 1e-9),
                "{} is not 1-2-5",
                bar.meters
            );
        }
        let bar = scale_bar(3.22, 96.0).expect("bar");
        assert_eq!(bar.label, "200 m");
        assert_eq!(scale_label(2000.0), "2 km");
        assert_eq!(scale_label(50.0), "50 m");
    }

    #[test]
    fn unusable_input_gives_no_bar() {
        assert_eq!(scale_bar(0.0, 100.0), None);
        assert_eq!(scale_bar(-1.0, 100.0), None);
        assert_eq!(scale_bar(f64::NAN, 100.0), None);
        assert_eq!(scale_bar(1.0, 2.0), None);
        // 0.05 m per pixel: nothing >= 1 m fits in 20 px
        assert_eq!(scale_bar(0.01, 20.0), None);
    }
}
