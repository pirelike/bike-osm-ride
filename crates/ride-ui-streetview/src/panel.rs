//! The Street View panel.
//!
//! * [`StreetViewView`] is the presentational half: it renders a [`PanelView`] (status, a
//!   downloaded image *or* two stacked iframes with a front index, readout, footer note) and
//!   is what the SSR preview draws for every state.
//! * [`StreetViewPanel`] is the stateful half used by the shell. It watches the ride
//!   snapshot, shows the full-resolution live embed whenever there is a key and falls back to a
//!   downloaded image of the current bucket (at most 640 px, Google's limit) without a key or
//!   while the live view fails. For the live view it asks the [`Planner`] when a new panorama is needed and
//!   resolves the id (cache first, then the metadata endpoint), loads the embed URL into the
//!   *hidden* iframe and swaps the two when it has loaded. With the offline option it can
//!   download every image of the route on request ([`crate::download`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use ride_model::{RideSnapshot, Route, UiSettings};
use ride_ui_kit::format::fmt_km;
use ride_ui_kit::{Icon, IconName};

use crate::cache::{now_unix, PanoCache};
use crate::client::MetadataClient;
use crate::download::{DownloadHandle, Downloader, Progress as DownloadProgress};
use crate::error::SvError;
use crate::image::ImageFetcher;
use crate::imagestore::{image_url, ImageStore};
use crate::planner::{Availability, Effect, Outcome, Planner};
use crate::prefetch::{bucket_point, resolve_live, PrefetchConfig, PrefetchHandle, Prefetcher};
use crate::url::{EmbedTarget, EmbedUrl};

/// Component stylesheet (tokens only). The shell injects it after the design kit.
pub const CSS: &str = include_str!("../assets/streetview.css");

static LOAD_TIMEOUT_MS: AtomicU64 = AtomicU64::new(10_000);

/// How often the network is re-checked while downloaded images stand in for the live view.
const LIVE_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// How long a hidden iframe may take before it is swapped in anyway (default 10 s).
pub fn load_timeout() -> Duration {
    Duration::from_millis(LOAD_TIMEOUT_MS.load(Ordering::Relaxed))
}

/// Shortens the load timeout (tests and QA runs, where no `load` event arrives).
pub fn set_load_timeout(d: Duration) {
    LOAD_TIMEOUT_MS.store(
        d.as_millis().min(u128::from(u64::MAX)) as u64,
        Ordering::Relaxed,
    );
}

/// What the panel is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelStatus {
    /// No route loaded.
    NoRoute,
    /// No API key in the settings and no downloaded image for this route.
    NoKey,
    /// First panorama is being looked up / loaded.
    Checking,
    /// Google has no panorama here.
    NoCoverage,
    /// Key / quota / network problem.
    Error(SvError),
    /// Offline mode: this part of the route was not downloaded.
    ImageMissing,
    /// A panorama (downloaded image or embed) is on screen.
    Ok,
}

impl PanelStatus {
    fn data_state(self) -> &'static str {
        match self {
            PanelStatus::NoRoute | PanelStatus::Checking => "checking",
            PanelStatus::NoKey => "nokey",
            PanelStatus::NoCoverage => "none",
            PanelStatus::Error(_) => "error",
            PanelStatus::ImageMissing => "offline",
            PanelStatus::Ok => "ok",
        }
    }

    /// Headline of the message that replaces the picture (`None` while a picture shows).
    pub fn title(self) -> Option<&'static str> {
        match self {
            PanelStatus::Ok => None,
            PanelStatus::NoRoute => Some("Nincs betöltött útvonal"),
            PanelStatus::NoKey => Some(SvError::MissingKey.title()),
            PanelStatus::Checking => Some("Panoráma keresése"),
            PanelStatus::NoCoverage => Some(SvError::NoCoverage.title()),
            PanelStatus::Error(e) => Some(e.title()),
            PanelStatus::ImageMissing => Some("Ez a szakasz még nincs letöltve"),
        }
    }

    /// Explanation / next step under the headline.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            PanelStatus::Ok => None,
            PanelStatus::NoRoute => Some("Tölts be egy útvonalat a Street View-hoz."),
            PanelStatus::NoKey => Some(SvError::MissingKey.hint()),
            PanelStatus::Checking => Some("A Google Street View-felvétel betöltése folyamatban."),
            PanelStatus::NoCoverage => Some(SvError::NoCoverage.hint()),
            PanelStatus::Error(e) => Some(e.hint()),
            PanelStatus::ImageMissing => Some(
                "Indítsd el a képek letöltését a fejléc gombjával: utána ez az útvonal Google-\
                 kérés és API-kulcs nélkül is végigtekered.",
            ),
        }
    }

    /// Does the message offer a button that opens the settings?
    pub fn offers_settings(self) -> bool {
        match self {
            PanelStatus::NoKey | PanelStatus::ImageMissing => true,
            PanelStatus::Error(e) => matches!(
                e,
                SvError::MissingKey
                    | SvError::KeyRejected
                    | SvError::InvalidRequest
                    | SvError::QuotaExceeded
            ),
            _ => false,
        }
    }

    /// Does the message offer the "download this route" button?
    pub fn offers_download(self) -> bool {
        self == PanelStatus::ImageMissing
    }

    /// Short text for the header readout when there is no distance to show.
    pub fn readout(self) -> Option<&'static str> {
        match self {
            PanelStatus::Ok => None,
            PanelStatus::NoRoute => Some("Nincs útvonal"),
            PanelStatus::NoKey => Some("API-kulcs szükséges"),
            PanelStatus::Checking => Some("Keresés…"),
            PanelStatus::NoCoverage => Some("Itt nincs Street View"),
            PanelStatus::Error(e) => Some(e.short()),
            PanelStatus::ImageMissing => Some("Nincs letöltött kép"),
        }
    }
}

/// Everything the presentational half needs.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelView {
    pub status: PanelStatus,
    /// Large instead of compact picture-in-picture.
    pub large: bool,
    /// Rendered inside a host tile that supplies the frame and the title: the panel drops its own
    /// title and ruler and keeps the readout and the actions.
    pub embedded: bool,
    /// The two stacked iframes' sources (contain the key; `Debug` is redacted).
    pub frames: [Option<EmbedUrl>; 2],
    /// Which iframe is visible.
    pub front: usize,
    /// URL of the downloaded image that replaces the iframes (offline copy).
    pub image: Option<String>,
    /// Route distance readout, e.g. `12,34`-style text from `fmt_km`.
    pub dist_text: String,
    /// Left footer text (prefetch/download progress, "only ids are saved").
    pub note: Option<String>,
    /// The route can be downloaded (key + offline store available).
    pub can_download: bool,
    /// A download is running right now.
    pub downloading: bool,
    /// 0..1 progress of the running download.
    pub download_fraction: f64,
}

impl PanelView {
    /// A view with no frames (for states without a picture).
    pub fn new(status: PanelStatus) -> PanelView {
        PanelView {
            status,
            large: false,
            embedded: false,
            frames: [None, None],
            front: 0,
            image: None,
            dist_text: String::new(),
            note: None,
            can_download: false,
            downloading: false,
            download_fraction: 0.0,
        }
    }
}

/// Presentational panel: header (title, readout, actions), stage (image / iframes + message),
/// footer. All state arrives through `view`.
#[component]
pub fn StreetViewView(
    view: PanelView,
    on_close: EventHandler<()>,
    on_toggle_size: EventHandler<()>,
    on_open_settings: EventHandler<()>,
    on_download: EventHandler<()>,
    /// Fired with the index (0 or 1) of the iframe that finished loading.
    #[props(default)]
    on_frame_load: EventHandler<usize>,
) -> Element {
    let status = view.status;
    let embedded = view.embedded;
    let size_class = match (embedded, view.large) {
        (true, _) => "sv--compact sv--embedded",
        (false, true) => "sv--large",
        (false, false) => "sv--compact",
    };
    let readout = match status.readout() {
        Some(t) => t.to_string(),
        None => format!("{} km", view.dist_text),
    };
    let size_label = if view.large {
        "Térkép és Street View cseréje (a térkép lesz nagy)"
    } else {
        "Térkép és Street View cseréje (a Street View lesz nagy)"
    };
    let size_icon = IconName::Fullscreen;
    let state = status.data_state();
    let title = status.title();
    let hint = status.hint();
    let offers_settings = status.offers_settings();
    let offers_download = status.offers_download() && view.can_download;
    let note = view.note.clone().unwrap_or_default();
    let download_label = if view.downloading {
        "Street View letöltés leállítása"
    } else {
        "Street View képek letöltése az útvonalhoz"
    };
    let download_icon = if view.downloading {
        IconName::Stop
    } else {
        IconName::Download
    };
    let bar_width = format!("{:.1}%", (view.download_fraction * 100.0).clamp(0.0, 100.0));

    rsx! {
        section { class: "sv {size_class}", "data-state": "{state}", aria_label: "Street View",
            header { class: "sv__head",
                if !embedded {
                    span { class: "sv__title", "Street View" }
                }
                span { class: "sv__readout num", title: "{readout}", "{readout}" }
                div { class: "sv__actions",
                    if view.can_download || view.downloading {
                        button {
                            class: "sv-btn sv-btn--download",
                            r#type: "button",
                            aria_label: "{download_label}",
                            title: "{download_label}",
                            "aria-pressed": if view.downloading { "true" } else { "false" },
                            onclick: move |_| on_download.call(()),
                            Icon { name: download_icon, size: 14 }
                        }
                    }
                    button {
                        class: "sv-btn",
                        r#type: "button",
                        aria_label: "{size_label}",
                        title: "{size_label}",
                        onclick: move |_| on_toggle_size.call(()),
                        Icon { name: size_icon, size: 14 }
                    }
                    button {
                        class: "sv-btn",
                        r#type: "button",
                        aria_label: "Street View beállítások",
                        title: "Street View beállítások",
                        onclick: move |_| on_open_settings.call(()),
                        Icon { name: IconName::Settings, size: 14 }
                    }
                    button {
                        class: "sv-btn",
                        r#type: "button",
                        aria_label: "Street View bezárása",
                        title: "Street View bezárása",
                        onclick: move |_| on_close.call(()),
                        Icon { name: IconName::Close, size: 14 }
                    }
                }
                if view.downloading {
                    div { class: "sv__bar", aria_hidden: "true",
                        i { style: "width: {bar_width}" }
                    }
                }
            }
            if !embedded {
                div { class: "sv__ruler", aria_hidden: "true" }
            }
            div { class: "sv__stage",
                if let Some(src) = view.image.clone() {
                    img {
                        class: "sv__img",
                        src: "{src}",
                        alt: "Letöltött Google Street View panoráma",
                        draggable: "false",
                    }
                } else {
                    for i in 0..2usize {
                        iframe {
                            key: "{i}",
                            class: if view.front == i { "sv__frame is-front" } else { "sv__frame" },
                            title: "Google Street View panoráma",
                            src: view.frames[i].as_ref().map(|u| u.as_str().to_string()),
                            tabindex: if view.front == i { "0" } else { "-1" },
                            "referrerpolicy": "strict-origin-when-cross-origin",
                            "allowfullscreen": "true",
                            "aria-hidden": if view.front == i { "false" } else { "true" },
                            onload: move |_| on_frame_load.call(i),
                        }
                    }
                }
                if let (Some(t), Some(h)) = (title, hint) {
                    div { class: "sv__msg", role: "status",
                        if status == PanelStatus::Checking {
                            div { class: "sv__scan", aria_hidden: "true" }
                        }
                        div { class: "sv__msg-title", "{t}" }
                        div { class: "sv__msg-hint", "{h}" }
                        if offers_download {
                            button {
                                class: "sv-action",
                                r#type: "button",
                                onclick: move |_| on_download.call(()),
                                Icon { name: IconName::Download, size: 14 }
                                "Képek letöltése"
                            }
                        }
                        if offers_settings {
                            button {
                                class: "sv-action",
                                r#type: "button",
                                onclick: move |_| on_open_settings.call(()),
                                Icon { name: IconName::Settings, size: 14 }
                                "Beállítások megnyitása"
                            }
                        }
                    }
                }
            }
            footer { class: "sv__foot",
                span { class: "sv__note", title: "{note}", "{note}" }
                span { class: "sv__credit", "Google Street View" }
            }
        }
    }
}

// ───────────────────────────── stateful panel ─────────────────────────────

/// Everything that exists while a route + key (+ cache option) combination is active.
struct Session {
    /// `None` in offline-only mode (no key, but downloaded images exist).
    client: Option<MetadataClient>,
    fetcher: Option<ImageFetcher>,
    cache: Arc<PanoCache>,
    images: Option<Arc<ImageStore>>,
    route: Arc<Route>,
    prefetch: Option<PrefetchHandle>,
    download: Option<DownloadHandle>,
}

fn prefetch_note(h: &PrefetchHandle) -> String {
    let p = h.progress();
    match p.stopped {
        Some(_) => "Az előtöltés leállt".to_string(),
        None => format!(
            "Panoráma-azonosítók: {}/{} előtöltve",
            p.ahead_ready, p.ahead_total
        ),
    }
}

/// The Street View panel of the ride screen.
///
/// `settings` supplies the key (`google_maps_key`), the id
/// cache (`streetview_cache_panos`) and the offline image option (`streetview_offline`);
/// `on_toggle_size` / `on_open_settings` / `on_close` are for the shell to act on (it owns
/// the settings).
#[component]
pub fn StreetViewPanel(
    route: ReadSignal<Option<Arc<Route>>>,
    snapshot: ReadSignal<RideSnapshot>,
    settings: ReadSignal<UiSettings>,
    on_close: EventHandler<()>,
    on_toggle_size: EventHandler<()>,
    on_open_settings: EventHandler<()>,
    /// The shell draws the frame, title and close button around the panel.
    #[props(default = false)]
    embedded: bool,
) -> Element {
    let started = use_hook(Instant::now);
    let mut session = use_signal(|| None::<Session>);
    let mut planner = use_signal(Planner::default);
    let mut status = use_signal(|| PanelStatus::Checking);
    let mut frames = use_signal(|| [None::<EmbedUrl>, None]);
    let mut front = use_signal(|| 0usize);
    let mut pending = use_signal(|| None::<usize>);
    let mut load_id = use_signal(|| 0u64);
    let mut generation = use_signal(|| 0u64);
    let mut note = use_signal(|| None::<String>);
    let mut image = use_signal(|| None::<String>);
    // The live embed failed (network, quota): fall back to the downloaded images until it
    // shows a panorama again.
    let mut live_failed = use_signal(|| false);
    let mut download_progress = use_signal(DownloadProgress::default);

    // Settings that rebuild the session: key text, id cache, offline images, which route.
    let cfg = use_memo(move || {
        let s = settings.read();
        (
            s.google_maps_key.trim().to_string(),
            s.streetview_cache_panos,
            s.streetview_offline,
        )
    });
    let route_id = use_memo(move || route.read().as_ref().map(|r| r.id.clone()));

    // (Re)build the session.
    use_effect(move || {
        let (key, persist, offline) = cfg.read().clone();
        let rid = route_id.read().clone();
        let route_now = (*route.peek()).clone();
        *generation.write() += 1;
        session.set(None);
        planner.set(Planner::default());
        frames.set([None, None]);
        front.set(0);
        pending.set(None);
        note.set(None);
        image.set(None);
        live_failed.set(false);
        download_progress.set(DownloadProgress::default());

        let Some(route_now) = route_now.filter(|r| Some(&r.id) == rid.as_ref()) else {
            status.set(PanelStatus::NoRoute);
            return;
        };
        let data_dir = ride_route::store::data_dir();
        let images = if offline {
            ImageStore::open(&data_dir, &route_now.id, route_now.total_dist).map(Arc::new)
        } else {
            None
        };
        let has_images = images.as_ref().is_some_and(|s| !s.is_empty());
        let persistent = persist || offline;
        let cache = Arc::new(if persistent {
            PanoCache::persistent_default(&route_now.id)
        } else {
            PanoCache::memory(&route_now.id)
        });

        if key.is_empty() {
            if has_images {
                status.set(PanelStatus::Checking);
                session.set(Some(Session {
                    client: None,
                    fetcher: None,
                    cache,
                    images,
                    route: route_now,
                    prefetch: None,
                    download: None,
                }));
            } else {
                status.set(PanelStatus::NoKey);
            }
            return;
        }
        let client = match MetadataClient::from_settings_key(&key) {
            Ok(c) => c,
            Err(e) => {
                status.set(if e == SvError::MissingKey {
                    PanelStatus::NoKey
                } else {
                    PanelStatus::Error(e)
                });
                return;
            }
        };
        let fetcher = ImageFetcher::from_settings_key(&key).ok();
        // A route whose images are (nearly) all downloaded needs no id lookups any more.
        let downloaded = images.as_ref().is_some_and(|st| {
            st.plan_len() > 0 && st.len().saturating_mul(10) >= st.plan_len().saturating_mul(9)
        });
        let prefetch = (persistent && !downloaded).then(|| {
            Prefetcher::spawn(
                client.clone(),
                Arc::clone(&cache),
                Arc::clone(&route_now),
                snapshot.peek().dist,
                PrefetchConfig::default(),
            )
        });
        status.set(PanelStatus::Checking);
        if has_images {
            // The embed needs the network even when every panorama id is cached, so probe it
            // with a metadata request (free of charge) and fall back to the downloaded images
            // while it fails; the probe repeats, so the live view returns with the network.
            let (client, route) = (client.clone(), Arc::clone(&route_now));
            let gen = *generation.peek();
            spawn(async move {
                loop {
                    let (lat, lng, _) = bucket_point(&route, 0);
                    let reachable = match client.lookup(lat, lng, 50).await {
                        Ok(_) => true,
                        Err(e) => e.is_no_coverage(),
                    };
                    if *generation.peek() != gen {
                        return;
                    }
                    if *live_failed.peek() == reachable {
                        live_failed.set(!reachable);
                        let fallback = session
                            .peek()
                            .as_ref()
                            .and_then(|s| cached_image(s, snapshot.peek().dist, !reachable));
                        image.set(fallback);
                    }
                    tokio::time::sleep(LIVE_PROBE_INTERVAL).await;
                }
            });
        }
        session.set(Some(Session {
            client: Some(client),
            fetcher,
            cache,
            images,
            route: route_now,
            prefetch,
            download: None,
        }));
    });

    // Swap the double buffer when the hidden iframe reports it loaded.
    let mut swap_in = move |idx: usize| {
        front.set(idx);
        pending.set(None);
        if planner.peek().availability() == Availability::Available {
            status.set(PanelStatus::Ok);
        }
    };

    /// Image URL of the bucket containing `dist`, if it was downloaded and is fresh and should
    /// be shown. A downloaded image is at most 640 × 640 px (the Street View Static API limit)
    /// and looks soft in a large panel, so with a key the full-resolution live embed wins; the
    /// download is the fallback without a key or while the live view fails.
    fn cached_image(session: &Session, dist: f64, live_failed: bool) -> Option<String> {
        if session.client.is_some() && !live_failed {
            return None;
        }
        let store = session.images.as_ref()?;
        let bucket = store.bucket_of(dist);
        store
            .get_at(bucket, now_unix())
            .map(|_| image_url(&session.route.id, bucket))
    }

    // Follow the ride.
    use_effect(move || {
        let dist = snapshot.read().dist;
        let guard = session.read();
        let Some(s) = guard.as_ref() else {
            return;
        };
        if let Some(h) = &s.prefetch {
            h.set_position(dist);
            let n = Some(prefetch_note(h));
            if *note.peek() != n {
                note.set(n);
            }
        }
        let cached = cached_image(s, dist, *live_failed.peek());
        if *image.peek() != cached {
            image.set(cached.clone());
        }
        if cached.is_some() {
            if *status.peek() != PanelStatus::Ok {
                status.set(PanelStatus::Ok);
            }
            return;
        }
        let now = started.elapsed().as_secs_f64();
        let Some(plan) = planner.write().tick(now, &s.route, dist) else {
            if s.client.is_none() && *status.peek() != PanelStatus::ImageMissing {
                status.set(PanelStatus::ImageMissing);
            }
            return;
        };
        if s.client.is_none() {
            // Offline-only mode: the planner cannot resolve anything; show the hint instead.
            if *status.peek() != PanelStatus::ImageMissing {
                status.set(PanelStatus::ImageMissing);
            }
            return;
        }
        let (client, cache, route_arc) =
            (s.client.clone(), Arc::clone(&s.cache), Arc::clone(&s.route));
        drop(guard);
        let Some(client) = client else { return };
        let gen = *generation.peek();
        spawn(async move {
            let outcome = match resolve_live(&client, &cache, &route_arc, plan.bucket).await {
                Ok(Some(pano)) => Outcome::Found { pano },
                Ok(None) => Outcome::NoPano,
                Err(e) => Outcome::Failed(e),
            };
            if *generation.peek() != gen {
                return; // settings or route changed while the lookup was running
            }
            let effect = planner.write().apply(&plan, outcome);
            match effect {
                Effect::Nothing => {}
                Effect::Unavailable(e) => {
                    pending.set(None);
                    if e.is_no_coverage() {
                        status.set(PanelStatus::NoCoverage);
                        return;
                    }
                    live_failed.set(true);
                    let fallback = session
                        .peek()
                        .as_ref()
                        .and_then(|s| cached_image(s, snapshot.peek().dist, true));
                    if fallback.is_some() {
                        image.set(fallback);
                        status.set(PanelStatus::Ok);
                    } else {
                        status.set(PanelStatus::Error(e));
                    }
                }
                Effect::Show(shown) => {
                    let (lat, lng, _) = bucket_point(&route_arc, plan.bucket);
                    let url = client.embed_url(
                        &EmbedTarget::Pano {
                            id: shown.pano,
                            fallback: Some((lat, lng)),
                        },
                        shown.view,
                    );
                    let cur = *front.peek();
                    if frames.peek()[cur].as_ref() == Some(&url)
                        && *status.peek() == PanelStatus::Ok
                    {
                        return;
                    }
                    let back = 1 - cur;
                    frames.write()[back] = Some(url);
                    pending.set(Some(back));
                    if matches!(
                        *status.peek(),
                        PanelStatus::NoCoverage | PanelStatus::Error(_)
                    ) {
                        status.set(PanelStatus::Checking);
                    }
                    *load_id.write() += 1;
                    let id = *load_id.peek();
                    spawn(async move {
                        tokio::time::sleep(load_timeout()).await;
                        if *load_id.peek() == id && *pending.peek() == Some(back) {
                            swap_in(back);
                        }
                    });
                }
            }
        });
    });

    // A finished download can fill the current bucket: show it without waiting for the rider
    // to move (this effect reads the progress signal, so it runs when the download advances).
    use_effect(move || {
        let progress = *download_progress.read();
        let dist = snapshot.peek().dist;
        let guard = session.read();
        let Some(s) = guard.as_ref() else {
            return;
        };
        let cached = cached_image(s, dist, *live_failed.peek());
        if *image.peek() != cached {
            image.set(cached.clone());
        }
        if cached.is_some() {
            if *status.peek() != PanelStatus::Ok {
                status.set(PanelStatus::Ok);
            }
        } else if s.client.is_none() && *status.peek() != PanelStatus::ImageMissing {
            status.set(PanelStatus::ImageMissing);
        }
        let _ = progress;
    });

    // Download toggle: starts the route download or cancels the running one.
    let mut toggle_download = move || {
        if download_progress.peek().running {
            if let Some(s) = session.write().as_mut() {
                s.download.take();
            }
            let mut p = download_progress;
            let mut next = *p.peek();
            next.running = false;
            next.note();
            p.set(next);
            return;
        }
        let parts = {
            let guard = session.read();
            let Some(s) = guard.as_ref() else { return };
            let (Some(client), Some(fetcher), Some(images)) =
                (s.client.clone(), s.fetcher.clone(), s.images.clone())
            else {
                return;
            };
            (
                client,
                fetcher,
                Arc::clone(&s.cache),
                images,
                Arc::clone(&s.route),
            )
        };
        let (client, fetcher, cache, images, route) = parts;
        let handle = Downloader::spawn(client, fetcher, cache, images, route);
        let mut rx = handle.subscribe();
        let mut progress = download_progress;
        progress.set(*rx.borrow_and_update());
        {
            let mut guard = session.write();
            if let Some(s) = guard.as_mut() {
                s.download = Some(handle);
            }
        }
        spawn(async move {
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let p = *rx.borrow_and_update();
                progress.set(p);
                if !p.running {
                    break;
                }
            }
        });
    };

    use_drop(move || {
        if let Ok(mut s) = session.try_write() {
            *s = None;
        }
    });

    let dist_text = use_memo(move || fmt_km(snapshot.read().dist));
    let (can_download, downloading, fraction) = {
        let p = *download_progress.read();
        let s = session.read();
        let can = s
            .as_ref()
            .is_some_and(|s| s.client.is_some() && s.fetcher.is_some() && s.images.is_some());
        (can, p.running, p.fraction())
    };
    let footer_note = {
        let p = *download_progress.read();
        if p.total > 0 {
            Some(p.note())
        } else {
            note.read().clone().or_else(|| {
                session
                    .read()
                    .as_ref()
                    .and_then(|s| s.images.as_ref())
                    .filter(|st| !st.is_empty())
                    .map(|st| format!("Offline Street View: {} kép", st.len()))
            })
        }
    };
    let view = PanelView {
        status: *status.read(),
        large: false,
        embedded,
        frames: frames.read().clone(),
        front: *front.read(),
        image: image.read().clone(),
        dist_text: dist_text.read().clone(),
        note: footer_note,
        can_download,
        downloading,
        download_fraction: fraction,
    };

    rsx! {
        StreetViewView {
            view,
            on_close,
            on_toggle_size,
            on_open_settings,
            on_download: move |_| toggle_download(),
            on_frame_load: move |i: usize| {
                if *pending.peek() == Some(i) {
                    swap_in(i);
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::{ApiKey, Endpoints, View};
    use ride_model::Theme;

    fn render(view: PanelView) -> String {
        #[derive(Clone, PartialEq, Props)]
        struct P {
            view: PanelView,
        }
        #[component]
        fn Host(props: P) -> Element {
            rsx! {
                StreetViewView {
                    view: props.view.clone(),
                    on_close: move |_| {},
                    on_toggle_size: move |_| {},
                    on_open_settings: move |_| {},
                    on_download: move |_| {},
                }
            }
        }
        let mut vdom = VirtualDom::new_with_props(Host, P { view });
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    fn url() -> EmbedUrl {
        Endpoints::google().embed_url(
            &ApiKey::new("AIzaTESTKEY").expect("key"),
            &EmbedTarget::Pano {
                id: "PANO1".into(),
                fallback: None,
            },
            View {
                heading: 90.0,
                pitch: 2.0,
                fov: 90.0,
            },
        )
    }

    #[test]
    fn every_status_renders_hungarian_text_and_state() {
        let cases = [
            (PanelStatus::NoKey, "nokey", "API-kulcs szükséges"),
            (PanelStatus::Checking, "checking", "Panoráma keresése"),
            (PanelStatus::NoCoverage, "none", "Itt nincs Street View"),
            (
                PanelStatus::ImageMissing,
                "offline",
                "Ez a szakasz még nincs letöltve",
            ),
            (
                PanelStatus::Error(SvError::KeyRejected),
                "error",
                "A Google elutasította a kulcsot",
            ),
            (
                PanelStatus::Error(SvError::QuotaExceeded),
                "error",
                "Elfogyott a Google-kvóta",
            ),
        ];
        for (st, state, text) in cases {
            let html = render(PanelView::new(st));
            assert!(html.contains(&format!("data-state=\"{state}\"")), "{st:?}");
            assert!(html.contains(text), "{st:?}: {html}");
            assert!(html.contains("sv__msg"));
        }
        let nokey = render(PanelView::new(PanelStatus::NoKey));
        assert!(nokey.contains("Maps Embed API") && nokey.contains("Street View Static API"));
        assert!(nokey.contains("Beállítások megnyitása"));
        // "no coverage" has no settings button
        assert!(!render(PanelView::new(PanelStatus::NoCoverage)).contains("Beállítások megnyitása"));
    }

    #[test]
    fn ok_state_has_two_stacked_frames_with_one_in_front() {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.dist_text = "12.34".into();
        v.frames = [Some(url()), None];
        v.front = 0;
        v.note = Some("Panoráma-azonosítók: 40/51 előtöltve".into());
        let html = render(v);
        assert_eq!(html.matches("<iframe").count(), 2);
        assert_eq!(html.matches("is-front").count(), 1);
        assert!(html.contains("12.34 km"));
        assert!(
            !html.contains("sv__msg"),
            "no overlay while a picture shows"
        );
        assert!(html.contains("referrerpolicy=\"strict-origin-when-cross-origin\""));
        assert!(html.contains("allowfullscreen"));
        assert!(html.contains("Panoráma-azonosítók: 40/51 előtöltve"));
        // the hidden frame has no src yet
        assert_eq!(html.matches(" src=").count(), 1);
    }

    #[test]
    fn a_downloaded_image_replaces_the_iframes() {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.image = Some(image_url("route1", 7));
        v.dist_text = "3.00".into();
        let html = render(v);
        assert!(html.contains("<img"), "{html}");
        assert!(
            html.contains(&image_url("route1", 7)),
            "the image comes from the svimg protocol: {html}"
        );
        assert!(
            !html.contains("<iframe"),
            "no embed when the image is local"
        );
        assert!(html.contains("Letöltött Google Street View panoráma"));
    }

    #[test]
    fn download_controls_and_progress_are_rendered() {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.can_download = true;
        v.downloading = true;
        v.download_fraction = 0.4;
        v.note = Some("Képek letöltése: 40/100 (40%)".into());
        let html = render(v);
        assert!(html.contains("sv-btn--download"));
        assert!(html.contains("aria-pressed=\"true\""));
        assert!(html.contains("Street View letöltés leállítása"));
        assert!(html.contains("sv__bar"));
        assert!(html.contains("width: 40.0%"));
        assert!(html.contains("40/100"));
        // the download button is hidden without the capability
        let plain = render(PanelView::new(PanelStatus::Ok));
        assert!(!plain.contains("sv-btn--download"));
    }

    #[test]
    fn offline_state_offers_the_download_action() {
        let mut v = PanelView::new(PanelStatus::ImageMissing);
        v.can_download = true;
        let html = render(v);
        assert!(html.contains("Ez a szakasz még nincs letöltve"));
        assert!(html.contains("Képek letöltése"));
        assert!(html.contains("Beállítások megnyitása"));
        // without a key the download cannot start, only the settings are offered
        let html = render(PanelView::new(PanelStatus::ImageMissing));
        assert!(!html.contains("Képek letöltése"));
        assert!(html.contains("Beállítások megnyitása"));
    }

    #[test]
    fn size_classes_and_button_labels() {
        let mut v = PanelView::new(PanelStatus::Ok);
        assert!(render(v.clone()).contains("sv--compact"));
        assert!(render(v.clone()).contains("Térkép és Street View cseréje"));
        v.large = true;
        let html = render(v);
        assert!(html.contains("sv--large"));
        assert!(html.contains("a térkép lesz nagy"));
        assert!(html.contains("Street View bezárása") && html.contains("Street View beállítások"));
    }

    #[test]
    fn debug_of_the_view_never_contains_the_key() {
        let mut v = PanelView::new(PanelStatus::Ok);
        v.frames = [Some(url()), Some(url())];
        let dbg = format!("{v:?} {v:#?}");
        assert!(!dbg.contains("AIzaTESTKEY"), "{dbg}");
        let _ = Theme::Dark;
    }

    #[test]
    fn status_helpers() {
        assert!(PanelStatus::Ok.title().is_none() && PanelStatus::Ok.hint().is_none());
        assert!(PanelStatus::Error(SvError::KeyRejected).offers_settings());
        assert!(!PanelStatus::Error(SvError::Network).offers_settings());
        assert!(PanelStatus::ImageMissing.offers_settings());
        assert!(PanelStatus::ImageMissing.offers_download());
        assert!(!PanelStatus::NoKey.offers_download());
        assert_eq!(
            PanelStatus::NoCoverage.readout(),
            Some("Itt nincs Street View")
        );
        assert_eq!(
            PanelStatus::ImageMissing.readout(),
            Some("Nincs letöltött kép")
        );
    }
}
