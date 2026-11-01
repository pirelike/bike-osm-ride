//! SSR preview of the Street View panel in every state, dark and light:
//!
//!   cargo run -p ride-ui-streetview --example preview
//!
//! Writes `target/preview/ride-ui-streetview-{dark,light}.html` and, when Microsoft Edge is
//! installed (the engine of the app's WebView2), screenshots them to
//! `target/preview/shots/streetview-{dark,light}.png`. The "ok" panels contain real iframes
//! that load the *mock* embed page served by `ride_ui_streetview::mock` (no Google, no key),
//! which stays up while Edge takes the pictures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use ride_model::Theme;
use ride_ui_kit::html_document;
use ride_ui_streetview::mock::{MockResponse, MockServer, StreetviewMock};
use ride_ui_streetview::{
    ApiKey, EmbedTarget, EmbedUrl, Endpoints, PanelStatus, PanelView, StreetViewView, SvError,
    View, CSS,
};

const PREVIEW_CSS: &str = r#"
.vp { max-width: 1240px; margin: 0 auto; padding: 32px 36px 56px; display: flex; flex-direction: column; gap: 30px; }
.vp-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; padding-bottom: 18px; border-bottom: 1px solid var(--border); }
.vp-head h1 { font: 600 var(--fs-3xl)/1.1 var(--font-display); letter-spacing: -0.02em; }
.vp-head p { color: var(--text-dim); margin-top: 8px; max-width: 70ch; }
.vp-sec { display: flex; flex-direction: column; gap: 12px; }
.vp-sec > h2 { font: 600 var(--fs-xs)/1 var(--font-ui); letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-dim); display: flex; align-items: center; gap: 12px; }
.vp-sec > h2::after { content: ""; flex: 1; height: 1px; background: var(--border); }
.vp-grid { display: grid; grid-template-columns: repeat(3, 340px); gap: 24px 28px; align-items: start; }
.vp-cap { font: 600 var(--fs-2xs)/1 var(--font-ui); letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 8px; }
.vp-scene {
  position: relative; height: 520px; border: 1px solid var(--border); border-radius: var(--radius-sm);
  background:
    repeating-linear-gradient(0deg, var(--chart-grid) 0 1px, transparent 1px 44px),
    repeating-linear-gradient(90deg, var(--chart-grid) 0 1px, transparent 1px 44px),
    var(--bg-2);
}
.vp-scene__pip { position: absolute; left: 16px; bottom: 16px; }
.vp-scene__note { position: absolute; right: 16px; top: 14px; font: 500 var(--fs-sm)/1.4 var(--font-ui); color: var(--text-faint); }
"#;

struct Ctx {
    ep: Endpoints,
    key: ApiKey,
    base: String,
}

impl Ctx {
    fn frame(&self, pano: &str, heading: f64) -> EmbedUrl {
        self.ep.embed_url(
            &self.key,
            &EmbedTarget::Pano {
                id: pano.to_string(),
                fallback: Some((47.5, 19.04)),
            },
            View {
                heading,
                pitch: 2.0,
                fov: 90.0,
            },
        )
    }

    fn ok(&self, large: bool, pano: &str, note: Option<&str>) -> PanelView {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.large = large;
        // front = 1: the second iframe is the visible one, the first still holds the previous
        // panorama (what the cross-fade looks like right after a swap)
        v.frames = [
            Some(self.frame("MOCK475000_190300", 80.0)),
            Some(self.frame(pano, 92.0)),
        ];
        v.front = 1;
        v.dist_text = "12.34".to_string();
        v.note = note.map(str::to_string);
        v
    }

    /// The offline state: a downloaded image, no iframes, no key needed.
    fn offline(&self) -> PanelView {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.image = Some(format!("{}/mock-street.jpg", self.base));
        v.dist_text = "12.34".to_string();
        v.can_download = true;
        v.note = Some("Offline Street View: 412 kép".to_string());
        v
    }

    fn missing(&self) -> PanelView {
        let mut v = PanelView::new(PanelStatus::ImageMissing);
        v.dist_text = "18.90".to_string();
        v.can_download = true;
        v
    }

    fn downloading(&self) -> PanelView {
        let mut v = self.offline();
        v.downloading = true;
        v.download_fraction = 0.42;
        v.note = Some("Képek letöltése: 173/412 (42%)".to_string());
        v
    }
}

#[derive(Clone, PartialEq, Props)]
struct PageProps {
    theme: Theme,
    shots: Vec<(String, PanelView)>,
    large: Vec<(String, PanelView)>,
    scene: PanelView,
}

#[component]
fn Panel(view: PanelView) -> Element {
    rsx! {
        StreetViewView {
            view,
            on_close: move |_| {},
            on_toggle_size: move |_| {},
            on_open_settings: move |_| {},
            on_download: move |_| {},
        }
    }
}

#[component]
fn Page(props: PageProps) -> Element {
    let theme_name = match props.theme {
        Theme::Dark => "Sötét téma",
        Theme::Light => "Világos téma",
    };
    rsx! {
        div { class: "vp",
            header { class: "vp-head",
                div {
                    div { class: "t-eyebrow", "bike-osm-ride · Street View" }
                    h1 { "Street View panel" }
                    p { "Saját Google API-kulcs, a Maps Embed API iframe-je két egymásra rakott pufferben, vagy letöltött kép (offline mód). Az állapotok: nincs kulcs, keresés, nincs felvétel itt, hiba, nincs letöltve, letöltés, kész. A kép itt egy helyi mock oldal, nem a Google." }
                }
                span { class: "chip chip--accent", "{theme_name}" }
            }
            section { class: "vp-sec",
                h2 { "Állapotok — kompakt" }
                div { class: "vp-grid",
                    for (cap, view) in props.shots.iter() {
                        div {
                            div { class: "vp-cap", "{cap}" }
                            Panel { view: view.clone() }
                        }
                    }
                }
            }
            section { class: "vp-sec",
                h2 { "Nagy méret" }
                div { class: "vp-grid", style: "grid-template-columns: 720px 340px;",
                    for (cap, view) in props.large.iter() {
                        div {
                            div { class: "vp-cap", "{cap}" }
                            Panel { view: view.clone() }
                        }
                    }
                }
            }
            section { class: "vp-sec",
                h2 { "A térkép sarkában" }
                div { class: "vp-scene",
                    div { class: "vp-scene__note", "térkép helye" }
                    div { class: "vp-scene__pip", Panel { view: props.scene.clone() } }
                }
            }
        }
    }
}

fn page_props(ctx: &Ctx, theme: Theme) -> PageProps {
    let note = "Panoráma-azonosítók: 48/51 előtöltve";
    let mut checking = PanelView::new(PanelStatus::Checking);
    checking.dist_text = "0.00".into();
    let shots = vec![
        (
            "Nincs kulcs".to_string(),
            PanelView::new(PanelStatus::NoKey),
        ),
        ("Keresés".to_string(), checking),
        (
            "Nincs felvétel itt".to_string(),
            PanelView::new(PanelStatus::NoCoverage),
        ),
        (
            "A kulcsot elutasították".to_string(),
            PanelView::new(PanelStatus::Error(SvError::KeyRejected)),
        ),
        (
            "Elfogyott a kvóta".to_string(),
            PanelView::new(PanelStatus::Error(SvError::QuotaExceeded)),
        ),
        (
            "Nincs kapcsolat".to_string(),
            PanelView::new(PanelStatus::Error(SvError::Network)),
        ),
        ("Nincs letöltve".to_string(), ctx.missing()),
        ("Letöltött kép (offline)".to_string(), ctx.offline()),
        ("Letöltés folyamatban".to_string(), ctx.downloading()),
        ("Kész".to_string(), ctx.ok(false, "MOCK475020_190420", None)),
        (
            "Kész, azonosítók mentésével".to_string(),
            ctx.ok(false, "MOCK475030_190520", Some(note)),
        ),
    ];
    let mut large_none = PanelView::new(PanelStatus::NoCoverage);
    large_none.large = true;
    let mut small_none = PanelView::new(PanelStatus::NoKey);
    small_none.large = false;
    let large = vec![
        (
            "Kész, nagy".to_string(),
            ctx.ok(true, "MOCK475040_190620", Some(note)),
        ),
        ("Nincs kulcs, kompakt".to_string(), small_none),
    ];
    let _ = (theme, large_none);
    PageProps {
        theme,
        shots,
        large,
        scene: ctx.ok(false, "MOCK475050_190720", None),
    }
}

fn render(ctx: &Ctx, theme: Theme) -> String {
    let props = page_props(ctx, theme);
    let mut vdom = VirtualDom::new_with_props(Page, props);
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    html_document(theme, &[CSS, PREVIEW_CSS], &body)
}

fn edge() -> Option<PathBuf> {
    [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// Runs Edge through PowerShell's `Start-Process -Wait`: launched directly from a cargo / bash
/// child process, msedge.exe forwards the command to a "ghost" instance and exits after
/// 0.1 s without writing anything.
fn screenshot(edge: &Path, url: &str, png: &Path) -> bool {
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList '--headless=new','--disable-gpu',         '--hide-scrollbars','--force-device-scale-factor=1','--window-size=1320,2420',         '--virtual-time-budget=6000','--screenshot={}','{}' -Wait -PassThru; exit $p.ExitCode",
        edge.display(),
        png.display(),
        url
    );
    let _ = std::fs::remove_file(png);
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        && png.is_file()
}

fn main() -> std::io::Result<()> {
    // The pages are served by the mock itself (same host as the iframes) so Edge does not
    // block file:// -> http:// loopback frames.
    let pages: Arc<Mutex<HashMap<String, String>>> = Arc::default();
    let served = Arc::clone(&pages);
    let opts = StreetviewMock::default();
    let server = MockServer::start(move |req| {
        if req.path == "/mock-street.jpg" {
            return MockResponse::jpeg(200, ride_ui_streetview::mock::MOCK_JPEG.to_vec());
        }
        if let Some(name) = req.path.strip_prefix("/preview/") {
            if let Some(page) = served.lock().ok().and_then(|m| m.get(name).cloned()) {
                return MockResponse::html(page);
            }
        }
        opts.reply(req)
    });
    let ctx = Ctx {
        ep: Endpoints::local(&server.base).expect("loopback mock"),
        key: ApiKey::new("PREVIEWKEY").expect("key"),
        base: server.base.clone(),
    };
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "target", "preview"]
        .iter()
        .collect();
    let shots = dir.join("shots");
    std::fs::create_dir_all(&shots)?;
    let edge = edge();
    for (theme, name) in [(Theme::Dark, "dark"), (Theme::Light, "light")] {
        let html_text = render(&ctx, theme);
        let html = dir.join(format!("ride-ui-streetview-{name}.html"));
        std::fs::write(&html, &html_text)?;
        println!("{}", html.display());
        if let Ok(mut m) = pages.lock() {
            m.insert(format!("{name}.html"), html_text);
        }
        if let Some(edge) = &edge {
            let png = shots.join(format!("streetview-{name}.png"));
            let url = format!("{}/preview/{name}.html", server.base);
            let ok = screenshot(edge, &url, &png);
            println!(
                "{} {}",
                if ok {
                    "screenshot"
                } else {
                    "screenshot FAILED"
                },
                png.display()
            );
        }
    }
    println!("mock requests served: {}", server.request_count());
    Ok(())
}
