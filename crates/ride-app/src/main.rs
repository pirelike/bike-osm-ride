//! bike-osm-ride: virtual rides on GPX routes with a BLE smart trainer (Dioxus 0.7 desktop).
//!
//! This crate is the shell: window configuration, the tile proxy, application state, the
//! controller that wires the ride engine to the sensor hub, and the page layout. All
//! user-visible text is Hungarian. See `hooks.rs` for the `RIDE_*` test environment variables.
//!
//! The same shell runs on Windows (WebView2), macOS (WKWebView), Linux (WebKitGTK) and Android;
//! what differs per platform is listed in `docs/ARCHITECTURE.md`.

// Release builds are GUI-subsystem executables (no console window); debug builds keep the console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "android")]
mod android_picker;
mod browser;
mod controller;
mod crash;
mod dock;
mod gearbox;
mod guard;
mod history;
mod hooks;
mod icon;
mod paths;
mod ride_core;
mod shell;
mod state;
mod streetview_images;
mod tiles;
mod updater;
mod wake;

use dioxus::desktop::tao::event::Event;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::LaunchBuilder;

/// WebKitGTK's DMA-BUF renderer leaves the window empty on some NVIDIA and Wayland setups; the
/// documented workaround is to switch it off. An explicit user setting always wins, so this only
/// fills in the default before the webview exists.
#[cfg(target_os = "linux")]
fn init_linux_webkit() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    // First of all: a panic must never disappear. On Android it would otherwise take the process
    // down without a single line anywhere (see `crash.rs`).
    #[cfg(target_os = "android")]
    paths::init_android_data_dir();
    crash::install();
    #[cfg(target_os = "linux")]
    init_linux_webkit();
    #[cfg(not(target_os = "android"))]
    updater::cleanup_previous();

    // Temporary files of a write a crash or a power cut interrupted.
    let _ = guard::guarded("store::remove_stale_temp_files", || {
        ride_route::store::remove_stale_temp_files(&ride_route::store::data_dir())
    });

    // Test hooks and the persisted settings are read before the window exists, so the
    // first paint already has the right size and theme.
    let boot = state::boot();
    let (width, height) = boot.hooks.window_size();
    let (min_width, min_height) = hooks::MIN_WINDOW;
    let theme = boot.settings.ui.theme;

    let window = WindowBuilder::new()
        .with_title("bike-osm-ride")
        .with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
        .with_min_inner_size(LogicalSize::new(
            f64::from(min_width),
            f64::from(min_height),
        ))
        .with_window_icon(icon::window_icon());

    let config = Config::new()
        .with_window(window)
        .with_background_color(shell::background_rgba(theme))
        .with_disable_context_menu(true)
        .with_data_directory(paths::webview_dir())
        .with_custom_head(shell::head_html(
            theme,
            boot.settings.ui.text_scale(),
            ride_i18n::current(),
        ))
        .with_custom_event_handler(|event, _| {
            if matches!(event, Event::LoopDestroyed) {
                controller::flush_pending_settings();
            }
        });
    // macOS keeps the default menu: WKWebView takes Cmd+C / Cmd+V / Cmd+Q from it, so without it
    // the rider cannot paste a Google key or quit with the keyboard. Elsewhere the menu bar would
    // only take room (and Windows would show it inside the window).
    #[cfg(not(target_os = "macos"))]
    let config = config.with_menu(None);
    let config = tiles::register(config);
    let config = streetview_images::register(config);
    // Diagnostics: RIDE_DEBUG_PORT=9222 opens the WebView2 DevTools protocol on that port
    // (DOM / console inspection and reliable screenshots from tools such as scripts/cdp_probe.mjs).
    #[cfg(windows)]
    let config = match std::env::var("RIDE_DEBUG_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p >= 1024)
    {
        Some(port) => config.with_windows_browser_args(format!("--remote-debugging-port={port}")),
        None => config,
    };

    #[cfg(not(target_os = "android"))]
    LaunchBuilder::desktop().with_cfg(config).launch(shell::App);
    #[cfg(target_os = "android")]
    LaunchBuilder::mobile().with_cfg(config).launch(shell::App);
}

/// The Android package is partly hand-maintained (`android/AndroidManifest.xml`), because the app
/// is a `NativeActivity` and its activity must survive every configuration change. These checks
/// keep the pinned copy in step with the configuration it replaces: a permission added to
/// `Dioxus.toml` but forgotten here would be missing from the APK, and a missing
/// `configChanges` entry recreates the activity — which kills the process.
#[cfg(test)]
mod packaging_tests {
    const MANIFEST: &str = include_str!("../android/AndroidManifest.xml");
    const DIOXUS_TOML: &str = include_str!("../Dioxus.toml");

    /// Everything Android needs from the activity, or the app dies instead of redrawing.
    const REQUIRED_CONFIG_CHANGES: [&str; 12] = [
        "orientation",
        "screenLayout",
        "screenSize",
        "smallestScreenSize",
        "keyboardHidden",
        "keyboard",
        "navigation",
        "density",
        "uiMode",
        "fontScale",
        "locale",
        "layoutDirection",
    ];

    /// `dx` writes the custom activity to `<app>/src/main/kotlin/dev/dioxus/main`, which is the
    /// directory this file has to live in (its package is `dev.dioxus.main`).
    #[test]
    fn the_custom_activity_is_the_one_the_build_uses() {
        const ACTIVITY: &str = include_str!("../android/MainActivity.kt");
        assert!(
            DIOXUS_TOML.contains(r#"android_main_activity = "android/MainActivity.kt""#),
            "Dioxus.toml must point at the custom activity"
        );
        let identifier = DIOXUS_TOML
            .split_once("identifier = \"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(id, _)| id)
            .expect("bundle.identifier in Dioxus.toml");
        // the generated `Logger.kt` refers to `BuildConfig`, which only resolves through this alias
        assert!(
            ACTIVITY.contains(&format!("typealias BuildConfig = {identifier}.BuildConfig")),
            "MainActivity.kt has to alias {identifier}.BuildConfig"
        );
        assert!(ACTIVITY.contains("package dev.dioxus.main"));
    }

    /// Android is built by CI only, so a module that is switched off there can break the APK without
    /// any host build noticing (0.4.0: a stray `cfg` left behind by a deleted module hid
    /// `streetview_images` from the phone). Only `android_picker` is Android-only; nothing is
    /// hidden from Android.
    #[test]
    fn no_module_of_the_shell_is_hidden_from_android() {
        const MAIN: &str = include_str!("main.rs");
        let mut attrs = String::new();
        for line in MAIN.split("fn main()").next().unwrap_or(MAIN).lines() {
            let line = line.trim();
            if line.starts_with("mod ") {
                if !line.starts_with("mod android_picker") {
                    assert!(
                        !attrs.contains("cfg"),
                        "unexpected platform gate before {line}: {attrs}"
                    );
                }
                attrs.clear();
            } else if line.starts_with("use ") || line.starts_with("fn ") {
                attrs.clear();
            } else if !line.starts_with("//") {
                attrs.push_str(line);
            }
        }
    }

    /// The screen flag may only change on the UI thread, and Rust calls from its own thread.
    #[test]
    fn the_activity_keeps_the_screen_on_from_the_ui_thread() {
        const ACTIVITY: &str = include_str!("../android/MainActivity.kt");
        let body = ACTIVITY
            .split_once("fun setKeepScreenOn(on: Boolean)")
            .map(|(_, rest)| rest.split("    fun ").next().unwrap_or(rest))
            .expect("MainActivity.kt declares setKeepScreenOn");
        assert!(
            body.contains("runOnUiThread"),
            "the flag changes on the UI thread"
        );
        assert!(body.contains("FLAG_KEEP_SCREEN_ON"));
        assert!(body.contains("addFlags") && body.contains("clearFlags"));
        assert!(body.starts_with(": Boolean"));
        assert!(body.contains("task.get(") && body.contains("task.cancel(false)"));
        assert!(include_str!("android_picker.rs").contains("\"(Z)Z\""));
    }

    #[test]
    fn the_pinned_manifest_is_the_one_the_build_uses() {
        assert!(
            DIOXUS_TOML.contains(r#"android_manifest = "android/AndroidManifest.xml""#),
            "Dioxus.toml must point at the pinned manifest"
        );
        assert!(MANIFEST.contains(r#"android:name="dev.dioxus.main.MainActivity""#));
        assert!(
            MANIFEST.contains(r#"android:name="android.app.lib_name" android:value="main""#),
            "`dx` writes libmain.so; the manifest has to load it"
        );
        assert!(
            MANIFEST.contains(r#"android:networkSecurityConfig="@xml/network_security_config""#),
            "the webview talks to 127.0.0.1 over cleartext"
        );
    }

    #[test]
    fn the_activity_survives_every_configuration_change() {
        let activity = MANIFEST
            .split_once("<activity")
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(attrs, _)| attrs)
            .expect("an <activity> element");
        let listed = activity
            .split_once(r#"android:configChanges=""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(value, _)| value)
            .expect("configChanges on the activity");
        let listed: Vec<&str> = listed.split('|').collect();
        for change in REQUIRED_CONFIG_CHANGES {
            assert!(
                listed.contains(&change),
                "configChanges is missing {change}"
            );
        }
    }

    /// The manifest grants exactly what `Dioxus.toml` asks for: every `[permissions]` entry maps
    /// to its Android permissions here, so a new entry fails this test until it is mapped (and
    /// pinned in the manifest), and a manifest permission nobody asks for fails it too.
    #[test]
    fn the_permissions_match_the_configuration() {
        use std::collections::BTreeSet;
        let expands = |entry: &str| -> Option<&'static [&'static str]> {
            Some(match entry {
                "bluetooth" => &[
                    "android.permission.BLUETOOTH_CONNECT",
                    "android.permission.BLUETOOTH_SCAN",
                    "android.permission.BLUETOOTH",
                    "android.permission.BLUETOOTH_ADMIN",
                ],
                "location" => &[
                    "android.permission.ACCESS_FINE_LOCATION",
                    "android.permission.ACCESS_COARSE_LOCATION",
                ],
                _ => return None,
            })
        };
        let section = DIOXUS_TOML
            .split_once("[permissions]")
            .map(|(_, rest)| rest.split("\n[").next().unwrap_or(rest))
            .expect("Dioxus.toml has a [permissions] table");
        let entries: Vec<&str> = section
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
            .collect();
        assert!(!entries.is_empty());
        // the internet is not a runtime permission: the manifest always declares it
        let mut wanted: BTreeSet<&str> = BTreeSet::from(["android.permission.INTERNET"]);
        for entry in &entries {
            let mapped = expands(entry).unwrap_or_else(|| {
                panic!("Dioxus.toml asks for `{entry}`: map it here and pin it in the manifest")
            });
            wanted.extend(mapped.iter().copied());
        }
        let granted: BTreeSet<&str> = MANIFEST
            .split("<uses-permission")
            .skip(1)
            .filter_map(|tag| tag.split("android:name=\"").nth(1)?.split('"').next())
            .collect();
        assert_eq!(granted, wanted, "manifest permissions vs Dioxus.toml");
        // Android 12+ only delivers scan results without a location permission when the scan
        // permission says it is never used for location
        assert!(
            MANIFEST.contains("neverForLocation"),
            "BLUETOOTH_SCAN needs usesPermissionFlags=neverForLocation"
        );
    }
}
