//! Test hooks read from environment variables (used by screenshot-based verification
//! and the scripts in `scripts/`). None of them is needed for normal use.
//!
//! | variable | effect |
//! |---|---|
//! | `RIDE_DEMO=1` | load the built-in demo route at start-up |
//! | `RIDE_SIM=<watts>` | connect the simulated rider at that power and start the ride (loads the demo route unless `RIDE_GPX` is set) |
//! | `RIDE_WINDOW=1600x900` | initial window size |
//! | `RIDE_THEME=light\|dark` | theme override (not written to the settings file unless the user changes it) |
//! | `RIDE_GPX=<path>` | load a GPX file at start-up |
//! | `RIDE_BLE_LOG=1` | write the raw BLE hex log to `%LOCALAPPDATA%/bike-osm-ride/ble.log` |
//! | `RIDE_DIALOG=route|devices|settings|workouts|about` | open a dialog at start-up |
//! | `RIDE_REVIEW=1` | open the review page at start-up |
//! | `RIDE_WORKOUT=<id>` | select this workout at start-up (e.g. `e-ramp-20w`; ignored when unknown) |
//! | `RIDE_TIMESCALE=<n>` | run the ride clock `n` times faster (1..=200), to reach the summary quickly |
//! | `RIDE_DATA_DIR=<dir>` | use another data directory (tile cache, settings, recents) |
//! | `RIDE_TILE_BASE=<url>` | fetch tiles from `<url>/<style>/<z>/<x>/<y>.png` instead of the real servers (e.g. `http://127.0.0.1:9` simulates "no network") |
//! | `RIDE_TILE_PROBE=1` | load one map tile through the `tiles` protocol at start-up and print the result to stderr |
//! | `RIDE_STREETVIEW_BASE=<url>` | send the Street View metadata requests **and** the embed iframe to a local mock (`http://127.0.0.1:<port>`, loopback only; anything else is ignored and Google is used) instead of Google, e.g. `node scripts/streetview_mock.mjs 9300`. The API key still comes from the settings file; with the mock any key works except the special `BADKEY` / `QUOTAKEY`. The variable is read by `ride-ui-streetview` itself (`url::Endpoints::from_env`); this crate only validates and reports it at start-up. |

use std::path::PathBuf;

use ride_model::Theme;

use crate::state::DialogKind;

pub const MIN_WINDOW: (u32, u32) = (360, 560);
pub const DEFAULT_WINDOW: (u32, u32) = (1440, 900);

#[derive(Debug, Clone, PartialEq)]
pub struct Hooks {
    pub demo: bool,
    pub sim_watts: Option<f64>,
    pub window: Option<(u32, u32)>,
    pub theme: Option<Theme>,
    pub gpx: Option<PathBuf>,
    pub ble_log: bool,
    pub dialog: Option<DialogKind>,
    /// Open the review page at start-up (`RIDE_REVIEW=1`).
    pub review: bool,
    pub time_scale: f64,
    pub tile_probe: bool,
    pub tile_base: Option<String>,
    /// Validated `RIDE_STREETVIEW_BASE` (loopback only); `None` = real Google endpoints.
    pub streetview_base: Option<String>,
    /// `RIDE_WORKOUT`: workout id selected at start-up.
    pub workout: Option<String>,
}

impl Default for Hooks {
    fn default() -> Self {
        Self {
            demo: false,
            sim_watts: None,
            window: None,
            theme: None,
            gpx: None,
            ble_log: false,
            dialog: None,
            review: false,
            time_scale: 1.0,
            tile_probe: false,
            tile_base: None,
            streetview_base: None,
            workout: None,
        }
    }
}

impl Hooks {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| {
            lookup(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let flag = |key: &str| get(key).map(|v| is_truthy(&v)).unwrap_or(false);

        let sim_watts = get("RIDE_SIM").and_then(|v| match v.parse::<f64>() {
            Ok(w) if w.is_finite() && (0.0..=2000.0).contains(&w) => Some(w),
            _ => {
                eprintln!("[ride-app] RIDE_SIM={v}: érvénytelen teljesítmény (0..2000 W), figyelmen kívül hagyva");
                None
            }
        });
        let time_scale = get("RIDE_TIMESCALE")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|s| s.is_finite())
            .map(|s| s.clamp(1.0, 200.0))
            .unwrap_or(1.0);

        Self {
            demo: flag("RIDE_DEMO"),
            sim_watts,
            window: get("RIDE_WINDOW").and_then(|v| parse_window(&v)),
            theme: get("RIDE_THEME").and_then(|v| parse_theme(&v)),
            gpx: get("RIDE_GPX").map(PathBuf::from),
            ble_log: flag("RIDE_BLE_LOG"),
            dialog: get("RIDE_DIALOG").and_then(|v| parse_dialog(&v)),
            review: flag("RIDE_REVIEW"),
            time_scale,
            tile_probe: flag("RIDE_TILE_PROBE"),
            tile_base: get("RIDE_TILE_BASE").map(|b| b.trim_end_matches('/').to_string()),
            streetview_base: get(ride_ui_streetview::url::BASE_ENV)
                .and_then(|b| parse_streetview_base(&b)),
            workout: get("RIDE_WORKOUT"),
        }
    }

    /// Window size to open with: the hook, else the default, never below the minimum.
    pub fn window_size(&self) -> (u32, u32) {
        let (w, h) = self.window.unwrap_or(DEFAULT_WINDOW);
        (w.max(MIN_WINDOW.0), h.max(MIN_WINDOW.1))
    }
}

/// `RIDE_STREETVIEW_BASE`: only a loopback http(s) base is honoured (the key must never reach
/// another host); the value is reported without any key material.
pub fn parse_streetview_base(v: &str) -> Option<String> {
    match ride_ui_streetview::url::Endpoints::local(v) {
        Some(_) => Some(v.trim().trim_end_matches('/').to_string()),
        None => {
            eprintln!(
                "[ride-app] RIDE_STREETVIEW_BASE: csak helyi (127.0.0.1 / localhost) cím fogadható el, figyelmen kívül hagyva"
            );
            None
        }
    }
}

fn is_truthy(v: &str) -> bool {
    matches!(
        v.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "igen"
    )
}

/// `1600x900` (also `1600X900` and `1600×900`).
pub fn parse_window(v: &str) -> Option<(u32, u32)> {
    let mut parts = v.split(['x', 'X', '×']);
    let (w, h) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    ((320..=7680).contains(&w) && (240..=4320).contains(&h)).then_some((w, h))
}

fn parse_theme(v: &str) -> Option<Theme> {
    match v.to_ascii_lowercase().as_str() {
        "dark" | "sotet" | "sötét" => Some(Theme::Dark),
        "light" | "vilagos" | "világos" => Some(Theme::Light),
        _ => None,
    }
}

fn parse_dialog(v: &str) -> Option<DialogKind> {
    match v.to_ascii_lowercase().as_str() {
        "route" | "utvonal" | "útvonal" => Some(DialogKind::Route),
        "devices" | "eszkozok" => Some(DialogKind::Devices),
        "settings" | "beallitasok" => Some(DialogKind::Settings),
        "workouts" | "edzes" | "edzések" => Some(DialogKind::Workouts),
        "about" | "nevjegy" => Some(DialogKind::About),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks(pairs: &[(&str, &str)]) -> Hooks {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Hooks::from_lookup(move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    }

    #[test]
    fn nothing_set_is_all_defaults() {
        assert_eq!(hooks(&[]), Hooks::default());
    }

    #[test]
    fn parses_every_hook() {
        let h = hooks(&[
            ("RIDE_DEMO", "1"),
            ("RIDE_SIM", "250"),
            ("RIDE_WINDOW", "1600x900"),
            ("RIDE_THEME", "light"),
            ("RIDE_GPX", "C:/x/út.gpx"),
            ("RIDE_BLE_LOG", "true"),
            ("RIDE_DIALOG", "devices"),
            ("RIDE_TIMESCALE", "60"),
            ("RIDE_TILE_PROBE", "yes"),
            ("RIDE_TILE_BASE", "http://127.0.0.1:9/"),
            ("RIDE_STREETVIEW_BASE", "http://127.0.0.1:9300/"),
            ("RIDE_WORKOUT", "e-ramp-20w"),
        ]);
        assert_eq!(h.tile_base.as_deref(), Some("http://127.0.0.1:9"));
        assert_eq!(h.streetview_base.as_deref(), Some("http://127.0.0.1:9300"));
        assert!(h.demo && h.ble_log && h.tile_probe);
        assert_eq!(h.sim_watts, Some(250.0));
        assert_eq!(h.window, Some((1600, 900)));
        assert_eq!(h.theme, Some(Theme::Light));
        assert_eq!(h.gpx, Some(PathBuf::from("C:/x/út.gpx")));
        assert_eq!(h.dialog, Some(DialogKind::Devices));
        assert_eq!(h.time_scale, 60.0);
        assert_eq!(h.workout.as_deref(), Some("e-ramp-20w"));
        assert_eq!(
            hooks(&[("RIDE_DIALOG", "workouts")]).dialog,
            Some(DialogKind::Workouts)
        );
    }

    #[test]
    fn rejects_garbage() {
        let h = hooks(&[
            ("RIDE_STREETVIEW_BASE", "https://evil.example.com"),
            ("RIDE_DEMO", "0"),
            ("RIDE_SIM", "sok"),
            ("RIDE_WINDOW", "wide"),
            ("RIDE_THEME", "blue"),
            ("RIDE_DIALOG", "x"),
            ("RIDE_TIMESCALE", "NaN"),
        ]);
        assert_eq!(h, Hooks::default());
        assert_eq!(hooks(&[("RIDE_SIM", "-5")]).sim_watts, None);
        assert_eq!(hooks(&[("RIDE_SIM", "99999")]).sim_watts, None);
        assert_eq!(hooks(&[("RIDE_SIM", "0")]).sim_watts, Some(0.0));
    }

    #[test]
    fn window_syntax() {
        assert_eq!(parse_window("1280x720"), Some((1280, 720)));
        assert_eq!(parse_window(" 1280 X 720 "), Some((1280, 720)));
        assert_eq!(parse_window("1280×720"), Some((1280, 720)));
        assert_eq!(parse_window("1280"), None);
        assert_eq!(parse_window("1280x720x3"), None);
        assert_eq!(parse_window("0x0"), None);
        assert_eq!(parse_window("99999x99999"), None);
    }

    #[test]
    fn window_is_never_smaller_than_the_minimum() {
        assert_eq!(
            hooks(&[("RIDE_WINDOW", "330x400")]).window_size(),
            MIN_WINDOW
        );
        assert_eq!(hooks(&[]).window_size(), DEFAULT_WINDOW);
    }

    #[test]
    fn time_scale_is_clamped() {
        assert_eq!(hooks(&[("RIDE_TIMESCALE", "100000")]).time_scale, 200.0);
        assert_eq!(hooks(&[("RIDE_TIMESCALE", "0.1")]).time_scale, 1.0);
    }
}
