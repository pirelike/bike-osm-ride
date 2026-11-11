//! The modal dialogs of bike-osm-ride: the route chooser (file, bikerouter link,
//! recents, demo), sensor management, settings, workouts, the end-of-ride
//! summary and the licence/attribution dialog, plus the simulated-rider panel.
//!
//! Everything here is presentation only: the components receive `ReadSignal` props, emit the
//! user's intent through `EventHandler`s and never own business state. All user-facing text is
//! Hungarian; numbers go through [`ride_ui_kit::format`]; colours come from the CSS tokens or
//! [`ride_ui_kit::palette`] (the stylesheet is [`CSS`], class names are prefixed `dlg-`).
//!
//! The public signatures below are the contract with `ride-app`: they may grow, never change.
//!
//! * [`RouteDialog`] — drop zone, "GPX megnyitása", bikerouter link with validation,
//!   recents with metadata + delete, demo route, busy state and an inline error line.
//! * [`DevicesDialog`] — slot cards (trainer, heart rate, power meter, cadence, shifter) with state
//!   badges, scan button, discovered devices with RSSI + suggested kind and connect/disconnect.
//! * [`VirtualRiderPanel`] — floating power control for the simulated rider.
//! * [`SettingsDialog`] — grouped, immediately applied settings.
//! * [`WorkoutsDialog`] — the embedded workout library, grouped by category, with the
//!   selected workout's steps.
//! * [`SummaryDialog`] — hero statistics and the power/HR zone distribution.
//! * [`AboutDialog`] — version, project licence, third-party code and data attributions.

use ride_model::{ConnState, DeviceKind};

mod about;
mod devices;
pub mod helpers;
mod settings;
mod summary;
mod virtual_rider;
mod welcome;
mod workouts;

pub use about::{
    bundled_fonts, data_sources, third_party_code, third_party_licenses_text, AboutDialog,
    APP_NAME, APP_VERSION, PROJECT_LICENSE,
};
pub use devices::DevicesDialog;
pub use helpers::*;
pub use settings::SettingsDialog;
pub use summary::SummaryDialog;
pub use virtual_rider::VirtualRiderPanel;
pub use welcome::RouteDialog;
pub use workouts::WorkoutsDialog;

/// Component stylesheet. The shell injects [`ride_ui_kit::CSS`] first, then this block.
/// Tokens only (no literal colours), every class name is prefixed `dlg-`.
pub const CSS: &str = include_str!("../assets/dialogs.css");

/// Connection state of one sensor slot as shown in the devices dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSlot {
    pub kind: DeviceKind,
    pub state: ConnState,
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local custom properties this crate sets inline (`--c`: data colour of a zone bar,
    /// `--p`: bar fill in percent) and one it defines itself (`--cols`).
    const LOCAL: &[&str] = &["c", "p", "cols"];

    /// Names kept in the kit only so older CSS keeps working: the new look must not use them.
    const LEGACY: &[&str] = &[
        "glass",
        "glass-strong",
        "highlight",
        "accent-glow",
        "shadow-1",
        "radius-xl",
        "radius-lg",
        "radius-pill",
    ];

    #[test]
    fn stylesheet_is_embedded_and_covers_every_screen() {
        assert!(!CSS.trim().is_empty());
        for class in [
            ".dlg-welcome",
            ".dlg-hero",
            ".dlg-source",
            ".dlg-drop",
            ".dlg-recent",
            ".dlg-slot",
            ".dlg-found",
            ".dlg-vr",
            ".dlg-settings",
            ".dlg-group",
            ".dlg-workouts",
            ".dlg-summary",
            ".dlg-zone",
            ".dlg-about",
            ".dlg-key",
            ".dlg-howto",
            ".dlg-foot",
        ] {
            assert!(CSS.contains(class), "missing {class} in dialogs.css");
        }
    }

    #[test]
    fn stylesheet_uses_no_literal_colours() {
        for forbidden in ["#", "rgb(", "rgba(", "hsl(", "hwb(", "lab(", "lch("] {
            assert!(
                !CSS.contains(forbidden),
                "literal colour {forbidden:?} in dialogs.css"
            );
        }
    }

    #[test]
    fn every_referenced_token_exists_in_the_kit() {
        let mut rest = CSS;
        let mut seen = 0;
        while let Some(at) = rest.find("var(--") {
            let after = &rest[at + 6..];
            let end = after.find([')', ',']).unwrap_or(after.len());
            let name = &after[..end];
            assert!(
                LOCAL.contains(&name) || ride_ui_kit::CSS.contains(&format!("--{name}:")),
                "--{name} is not defined by ride-ui-kit"
            );
            assert!(!LEGACY.contains(&name), "--{name} is a legacy token");
            seen += 1;
            rest = &after[end..];
        }
        assert!(seen > 80, "found only {seen} token uses");
    }

    #[test]
    fn stylesheet_follows_the_head_unit_design_rules() {
        for banned in [
            "backdrop-filter",
            "blur(",
            "text-shadow",
            "drop-shadow",
            "text-transform",
            "letter-spacing",
            "gradient",
            "dashed",
        ] {
            assert!(!CSS.contains(banned), "{banned} is not part of the look");
        }
        // every box-shadow in this sheet is a hairline focus ring (0 blur) or a hard offset
        for (at, _) in CSS.match_indices("box-shadow:") {
            let value = CSS[at + 11..].split(';').next().unwrap_or("");
            let numbers: Vec<&str> = value.split_whitespace().take(4).collect();
            assert!(
                numbers
                    .get(2)
                    .map(|blur| blur.starts_with('0'))
                    .unwrap_or(false),
                "box-shadow with a blur radius: {value}"
            );
        }
        // sizes come from the type scale, never literal px in a `font` shorthand
        for line in CSS.lines().filter(|l| l.trim_start().starts_with("font:")) {
            assert!(
                !line.contains("px/") && !line.contains("px "),
                "literal px in {line}"
            );
        }
        // no pill outside the badge/chip components of the kit
        assert!(
            !CSS.contains("999px") && !CSS.contains("50%"),
            "no pills / discs"
        );
    }

    #[test]
    fn every_class_name_is_prefixed() {
        // Every `.` followed by a letter introduces a class selector; all of them must be
        // namespaced so the dialogs can never style another crate's DOM.
        let bytes = CSS.as_bytes();
        for (i, window) in bytes.windows(2).enumerate() {
            if window[0] == b'.' && window[1].is_ascii_alphabetic() {
                let rest = &CSS[i + 1..];
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                    .unwrap_or(rest.len());
                let name = &rest[..end];
                assert!(
                    name.starts_with("dlg-"),
                    "class .{name} is not dlg- prefixed"
                );
            }
        }
    }

    #[test]
    fn device_slot_is_the_public_contract_shape() {
        let slot = DeviceSlot {
            kind: DeviceKind::Trainer,
            state: ConnState::Connecting,
            name: Some("KICKR".into()),
        };
        assert_eq!(slot.kind, DeviceKind::Trainer);
        assert_eq!(slot.state, ConnState::Connecting);
        assert_eq!(slot.name.as_deref(), Some("KICKR"));
        // Clone + PartialEq are part of the shell's expectations.
        assert_eq!(slot.clone(), slot);
    }

    /// Every dialog closes with the title bar's × (or Esc). A second, redundant close button at
    /// the bottom was removed on request — the footer carries notes and real actions only — so
    /// nothing here calls `on_close` from a click handler any more.
    #[test]
    fn no_dialog_has_a_second_close_button_in_the_footer() {
        for (name, source) in [
            ("welcome", include_str!("welcome.rs")),
            ("devices", include_str!("devices.rs")),
            ("settings", include_str!("settings.rs")),
            ("about", include_str!("about.rs")),
            ("workouts", include_str!("workouts.rs")),
            ("summary", include_str!("summary.rs")),
        ] {
            assert!(
                !source.contains("on_close.call(())"),
                "{name}: the footer must not close the dialog, the × does that"
            );
        }
    }
}
