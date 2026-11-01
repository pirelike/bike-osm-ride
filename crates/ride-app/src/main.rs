//! bike-osm-ride: virtual rides on GPX routes with a BLE smart trainer (Dioxus 0.7 desktop).
//!
//! This crate is the shell: window configuration, the tile proxy, application state, the
//! controller that wires the ride engine to the sensor hub, and the page layout. All
//! user-visible text is Hungarian. See `hooks.rs` for the `RIDE_*` test environment variables.

// Release builds are GUI-subsystem executables (no console window); debug builds keep the console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod browser;
mod controller;
mod dock;
mod gearbox;
mod guard;
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

fn main() {
    #[cfg(target_os = "android")]
    paths::init_android_data_dir();
    #[cfg(not(target_os = "android"))]
    updater::cleanup_previous();

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
        .with_menu(None)
        .with_background_color(shell::background_rgba(theme))
        .with_disable_context_menu(true)
        .with_data_directory(paths::webview_dir())
        .with_custom_head(shell::head_html(theme))
        .with_custom_event_handler(|event, _| {
            if matches!(event, Event::LoopDestroyed) {
                controller::flush_pending_settings();
            }
        });
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
