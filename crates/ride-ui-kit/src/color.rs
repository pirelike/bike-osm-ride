//! Small colour toolbox: `#rrggbb` parsing, OKLab interpolation, WCAG contrast and the
//! OKLab ΔE used by the dataviz palette validator. Pure functions, no allocation except the
//! returned strings. Everything is total: invalid input yields `None` / a neutral fallback,
//! never a panic.

/// 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Parses `#rrggbb` or `rrggbb` (case-insensitive). Anything else → `None`.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// `#rrggbb`, lower-case.
pub fn to_hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn to_linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// OKLab `[L, a, b]` (L in 0..1).
pub fn to_oklab(c: Rgb) -> [f64; 3] {
    let (r, g, b) = (to_linear(c.0), to_linear(c.1), to_linear(c.2));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// Inverse of [`to_oklab`], clamped into the sRGB gamut.
pub fn from_oklab(lab: [f64; 3]) -> Rgb {
    let [ll, a, b] = lab;
    let l = (ll + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (ll - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (ll - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    Rgb(
        from_linear(4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s),
        from_linear(-1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s),
        from_linear(-0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s),
    )
}

/// Perceptual interpolation in OKLab; `t` is clamped to 0..=1.
pub fn lerp_oklab(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (la, lb) = (to_oklab(a), to_oklab(b));
    from_oklab([
        la[0] + (lb[0] - la[0]) * t,
        la[1] + (lb[1] - la[1]) * t,
        la[2] + (lb[2] - la[2]) * t,
    ])
}

/// WCAG relative luminance.
pub fn relative_luminance(c: Rgb) -> f64 {
    0.2126 * to_linear(c.0) + 0.7152 * to_linear(c.1) + 0.0722 * to_linear(c.2)
}

/// WCAG contrast ratio (1..=21).
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Euclidean distance in OKLab ×100 (the ΔE the dataviz validator uses).
pub fn delta_e(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (to_oklab(a), to_oklab(b));
    100.0 * ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
}

/// OKLCH lightness of a colour (0..1).
pub fn oklch_lightness(c: Rgb) -> f64 {
    to_oklab(c)[0]
}

/// `rgba(r, g, b, a)` from a `#rrggbb` string; invalid input gives fully transparent black.
pub fn with_alpha(hex: &str, alpha: f64) -> String {
    let c = parse_hex(hex).unwrap_or(Rgb(0, 0, 0));
    let a = if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if parse_hex(hex).is_none() {
        return "rgba(0, 0, 0, 0)".into();
    }
    format!("rgba({}, {}, {}, {})", c.0, c.1, c.2, trim_float(a))
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

/// Ink colour (`#0b0e14` or `#ffffff`) that reads best on top of `fill`, for labels set inside
/// a coloured mark (the one case where text may sit on a data colour).
pub fn ink_on(fill: &str) -> &'static str {
    match parse_hex(fill) {
        Some(c)
            if contrast_ratio(c, Rgb(0x0b, 0x0e, 0x14))
                >= contrast_ratio(c, Rgb(255, 255, 255)) =>
        {
            "#0b0e14"
        }
        Some(_) => "#ffffff",
        None => "#ffffff",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_rejects_garbage() {
        assert_eq!(parse_hex("#4DE3CA"), Some(Rgb(0x4d, 0xe3, 0xca)));
        assert_eq!(to_hex(Rgb(0x4d, 0xe3, 0xca)), "#4de3ca");
        for bad in ["", "#12", "#12345", "#1234567", "zzzzzz", "#ééé", "#12345g"] {
            assert_eq!(parse_hex(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn oklab_round_trip_is_stable() {
        for hex in [
            "#000000", "#ffffff", "#4de3ca", "#bb1868", "#808693", "#af9001",
        ] {
            let c = parse_hex(hex).unwrap();
            let back = from_oklab(to_oklab(c));
            assert!(
                (i32::from(back.0) - i32::from(c.0)).abs() <= 1,
                "{hex} {back:?}"
            );
            assert!(
                (i32::from(back.1) - i32::from(c.1)).abs() <= 1,
                "{hex} {back:?}"
            );
            assert!(
                (i32::from(back.2) - i32::from(c.2)).abs() <= 1,
                "{hex} {back:?}"
            );
        }
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        let black = Rgb(0, 0, 0);
        let white = Rgb(255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
        // #767676 on white is the classic 4.54:1 AA boundary
        assert!((contrast_ratio(Rgb(0x76, 0x76, 0x76), white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn lerp_hits_endpoints_and_clamps() {
        let (a, b) = (Rgb(10, 20, 30), Rgb(200, 150, 100));
        assert_eq!(lerp_oklab(a, b, 0.0), a);
        assert_eq!(lerp_oklab(a, b, 1.0), b);
        assert_eq!(lerp_oklab(a, b, -5.0), a);
        assert_eq!(lerp_oklab(a, b, 9.0), b);
        assert_eq!(lerp_oklab(a, b, f64::NAN), a);
    }

    #[test]
    fn alpha_and_ink() {
        assert_eq!(with_alpha("#ff0000", 0.5), "rgba(255, 0, 0, 0.5)");
        assert_eq!(with_alpha("#ff0000", 2.0), "rgba(255, 0, 0, 1)");
        assert_eq!(with_alpha("nope", 0.5), "rgba(0, 0, 0, 0)");
        assert_eq!(ink_on("#f6c330"), "#0b0e14");
        assert_eq!(ink_on("#552990"), "#ffffff");
        assert_eq!(ink_on("garbage"), "#ffffff");
    }
}
