//! Map component. Tiles are requested from the app's own tile proxy (a Dioxus custom protocol
//! named `tiles` in `ride-app`, which fetches upstream with a proper User-Agent and caches on
//! disk). [`tile_url`] is the URL of one tile; the webview-specific base
//! (`http://tiles.localhost` on Windows, `tiles://localhost` on macOS/Linux,
//! `https://tiles.localhost` on Android) comes from [`ride_ui_kit::protocol`].
//!
//! The map is pure Rust: [`mercator`] holds the projection and tile maths, [`camera`] the
//! viewport/camera maths (fit, pan, wheel zoom, follow, projection), [`route_path`] turns the
//! route into one SVG path per grade colour bucket. `MapView` owns only the interaction state
//! and the markup, so the whole geometry is unit-tested without a browser.
//!
//! Layout model (no per-frame re-layout of the tiles):
//!
//! * every tile is an `<img>` at a fixed position inside `.map__layer`; the layer carries a
//!   single `transform: translate3d(…) scale(…)` that encodes the camera, so panning and
//!   zooming touch one attribute and the browser only re-composites;
//! * the route lives in an SVG whose `<g transform>` is the same world→screen mapping; the
//!   path data is built by [`route_path`] once per route and integer tile zoom (memoised);
//! * markers are projected into screen space, so their size, labels and the pulse animation
//!   never scale with the map.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use ride_model::{MapStyle, RideSnapshot, RideState, Route, RoutePosition};
use ride_ui_kit::palette;
use ride_ui_kit::{Icon, IconName, Segmented};

pub mod camera;
pub mod mercator;
pub mod route_path;
pub mod scale;

pub use camera::{Camera, GeoPoint, ScreenTransform, Viewport};
pub use mercator::{MAX_ZOOM, MIN_ZOOM, TILE_PX};

/// `document::eval`, but a panic inside it cannot take the UI down: `dioxus-desktop` `expect`s a
/// live window when it evaluates (`DesktopDocument::eval`), and Android takes the window away far
/// sooner than a desktop does (backgrounding, a configuration change). Without a measurement the
/// map simply keeps its last viewport.
fn eval_script(script: &str) -> Option<dioxus::document::Eval> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| document::eval(script))).ok()
}
pub use route_path::{build_route_paths, RoutePaths};

/// Component styles; the shell injects this after `ride_ui_kit::CSS`.
pub const CSS: &str = include_str!("../assets/map.css");

/// URL of one tile inside the `tiles` protocol, e.g.
/// `http://tiles.localhost/dark/14/8923/5701.png` on Windows. The app's protocol handler takes
/// `uri().path()` of it and parses that with [`parse_tile_path`], then fetches [`upstream_url`].
pub fn tile_url(style: MapStyle, z: u8, x: u32, y: u32) -> String {
    let s = match style {
        MapStyle::Dark => "dark",
        MapStyle::Light => "light",
        MapStyle::Osm => "osm",
    };
    ride_ui_kit::protocol::url("tiles", &format!("{s}/{z}/{x}/{y}.png"))
}

/// Inverse of [`tile_url`] on the path component (leading `/` allowed).
pub fn parse_tile_path(path: &str) -> Option<(MapStyle, u8, u32, u32)> {
    let mut it = path.trim_start_matches('/').split('/');
    let style = match it.next()? {
        "dark" => MapStyle::Dark,
        "light" => MapStyle::Light,
        "osm" => MapStyle::Osm,
        _ => return None,
    };
    let z = it.next()?.parse().ok()?;
    let x = it.next()?.parse().ok()?;
    let y = it.next()?.trim_end_matches(".png").parse().ok()?;
    Some((style, z, x, y))
}

/// The real upstream tile server URL for a style (used by the proxy only).
///
/// Every style is served from the standard OpenStreetMap raster tiles: the CARTO basemaps
/// (`dark_all` / `light_all`) started returning an "API KEY REQUIRED" watermark tile, so they
/// can no longer be used without a key. The dark and light looks are produced locally by a CSS
/// filter on the tile layer (`map.css`, `.map[data-map-style=…] .map-tile`), which keeps the
/// attribution to OpenStreetMap only and the tile usage policy identical for all styles.
pub fn upstream_url(style: MapStyle, z: u8, x: u32, y: u32) -> String {
    let _ = style;
    format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png")
}

/// Mandatory attribution text for the bottom-right corner of the map.
pub fn attribution(style: MapStyle) -> &'static str {
    let _ = style;
    "© OpenStreetMap contributors"
}

// ───────────────────────────── small helpers ─────────────────────────────

/// Style name used in the proxy path, the tile URL and the `data-map-style` attribute.
fn style_dir(style: MapStyle) -> &'static str {
    match style {
        MapStyle::Dark => "dark",
        MapStyle::Light => "light",
        MapStyle::Osm => "osm",
    }
}

/// Number that distinguishes tiles of different styles in the failed-tile set.
fn style_id(style: MapStyle) -> u8 {
    match style {
        MapStyle::Dark => 0,
        MapStyle::Light => 1,
        MapStyle::Osm => 2,
    }
}

/// Reverse of [`style_dir`]; unknown values are ignored instead of panicking.
fn style_from_value(value: &str) -> Option<MapStyle> {
    match value {
        "dark" => Some(MapStyle::Dark),
        "light" => Some(MapStyle::Light),
        "osm" => Some(MapStyle::Osm),
        _ => None,
    }
}

/// Whether an interpolated position can be projected (a fresh `Ready` snapshot has none yet).
fn usable_position(pos: &RoutePosition) -> bool {
    pos.dist.is_finite()
        && pos.lat.is_finite()
        && pos.lon.is_finite()
        && pos.lat.abs() <= mercator::MAX_LAT
        && (pos.lat != 0.0 || pos.lon != 0.0)
}

/// Position of the rider marker: the snapshot's interpolated position when it is usable,
/// else the route point at `snapshot.dist` (a fresh `Ready` snapshot has no position yet).
pub fn rider_position(route: &Route, snapshot: &RideSnapshot) -> Option<RoutePosition> {
    if route.points.is_empty() || snapshot.state == RideState::Empty {
        return None;
    }
    let pos = snapshot.pos;
    if usable_position(&pos) {
        return Some(RoutePosition {
            heading_deg: camera::normalize_heading(pos.heading_deg),
            ..pos
        });
    }
    let dist = if snapshot.dist.is_finite() {
        snapshot.dist
    } else {
        0.0
    };
    Some(route.locate(dist))
}

/// Position of the ghost marker (an earlier ride replayed on the same route), if the snapshot
/// carries one. Falls back to the route point at the ghost's distance, like the rider.
pub fn ghost_position(route: &Route, snapshot: &RideSnapshot) -> Option<RoutePosition> {
    if route.points.is_empty() || snapshot.state == RideState::Empty {
        return None;
    }
    let ghost = snapshot.ghost.as_ref()?;
    if usable_position(&ghost.pos) {
        return Some(RoutePosition {
            heading_deg: camera::normalize_heading(ghost.pos.heading_deg),
            ..ghost.pos
        });
    }
    ghost
        .dist
        .is_finite()
        .then(|| route.locate(ghost.dist.clamp(0.0, route.total_dist)))
}

/// One `<img>` of the tile grid: its world position inside the transformed layer is fixed,
/// only the layer moves.
#[derive(Clone, Debug, PartialEq)]
struct TileView {
    z: u8,
    x: u32,
    y: u32,
    key: String,
    src: String,
    style: String,
    /// Identifies the tile (including its style) in the failed-tile set.
    fail_key: (u8, u8, u32, u32),
}

/// Page script that reports the map element's size and device pixel ratio whenever it
/// changes. The element id is generated by this component, never user input.
fn resize_script(element_id: &str) -> String {
    format!(
        r#"(function () {{
  var el = document.getElementById("{element_id}");
  if (!el) return;
  var send = function () {{
    dioxus.send([el.clientWidth, el.clientHeight, window.devicePixelRatio || 1]);
  }};
  if (typeof ResizeObserver === "function") {{
    try {{ new ResizeObserver(send).observe(el); }} catch (e) {{}}
  }}
  window.addEventListener("resize", send);
  send();
  return new Promise(function () {{}});
}})();"#
    )
}

/// The rider's heading wedge (north-up, rotated by the heading): a notched arrow head that
/// starts outside the 16 px disc. Fill and paper outline come from `map.css`.
const RIDER_WEDGE: &str = r#"<svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true"><path d="M24 3 32 19 24 15 16 19z"/></svg>"#;

/// The ghost's wedge: the same shape, drawn translucent and outlined (the ghost is a memory of
/// an earlier ride, not a second rider). Fill and outline come from `map.css`.
const GHOST_WEDGE: &str = r#"<svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true"><path d="M24 3 32 19 24 15 16 19z" fill-opacity="0.28" stroke-width="2.5"/></svg>"#;

static MAP_UID: AtomicU64 = AtomicU64::new(1);

/// State of an in-progress drag: pointer start plus the camera it started from, so every
/// move is applied to the *start* camera (no accumulation drift).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    x: f64,
    y: f64,
    center: GeoPoint,
    zoom: f64,
}

fn apply_zoom(
    viewport: Signal<Viewport>,
    zoom: Signal<f64>,
    center: Signal<GeoPoint>,
    following: bool,
    fit_pending: Signal<bool>,
    delta: f64,
    anchor: Option<(f64, f64)>,
) {
    let mut zoom = zoom;
    let mut center = center;
    let mut fit_pending = fit_pending;
    let view = *viewport.peek();
    let camera = Camera {
        center: *center.peek(),
        zoom: *zoom.peek(),
    };
    let (ax, ay) = anchor.unwrap_or(view.center());
    let next = camera::zoom_at(camera, view, ax, ay, delta);
    zoom.set(next.zoom);
    if !following {
        center.set(next.center);
    }
    fit_pending.set(false);
}

#[allow(clippy::too_many_arguments)]
fn recenter_map(
    route: ReadSignal<Option<Arc<Route>>>,
    viewport: Signal<Viewport>,
    zoom: Signal<f64>,
    center: Signal<GeoPoint>,
    free_pan: Signal<bool>,
    fit_pending: Signal<bool>,
    on_follow: EventHandler<bool>,
    rider: Option<RoutePosition>,
) {
    let mut zoom = zoom;
    let mut center = center;
    let mut free_pan = free_pan;
    let mut fit_pending = fit_pending;
    let view = *viewport.peek();
    let current = route.peek().clone();
    let Some(route) = current.as_deref() else {
        return;
    };
    if let Some(pos) = rider {
        // Back to the rider: close enough to see the street, not a whole-route overview.
        if *zoom.peek() < RIDER_ZOOM {
            zoom.set(RIDER_ZOOM);
        }
        let target = camera::follow_center(GeoPoint::from_position(pos), *zoom.peek(), view);
        center.set(target);
        free_pan.set(false);
        fit_pending.set(false);
        on_follow.call(true);
    } else {
        let fit = camera::fit_route(route, view);
        center.set(fit.center);
        zoom.set(fit.zoom);
        free_pan.set(false);
        fit_pending.set(false);
        on_follow.call(false);
    }
}

// ───────────────────────────── MapView ─────────────────────────────

/// Zoom of the picture-in-picture map: a readable rider minimap (about 3 km across at
/// 340 px), instead of fitting a whole route into the small pane.
const COMPACT_ZOOM: f64 = 13.0;

/// Zoom the big map opens with when it follows the rider: about 1.5–2 km across a phone screen,
/// so the rider sees the street they are on. The whole route stays one pinch away.
const RIDER_ZOOM: f64 = 15.0;

/// Full-bleed 2D map: tiles, route polyline (coloured by grade), start/finish flags,
/// rider marker (heading arrow), follow-rider mode, drag to pan / wheel to zoom,
/// style switcher + recenter/follow buttons. `follow` true = camera tracks the rider.
#[component]
pub fn MapView(
    route: ReadSignal<Option<Arc<Route>>>,
    snapshot: ReadSignal<RideSnapshot>,
    style: ReadSignal<MapStyle>,
    follow: ReadSignal<bool>,
    on_style: EventHandler<MapStyle>,
    on_follow: EventHandler<bool>,
    /// Small picture-in-picture mode: keeps zoom/follow, drops the style switcher and the
    /// scale bar/attribution so a few hundred pixels stay readable.
    #[props(default = false)]
    compact: bool,
) -> Element {
    let mut viewport = use_signal(Viewport::default);
    let mut measured = use_signal(|| false);

    // First frame (and server-side rendering): fit with the fallback viewport. The effect
    // below repeats it as soon as the webview reports the real size. The small pane is a
    // rider minimap: it keeps a readable street-level zoom instead of fitting a whole route
    // into a few hundred pixels (which would upscale very coarse tiles). Following the rider
    // opens on the rider too, so the first paint is not a route overview that jumps.
    let initial = use_hook(|| {
        let fit = camera::fit_optional(route.peek().as_deref(), Viewport::default());
        if compact {
            return Camera {
                center: fit.center,
                zoom: COMPACT_ZOOM,
            };
        }
        if *follow.peek() {
            let snapshot_now = snapshot.peek().clone();
            if let Some(pos) = route
                .peek()
                .as_deref()
                .and_then(|route| rider_position(route, &snapshot_now))
            {
                return Camera {
                    center: GeoPoint::from_position(pos),
                    zoom: RIDER_ZOOM,
                };
            }
        }
        fit
    });
    let mut center = use_signal(|| initial.center);
    let mut zoom = use_signal(|| initial.zoom);
    let mut fit_pending = use_signal(|| !compact);
    let mut free_pan = use_signal(|| false);
    let mut seen_route = use_signal(|| Option::<String>::None);
    let mut was_compact = use_signal(|| compact);
    let mut drag = use_signal(|| Option::<Drag>::None);
    let mut failed = use_signal(HashSet::<(u8, u8, u32, u32)>::new);
    let mut was_following = use_signal(|| *follow.peek());
    let mut last_follow_center = use_signal(|| initial.center);
    let root_id = use_hook(|| format!("ride-map-{}", MAP_UID.fetch_add(1, Ordering::Relaxed)));

    // A new route (or the first one) is fitted again, unless the user panned away already.
    use_effect(move || {
        let id = route.read().as_ref().map(|route| route.id.clone());
        if *seen_route.peek() == id {
            return;
        }
        seen_route.set(id.clone());
        if compact {
            fit_pending.set(false);
            if id.is_some() {
                zoom.set(COMPACT_ZOOM);
                free_pan.set(false);
            }
        } else {
            fit_pending.set(id.is_some());
        }
    });

    // Entering the small pane from a route overview: zoom in on the rider, so the mini map is
    // sharp. Going back to the big map keeps whatever camera the rider had.
    use_effect(move || {
        let now = compact;
        let previous = *was_compact.peek();
        if now == previous {
            return;
        }
        was_compact.set(now);
        if now {
            fit_pending.set(false);
            free_pan.set(false);
            if *zoom.peek() < COMPACT_ZOOM {
                zoom.set(COMPACT_ZOOM);
            }
            // Even with follow turned off, the minimap should show where the rider is.
            let snapshot_now = snapshot.read();
            let current = route.read();
            if let Some(route) = current.as_deref() {
                if let Some(pos) = rider_position(route, &snapshot_now) {
                    center.set(GeoPoint::from_position(pos));
                }
            }
        }
    });

    // Fit-to-route: on load, when the route changes and whenever the measured viewport
    // changes while no manual interaction has happened since. In follow mode the camera opens on
    // the **rider** instead (the app is about where you are, not about the whole route): the
    // centred rider at a street-level zoom, which is what "Vissza a kerékpároshoz" gives too.
    use_effect(move || {
        let view = *viewport.read();
        let current = route.read().clone();
        if compact || !*fit_pending.peek() || !view.is_usable() {
            return;
        }
        if let Some(route) = current.as_deref() {
            let rider = if *follow.peek() && !*free_pan.peek() {
                rider_position(route, &snapshot.peek())
            } else {
                None
            };
            match rider {
                Some(pos) => {
                    zoom.set(RIDER_ZOOM);
                    center.set(camera::follow_center(
                        GeoPoint::from_position(pos),
                        RIDER_ZOOM,
                        view,
                    ));
                }
                None => {
                    let fit = camera::fit_route(route, view);
                    center.set(fit.center);
                    zoom.set(fit.zoom);
                }
            }
        }
        // Keep the flag until a real measurement arrived, so the fallback fit is corrected.
        if *measured.peek() {
            fit_pending.set(false);
        }
    });

    // While following, remember where the camera currently is. Leaving follow mode (our own
    // button *or* the shell's M shortcut) then freezes the view there instead of jumping back
    // to a stale free camera.
    use_effect(move || {
        if !*follow.read() || *free_pan.read() {
            return;
        }
        let view = *viewport.read();
        let current_zoom = *zoom.read();
        let snapshot_now = snapshot.read();
        let current = route.read();
        if let Some(route) = current.as_deref() {
            if let Some(pos) = rider_position(route, &snapshot_now) {
                last_follow_center.set(camera::follow_center(
                    GeoPoint::from_position(pos),
                    current_zoom,
                    view,
                ));
            }
        }
    });

    use_effect(move || {
        let wanted = *follow.read();
        let previous = *was_following.peek();
        if !wanted {
            // A drag already moved the camera and asked for follow to stop: keep the drag.
            if previous && !*free_pan.peek() {
                center.set(*last_follow_center.peek());
            }
            // `free_pan` only exists to stop following immediately while the parent applies the
            // new prop; once it is off, the prop alone decides.
            free_pan.set(false);
        }
        if previous != wanted {
            was_following.set(wanted);
        }
    });

    let route_now: Option<Arc<Route>> = route.read().clone();
    let snapshot_now: RideSnapshot = snapshot.read().clone();
    let style_now: MapStyle = *style.read();
    let follow_now: bool = *follow.read();
    let view = *viewport.read();
    let following = follow_now && !*free_pan.read();
    let rider = route_now
        .as_deref()
        .and_then(|route| rider_position(route, &snapshot_now));
    let ghost = route_now
        .as_deref()
        .and_then(|route| ghost_position(route, &snapshot_now));

    // While following, the camera centre is derived from the rider (rider at 60 % height).
    // The memo subscribes to the snapshot, so the map moves with the 10 Hz ride updates.
    let cam_center = use_memo(move || -> GeoPoint {
        let following = *follow.read() && !*free_pan.read();
        if following {
            let snapshot = snapshot.read();
            let current = route.read();
            if let Some(route) = current.as_deref() {
                if let Some(pos) = rider_position(route, &snapshot) {
                    return camera::follow_center(
                        GeoPoint::from_position(pos),
                        *zoom.read(),
                        *viewport.read(),
                    );
                }
            }
        }
        *center.read()
    });

    // Integer tile zoom: rebuilt only when the rounding changes, so the route paths and the
    // tile grid are not recomputed for every pixel of a wheel gesture.
    let tile_z = use_memo(move || mercator::tile_zoom(*zoom.read(), viewport.read().dpr));

    // Route overlay: one path per grade bucket, built per route and per tile zoom.
    let paths = use_memo(move || {
        Arc::new(route_path::build_route_paths(
            route.read().as_deref(),
            *tile_z.read(),
        ))
    });

    // Visible tile grid (with one spare ring), rebuilt when the tile range changes.
    let tiles = use_memo(move || {
        let z = *tile_z.read();
        let camera_center = *cam_center.read();
        let view = *viewport.read();
        let tile_style = *style.read();
        let range = mercator::tiles_for_view(
            camera_center.lat,
            camera_center.lon,
            z,
            view.width,
            view.height,
        );
        let mut out = Vec::with_capacity(range.count());
        for (x, y) in range.iter() {
            out.push(TileView {
                z,
                x,
                y,
                key: format!("{z}/{x}/{y}"),
                src: tile_url(tile_style, z, x, y),
                style: format!(
                    "transform: translate3d({:.3}px, {:.3}px, 0)",
                    f64::from(x) * TILE_PX,
                    f64::from(y) * TILE_PX
                ),
                fail_key: (style_id(tile_style), z, x, y),
            });
        }
        Arc::new(out)
    });

    let theme = camera::style_theme(style_now);
    let style_name = style_dir(style_now);
    let attribution_text = attribution(style_now);

    if route_now.is_none() {
        return rsx! {
            div {
                class: "map map--empty",
                "data-map-style": "{style_name}",
                role: "region",
                "aria-label": "Térkép",
                div { class: "map-empty",
                    Icon { name: IconName::Route, size: 24 }
                    div { class: "map-empty__title", "Nincs betöltött útvonal" }
                    div { class: "map-empty__hint", "Tölts be egy GPX-fájlt vagy illessz be egy bikerouter-linket." }
                }
            }
        };
    }

    let camera = Camera {
        center: *cam_center.read(),
        zoom: *zoom.read(),
    };
    let transform = ScreenTransform::new(camera, view);
    let layer_style = transform.layer_transform();
    // printed scale bar: ground resolution at the camera centre, longest 1-2-5 length <= 96 px
    let scale_bar = scale::meters_per_pixel(camera.center.lat, camera.zoom)
        .and_then(|meters_per_px| scale::scale_bar(meters_per_px, 96.0));

    let start_marker = route_now
        .as_deref()
        .and_then(|route| route.points.first())
        .map(|point| transform.project(GeoPoint::new(point.lat, point.lon)));
    let finish_marker = route_now
        .as_deref()
        .and_then(|route| route.points.last())
        .map(|point| transform.project(GeoPoint::new(point.lat, point.lon)));
    let rider_marker = rider.map(|pos| {
        (
            transform.project(GeoPoint::from_position(pos)),
            camera::normalize_heading(pos.heading_deg),
        )
    });
    let ghost_marker = ghost.map(|pos| {
        (
            transform.project(GeoPoint::from_position(pos)),
            camera::normalize_heading(pos.heading_deg),
        )
    });

    let paths_value = Arc::clone(&paths.read());
    let route_shift_x = transform.origin_x + paths_value.shift_x * transform.scale;
    let route_shift_y = transform.origin_y + paths_value.shift_y * transform.scale;
    let route_transform = format!(
        "translate({route_shift_x:.3},{route_shift_y:.3}) scale({:.6})",
        transform.scale
    );
    let dragging_now = drag.read().is_some();
    let base_class = if dragging_now {
        "map is-dragging"
    } else if following {
        "map is-following"
    } else {
        "map"
    };
    let classes = if compact {
        format!("{base_class} map--compact")
    } else {
        base_class.to_string()
    };
    // one borrow for the whole tile grid; a failed tile stays hidden until its style changes
    let failed_keys = failed.read();

    let mount_element_id = root_id.clone();
    let on_mount = move |_| {
        let Some(mut eval) = eval_script(&resize_script(&mount_element_id)) else {
            return;
        };
        spawn(async move {
            while let Ok(values) = eval.recv::<Vec<f64>>().await {
                let (Some(width), Some(height), Some(dpr)) =
                    (values.first(), values.get(1), values.get(2))
                else {
                    continue;
                };
                let next = Viewport::new(*width, *height, *dpr);
                if *viewport.peek() != next {
                    viewport.set(next);
                    measured.set(true);
                }
            }
        });
    };

    rsx! {
        div {
            id: "{root_id}",
            class: "{classes}",
            "data-map-style": "{style_name}",
            role: "region",
            "aria-label": "Térkép",
            tabindex: "0",
            onmounted: on_mount,
            onmousedown: move |event: Event<MouseData>| {
                if event.trigger_button() != Some(MouseButton::Primary) {
                    return;
                }
                let point = event.element_coordinates();
                if following {
                    free_pan.set(true);
                    on_follow.call(false);
                }
                drag.set(Some(Drag {
                    x: point.x,
                    y: point.y,
                    center: *cam_center.peek(),
                    zoom: *zoom.peek(),
                }));
                fit_pending.set(false);
                event.prevent_default();
            },
            onmousemove: move |event: Event<MouseData>| {
                let Some(active) = *drag.peek() else {
                    return;
                };
                let point = event.element_coordinates();
                let moved = camera::pan_to(
                    active.center,
                    active.zoom,
                    point.x - active.x,
                    point.y - active.y,
                );
                center.set(moved.center);
            },
            onmouseup: move |_| {
                if drag.peek().is_some() {
                    drag.set(None);
                }
            },
            onmouseleave: move |_| {
                if drag.peek().is_some() {
                    drag.set(None);
                }
            },
            onwheel: move |event: Event<WheelData>| {
                event.prevent_default();
                let delta = event.delta();
                let mode = match delta {
                    WheelDelta::Pixels(_) => 0u8,
                    WheelDelta::Lines(_) => 1,
                    WheelDelta::Pages(_) => 2,
                };
                let pixels = mercator::wheel_pixels(mode, delta.strip_units().y);
                let step = mercator::wheel_zoom_delta(pixels);
                if step == 0.0 {
                    return;
                }
                let point = event.element_coordinates();
                let view = *viewport.peek();
                let camera = Camera {
                    center: *cam_center.peek(),
                    zoom: *zoom.peek(),
                };
                let next = camera::zoom_at(camera, view, point.x, point.y, step);
                zoom.set(next.zoom);
                if !following {
                    center.set(next.center);
                }
                fit_pending.set(false);
            },
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();
                let zoom_step = match &key {
                    Key::Character(text) if text == "+" || text == "=" => Some(1.0),
                    Key::Character(text) if text == "-" || text == "_" => Some(-1.0),
                    _ => None,
                };
                if let Some(step) = zoom_step {
                    event.prevent_default();
                    apply_zoom(viewport, zoom, center, following, fit_pending, step, None);
                } else if let Key::Character(text) = &key {
                    if text == "0" {
                        event.prevent_default();
                        recenter_map(
                            route,
                            viewport,
                            zoom,
                            center,
                            free_pan,
                            fit_pending,
                            on_follow,
                            rider,
                        );
                    }
                }
            },
            div { class: "map__canvas",
                div { class: "map__layer", style: "{layer_style}",
                    for (index, tile) in tiles.read().iter().enumerate() {
                        img {
                            key: "{tile.key}",
                            class: if failed_keys.contains(&tile.fail_key) { "map-tile is-failed" } else { "map-tile" },
                            src: "{tile.src}",
                            style: "{tile.style}",
                            alt: "",
                            draggable: "false",
                            decoding: "async",
                            "aria-hidden": "true",
                            onerror: move |_| {
                                if let Some(view) = tiles.peek().get(index) {
                                    let mut set = failed.write();
                                    // a long session must not grow this without a bound
                                    if set.len() > 4096 {
                                        set.clear();
                                    }
                                    set.insert(view.fail_key);
                                }
                            },
                        }
                    }
                }
                svg { class: "map-route", "aria-hidden": "true",
                    g { transform: "{route_transform}",
                        if !paths_value.casing.is_empty() {
                            path { class: "map-route__casing", d: "{paths_value.casing}" }
                        }
                        for (bucket, data) in paths_value.buckets.iter() {
                            path {
                                key: "{bucket}",
                                class: "map-route__seg",
                                d: "{data}",
                                style: format!("stroke: {}", palette::grade_bucket_color(*bucket, theme)),
                            }
                        }
                    }
                }
                div { class: "map-markers",
                    if let Some((x, y)) = start_marker {
                        div {
                            class: "map-marker map-marker--start",
                            style: "transform: translate({x:.1}px, {y:.1}px)",
                            title: "Rajt",
                            div { class: "map-marker__body",
                                span { class: "map-pin map-pin--start" }
                                span { class: "map-marker__badge", "Rajt" }
                            }
                        }
                    }
                    if let Some((x, y)) = finish_marker {
                        div {
                            class: "map-marker map-marker--finish",
                            style: "transform: translate({x:.1}px, {y:.1}px)",
                            title: "Cél",
                            div { class: "map-marker__body",
                                span { class: "map-pin map-pin--finish" }
                                span { class: "map-marker__badge", "Cél" }
                            }
                        }
                    }
                    if let Some(((x, y), heading)) = ghost_marker {
                        div {
                            class: "map-marker map-marker--ghost",
                            style: "transform: translate({x:.1}px, {y:.1}px)",
                            role: "img",
                            "aria-label": "Szellem",
                            title: "Szellem",
                            div {
                                class: "map-ghost__wedge",
                                style: "transform: rotate({heading:.1}deg)",
                                dangerous_inner_html: GHOST_WEDGE,
                            }
                            div { class: "map-ghost__dot" }
                        }
                    }
                    if let Some(((x, y), heading)) = rider_marker {
                        div {
                            class: "map-marker map-marker--rider",
                            style: "transform: translate({x:.1}px, {y:.1}px)",
                            role: "img",
                            "aria-label": "Jelenlegi pozíció",
                            title: "Jelenlegi pozíció",
                            div {
                                class: "map-rider__wedge",
                                style: "transform: rotate({heading:.1}deg)",
                                dangerous_inner_html: RIDER_WEDGE,
                            }
                            div { class: "map-rider__dot" }
                        }
                    }
                }
            }
            div {
                class: "map-controls",
                onmousedown: move |event: Event<MouseData>| event.stop_propagation(),
                onwheel: move |event: Event<WheelData>| event.stop_propagation(),
                div { class: "map-controls__style",
                    Segmented {
                        options: vec![
                            ("dark".to_string(), "Sötét".to_string()),
                            ("light".to_string(), "Világos".to_string()),
                            ("osm".to_string(), "OSM".to_string()),
                        ],
                        value: style_name.to_string(),
                        on_change: move |value: String| {
                            if let Some(style) = style_from_value(&value) {
                                on_style.call(style);
                            }
                        },
                        aria_label: "Térképstílus".to_string(),
                        small: true,
                    }
                }
                div { class: "btn-group btn-group--v map-controls__zoom", role: "group", "aria-label": "Nagyítás",
                    button {
                        class: "btn btn--icon",
                        r#type: "button",
                        "aria-label": "Nagyítás",
                        title: "Nagyítás",
                        onclick: move |_| apply_zoom(viewport, zoom, center, following, fit_pending, 1.0, None),
                        Icon { name: IconName::Plus, size: 18 }
                    }
                    button {
                        class: "btn btn--icon",
                        r#type: "button",
                        "aria-label": "Kicsinyítés",
                        title: "Kicsinyítés",
                        onclick: move |_| apply_zoom(viewport, zoom, center, following, fit_pending, -1.0, None),
                        Icon { name: IconName::Minus, size: 18 }
                    }
                }
                div { class: "btn-group btn-group--v map-controls__follow", role: "group", "aria-label": "Követés",
                    button {
                        class: if following { "btn btn--icon is-active" } else { "btn btn--icon" },
                        r#type: "button",
                        "aria-label": "Követés",
                        title: if following { "Követés kikapcsolása" } else { "Követés bekapcsolása" },
                        aria_pressed: if following { "true" } else { "false" },
                        onclick: move |_| {
                            if following {
                                // the freeze happens in the follow effect once the prop flips
                                on_follow.call(false);
                            } else {
                                free_pan.set(false);
                                on_follow.call(true);
                            }
                        },
                        Icon { name: IconName::Locate, size: 18 }
                    }
                    button {
                        class: "btn btn--icon",
                        r#type: "button",
                        "aria-label": "Vissza a kerékpároshoz",
                        title: "Vissza a kerékpároshoz",
                        onclick: move |_| recenter_map(route, viewport, zoom, center, free_pan, fit_pending, on_follow, rider),
                        Icon { name: IconName::Compass, size: 18 }
                    }
                }
            }
            if let Some(bar) = scale_bar {
                div { class: "map-scale", role: "note", "aria-label": "Méretarány",
                    span { class: "map-scale__label", "{bar.label}" }
                    span { class: "map-scale__bar", style: "width: {bar.px:.0}px" }
                }
            }
            div { class: "map-attrib", role: "note", "{attribution_text}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::testing::straight_route;
    use std::collections::HashSet;

    #[test]
    fn tile_urls_round_trip_through_the_proxy_path_parser() {
        // The base is platform specific (`http://tiles.localhost` on Windows,
        // `tiles://localhost` elsewhere, `https://…` on Android); the path never is.
        let base = ride_ui_kit::protocol::base("tiles");
        for style in [MapStyle::Dark, MapStyle::Light, MapStyle::Osm] {
            for (z, x, y) in [(3u8, 0u32, 0u32), (14, 8923, 5701), (19, 524_287, 524_287)] {
                let url = tile_url(style, z, x, y);
                assert!(url.starts_with(&format!("{base}/")), "{url}");
                let path = url.trim_start_matches(&base);
                assert_eq!(parse_tile_path(path), Some((style, z, x, y)), "{url}");
                assert_eq!(parse_tile_path(&format!("/{path}")), Some((style, z, x, y)));
            }
        }
        assert_eq!(
            tile_url(MapStyle::Osm, 5, 17, 11),
            format!("{base}/osm/5/17/11.png")
        );
    }

    #[test]
    fn malformed_tile_paths_are_rejected() {
        for bad in [
            "",
            "/",
            "//",
            "nope/1/2/3.png",
            "dark/x/2/3.png",
            "dark/1/2",
            "dark/1/2/notanumber.png",
            "light/1/-2/3.png",
        ] {
            assert_eq!(parse_tile_path(bad), None, "{bad:?}");
        }
        // the trailing extension is optional and a leading slash is allowed
        assert_eq!(
            parse_tile_path("/dark/14/8923/5701"),
            Some((MapStyle::Dark, 14, 8923, 5701))
        );
    }

    #[test]
    fn upstream_urls_are_the_documented_tile_server() {
        // One keyless source for every style; the look is a local CSS treatment.
        for style in [MapStyle::Dark, MapStyle::Light, MapStyle::Osm] {
            assert_eq!(
                upstream_url(style, 9, 5, 6),
                "https://tile.openstreetmap.org/9/5/6.png",
                "{style:?}"
            );
            assert_eq!(attribution(style), "© OpenStreetMap contributors");
        }
    }

    #[test]
    fn style_helpers_round_trip() {
        for style in [MapStyle::Dark, MapStyle::Light, MapStyle::Osm] {
            assert_eq!(style_from_value(style_dir(style)), Some(style));
            assert_ne!(style_id(style), style_id(MapStyle::Dark) ^ 0xff);
        }
        assert_eq!(style_from_value("satellite"), None);
        assert_eq!(style_from_value(""), None);
        assert_eq!(style_id(MapStyle::Dark), 0);
        assert_eq!(style_id(MapStyle::Light), 1);
        assert_eq!(style_id(MapStyle::Osm), 2);
    }

    #[test]
    fn rider_position_prefers_the_snapshot_and_falls_back_to_the_route() {
        let route = straight_route(&[(1000.0, 4.0)], 100.0);

        let empty = RideSnapshot::empty();
        assert_eq!(rider_position(&route, &empty), None);

        let mut ready = RideSnapshot::empty();
        ready.state = RideState::Ready;
        let start = rider_position(&route, &ready).expect("start position");
        assert!((start.dist).abs() < 1e-9);
        assert!((start.lat - 47.5).abs() < 1e-6);

        let mut riding = ready.clone();
        riding.state = RideState::Riding;
        riding.dist = 500.0;
        riding.pos = route.locate(500.0);
        let riding_pos = rider_position(&route, &riding).expect("riding position");
        assert!((riding_pos.dist - 500.0).abs() < 1e-6);

        // a snapshot whose position is still the default (0, 0) falls back to `dist`
        let mut broken = riding.clone();
        broken.pos = RoutePosition::default();
        broken.pos.lat = 0.0;
        broken.pos.lon = 0.0;
        let fallback = rider_position(&route, &broken).expect("fallback position");
        assert!((fallback.dist - 500.0).abs() < 1e-6);
        assert!(fallback.lat > 40.0);

        // NaN in the snapshot must not poison the projection
        let mut nan = riding;
        nan.pos.lat = f64::NAN;
        nan.pos.heading_deg = f64::NAN;
        let safe = rider_position(&route, &nan).expect("safe position");
        assert!((safe.dist - 500.0).abs() < 1e-6);

        // an empty route has no position at all
        let mut no_points = straight_route(&[(0.0, 0.0)], 0.0);
        no_points.points.clear();
        assert_eq!(rider_position(&no_points, &empty), None);
        assert_eq!(rider_position(&route, &RideSnapshot::empty()), None);
    }

    #[test]
    fn ghost_position_follows_the_snapshot_and_falls_back_to_the_route() {
        let route = straight_route(&[(1_000.0, 4.0)], 100.0);
        let mut snapshot = RideSnapshot::empty();
        assert_eq!(ghost_position(&route, &snapshot), None, "no ghost yet");

        snapshot.state = RideState::Riding;
        snapshot.ghost = Some(ride_model::GhostLive {
            dist: 400.0,
            pos: route.locate(400.0),
            delta_s: Some(12.0),
        });
        let ghost = ghost_position(&route, &snapshot).expect("ghost position");
        assert!((ghost.dist - 400.0).abs() < 1e-6);
        assert!((ghost.lat - 47.5).abs() < 1e-6);

        // a ghost whose position is still the default falls back to its distance
        snapshot.ghost = Some(ride_model::GhostLive {
            dist: 700.0,
            pos: RoutePosition::default(),
            delta_s: None,
        });
        let ghost = ghost_position(&route, &snapshot).expect("fallback");
        assert!((ghost.dist - 700.0).abs() < 1e-6);
        assert!(ghost.lat > 40.0);

        // NaN distance: nothing to draw, no panic
        snapshot.ghost = Some(ride_model::GhostLive {
            dist: f64::NAN,
            pos: RoutePosition::default(),
            delta_s: None,
        });
        assert_eq!(ghost_position(&route, &snapshot), None);
        // an empty route has no ghost either
        let mut no_points = straight_route(&[(0.0, 0.0)], 0.0);
        no_points.points.clear();
        assert_eq!(ghost_position(&no_points, &snapshot), None);
    }

    #[test]
    fn map_css_stays_on_the_design_tokens() {
        assert!(!CSS.is_empty(), "map.css is embedded");
        assert!(CSS.contains(".map-tile"), "tile styles present");
        assert!(CSS.contains("translate3d"), "tiles move with transforms");
        // One pixel larger than mercator::TILE_PX so fractional zoom cannot show seams.
        assert!(
            CSS.contains("257px"),
            "the tile size is TILE_PX + 1 px of overlap"
        );
        assert!(!CSS.contains('#'), "no hex colours, use tokens");
        assert!(!CSS.contains("rgb"), "no rgb()/rgba() colours, use tokens");
        // the design direction: flat, hairline, square
        for banned in [
            "backdrop-filter",
            "blur(",
            "box-shadow",
            "text-shadow",
            "gradient",
            "text-transform",
            "letter-spacing",
        ] {
            assert!(
                !CSS.contains(banned),
                "{banned} is not part of the map's look"
            );
        }

        // every custom property the map uses must exist in the kit's stylesheet
        let mut rest = CSS;
        let mut seen = 0usize;
        while let Some(at) = rest.find("var(--") {
            let after = &rest[at + 6..];
            let Some(end) = after.find([')', ',']) else {
                break;
            };
            let name = &after[..end];
            if !name.is_empty() {
                assert!(
                    ride_ui_kit::CSS.contains(&format!("--{name}:")),
                    "--{name} is not defined by ride-ui-kit"
                );
                seen += 1;
            }
            rest = &after[end..];
        }
        assert!(seen >= 20, "only {seen} token uses found");
    }

    #[test]
    fn map_css_declares_the_expected_parts() {
        for selector in [
            ".map {",
            ".map__canvas",
            ".map__layer",
            ".map-route",
            ".map-route__casing",
            ".map-route__seg",
            ".map-marker",
            ".map-ghost__dot",
            ".map-ghost__wedge",
            ".map-rider__dot",
            ".map-rider__wedge",
            ".map-scale",
            ".map-controls",
            ".map-attrib",
            ".map-empty",
            ".map-tile",
        ] {
            assert!(CSS.contains(selector), "missing {selector}");
        }
        // no animation frames on the tiles themselves
        assert!(
            !CSS.contains(".map-tile { animation"),
            "tiles must not animate"
        );
        // functional motion only: the map has no decorative animation at all
        assert!(!CSS.contains("@keyframes"), "no decorative animation");
        assert!(!CSS.contains("animation:"), "no decorative animation");
    }

    #[test]
    fn the_resize_script_only_embeds_our_own_element_id() {
        let script = resize_script("ride-map-7");
        assert!(script.contains("getElementById(\"ride-map-7\")"));
        assert!(script.contains("ResizeObserver"));
        assert!(script.contains("devicePixelRatio"));
        assert!(script.contains("dioxus.send"));
        assert!(!script.contains("fetch("));
    }

    #[test]
    fn failed_tile_keys_keep_styles_apart() {
        let mut failed: HashSet<(u8, u8, u32, u32)> = HashSet::new();
        failed.insert((style_id(MapStyle::Dark), 14, 8923, 5701));
        assert!(failed.contains(&(0, 14, 8923, 5701)));
        assert!(!failed.contains(&(style_id(MapStyle::Light), 14, 8923, 5701)));
        assert!(!failed.contains(&(style_id(MapStyle::Dark), 15, 8923, 5701)));
    }
}
