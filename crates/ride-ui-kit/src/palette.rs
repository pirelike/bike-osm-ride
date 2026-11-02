//! Colour functions for canvas/SVG drawing (power zones, HR zones, route grades, climb
//! categories). The values are the palettes validated with the dataviz `validate_palette.js`
//! script for both themes (results in `docs/DESIGN.md`) and are mirrored 1:1 by the CSS tokens
//! `--z1…--z7 --h1…--h5 --grade-* --cat-*` in `assets/kit.css`; a unit test fails when the two
//! drift apart.
//!
//! Design rules baked in: grade is a *diverging* scale (cool = descent, neutral grey ≈ 0 %,
//! warm → deep red = climb) interpolated continuously in OKLab; zones are ordered ramps; every
//! colour that carries meaning is also shown with a text label by the UI crates.

use crate::color::{self, Rgb};
use ride_model::{ClimbCategory, Theme};
use std::sync::OnceLock;

pub const POWER_ZONE_NAMES: [&str; 7] = [
    "Aktív regeneráció",
    "Állóképesség",
    "Tempó",
    "Küszöb",
    "VO₂max",
    "Anaerob",
    "Neuromuszkuláris",
];

pub const HR_ZONE_NAMES: [&str; 5] = ["Pihenő", "Könnyű", "Aerob", "Küszöb", "Maximális"];

// ───────────────────────────── palettes (validated, see docs/DESIGN.md) ─────────────────────────────

/// Coggan zones Z1..Z7, ordered cool → hot. Dark theme.
pub const POWER_ZONES_DARK: [&str; 7] = [
    "#5b5de3", "#239ec8", "#00ae67", "#be8900", "#bd4014", "#c1108c", "#9e66dd",
];
/// Coggan zones Z1..Z7. Light theme.
pub const POWER_ZONES_LIGHT: [&str; 7] = [
    "#594ed2", "#248cbe", "#037546", "#af8719", "#cf3313", "#901d54", "#954fb9",
];

/// HR zones H1..H5, one rose hue, dim → bright (intensity = brightness on dark). Dark theme.
pub const HR_ZONES_DARK: [&str; 5] = ["#7b435f", "#a04b6e", "#c65377", "#ee5c77", "#ff7e79"];
/// HR zones H1..H5, pale → deep (intensity = darkness on light). Light theme.
pub const HR_ZONES_LIGHT: [&str; 5] = ["#d29db5", "#cf7b9b", "#cd597d", "#bf2f52", "#a7001c"];

/// Climb categories ordered easy → hard: Cat4, Cat3, Cat2, Cat1, HC. Dark theme.
pub const CLIMB_CATEGORIES_DARK: [&str; 5] =
    ["#6d50a3", "#8663c6", "#9f79e6", "#b793fe", "#cfbafe"];
/// Climb categories ordered easy → hard. Light theme.
pub const CLIMB_CATEGORIES_LIGHT: [&str; 5] =
    ["#b2a4d4", "#a189d3", "#8663c6", "#6e42b2", "#552990"];
/// Uncategorised climb ("Emelkedő"): neutral grey.
pub const CLIMB_NONE_DARK: &str = "#958e82";
pub const CLIMB_NONE_LIGHT: &str = "#8a857a";

/// Anchor colours of the grade scale: `down` (≤ −10 %), `flat` (|g| ≤ 0.5 %), `up_1..up_4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradeAnchors {
    pub down: &'static str,
    pub flat: &'static str,
    pub up: [&'static str; 4],
}

pub const GRADE_ANCHORS_DARK: GradeAnchors = GradeAnchors {
    down: "#4a90e8",
    flat: "#9b9488",
    up: ["#f6c330", "#fa8927", "#f1453f", "#c01a5e"],
};
pub const GRADE_ANCHORS_LIGHT: GradeAnchors = GradeAnchors {
    down: "#2171cc",
    flat: "#8a857a",
    up: ["#d9a200", "#de7000", "#d73431", "#990247"],
};

/// Grade (percent) at which each anchor is reached: `[down, flat−, flat+, up_1, up_2, up_3, up_4]`.
/// Between two stops the colour is interpolated in OKLab, beyond the ends it is clamped.
pub const GRADE_STOP_PCT: [f64; 7] = [-10.0, -0.5, 0.5, 3.0, 6.0, 9.5, 13.0];

/// Accent colours (UI chrome, never data).
pub const ACCENT_DARK: &str = "#ff6f1f";
pub const ACCENT_LIGHT: &str = "#b93405";
/// The ghost of an earlier ride: a cool aqua that stays apart from every grade colour (the route
/// scale) and from the vermilion rider marker. Dark theme.
pub const GHOST_DARK: &str = "#63d8cf";
/// The ghost of an earlier ride. Light theme.
pub const GHOST_LIGHT: &str = "#0f6f74";
/// Status colours (usable as text on the panel surfaces): `[ok, warn, crit]`.
pub const STATUS_DARK: [&str; 3] = ["#7fd18b", "#f2c14e", "#ff6f94"];
pub const STATUS_LIGHT: [&str; 3] = ["#1b6e38", "#845000", "#a8195a"];

fn pick<'a, T: ?Sized>(theme: Theme, dark: &'a T, light: &'a T) -> &'a T {
    match theme {
        Theme::Dark => dark,
        Theme::Light => light,
    }
}

fn hex_of(h: &str) -> Rgb {
    color::parse_hex(h).unwrap_or(Rgb(0x80, 0x80, 0x80))
}

// ───────────────────────────── grade scale ─────────────────────────────

fn grade_stops(theme: Theme) -> &'static [(f64, Rgb); 7] {
    static DARK: OnceLock<[(f64, Rgb); 7]> = OnceLock::new();
    static LIGHT: OnceLock<[(f64, Rgb); 7]> = OnceLock::new();
    let build = |a: GradeAnchors| {
        [
            (GRADE_STOP_PCT[0], hex_of(a.down)),
            (GRADE_STOP_PCT[1], hex_of(a.flat)),
            (GRADE_STOP_PCT[2], hex_of(a.flat)),
            (GRADE_STOP_PCT[3], hex_of(a.up[0])),
            (GRADE_STOP_PCT[4], hex_of(a.up[1])),
            (GRADE_STOP_PCT[5], hex_of(a.up[2])),
            (GRADE_STOP_PCT[6], hex_of(a.up[3])),
        ]
    };
    match theme {
        Theme::Dark => DARK.get_or_init(|| build(GRADE_ANCHORS_DARK)),
        Theme::Light => LIGHT.get_or_init(|| build(GRADE_ANCHORS_LIGHT)),
    }
}

/// Continuous grade colour as RGB. NaN is treated as flat.
pub fn grade_color_rgb(grade_pct: f64, theme: Theme) -> Rgb {
    let stops = grade_stops(theme);
    let g = if grade_pct.is_finite() {
        grade_pct
    } else {
        0.0
    };
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if g <= first.0 {
        return first.1;
    }
    if g >= last.0 {
        return last.1;
    }
    for w in stops.windows(2) {
        let ((g0, c0), (g1, c1)) = (w[0], w[1]);
        if g <= g1 {
            let t = if g1 > g0 { (g - g0) / (g1 - g0) } else { 0.0 };
            return color::lerp_oklab(c0, c1, t);
        }
    }
    last.1
}

/// Colour (CSS `#rrggbb`) for a route grade in percent. Diverging scale: cool for
/// descents, neutral grey near 0 %, warm → dark red for climbs; continuous (interpolated).
pub fn grade_color(grade_pct: f64, theme: Theme) -> String {
    color::to_hex(grade_color_rgb(grade_pct, theme))
}

/// Upper edges of the grade buckets (percent). Bucket `i` covers `[edge[i-1], edge[i])`.
pub const GRADE_BUCKET_EDGES: [f64; 14] = [
    -8.0, -5.0, -3.0, -1.5, -0.5, 0.5, 1.5, 3.0, 4.5, 6.0, 8.0, 10.0, 12.5, 15.0,
];
/// Number of grade buckets (`GRADE_BUCKET_EDGES.len() + 1`).
pub const GRADE_BUCKETS: usize = 15;

/// Index `0..GRADE_BUCKETS` of the bucket a grade falls into (NaN → the flat bucket).
/// Lets the map draw one SVG path per bucket instead of one per segment.
pub fn grade_bucket(grade_pct: f64) -> usize {
    let g = if grade_pct.is_finite() {
        grade_pct
    } else {
        0.0
    };
    GRADE_BUCKET_EDGES
        .iter()
        .position(|&e| g < e)
        .unwrap_or(GRADE_BUCKETS - 1)
}

/// Representative grade of a bucket (its middle; the open ends use −10 % / +17 %).
pub fn grade_bucket_center(bucket: usize) -> f64 {
    let b = bucket.min(GRADE_BUCKETS - 1);
    let lo = if b == 0 {
        -10.0
    } else {
        GRADE_BUCKET_EDGES[b - 1]
    };
    let hi = if b == GRADE_BUCKETS - 1 {
        17.0
    } else {
        GRADE_BUCKET_EDGES[b]
    };
    (lo + hi) / 2.0
}

/// Colour of a grade bucket (`grade_color` at its centre).
pub fn grade_bucket_color(bucket: usize, theme: Theme) -> String {
    grade_color(grade_bucket_center(bucket), theme)
}

// ───────────────────────────── zones ─────────────────────────────

/// Zone 1..=7 (values outside are clamped).
pub fn power_zone_color(zone: u8, theme: Theme) -> String {
    pick(theme, &POWER_ZONES_DARK, &POWER_ZONES_LIGHT)[usize::from(zone.clamp(1, 7)) - 1].into()
}

/// Zone 1..=5.
pub fn hr_zone_color(zone: u8, theme: Theme) -> String {
    pick(theme, &HR_ZONES_DARK, &HR_ZONES_LIGHT)[usize::from(zone.clamp(1, 5)) - 1].into()
}

/// Hungarian zone names, clamped like the colour functions.
pub fn power_zone_name(zone: u8) -> &'static str {
    POWER_ZONE_NAMES[usize::from(zone.clamp(1, 7)) - 1]
}

pub fn hr_zone_name(zone: u8) -> &'static str {
    HR_ZONE_NAMES[usize::from(zone.clamp(1, 5)) - 1]
}

/// CSS custom property reference for a zone, for inline styles that should follow the theme
/// automatically: `power_zone_var(3)` → `"var(--z3)"`.
pub fn power_zone_var(zone: u8) -> &'static str {
    [
        "var(--z1)",
        "var(--z2)",
        "var(--z3)",
        "var(--z4)",
        "var(--z5)",
        "var(--z6)",
        "var(--z7)",
    ][usize::from(zone.clamp(1, 7)) - 1]
}

pub fn hr_zone_var(zone: u8) -> &'static str {
    [
        "var(--h1)",
        "var(--h2)",
        "var(--h3)",
        "var(--h4)",
        "var(--h5)",
    ][usize::from(zone.clamp(1, 5)) - 1]
}

// ───────────────────────────── climb categories ─────────────────────────────

pub fn climb_category_label(cat: Option<ClimbCategory>) -> &'static str {
    match cat {
        Some(ClimbCategory::Hc) => "HC",
        Some(ClimbCategory::Cat1) => "1. kat.",
        Some(ClimbCategory::Cat2) => "2. kat.",
        Some(ClimbCategory::Cat3) => "3. kat.",
        Some(ClimbCategory::Cat4) => "4. kat.",
        None => "Emelkedő",
    }
}

/// Position of a category in the easy → hard ramps (`None` for uncategorised).
fn climb_index(cat: ClimbCategory) -> usize {
    match cat {
        ClimbCategory::Cat4 => 0,
        ClimbCategory::Cat3 => 1,
        ClimbCategory::Cat2 => 2,
        ClimbCategory::Cat1 => 3,
        ClimbCategory::Hc => 4,
    }
}

pub fn climb_category_color(cat: Option<ClimbCategory>, theme: Theme) -> String {
    match cat {
        Some(c) => {
            pick(theme, &CLIMB_CATEGORIES_DARK, &CLIMB_CATEGORIES_LIGHT)[climb_index(c)].into()
        }
        None => (*pick(theme, &CLIMB_NONE_DARK, &CLIMB_NONE_LIGHT)).into(),
    }
}

/// CSS custom property for a climb category (`var(--cat-hc)`, `var(--cat-1)`…).
pub fn climb_category_var(cat: Option<ClimbCategory>) -> &'static str {
    match cat {
        Some(ClimbCategory::Hc) => "var(--cat-hc)",
        Some(ClimbCategory::Cat1) => "var(--cat-1)",
        Some(ClimbCategory::Cat2) => "var(--cat-2)",
        Some(ClimbCategory::Cat3) => "var(--cat-3)",
        Some(ClimbCategory::Cat4) => "var(--cat-4)",
        None => "var(--cat-none)",
    }
}

// ───────────────────────────── chrome ─────────────────────────────

/// Accent colour for canvas/SVG drawing (rider marker, progress) that cannot use CSS variables.
pub fn accent_color(theme: Theme) -> &'static str {
    pick(theme, ACCENT_DARK, ACCENT_LIGHT)
}

/// Colour of the ghost marker (an earlier ride replayed on the same route).
pub fn ghost_color(theme: Theme) -> &'static str {
    pick(theme, GHOST_DARK, GHOST_LIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const CSS: &str = include_str!("../assets/kit.css");

    /// `--name: value;` declarations of the `:root {` (dark) or `:root[data-theme="light"] {` block.
    fn css_vars(theme: Theme) -> HashMap<String, String> {
        let head = match theme {
            Theme::Dark => ":root {",
            Theme::Light => ":root[data-theme=\"light\"] {",
        };
        let start = CSS.find(head).expect("theme block present") + head.len();
        let body = &CSS[start..];
        let end = body.find("\n}").expect("block closes");
        let mut out = HashMap::new();
        for decl in body[..end].split(';') {
            let decl = decl.trim();
            // strip a leading comment line, if any
            let decl = decl.rsplit("*/").next().unwrap_or(decl).trim();
            if let Some((k, v)) = decl.split_once(':') {
                let k = k.trim();
                if k.starts_with("--") {
                    out.insert(k.to_string(), v.trim().to_string());
                }
            }
        }
        out
    }

    fn themes() -> [Theme; 2] {
        [Theme::Dark, Theme::Light]
    }

    #[test]
    fn css_tokens_mirror_the_rust_palettes() {
        for theme in themes() {
            let vars = css_vars(theme);
            let get = |k: &str| {
                vars.get(k)
                    .unwrap_or_else(|| panic!("missing {k} ({theme:?})"))
                    .to_lowercase()
            };
            for z in 1..=7u8 {
                assert_eq!(
                    get(&format!("--z{z}")),
                    power_zone_color(z, theme),
                    "z{z} {theme:?}"
                );
            }
            for z in 1..=5u8 {
                assert_eq!(
                    get(&format!("--h{z}")),
                    hr_zone_color(z, theme),
                    "h{z} {theme:?}"
                );
            }
            let a = pick(theme, &GRADE_ANCHORS_DARK, &GRADE_ANCHORS_LIGHT);
            assert_eq!(get("--grade-down"), a.down);
            assert_eq!(get("--grade-flat"), a.flat);
            for (i, hex) in a.up.iter().enumerate() {
                assert_eq!(get(&format!("--grade-up-{}", i + 1)), *hex);
            }
            use ClimbCategory::*;
            for (name, cat) in [
                ("hc", Some(Hc)),
                ("1", Some(Cat1)),
                ("2", Some(Cat2)),
                ("3", Some(Cat3)),
                ("4", Some(Cat4)),
                ("none", None),
            ] {
                assert_eq!(
                    get(&format!("--cat-{name}")),
                    climb_category_color(cat, theme),
                    "cat-{name} {theme:?}"
                );
            }
            assert_eq!(get("--accent"), accent_color(theme));
            assert_eq!(get("--ghost"), ghost_color(theme));
            let s = pick(theme, &STATUS_DARK, &STATUS_LIGHT);
            assert_eq!(
                [get("--ok"), get("--warn"), get("--crit")],
                [s[0], s[1], s[2]]
            );
        }
    }

    #[test]
    fn every_contract_token_is_defined_in_both_themes() {
        let names = "bg bg-1 bg-2 bg-3 glass glass-strong border border-strong text text-dim text-faint accent accent-ink accent-soft ok warn crit \
            z1 z2 z3 z4 z5 z6 z7 h1 h2 h3 h4 h5 grade-down grade-flat grade-up-1 grade-up-2 grade-up-3 grade-up-4 \
            radius-sm radius-md radius-lg radius-xl shadow-1 shadow-2 space-1 space-2 space-3 space-4 space-5 space-6 space-7 \
            ease dur-fast dur dur-slow font-ui font-display font-num \
            rule rule-strong rule-ink accent-strong chart-grid chart-axis chart-ink route-casing marker-ring shadow-ink ghost \
            fs-2xs fs-xs fs-sm fs-md fs-base fs-lg fs-xl fs-2xl fs-3xl fs-4xl fs-hero t-11 t-13 t-15 t-20 t-28 t-56 t-96 \
            fill-1 fill-2 fill-3 hover scrim accent-line accent-glow ok-soft warn-soft crit-soft cat-none cat-4 cat-3 cat-2 cat-1 cat-hc \
            h1 h2 h3 h4 h5 radius-xs radius-pill";
        for theme in themes() {
            let vars = css_vars(theme);
            for n in names.split_whitespace() {
                // radius/space/motion/font tokens are theme independent and live in the dark `:root` block only
                let theme_free = n.starts_with("radius")
                    || n.starts_with("space")
                    || n.starts_with("dur")
                    || n.starts_with("font")
                    || n.starts_with("fs-")
                    || n.starts_with("t-")
                    || n == "shadow-1"
                    || n == "shadow-2"
                    || n == "ease";
                if theme_free && theme == Theme::Light {
                    continue;
                }
                assert!(
                    vars.contains_key(&format!("--{n}")),
                    "--{n} missing for {theme:?}"
                );
            }
        }
    }

    #[test]
    fn grade_scale_is_neutral_at_zero_and_clamped() {
        for theme in themes() {
            let a = pick(theme, &GRADE_ANCHORS_DARK, &GRADE_ANCHORS_LIGHT);
            assert_eq!(grade_color(0.0, theme), a.flat);
            assert_eq!(grade_color(0.4, theme), a.flat);
            assert_eq!(grade_color(-0.4, theme), a.flat);
            assert_eq!(grade_color(-10.0, theme), a.down);
            assert_eq!(grade_color(-40.0, theme), a.down);
            assert_eq!(grade_color(13.0, theme), a.up[3]);
            assert_eq!(grade_color(99.0, theme), a.up[3]);
            assert_eq!(grade_color(f64::NAN, theme), a.flat);
            assert_eq!(grade_color(3.0, theme), a.up[0]);
            assert_eq!(grade_color(6.0, theme), a.up[1]);
            assert_eq!(grade_color(9.5, theme), a.up[2]);
        }
    }

    #[test]
    fn grade_scale_is_continuous_and_monotone_on_the_warm_arm() {
        for theme in themes() {
            let mut prev = grade_color_rgb(-12.0, theme);
            let mut prev_l = color::oklch_lightness(grade_color_rgb(GRADE_STOP_PCT[3], theme));
            let mut g = -12.0;
            while g <= 16.0 {
                let c = grade_color_rgb(g, theme);
                // a 0.1 % step must never jump visibly
                assert!(
                    color::delta_e(prev, c) < 2.5,
                    "jump at {g} % ({theme:?}): {}",
                    color::delta_e(prev, c)
                );
                prev = c;
                if g >= GRADE_STOP_PCT[3] {
                    let l = color::oklch_lightness(c);
                    assert!(
                        l <= prev_l + 1e-6,
                        "warm arm must get darker: {g} % {l} > {prev_l} ({theme:?})"
                    );
                    prev_l = l;
                }
                g += 0.1;
            }
        }
    }

    #[test]
    fn grade_buckets_cover_the_range_and_match_the_scale() {
        assert_eq!(grade_bucket(f64::NAN), grade_bucket(0.0));
        assert_eq!(grade_bucket(-50.0), 0);
        assert_eq!(grade_bucket(50.0), GRADE_BUCKETS - 1);
        assert_eq!(grade_bucket(0.0), 5);
        let mut prev = 0;
        let mut g = -20.0;
        while g < 25.0 {
            let b = grade_bucket(g);
            assert!(b >= prev && b < GRADE_BUCKETS, "monotone buckets at {g}");
            prev = b;
            g += 0.05;
        }
        for theme in themes() {
            for b in 0..GRADE_BUCKETS {
                assert_eq!(
                    grade_bucket(grade_bucket_center(b)),
                    b,
                    "center of bucket {b} lies inside it"
                );
                assert_eq!(
                    grade_bucket_color(b, theme),
                    grade_color(grade_bucket_center(b), theme)
                );
            }
        }
    }

    #[test]
    fn zone_lookups_clamp_instead_of_panicking() {
        for theme in themes() {
            assert_eq!(power_zone_color(0, theme), power_zone_color(1, theme));
            assert_eq!(power_zone_color(200, theme), power_zone_color(7, theme));
            assert_eq!(hr_zone_color(0, theme), hr_zone_color(1, theme));
            assert_eq!(hr_zone_color(9, theme), hr_zone_color(5, theme));
        }
        assert_eq!(power_zone_name(0), "Aktív regeneráció");
        assert_eq!(power_zone_name(9), "Neuromuszkuláris");
        assert_eq!(hr_zone_name(9), "Maximális");
        assert_eq!(power_zone_var(3), "var(--z3)");
        assert_eq!(hr_zone_var(99), "var(--h5)");
        assert_eq!(climb_category_var(Some(ClimbCategory::Hc)), "var(--cat-hc)");
        assert_eq!(climb_category_var(None), "var(--cat-none)");
        assert_eq!(climb_category_label(None), "Emelkedő");
    }

    fn chart_surface(theme: Theme) -> Rgb {
        match theme {
            Theme::Dark => hex_of("#1a1916"),
            Theme::Light => hex_of("#f8f5ec"),
        }
    }

    #[test]
    fn zone_ramps_pass_the_validator_hard_gates() {
        // dataviz gates: adjacent normal-vision ΔE ≥ 15, lightness band, chroma floor, contrast ≥ 3:1
        for theme in themes() {
            let pal: Vec<Rgb> = (1..=7)
                .map(|z| hex_of(&power_zone_color(z, theme)))
                .collect();
            for w in pal.windows(2) {
                assert!(
                    color::delta_e(w[0], w[1]) >= 15.0,
                    "{theme:?} adjacent ΔE {}",
                    color::delta_e(w[0], w[1])
                );
            }
            let (lo, hi) = if theme == Theme::Dark {
                (0.48, 0.67)
            } else {
                (0.43, 0.77)
            };
            for c in &pal {
                let l = color::oklch_lightness(*c);
                assert!(l >= lo && l <= hi, "{theme:?} L {l}");
                assert!(
                    color::contrast_ratio(*c, chart_surface(theme)) >= 3.0,
                    "{theme:?} contrast"
                );
            }
        }
    }

    #[test]
    fn ordinal_ramps_are_monotone_with_visible_steps() {
        let ramps: [(&str, &[&str]); 4] = [
            ("hr dark", &HR_ZONES_DARK),
            ("hr light", &HR_ZONES_LIGHT),
            ("climb dark", &CLIMB_CATEGORIES_DARK),
            ("climb light", &CLIMB_CATEGORIES_LIGHT),
        ];
        for (name, ramp) in ramps {
            let ls: Vec<f64> = ramp
                .iter()
                .map(|h| color::oklch_lightness(hex_of(h)))
                .collect();
            let rising = ls.windows(2).all(|w| w[1] > w[0]);
            let falling = ls.windows(2).all(|w| w[1] < w[0]);
            assert!(rising || falling, "{name} not monotone: {ls:?}");
            assert!(
                ls.windows(2).all(|w| (w[1] - w[0]).abs() >= 0.06),
                "{name} steps too close: {ls:?}"
            );
        }
    }

    #[test]
    fn text_and_status_tokens_are_readable() {
        for theme in themes() {
            let vars = css_vars(theme);
            let c = |k: &str| hex_of(vars.get(k).unwrap_or_else(|| panic!("missing {k}")));
            for surface in ["--bg-1", "--bg-2"] {
                for ink in [
                    "--text",
                    "--text-dim",
                    "--text-faint",
                    "--ok",
                    "--warn",
                    "--crit",
                    "--accent",
                ] {
                    let r = color::contrast_ratio(c(ink), c(surface));
                    assert!(r >= 4.5, "{ink} on {surface} ({theme:?}) only {r:.2}:1");
                }
            }
            assert!(
                color::contrast_ratio(c("--accent-ink"), c("--accent")) >= 4.5,
                "accent-ink on accent ({theme:?})"
            );
        }
    }

    #[test]
    fn boundaries_and_light_ends_are_visible() {
        for theme in themes() {
            let vars = css_vars(theme);
            let c = |k: &str| hex_of(vars.get(k).unwrap_or_else(|| panic!("missing {k}")));
            // a control boundary (input border, switch track) must reach 3:1 on the panel surface
            let r = color::contrast_ratio(c("--rule-strong"), c("--bg-1"));
            assert!(
                r >= 3.0,
                "--rule-strong on --bg-1 ({theme:?}) only {r:.2}:1"
            );
            // the hairline itself stays quiet: visible, but far from the ink
            let h = color::contrast_ratio(c("--rule"), c("--bg-1"));
            assert!(
                (1.25..=2.2).contains(&h),
                "--rule on --bg-1 ({theme:?}) {h:.2}:1"
            );
            // the light end of every ordinal ramp still clears the surface (validator floor 2:1)
            let surface = chart_surface(theme);
            let (hr, climb, a) = (
                pick(theme, &HR_ZONES_DARK, &HR_ZONES_LIGHT),
                pick(theme, &CLIMB_CATEGORIES_DARK, &CLIMB_CATEGORIES_LIGHT),
                pick(theme, &GRADE_ANCHORS_DARK, &GRADE_ANCHORS_LIGHT),
            );
            let ends = [
                ("hr", hr[0]),
                ("hr", hr[4]),
                ("climb", climb[0]),
                ("climb", climb[4]),
                ("grade", a.up[0]),
                ("grade", a.flat),
                ("grade", a.down),
            ];
            for (name, hex) in ends {
                let r = color::contrast_ratio(hex_of(hex), surface);
                assert!(
                    r >= 2.0,
                    "{name} {hex} on surface ({theme:?}) only {r:.2}:1"
                );
            }
        }
    }

    #[test]
    fn the_accent_stays_apart_from_the_status_colours() {
        // vermilion (accent) and rose-red (crit) are both warm: keep them visibly apart
        for theme in themes() {
            let vars = css_vars(theme);
            let c = |k: &str| hex_of(vars.get(k).unwrap_or_else(|| panic!("missing {k}")));
            let d = color::delta_e(c("--accent"), c("--crit"));
            assert!(d >= 10.0, "accent vs crit ΔE {d:.1} ({theme:?})");
            let d = color::delta_e(c("--accent"), c("--warn"));
            assert!(d >= 10.0, "accent vs warn ΔE {d:.1} ({theme:?})");
        }
    }

    #[test]
    fn rider_marker_ring_and_route_casing_separate_from_the_route() {
        // the marker ring and the casing are what keep the vermilion marker and the route readable
        // on the map, whatever grade colour is underneath
        for theme in themes() {
            let vars = css_vars(theme);
            let c = |k: &str| hex_of(vars.get(k).unwrap_or_else(|| panic!("missing {k}")));
            let ring = c("--marker-ring");
            let casing = c("--route-casing");
            let a = pick(theme, &GRADE_ANCHORS_DARK, &GRADE_ANCHORS_LIGHT);
            for g in a.up.iter().chain([&a.flat, &a.down]) {
                assert!(
                    color::contrast_ratio(hex_of(g), casing) >= 1.8,
                    "route colour {g} vs casing ({theme:?})"
                );
            }
            assert!(
                color::contrast_ratio(c("--accent"), ring) >= 2.0,
                "accent vs marker ring ({theme:?})"
            );
        }
    }

    #[test]
    fn the_ghost_stays_apart_from_the_route_and_the_rider() {
        // the ghost marker is drawn *on* the route: it must never be mistaken for a grade colour,
        // for the casing under it, or for the rider's vermilion marker
        for theme in themes() {
            let vars = css_vars(theme);
            let c = |k: &str| hex_of(vars.get(k).unwrap_or_else(|| panic!("missing {k}")));
            let ghost = c("--ghost");
            let a = pick(theme, &GRADE_ANCHORS_DARK, &GRADE_ANCHORS_LIGHT);
            for g in a.up.iter().chain([&a.down, &a.flat]) {
                let d = color::delta_e(ghost, hex_of(g));
                assert!(d >= 12.0, "ghost vs route colour {g} ΔE {d:.1} ({theme:?})");
            }
            let d = color::delta_e(ghost, c("--accent"));
            assert!(d >= 18.0, "ghost vs accent ΔE {d:.1} ({theme:?})");
            let r = color::contrast_ratio(ghost, c("--route-casing"));
            assert!(r >= 3.0, "ghost vs casing ({theme:?}) only {r:.2}:1");
        }
    }
}
