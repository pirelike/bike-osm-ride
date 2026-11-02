//! Display formatting shared by every UI crate (Hungarian conventions: decimal comma
//! is NOT used in the HUD — numbers stay dot-free for tabular alignment; units are
//! rendered by the caller). Pure functions, unit-tested.

/// `1:05:09` for >= 1 h, `5:09` otherwise. Negative / NaN → `0:00`.
pub fn fmt_duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00".into();
    }
    let s = secs.round() as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Metres → kilometres with sensible precision: `0.42`, `12.34`, `123.4`.
pub fn fmt_km(m: f64) -> String {
    let km = (m / 1000.0).max(0.0);
    if km >= 100.0 {
        format!("{km:.1}")
    } else {
        format!("{km:.2}")
    }
}

/// m/s → km/h, one decimal.
pub fn fmt_speed_kmh(ms: f64) -> String {
    format!("{:.1}", (ms * 3.6).max(0.0))
}

/// Whole-number value or an em dash when unavailable.
pub fn fmt_opt0(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{}", x.round() as i64),
        _ => "—".into(),
    }
}

/// Signed grade with one decimal: `+4.2`, `-3.1`, `0.0`.
pub fn fmt_grade(g: f64) -> String {
    if g.abs() < 0.05 {
        "0.0".into()
    } else {
        format!("{g:+.1}")
    }
}

/// Metres, whole: `1234`.
pub fn fmt_m(m: f64) -> String {
    format!("{}", m.round() as i64)
}

/// A signed difference in seconds, whole seconds: `+12`, `-6`, `0`. Used for the ghost and the
/// segment read-outs (positive = behind). NaN or an infinity yields an em dash.
pub fn fmt_delta_s(delta: f64) -> String {
    if !delta.is_finite() {
        return "—".into();
    }
    let rounded = delta.round();
    if rounded == 0.0 {
        "0".into()
    } else {
        format!("{rounded:+.0}")
    }
}

/// A signed difference as a clock time: `-0:32`, `+1:05:09`, `0:00`. Positive = behind.
pub fn fmt_delta_duration(delta: f64) -> String {
    if !delta.is_finite() {
        return "—".into();
    }
    let rounded = delta.round();
    if rounded == 0.0 {
        return fmt_duration(0.0);
    }
    let sign = if rounded < 0.0 { "-" } else { "+" };
    format!("{sign}{}", fmt_duration(rounded.abs()))
}

/// Seconds → `±h:mm` style clock time of arrival is the caller's job; this renders
/// "remaining" like `1:23 ó` / `23 p`.
pub fn fmt_hm_text(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".into();
    }
    let total_min = (secs / 60.0).round() as u64;
    let (h, m) = (total_min / 60, total_min % 60);
    if h > 0 {
        // `h:mm ó`, not `h ó mm p`: in the condensed numeral font a lone "ó" between two numbers
        // reads as a 6 ("1 ó 23 p" looks like 16 23)
        format!("{h}:{m:02} ó")
    } else {
        format!("{m} p")
    }
}

// ───────────────────────────── form helpers (added by the design kit) ─────────────────────────────

/// Number of decimals a step needs (`1` → 0, `0.5` → 1, `0.001` → 3; capped at 6).
fn step_decimals(step: f64) -> usize {
    if !step.is_finite() || step <= 0.0 {
        return 0;
    }
    for d in 0..=6usize {
        let scaled = step * 10f64.powi(d as i32);
        if (scaled - scaled.round()).abs() < 1e-9 {
            return d;
        }
    }
    6
}

/// Formats a form value with exactly the precision its step implies, without float noise:
/// `fmt_step_value(0.30000000000000004, 0.01)` → `"0.30"`, `fmt_step_value(75.0, 1.0)` → `"75"`.
/// Negative zero is printed as `0`.
pub fn fmt_step_value(value: f64, step: f64) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let s = format!("{:.*}", step_decimals(step), value);
    match s.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
        _ => s,
    }
}

/// Parses user typed or browser supplied numbers. Accepts a decimal comma (Hungarian keyboards)
/// and surrounding spaces; rejects empty text, NaN and infinities.
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// [`parse_number`] restricted to `[min, max]`.
pub fn parse_in_range(text: &str, min: f64, max: f64) -> Option<f64> {
    parse_number(text).filter(|v| *v >= min && *v <= max)
}

/// Clamps to `[min, max]` and snaps to the grid `min + k·step` (float noise removed).
/// A non-positive/NaN step only clamps. NaN input becomes `min`.
pub fn snap_to_step(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let v = if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        lo
    };
    if !step.is_finite() || step <= 0.0 {
        return v;
    }
    let snapped = lo + ((v - lo) / step).round() * step;
    let factor = 10f64.powi(step_decimals(step) as i32);
    ((snapped * factor).round() / factor).clamp(lo, hi)
}

/// Position of `value` inside `[min, max]` as 0..=1 (0 for an empty/invalid range).
pub fn fraction(value: f64, min: f64, max: f64) -> f64 {
    if !(value.is_finite() && min.is_finite() && max.is_finite()) || max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_helpers() {
        assert_eq!(fmt_step_value(0.30000000000000004, 0.01), "0.30");
        assert_eq!(fmt_step_value(75.0, 1.0), "75");
        assert_eq!(fmt_step_value(1.225, 0.001), "1.225");
        assert_eq!(fmt_step_value(-0.0, 0.1), "0.0");
        assert_eq!(fmt_step_value(-0.04, 0.1), "0.0");
        assert_eq!(fmt_step_value(-2.5, 0.5), "-2.5");
        assert_eq!(fmt_step_value(f64::NAN, 1.0), "—");
        assert_eq!(parse_number(" 0,32 "), Some(0.32));
        assert_eq!(parse_number("12"), Some(12.0));
        for bad in ["", "  ", "abc", "NaN", "inf", "1e999"] {
            assert_eq!(parse_number(bad), None, "{bad:?}");
        }
        assert_eq!(parse_in_range("5", 0.0, 10.0), Some(5.0));
        assert_eq!(parse_in_range("11", 0.0, 10.0), None);
        assert_eq!(parse_in_range("-1", 0.0, 10.0), None);
        assert_eq!(snap_to_step(0.30000000000000004, 0.0, 1.0, 0.1), 0.3);
        assert_eq!(snap_to_step(7.3, 0.0, 10.0, 0.5), 7.5);
        assert_eq!(snap_to_step(99.0, 0.0, 10.0, 1.0), 10.0);
        assert_eq!(snap_to_step(-5.0, 0.0, 10.0, 1.0), 0.0);
        assert_eq!(snap_to_step(f64::NAN, 2.0, 10.0, 1.0), 2.0);
        assert_eq!(snap_to_step(3.3, 0.0, 10.0, 0.0), 3.3);
        assert_eq!(snap_to_step(0.3 - 0.1, 0.0, 1.0, 0.01), 0.2);
        assert_eq!(fraction(5.0, 0.0, 10.0), 0.5);
        assert_eq!(fraction(50.0, 0.0, 10.0), 1.0);
        assert_eq!(fraction(5.0, 10.0, 0.0), 0.0);
        assert_eq!(fraction(f64::NAN, 0.0, 10.0), 0.0);
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(0.0), "0:00");
        assert_eq!(fmt_duration(309.0), "5:09");
        assert_eq!(fmt_duration(3909.0), "1:05:09");
        assert_eq!(fmt_duration(f64::NAN), "0:00");
    }

    #[test]
    fn numbers() {
        assert_eq!(fmt_km(420.0), "0.42");
        assert_eq!(fmt_km(123_456.0), "123.5");
        assert_eq!(fmt_speed_kmh(10.0), "36.0");
        assert_eq!(fmt_opt0(None), "—");
        assert_eq!(fmt_opt0(Some(249.6)), "250");
        assert_eq!(fmt_grade(4.23), "+4.2");
        assert_eq!(fmt_grade(-3.16), "-3.2");
        assert_eq!(fmt_grade(0.01), "0.0");
        assert_eq!(fmt_hm_text(4980.0), "1:23 ó");
    }

    #[test]
    fn signed_differences() {
        assert_eq!(fmt_delta_s(12.4), "+12");
        assert_eq!(fmt_delta_s(-6.0), "-6");
        assert_eq!(fmt_delta_s(0.4), "0");
        assert_eq!(fmt_delta_s(-0.4), "0", "no negative zero");
        assert_eq!(fmt_delta_s(f64::NAN), "—");
        assert_eq!(fmt_delta_duration(-32.0), "-0:32");
        assert_eq!(fmt_delta_duration(65.0), "+1:05");
        assert_eq!(fmt_delta_duration(3909.0), "+1:05:09");
        assert_eq!(fmt_delta_duration(-0.4), "0:00");
        assert_eq!(fmt_delta_duration(f64::INFINITY), "—");
    }
}
