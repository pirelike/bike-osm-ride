//! SSR preview of every ride-ui-dialogs screen, rendered into static HTML so it can be
//! screenshotted with headless Edge (the same engine as the app's WebView2):
//!
//!   cargo run -p ride-ui-dialogs --example dialogs_preview
//!
//! Writes, into `target/preview/`:
//! * `ride-ui-dialogs-{dark,light}.html` — a gallery with all screens of that theme,
//! * `ride-ui-dialogs-<screen>-{dark,light}.html` — one self-contained page per screen.
//!
//! Realistic fake data only; nothing here touches the network, the file system (besides the
//! preview directory) or any application state.

use dioxus::prelude::*;
use ride_model::{
    ConnState, DeviceInfo, DeviceKind, MapStyle, ProfileMode, RecentRoute, RideSummary,
    RouteSource, SensorCommand, Settings, Theme,
};
use ride_ui_dialogs::{
    AboutDialog, DeviceSlot, DevicesDialog, RouteDialog, SettingsDialog, SummaryDialog,
    VirtualRiderPanel, CSS, SIM_DEVICE_ID,
};
use ride_ui_kit::{html_document, Icon, IconName};
use std::path::Path;

/// Preview-page chrome (`pk-` prefix). Cards stand in for the shell; the dialogs keep their
/// real fixed-overlay layout in the app, here they fill the card they are rendered into.
const PREVIEW_CSS: &str = r##"
.pk { max-width: 1040px; margin: 0 auto; padding: 36px 32px 72px; display: flex; flex-direction: column; gap: 30px; }
.pk-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; flex-wrap: wrap; padding-bottom: 18px; border-bottom: 1px solid var(--border); }
.pk-head h1 { font: 600 var(--fs-3xl)/1.1 var(--font-display); letter-spacing: -0.02em; }
.pk-head p { margin-top: 8px; max-width: 68ch; color: var(--text-dim); font-size: var(--fs-base); line-height: 1.55; }
.pk-badges { display: flex; align-items: center; gap: var(--space-2); }
.pk-sec { display: flex; flex-direction: column; gap: var(--space-2); }
.pk-sec__head { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-3); }
.pk-sec__head h2 { font: 600 var(--fs-lg)/1.25 var(--font-display); letter-spacing: -0.01em; }
.pk-sec__slug { font: 500 var(--fs-xs)/1 var(--font-num); letter-spacing: 0.04em; color: var(--text-faint); }
.pk-stage { position: relative; border: 1px solid var(--border-strong); border-radius: var(--radius-xl); overflow: hidden; background: var(--bg); box-shadow: var(--shadow-1); }
.pk-stage--welcome { height: 790px; }
/* Dialog cards grow with their content: the backdrop becomes a normal block inside the card
   so nothing is clipped and the sticky footer lands at the end of the content. */
.pk-stage--dialog { height: auto; }
.pk-stage--vr { height: 470px; display: flex; align-items: flex-end; padding: var(--space-4); background: linear-gradient(155deg, var(--bg-1), var(--bg)); }
.pk-stage__scroll { position: absolute; inset: 0; overflow: auto; }
.pk-stage .dialog-backdrop { position: absolute; inset: 0; }
.pk-stage--dialog .dialog-backdrop { position: relative; inset: auto; }
.pk-stage--dialog .dialog { max-height: none; }
.pk-stage--vr .pk-stage__map { position: absolute; inset: 0; background: radial-gradient(60% 80% at 20% 10%, var(--accent-soft), transparent 60%), radial-gradient(50% 60% at 90% 90%, var(--fill-2), transparent 60%); }
.pk-stage--vr .dlg-vr { position: relative; z-index: 1; }
"##;

// ───────────────────────────── fake data ─────────────────────────────

fn fake_recents() -> Vec<RecentRoute> {
    vec![
        RecentRoute {
            id: "gpx-budai".into(),
            name: "Budai-hegység kör".into(),
            total_dist: 24_100.0,
            ascent: 412.0,
            source: RouteSource::GpxFile,
            last_opened_unix: 1_790_000_000.0,
        },
        RecentRoute {
            id: "br-velence".into(),
            name: "Velencei-tó – Belső út".into(),
            total_dist: 42_800.0,
            ascent: 186.0,
            source: RouteSource::Bikerouter,
            last_opened_unix: 1_789_400_000.0,
        },
        RecentRoute {
            id: "gpx-normafa".into(),
            name: "Normafa – János-hegy".into(),
            total_dist: 8_600.0,
            ascent: 264.0,
            source: RouteSource::GpxFile,
            last_opened_unix: 1_788_100_000.0,
        },
    ]
}

fn fake_devices() -> Vec<DeviceInfo> {
    vec![
        DeviceInfo {
            id: SIM_DEVICE_ID.into(),
            name: "Szimulált kerékpáros".into(),
            kinds: vec![DeviceKind::Trainer],
            rssi: None,
        },
        DeviceInfo {
            id: "f4:12:34:56:78:90".into(),
            name: "KICKR CORE 6A1F".into(),
            kinds: vec![DeviceKind::Trainer, DeviceKind::PowerMeter],
            rssi: Some(-58),
        },
        DeviceInfo {
            id: "c2:9e:88:44:01:7b".into(),
            name: "Polar H10 9C2A".into(),
            kinds: vec![DeviceKind::HeartRate],
            rssi: Some(-71),
        },
        DeviceInfo {
            id: "de:ad:be:ef:11:22".into(),
            name: "Assioma DUO".into(),
            kinds: vec![DeviceKind::PowerMeter],
            rssi: Some(-84),
        },
        DeviceInfo {
            id: "aa:bb:cc:de:f0:12".into(),
            name: "Wahoo BlueSC".into(),
            kinds: vec![DeviceKind::Cadence],
            rssi: Some(-66),
        },
    ]
}

fn fake_slots() -> Vec<DeviceSlot> {
    vec![
        DeviceSlot {
            kind: DeviceKind::Trainer,
            state: ConnState::Connected,
            name: Some("KICKR CORE 6A1F".into()),
        },
        DeviceSlot {
            kind: DeviceKind::HeartRate,
            state: ConnState::Connecting,
            name: Some("Polar H10 9C2A".into()),
        },
        DeviceSlot {
            kind: DeviceKind::PowerMeter,
            state: ConnState::Failed("időtúllépés a kapcsolódásnál".into()),
            name: None,
        },
        DeviceSlot {
            kind: DeviceKind::Cadence,
            state: ConnState::Disconnected,
            name: None,
        },
    ]
}

fn fake_settings(theme: Theme) -> Settings {
    let mut settings = Settings::default();
    settings.rider.rider_kg = 78.5;
    settings.rider.bike_kg = 8.4;
    settings.rider.ftp_w = 265.0;
    settings.rider.max_hr = 188.0;
    settings.rider.cda = 0.34;
    settings.rider.crr = 0.004;
    settings.rider.trainer_grade_scale = 0.85;
    settings.rider.use_trainer_speed = true;
    settings.ui.theme = theme;
    settings.ui.map_style = match theme {
        Theme::Dark => MapStyle::Dark,
        Theme::Light => MapStyle::Light,
    };
    settings.ui.profile_mode = ProfileMode::Lookahead;
    settings.ui.follow_rider = true;
    settings.ui.lookahead_m = 4_000.0;
    settings
}

fn fake_summary() -> RideSummary {
    RideSummary {
        route_name: "Budai-hegység kör".into(),
        route_id: "demo-route".into(),
        started_unix_s: 1_790_000_000.0,
        elapsed_s: 6_129.0,
        dist: 24_100.0,
        ascent: 412.0,
        avg_speed_ms: 6.9,
        max_speed_ms: 13.2,
        avg_power_w: Some(212.0),
        max_power_w: Some(540.0),
        normalized_power_w: Some(238.0),
        intensity_factor: Some(0.9),
        tss: Some(138.0),
        work_kj: 1_300.0,
        kcal: 1_240.0,
        avg_hr: Some(148.0),
        max_hr: Some(176.0),
        avg_cadence: Some(88.0),
        power_zone_secs: [600.0, 1_800.0, 2_400.0, 900.0, 300.0, 100.0, 29.0],
        hr_zone_secs: [120.0, 1_800.0, 3_000.0, 1_100.0, 109.0],
    }
}

// ───────────────────────────── screens ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Welcome,
    WelcomeBusy,
    Devices,
    Settings,
    Summary,
    About,
    VirtualRider,
}

impl Screen {
    const ALL: [Screen; 7] = [
        Screen::Welcome,
        Screen::WelcomeBusy,
        Screen::Devices,
        Screen::Settings,
        Screen::Summary,
        Screen::About,
        Screen::VirtualRider,
    ];

    fn slug(self) -> &'static str {
        match self {
            Screen::Welcome => "welcome",
            Screen::WelcomeBusy => "welcome-busy",
            Screen::Devices => "devices",
            Screen::Settings => "settings",
            Screen::Summary => "summary",
            Screen::About => "about",
            Screen::VirtualRider => "virtual-rider",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Screen::Welcome => "Kezdőképernyő",
            Screen::WelcomeBusy => "Kezdőképernyő – betöltés és hiba",
            Screen::Devices => "Eszközök",
            Screen::Settings => "Beállítások",
            Screen::Summary => "Túra összegzése",
            Screen::About => "Névjegy és licencek",
            Screen::VirtualRider => "Szimulált kerékpáros panel",
        }
    }
}

#[component]
fn WelcomeStage(busy_flag: bool, error_text: Option<String>) -> Element {
    let recents = use_signal(fake_recents);
    let busy = use_signal(|| busy_flag);
    let error = use_signal(|| error_text.clone());
    rsx! {
        div { class: "pk-stage pk-stage--welcome",
            div { class: "pk-stage__scroll",
                RouteDialog {
                    open: true,
                    on_close: move |_| {},
                    recents,
                    busy,
                    error,
                    on_pick_file: move |_| {},
                    on_import_link: move |_: String| {},
                    on_open_recent: move |_: String| {},
                    on_delete_recent: move |_: String| {},
                    on_demo: move |_| {},
                }
            }
        }
    }
}

#[component]
fn DevicesStage() -> Element {
    let discovered = use_signal(fake_devices);
    let slots = use_signal(fake_slots);
    rsx! {
        div { class: "pk-stage pk-stage--dialog",
            DevicesDialog {
                open: true,
                discovered,
                slots,
                scanning: true,
                on_command: move |_: SensorCommand| {},
                on_close: move |_| {},
            }
        }
    }
}

#[component]
fn SettingsStage(theme: Theme) -> Element {
    let settings = use_signal(|| fake_settings(theme));
    // two stored rides on the loaded route: the ghost selector says the ghost is ready
    let ghost_status = use_signal(|| ride_model::GhostStatus {
        loaded: true,
        rides: 2,
        simulated: 0,
        replayable: true,
        partial: false,
        route_time_s: Some(5_025.0),
    });
    rsx! {
        div { class: "pk-stage pk-stage--dialog",
            SettingsDialog {
                open: true,
                settings,
                ghost_status,
                on_change: move |_: Settings| {},
                on_close: move |_| {},
            }
        }
    }
}

#[component]
fn SummaryStage() -> Element {
    let summary = use_signal(|| Some(fake_summary()));
    rsx! {
        div { class: "pk-stage pk-stage--dialog",
            SummaryDialog {
                open: true,
                summary,
                on_save_tcx: move |_| {},
                on_close: move |_| {},
                on_new_ride: move |_| {},
            }
        }
    }
}

#[component]
fn AboutStage() -> Element {
    rsx! {
        div { class: "pk-stage pk-stage--dialog",
            AboutDialog { open: true, on_close: move |_| {} }
        }
    }
}

#[component]
fn VirtualRiderStage() -> Element {
    rsx! {
        div { class: "pk-stage pk-stage--vr",
            div { class: "pk-stage__map" }
            VirtualRiderPanel { power_w: 250.0, on_power: move |_: f64| {} }
        }
    }
}

#[component]
fn ScreenBlock(screen: Screen, theme: Theme) -> Element {
    let body = match screen {
        Screen::Welcome => rsx! { WelcomeStage { busy_flag: false, error_text: None } },
        Screen::WelcomeBusy => rsx! {
            WelcomeStage {
                busy_flag: true,
                error_text: Some("Nem sikerült megnyitni a fájlt: hibás GPX (3. sor).".to_string()),
            }
        },
        Screen::Devices => rsx! { DevicesStage {} },
        Screen::Settings => rsx! { SettingsStage { theme } },
        Screen::Summary => rsx! { SummaryStage {} },
        Screen::About => rsx! { AboutStage {} },
        Screen::VirtualRider => rsx! { VirtualRiderStage {} },
    };
    rsx! {
        section { class: "pk-sec",
            div { class: "pk-sec__head",
                h2 { "{screen.title()}" }
                span { class: "pk-sec__slug", "{screen.slug()} · {ride_ui_kit::theme_attr(theme)}" }
            }
            {body}
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct PageProps {
    screens: Vec<Screen>,
    theme: Theme,
}

#[component]
fn Page(props: PageProps) -> Element {
    let theme = props.theme;
    let theme_icon = match theme {
        Theme::Dark => IconName::Moon,
        Theme::Light => IconName::Sun,
    };
    let theme_label = match theme {
        Theme::Dark => "Sötét téma",
        Theme::Light => "Világos téma",
    };
    rsx! {
        div { class: "pk",
            header { class: "pk-head",
                div {
                    div { class: "t-eyebrow", "bike-osm-ride · kezdőképernyő és dialógusok" }
                    h1 { "Párbeszédablakok előnézete" }
                    p {
                        "Statikus SSR-előnézet: minden képernyő valósághű mintaadattal, mindkét témában. A dialógusok a valódi kit Dialog komponensét használják (Esc, fókuszcsapda, háttérkattintás)."
                    }
                }
                div { class: "pk-badges",
                    span { class: "chip chip--accent",
                        Icon { name: theme_icon, size: 14 }
                        "{theme_label}"
                    }
                    span { class: "chip", "v{env!(\"CARGO_PKG_VERSION\")}" }
                }
            }
            for screen in props.screens.clone() {
                ScreenBlock { key: "{screen.slug()}", screen, theme }
            }
        }
    }
}

// ───────────────────────────── driver ─────────────────────────────

fn render_page(screens: &[Screen], theme: Theme) -> String {
    let mut vdom = VirtualDom::new_with_props(
        Page,
        PageProps {
            screens: screens.to_vec(),
            theme,
        },
    );
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[CSS, PREVIEW_CSS], &body)
}

fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

fn write_previews(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for theme in [Theme::Dark, Theme::Light] {
        let suffix = theme_name(theme);
        let gallery = dir.join(format!("ride-ui-dialogs-{suffix}.html"));
        std::fs::write(&gallery, render_page(&Screen::ALL, theme))?;
        println!("{}", gallery.display());
        for screen in Screen::ALL {
            let path = dir.join(format!("ride-ui-dialogs-{}-{suffix}.html", screen.slug()));
            std::fs::write(&path, render_page(&[screen], theme))?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

fn main() {
    let dir: std::path::PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    if let Err(err) = write_previews(&dir) {
        eprintln!("preview: {err}");
        std::process::exit(1);
    }
}
