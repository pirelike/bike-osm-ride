//! Camera, viewport and screen-space projection.
//!
//! The map keeps a camera (centre + fractional zoom) and a measured viewport. Everything
//! that turns geo coordinates into CSS pixels — the tile layer transform, the route overlay
//! transform, the marker positions, fit-to-route, panning, wheel zoom and follow mode — is
//! computed here from those two values, so all of it is pure and unit-tested.

use ride_model::{Bounds, MapStyle, Route, RoutePosition, Theme};

use crate::mercator::{self, lat_to_y, lon_to_x, x_to_lon, y_to_lat, MAX_VIEWPORT_PX};

/// Height of the viewport (as a fraction) the rider is kept at while following.
pub const FOLLOW_ANCHOR_Y: f64 = 0.60;

/// Largest zoom the fit-to-route calculation may use (a very short route should not fill the
/// whole screen with a few metres of road).
pub const FIT_MAX_ZOOM: f64 = 17.0;

/// Padding around a fitted route in CSS pixels (also keeps the start/finish labels visible).
pub const FIT_PADDING: f64 = 56.0;

/// Size of the map surface in CSS pixels plus the device pixel ratio of the display.
///
/// The default is a sane fallback for server-side rendering and for the first frame before
/// the webview measured the real element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// CSS pixels, 1..=[`MAX_VIEWPORT_PX`].
    pub width: f64,
    /// CSS pixels, 1..=[`MAX_VIEWPORT_PX`].
    pub height: f64,
    /// Clamped to `0.5..=4.0`.
    pub dpr: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            dpr: 1.0,
        }
    }
}

fn finite_px(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(1.0, MAX_VIEWPORT_PX)
    } else {
        1.0
    }
}

impl Viewport {
    /// Sanitised viewport: non-finite and non-positive values become 1 px, the size is
    /// capped at [`MAX_VIEWPORT_PX`] and the pixel ratio into `0.5..=4.0`.
    pub fn new(width: f64, height: f64, dpr: f64) -> Self {
        Self {
            width: finite_px(width),
            height: finite_px(height),
            dpr: if dpr.is_finite() {
                dpr.clamp(0.5, 4.0)
            } else {
                1.0
            },
        }
    }

    /// A viewport that is large enough to lay tiles out in.
    pub fn is_usable(&self) -> bool {
        self.width >= 32.0 && self.height >= 32.0
    }

    /// Centre of the viewport in CSS pixels.
    pub fn center(&self) -> (f64, f64) {
        (self.width / 2.0, self.height / 2.0)
    }
}

/// A geographic point (degrees, latitude first). Constructors clamp into the drawable world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GeoPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat: mercator::clamp_lat(lat),
            lon: mercator::wrap_lon(lon),
        }
    }

    /// Position of a route lookup (clamped like any other input).
    pub fn from_position(pos: RoutePosition) -> Self {
        Self::new(pos.lat, pos.lon)
    }
}

/// Centre and fractional zoom of the map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: GeoPoint,
    /// `MIN_ZOOM..=MAX_ZOOM` (fractional: the tile layer scales by the remainder).
    pub zoom: f64,
}

impl Camera {
    pub fn new(center: GeoPoint, zoom: f64) -> Self {
        Self {
            center,
            zoom: mercator::clamp_zoom(zoom),
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: GeoPoint::new(47.4979, 19.0402),
            zoom: 12.0,
        }
    }
}

/// Maps world pixels at an integer tile zoom onto CSS pixels of the viewport.
///
/// `screen = world * scale + origin`, where `world` is measured at [`ScreenTransform::zoom`].
/// Tiles live at fixed positions inside a layer that carries exactly this transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenTransform {
    /// Integer zoom of the world coordinate system (the tile zoom).
    pub zoom: u8,
    /// CSS pixels per world pixel (`2^(camera.zoom - zoom)`).
    pub scale: f64,
    pub origin_x: f64,
    pub origin_y: f64,
}

impl ScreenTransform {
    pub fn new(camera: Camera, view: Viewport) -> Self {
        let zoom = mercator::tile_zoom(camera.zoom, view.dpr);
        let zoom_f = f64::from(zoom);
        let scale = 2f64.powf(camera.zoom - zoom_f);
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let cx = lon_to_x(camera.center.lon, zoom_f);
        let cy = lat_to_y(camera.center.lat, zoom_f);
        Self {
            zoom,
            scale,
            origin_x: view.width / 2.0 - cx * scale,
            origin_y: view.height / 2.0 - cy * scale,
        }
    }

    /// World pixel (at [`Self::zoom`]) → CSS pixel inside the map.
    pub fn project_world(&self, x: f64, y: f64) -> (f64, f64) {
        (
            x * self.scale + self.origin_x,
            y * self.scale + self.origin_y,
        )
    }

    /// Geographic point → CSS pixel inside the map.
    pub fn project(&self, point: GeoPoint) -> (f64, f64) {
        let zoom = f64::from(self.zoom);
        self.project_world(lon_to_x(point.lon, zoom), lat_to_y(point.lat, zoom))
    }

    /// CSS pixel inside the map → geographic point (inverse of [`Self::project`]).
    pub fn unproject(&self, x: f64, y: f64) -> GeoPoint {
        let zoom = f64::from(self.zoom);
        let scale = if self.scale.is_finite() && self.scale.abs() > f64::MIN_POSITIVE {
            self.scale
        } else {
            1.0
        };
        let wx = (if x.is_finite() { x } else { 0.0 } - self.origin_x) / scale;
        let wy = (if y.is_finite() { y } else { 0.0 } - self.origin_y) / scale;
        GeoPoint::new(y_to_lat(wy, zoom), x_to_lon(wx, zoom))
    }

    /// CSS `transform` for the tile layer (tiles are placed at their world pixels).
    /// The `transform:` property name is part of the string: it is assigned to an inline
    /// `style` attribute, where a bare value would be an invalid declaration and dropped.
    pub fn layer_transform(&self) -> String {
        format!(
            "transform: translate3d({:.3}px, {:.3}px, 0) scale({:.6})",
            self.origin_x, self.origin_y, self.scale
        )
    }
}

fn bounds_are_usable(bounds: Bounds) -> bool {
    bounds.south.is_finite()
        && bounds.north.is_finite()
        && bounds.west.is_finite()
        && bounds.east.is_finite()
}

/// Camera that shows `bounds` inside the viewport with [`FIT_PADDING`] around it.
///
/// Longitudes are treated as a plain interval (`west <= east`); a route crossing the
/// antimeridian would be fitted across the world, which is acceptable for a regional ride
/// planner and avoids a second wrap-around code path.
pub fn fit_bounds(bounds: Bounds, view: Viewport) -> Camera {
    let (south, north) = if bounds.south.is_finite() && bounds.north.is_finite() {
        (
            mercator::clamp_lat(bounds.south.min(bounds.north)),
            mercator::clamp_lat(bounds.south.max(bounds.north)),
        )
    } else {
        (0.0, 0.0)
    };
    let (west, east) = if bounds.west.is_finite() && bounds.east.is_finite() {
        (bounds.west.min(bounds.east), bounds.west.max(bounds.east))
    } else {
        (0.0, 0.0)
    };

    // extents in world pixels at zoom 0 (256 px for the whole world)
    let x0 = lon_to_x(west, 0.0);
    let x1 = lon_to_x(east, 0.0);
    let y0 = lat_to_y(north, 0.0);
    let y1 = lat_to_y(south, 0.0);
    let span_x = (x1 - x0).abs().max(1e-6);
    let span_y = (y1 - y0).abs().max(1e-6);

    let pad = FIT_PADDING
        .min(view.width * 0.2)
        .min(view.height * 0.2)
        .max(0.0);
    let avail_w = (view.width - 2.0 * pad).max(64.0);
    let avail_h = (view.height - 2.0 * pad).max(64.0);

    // world pixels double with every zoom level, hence log2 of the ratio
    let zoom = (avail_w / span_x).log2().min((avail_h / span_y).log2());
    let zoom = mercator::clamp_zoom(zoom.min(FIT_MAX_ZOOM));

    let center = GeoPoint::new(
        y_to_lat((y0 + y1) / 2.0, 0.0),
        x_to_lon((x0 + x1) / 2.0, 0.0),
    );
    Camera { center, zoom }
}

/// Camera that shows `route` (bounds when they are usable, else the points).
pub fn fit_route(route: &Route, view: Viewport) -> Camera {
    let bounds = route.bounds;
    let degenerate = !bounds_are_usable(bounds)
        || ((bounds.north - bounds.south).abs() + (bounds.east - bounds.west).abs()) <= 1e-9;
    if !degenerate {
        return fit_bounds(bounds, view);
    }

    // Fall back to the geometry: this also covers a single-point route and a route whose
    // bounds were never filled in.
    let mut first: Option<GeoPoint> = None;
    let (mut south, mut north) = (f64::MAX, f64::MIN);
    let (mut west, mut east) = (f64::MAX, f64::MIN);
    for point in &route.points {
        if !(point.lat.is_finite() && point.lon.is_finite()) {
            continue;
        }
        if first.is_none() {
            first = Some(GeoPoint::new(point.lat, point.lon));
        }
        south = south.min(point.lat);
        north = north.max(point.lat);
        west = west.min(point.lon);
        east = east.max(point.lon);
    }
    let Some(point) = first else {
        return Camera::default();
    };
    if south == north && west == east {
        return Camera::new(point, 15.0);
    }
    fit_bounds(
        Bounds {
            south,
            west,
            north,
            east,
        },
        view,
    )
}

/// [`fit_route`] for an optional route: the default camera when there is none.
pub fn fit_optional(route: Option<&Route>, view: Viewport) -> Camera {
    match route {
        Some(route) => fit_route(route, view),
        None => Camera::default(),
    }
}

/// Camera after dragging the map by `(dx, dy)` CSS pixels: the content follows the pointer,
/// so the centre moves by the opposite amount. Longitude wraps, latitude stops at the poles.
pub fn pan_to(start: GeoPoint, zoom: f64, dx: f64, dy: f64) -> Camera {
    let zoom = mercator::clamp_zoom(zoom);
    let (dx, dy) = if dx.is_finite() && dy.is_finite() {
        (dx, dy)
    } else {
        (0.0, 0.0)
    };
    let size = mercator::world_size(zoom);
    let x = lon_to_x(start.lon, zoom) - dx;
    let y = (lat_to_y(start.lat, zoom) - dy).clamp(0.0, size);
    Camera {
        center: GeoPoint::new(y_to_lat(y, zoom), x_to_lon(x, zoom)),
        zoom,
    }
}

/// Zoom by `delta` levels while keeping the geo point under `(anchor_x, anchor_y)` where it
/// is. Every input is sanitised; the zoom stops at the supported limits.
pub fn zoom_at(camera: Camera, view: Viewport, anchor_x: f64, anchor_y: f64, delta: f64) -> Camera {
    let z0 = mercator::clamp_zoom(camera.zoom);
    let step = if delta.is_finite() { delta } else { 0.0 };
    let z1 = mercator::clamp_zoom(z0 + step);
    if (z1 - z0).abs() < 1e-9 {
        return Camera {
            center: camera.center,
            zoom: z0,
        };
    }
    let ax = if anchor_x.is_finite() {
        anchor_x
    } else {
        view.width / 2.0
    };
    let ay = if anchor_y.is_finite() {
        anchor_y
    } else {
        view.height / 2.0
    };

    // world point under the anchor before the zoom …
    let wx = lon_to_x(camera.center.lon, z0) + (ax - view.width / 2.0);
    let wy = lat_to_y(camera.center.lat, z0) + (ay - view.height / 2.0);
    // … scaled by the zoom change, then the camera is placed so it stays under the anchor
    let factor = 2f64.powf(z1 - z0);
    let cx = wx * factor - (ax - view.width / 2.0);
    let cy = wy * factor - (ay - view.height / 2.0);
    Camera {
        center: GeoPoint::new(y_to_lat(cy, z1), x_to_lon(cx, z1)),
        zoom: z1,
    }
}

/// Camera centre that keeps `rider` at [`FOLLOW_ANCHOR_Y`] of the viewport height and
/// horizontally centred (north-up map, so the rider has more road ahead when heading north).
pub fn follow_center(rider: GeoPoint, zoom: f64, view: Viewport) -> GeoPoint {
    let zoom = mercator::clamp_zoom(zoom);
    let size = mercator::world_size(zoom);
    let offset = (FOLLOW_ANCHOR_Y - 0.5) * view.height.max(0.0);
    let y = (lat_to_y(rider.lat, zoom) - offset).clamp(0.0, size);
    GeoPoint::new(y_to_lat(y, zoom), rider.lon)
}

/// Theme whose grade colours are used on top of a map style. The OSM raster style is a light
/// map, so it gets the light palette even in a dark app theme.
pub fn style_theme(style: MapStyle) -> Theme {
    match style {
        MapStyle::Dark => Theme::Dark,
        MapStyle::Light | MapStyle::Osm => Theme::Light,
    }
}

/// Normalises a heading into `0..360` (clockwise from north); nonsense becomes 0.
pub fn normalize_heading(degrees: f64) -> f64 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let heading = degrees % 360.0;
    if heading < 0.0 {
        heading + 360.0
    } else {
        heading
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RoutePoint, RouteSource};

    fn view() -> Viewport {
        Viewport::new(1000.0, 600.0, 1.0)
    }

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    fn route_with(points: Vec<RoutePoint>, bounds: Bounds) -> Route {
        Route {
            id: "test".into(),
            name: "Teszt".into(),
            source: RouteSource::Demo,
            total_dist: points.last().map_or(0.0, |p| p.dist),
            points,
            ascent: 0.0,
            descent: 0.0,
            min_ele: 0.0,
            max_ele: 0.0,
            bounds,
            climbs: vec![],
            waypoints: vec![],
            has_elevation: true,
            segments: Vec::new(),
            timeline: Vec::new(),
        }
    }

    fn point(lat: f64, lon: f64, dist: f64) -> RoutePoint {
        RoutePoint {
            lat,
            lon,
            ele: 100.0,
            dist,
            grade: 0.0,
        }
    }

    #[test]
    fn projection_round_trips_and_centres_the_camera() {
        let camera = Camera::new(GeoPoint::new(47.5, 19.05), 13.4);
        let transform = ScreenTransform::new(camera, view());
        let center = transform.project(camera.center);
        assert!(close(center.0, 500.0, 1e-6), "{center:?}");
        assert!(close(center.1, 300.0, 1e-6), "{center:?}");

        for p in [
            GeoPoint::new(47.51, 19.06),
            GeoPoint::new(47.0, 18.0),
            GeoPoint::new(-33.9, 151.2),
        ] {
            let screen = transform.project(p);
            let back = transform.unproject(screen.0, screen.1);
            assert!(close(back.lat, p.lat, 1e-7), "{p:?} → {back:?}");
            assert!(close(back.lon, p.lon, 1e-7), "{p:?} → {back:?}");
        }
        // a point north-east of the centre is right and above it
        let north_east = transform.project(GeoPoint::new(47.51, 19.06));
        assert!(
            north_east.0 > 500.0 && north_east.1 < 300.0,
            "{north_east:?}"
        );
    }

    #[test]
    fn transform_scale_matches_the_fractional_zoom() {
        // whole zoom: the tile layer is drawn 1:1
        let whole = ScreenTransform::new(Camera::new(GeoPoint::new(47.5, 19.0), 14.0), view());
        assert_eq!(whole.zoom, 14);
        assert!(close(whole.scale, 1.0, 1e-12));
        // half a level: tiles are scaled by sqrt(2), the visible world doubles
        let half = ScreenTransform::new(Camera::new(GeoPoint::new(47.5, 19.0), 14.5), view());
        assert_eq!(half.zoom, 15);
        assert!(close(half.scale, 2f64.powf(-0.5), 1e-12));
        // retina: one level deeper, half size
        let retina = ScreenTransform::new(
            Camera::new(GeoPoint::new(47.5, 19.0), 14.0),
            Viewport::new(1000.0, 600.0, 2.0),
        );
        assert_eq!(retina.zoom, 15);
        assert!(close(retina.scale, 0.5, 1e-12));
    }

    #[test]
    fn fit_puts_the_bounds_inside_the_padded_viewport() {
        let bounds = Bounds {
            south: 47.45,
            west: 18.95,
            north: 47.56,
            east: 19.10,
        };
        let camera = fit_bounds(bounds, view());
        assert!(
            camera.zoom > 10.0 && camera.zoom <= FIT_MAX_ZOOM,
            "{}",
            camera.zoom
        );
        let transform = ScreenTransform::new(camera, view());
        let pad = FIT_PADDING.min(view().width * 0.2).min(view().height * 0.2);
        for corner in [
            GeoPoint::new(bounds.south, bounds.west),
            GeoPoint::new(bounds.north, bounds.east),
            GeoPoint::new(bounds.south, bounds.east),
            GeoPoint::new(bounds.north, bounds.west),
        ] {
            let (x, y) = transform.project(corner);
            assert!(
                x >= pad - 1.0 && x <= view().width - pad + 1.0,
                "x {x} outside the padded box for {corner:?}"
            );
            assert!(
                y >= pad - 1.0 && y <= view().height - pad + 1.0,
                "y {y} outside the padded box for {corner:?}"
            );
        }
    }

    #[test]
    fn fit_handles_short_and_broken_routes() {
        // one point: a close-up of that point, no division by zero
        let single = route_with(
            vec![point(47.5, 19.0, 0.0)],
            Bounds {
                south: 47.5,
                west: 19.0,
                north: 47.5,
                east: 19.0,
            },
        );
        let camera = fit_route(&single, view());
        assert_eq!(camera.zoom, 15.0);
        assert!(close(camera.center.lat, 47.5, 1e-9));
        assert!(close(camera.center.lon, 19.0, 1e-9));

        // nonsense bounds, usable points: falls back to the geometry
        let broken_bounds = route_with(
            vec![point(47.4, 18.9, 0.0), point(47.6, 19.2, 20_000.0)],
            Bounds {
                south: f64::NAN,
                west: 0.0,
                north: 0.0,
                east: 0.0,
            },
        );
        let camera = fit_route(&broken_bounds, view());
        assert!(camera.center.lat > 47.4 && camera.center.lat < 47.6);
        assert!(camera.zoom >= mercator::MIN_ZOOM);

        // no points at all: the default camera, no panic
        let empty = route_with(
            vec![],
            Bounds {
                south: 0.0,
                west: 0.0,
                north: 0.0,
                east: 0.0,
            },
        );
        assert_eq!(fit_route(&empty, view()), Camera::default());
        assert_eq!(fit_optional(None, view()), Camera::default());

        // a tiny route must not zoom past the fit limit
        let tiny = route_with(
            vec![point(47.5, 19.0, 0.0), point(47.5001, 19.0001, 10.0)],
            Bounds {
                south: 47.5,
                west: 19.0,
                north: 47.5001,
                east: 19.0001,
            },
        );
        assert_eq!(fit_route(&tiny, view()).zoom, FIT_MAX_ZOOM);
    }

    #[test]
    fn pan_moves_the_content_with_the_pointer() {
        let start = GeoPoint::new(47.5, 19.0);
        let before = ScreenTransform::new(Camera::new(start, 14.0), view());
        let probe = GeoPoint::new(47.51, 19.01);
        let p0 = before.project(probe);

        let moved = pan_to(start, 14.0, 40.0, -25.0);
        let after = ScreenTransform::new(moved, view());
        let p1 = after.project(probe);
        assert!(close(p1.0 - p0.0, 40.0, 1e-6), "{p0:?} → {p1:?}");
        assert!(close(p1.1 - p0.1, -25.0, 1e-6), "{p0:?} → {p1:?}");
        assert_eq!(moved.zoom, 14.0);

        // nonsense deltas are ignored, not applied
        let same = pan_to(start, 14.0, f64::NAN, 0.0);
        assert!(close(same.center.lat, start.lat, 1e-9));
        assert!(close(
            lon_to_x(same.center.lon, 14.0),
            lon_to_x(start.lon, 14.0),
            1e-6
        ));
    }

    #[test]
    fn pan_stops_at_the_poles_and_wraps_the_antimeridian() {
        // dragging downwards moves the centre north until the world edge stops it
        let north = pan_to(GeoPoint::new(84.0, 179.9), 10.0, 0.0, 100_000.0);
        assert!(
            close(north.center.lat, mercator::MAX_LAT, 1e-6),
            "{:?}",
            north.center
        );
        let south = pan_to(GeoPoint::new(-84.0, 0.0), 10.0, 0.0, -100_000.0);
        assert!(
            close(south.center.lat, -mercator::MAX_LAT, 1e-6),
            "{:?}",
            south.center
        );
        let west = pan_to(GeoPoint::new(0.0, -179.9), 10.0, 100_000.0, 0.0);
        assert!(west.center.lon >= -180.0 && west.center.lon < 180.0);
        assert!(west.center.lon > 0.0, "wrapped east: {:?}", west.center);
    }

    #[test]
    fn zoom_at_keeps_the_anchor_pixel_fixed() {
        for (anchor_x, anchor_y) in [(500.0, 300.0), (0.0, 0.0), (990.0, 590.0), (250.0, 400.0)] {
            for delta in [0.4, -0.4, 1.5, -2.0] {
                let camera = Camera::new(GeoPoint::new(47.5, 19.05), 13.0);
                let before = ScreenTransform::new(camera, view());
                let geo = before.unproject(anchor_x, anchor_y);
                let after = zoom_at(camera, view(), anchor_x, anchor_y, delta);
                let transform = ScreenTransform::new(after, view());
                let (x, y) = transform.project(geo);
                assert!(
                    close(x, anchor_x, 2e-4) && close(y, anchor_y, 2e-4),
                    "delta {delta}: {geo:?} at ({x}, {y}) instead of ({anchor_x}, {anchor_y})"
                );
            }
        }
    }

    #[test]
    fn zoom_is_limited_and_sanitised() {
        let camera = Camera::new(GeoPoint::new(47.5, 19.0), 18.6);
        let after = zoom_at(camera, view(), 500.0, 300.0, 5.0);
        assert_eq!(after.zoom, mercator::MAX_ZOOM);
        let back = zoom_at(
            Camera::new(GeoPoint::new(47.5, 19.0), 3.2),
            view(),
            0.0,
            0.0,
            -5.0,
        );
        assert_eq!(back.zoom, mercator::MIN_ZOOM);
        let nan = zoom_at(camera, view(), f64::NAN, f64::NAN, f64::NAN);
        assert_eq!(nan, camera);
        // already at the limit: nothing changes
        let limited = Camera::new(GeoPoint::new(47.5, 19.0), mercator::MAX_ZOOM);
        assert_eq!(zoom_at(limited, view(), 1.0, 2.0, 1.0), limited);
    }

    #[test]
    fn follow_keeps_the_rider_at_sixty_percent_height() {
        for height in [600.0, 720.0, 1000.0] {
            let v = Viewport::new(1000.0, height, 1.0);
            let rider = GeoPoint::new(47.5123, 19.0432);
            let zoom = 15.0;
            let center = follow_center(rider, zoom, v);
            let transform = ScreenTransform::new(Camera::new(center, zoom), v);
            let (x, y) = transform.project(rider);
            assert!(close(x, v.width / 2.0, 1e-6), "{x}");
            assert!(
                close(y, v.height * FOLLOW_ANCHOR_Y, 1e-6),
                "{y} at h={height}"
            );
            // the camera looks slightly ahead of the rider (north on a north-up map)
            assert!(center.lat > rider.lat);
        }
    }

    #[test]
    fn viewport_and_theme_helpers_are_safe() {
        let broken = Viewport::new(f64::NAN, -5.0, 0.0);
        assert_eq!(broken.width, 1.0);
        assert_eq!(broken.height, 1.0);
        assert_eq!(broken.dpr, 0.5);
        assert!(!broken.is_usable());
        assert_eq!(Viewport::default(), Viewport::new(1280.0, 720.0, 1.0));
        assert_eq!(Viewport::new(1e9, 1e9, 1e9).width, MAX_VIEWPORT_PX);
        assert_eq!(Viewport::new(100.0, 50.0, 2.0).center(), (50.0, 25.0));

        assert_eq!(style_theme(MapStyle::Dark), Theme::Dark);
        assert_eq!(style_theme(MapStyle::Light), Theme::Light);
        assert_eq!(style_theme(MapStyle::Osm), Theme::Light);

        assert_eq!(normalize_heading(0.0), 0.0);
        assert_eq!(normalize_heading(370.0), 10.0);
        assert_eq!(normalize_heading(-10.0), 350.0);
        assert_eq!(normalize_heading(360.0), 0.0);
        assert_eq!(normalize_heading(f64::NAN), 0.0);
    }

    #[test]
    fn layer_transform_is_a_css_transform() {
        let transform = ScreenTransform::new(Camera::new(GeoPoint::new(47.5, 19.0), 12.0), view());
        let css = transform.layer_transform();
        // It goes into an inline style attribute, so the property name must be present.
        assert!(css.starts_with("transform: translate3d("), "{css}");
        assert!(css.contains("px, "), "{css}");
        assert!(css.ends_with(')'), "{css}");
        assert!(css.contains("scale(1.000000)"), "{css}");
        assert_eq!(crate::mercator::TILE_PX, 256.0);
    }
}
