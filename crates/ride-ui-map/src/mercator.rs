//! Web-Mercator projection and slippy-tile indexing.
//!
//! Everything in this module is pure arithmetic: no DOM, no signals, no I/O. The map
//! component only moves pixels around, all of the maths lives here so it can be unit-tested
//! (round-trips, clamps at the poles, tile ranges for a viewport, zoom limits).

/// Edge length of one tile in CSS pixels. The tile proxy serves 256×256 PNGs and the
/// `.map-tile` rule of `assets/map.css` uses the same size; a unit test keeps them in sync.
pub const TILE_PX: f64 = 256.0;

/// Lowest zoom the map allows (the whole world is visible, useful for orientation).
pub const MIN_ZOOM: f64 = 3.0;
/// Highest zoom the map allows (the upstream tile servers stop at 19).
pub const MAX_ZOOM: f64 = 19.0;

/// Latitude limit of the square Web-Mercator world: `atan(sinh(π))`, ±85.05112878°.
pub const MAX_LAT: f64 = 85.051_128_779_806_58;

/// Device pixel ratio from which the map requests one zoom level deeper tiles so that they
/// stay crisp on a HiDPI panel (at exactly 2× one tile pixel covers one device pixel).
pub const RETINA_DPR: f64 = 2.0;

/// Vertical wheel movement (CSS pixels) that corresponds to one full zoom level.
/// One mouse notch is ~100 px in Chromium, i.e. a quarter of a level per notch.
pub const WHEEL_PX_PER_ZOOM: f64 = 400.0;

/// Largest zoom change a single wheel event may apply (trackpads emit floods of tiny
/// events; one event must never jump more than a level).
pub const MAX_WHEEL_ZOOM_STEP: f64 = 1.0;

/// Largest viewport edge in CSS pixels that the tile grid accepts. A measured value above
/// this is a layout bug or a hidden element, never a real window.
pub const MAX_VIEWPORT_PX: f64 = 8192.0;

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// Clamps a zoom level into [`MIN_ZOOM`]..=[`MAX_ZOOM`]; NaN becomes [`MIN_ZOOM`].
pub fn clamp_zoom(zoom: f64) -> f64 {
    if zoom.is_finite() {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        MIN_ZOOM
    }
}

/// Clamps a latitude into the drawable Mercator band; NaN becomes 0.
pub fn clamp_lat(lat: f64) -> f64 {
    if lat.is_finite() {
        lat.clamp(-MAX_LAT, MAX_LAT)
    } else {
        0.0
    }
}

/// Wraps a longitude into `[-180, 180)`; NaN becomes 0.
pub fn wrap_lon(lon: f64) -> f64 {
    if !lon.is_finite() {
        return 0.0;
    }
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

/// Width and height of the whole world in pixels at `zoom` (no clamping: the fit maths
/// needs the raw formula).
pub fn world_size(zoom: f64) -> f64 {
    if zoom.is_finite() {
        TILE_PX * 2f64.powf(zoom)
    } else {
        TILE_PX
    }
}

/// Longitude → world X in pixels at `zoom`.
pub fn lon_to_x(lon: f64, zoom: f64) -> f64 {
    (wrap_lon(lon) + 180.0) / 360.0 * world_size(zoom)
}

/// Latitude → world Y in pixels at `zoom` (0 at the north edge, `world_size` at the south).
pub fn lat_to_y(lat: f64, zoom: f64) -> f64 {
    let phi = clamp_lat(lat).to_radians();
    let mercator = (phi.tan() + 1.0 / phi.cos()).ln();
    (1.0 - mercator / std::f64::consts::PI) / 2.0 * world_size(zoom)
}

/// World X in pixels at `zoom` → longitude.
pub fn x_to_lon(x: f64, zoom: f64) -> f64 {
    let size = world_size(zoom);
    if !x.is_finite() || size <= 0.0 {
        return 0.0;
    }
    wrap_lon(x / size * 360.0 - 180.0)
}

/// World Y in pixels at `zoom` → latitude.
pub fn y_to_lat(y: f64, zoom: f64) -> f64 {
    let size = world_size(zoom);
    if !y.is_finite() || size <= 0.0 {
        return 0.0;
    }
    let n = std::f64::consts::PI * (1.0 - 2.0 * y / size);
    clamp_lat(n.sinh().atan().to_degrees())
}

/// Integer zoom level a fractional map zoom is rendered with.
///
/// * the fractional part is drawn by scaling the tile layer, so the level is rounded
///   (down-scaled tiles stay sharp, up-scaled ones are slightly soft — rounding keeps the
///   scale factor in `0.71..=1.41` instead of `1..=2`);
/// * at a device pixel ratio of at least [`RETINA_DPR`] one level deeper tiles are requested,
///   so a tile pixel covers at most one device pixel up to a ratio of 2 (at 3× it covers 1.5:
///   a second extra level would mean four times the tiles from the OSM servers for every view
///   of a phone, which their tile policy asks apps not to do);
/// * the result is clamped into [`MIN_ZOOM`]..=[`MAX_ZOOM`].
pub fn tile_zoom(zoom: f64, dpr: f64) -> u8 {
    let base = clamp_zoom(zoom).round();
    let retina = if dpr.is_finite() && dpr >= RETINA_DPR {
        1.0
    } else {
        0.0
    };
    (base + retina).clamp(MIN_ZOOM, MAX_ZOOM) as u8
}

/// Inclusive tile rectangle covering a viewport, plus the integer zoom it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRange {
    pub z: u8,
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

impl TileRange {
    /// How many tiles the range contains (0 for an empty range).
    pub fn count(&self) -> usize {
        if self.max_x < self.min_x || self.max_y < self.min_y {
            return 0;
        }
        let w = usize::try_from(self.max_x - self.min_x).unwrap_or(0) + 1;
        let h = usize::try_from(self.max_y - self.min_y).unwrap_or(0) + 1;
        w.saturating_mul(h)
    }

    /// True when `(x, y)` is inside the range.
    pub fn contains_tile(&self, x: u32, y: u32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    /// Row-major iterator over the tiles.
    pub fn iter(&self) -> TileIter {
        TileIter {
            x: self.min_x,
            y: self.min_y,
            min_x: self.min_x,
            max_x: self.max_x,
            max_y: self.max_y,
        }
    }
}

/// Iterator returned by [`TileRange::iter`].
#[derive(Clone, Debug)]
pub struct TileIter {
    x: u32,
    y: u32,
    min_x: u32,
    max_x: u32,
    max_y: u32,
}

impl Iterator for TileIter {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<(u32, u32)> {
        if self.max_x < self.min_x || self.y > self.max_y {
            return None;
        }
        let out = (self.x, self.y);
        if self.x < self.max_x {
            self.x += 1;
        } else {
            self.x = self.min_x;
            self.y = self.y.saturating_add(1);
        }
        Some(out)
    }
}

fn sanitize_view_px(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, MAX_VIEWPORT_PX)
    } else {
        0.0
    }
}

/// Tiles needed to fill a `width`×`height` viewport centred on `center_lat`/`center_lon`
/// at integer zoom `z`.
///
/// The range always contains one extra ring of tiles so the half-visible tile at every edge
/// (and any rounding while panning) is present; it is clamped to the world, so a view that
/// crosses the antimeridian or a pole stops at the edge instead of wrapping around.
pub fn tiles_for_view(
    center_lat: f64,
    center_lon: f64,
    z: u8,
    width: f64,
    height: f64,
) -> TileRange {
    let z = z.clamp(MIN_ZOOM as u8, MAX_ZOOM as u8);
    let zf = f64::from(z);
    // z <= 19, so the world has at most 2^19 tiles per axis.
    let count: u32 = 1u32 << z;
    let last = count.saturating_sub(1);

    let cx = lon_to_x(center_lon, zf);
    let cy = lat_to_y(center_lat, zf);
    let half_w = sanitize_view_px(width) / 2.0;
    let half_h = sanitize_view_px(height) / 2.0;

    let tile_index = |px: f64| -> i64 {
        let raw = (px / TILE_PX).floor();
        if raw.is_finite() {
            raw.clamp(-2.0, f64::from(count) + 2.0) as i64
        } else {
            0
        }
    };

    let min_x = (tile_index(cx - half_w) - 1).clamp(0, i64::from(last)) as u32;
    let max_x = (tile_index(cx + half_w) + 1).clamp(0, i64::from(last)) as u32;
    let min_y = (tile_index(cy - half_h) - 1).clamp(0, i64::from(last)) as u32;
    let max_y = (tile_index(cy + half_h) + 1).clamp(0, i64::from(last)) as u32;

    TileRange {
        z,
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

/// Vertical wheel movement in CSS pixels. Chromium reports pixels (`mode == 0`); lines and
/// pages are converted so a Firefox-style event (or a future engine) zooms by a sane amount.
pub fn wheel_pixels(delta_mode: u8, delta_y: f64) -> f64 {
    let scale = match delta_mode {
        1 => 16.0,
        2 => 800.0,
        _ => 1.0,
    };
    finite_or(delta_y, 0.0) * scale
}

/// Wheel pixels → zoom step: wheel up (negative `delta_y`) zooms in, one event is clamped to
/// [`MAX_WHEEL_ZOOM_STEP`].
pub fn wheel_zoom_delta(pixels: f64) -> f64 {
    if !pixels.is_finite() {
        return 0.0;
    }
    (-pixels / WHEEL_PX_PER_ZOOM).clamp(-MAX_WHEEL_ZOOM_STEP, MAX_WHEEL_ZOOM_STEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn mercator_round_trips_lat_lon() {
        for zoom in [0.0, 3.0, 7.5, 14.0, 19.0] {
            for lat in [-85.0, -47.5, 0.0, 19.4, 47.5, 84.9] {
                let y = lat_to_y(lat, zoom);
                let back = y_to_lat(y, zoom);
                assert!(close(back, lat, 1e-9), "lat {lat} at z{zoom}: {back}");
            }
            for lon in [-180.0, -179.9, -90.0, -19.05, 0.0, 19.05, 179.9] {
                let x = lon_to_x(lon, zoom);
                let back = x_to_lon(x, zoom);
                assert!(close(back, lon, 1e-9), "lon {lon} at z{zoom}: {back}");
            }
        }
    }

    #[test]
    fn world_is_a_square_of_projected_pixels() {
        assert_eq!(world_size(0.0), 256.0);
        assert_eq!(world_size(1.0), 512.0);
        assert_eq!(world_size(14.0), 256.0 * 16384.0);
        // the equator sits exactly in the middle of the world
        assert!(close(lat_to_y(0.0, 5.0), world_size(5.0) / 2.0, 1e-9));
        // Greenwich is the middle column, and the projected width is the world width
        assert!(close(lon_to_x(0.0, 5.0), world_size(5.0) / 2.0, 1e-9));
        assert!(close(lon_to_x(180.0, 5.0), 0.0, 1e-9));
        assert!(close(lon_to_x(-180.0, 5.0), 0.0, 1e-9));
    }

    #[test]
    fn poles_and_nonsense_are_clamped_instead_of_panicking() {
        assert_eq!(clamp_lat(90.0), MAX_LAT);
        assert_eq!(clamp_lat(-90.0), -MAX_LAT);
        assert_eq!(clamp_lat(f64::NAN), 0.0);
        assert!(lat_to_y(89.0, 10.0).is_finite());
        assert!(lat_to_y(89.0, 10.0) >= 0.0);
        assert!(lat_to_y(-89.0, 10.0) <= world_size(10.0));
        assert_eq!(y_to_lat(f64::NAN, 3.0), 0.0);
        assert_eq!(x_to_lon(f64::NAN, 3.0), 0.0);
        assert_eq!(wrap_lon(190.0), -170.0);
        assert_eq!(wrap_lon(-190.0), 170.0);
        assert_eq!(wrap_lon(f64::INFINITY), 0.0);
        assert_eq!(clamp_zoom(f64::NAN), MIN_ZOOM);
    }

    #[test]
    fn zoom_is_clamped_to_the_supported_range() {
        assert_eq!(clamp_zoom(0.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(40.0), MAX_ZOOM);
        assert_eq!(clamp_zoom(12.5), 12.5);
    }

    #[test]
    fn tile_zoom_rounds_and_honours_the_pixel_ratio() {
        assert_eq!(tile_zoom(14.0, 1.0), 14);
        assert_eq!(tile_zoom(14.4, 1.0), 14);
        assert_eq!(tile_zoom(14.5, 1.0), 15);
        assert_eq!(tile_zoom(14.6, 1.0), 15);
        // at 2x one extra level, never beyond the last available level
        assert_eq!(tile_zoom(14.0, 2.0), 15);
        assert_eq!(tile_zoom(14.9, 2.0), 16);
        assert_eq!(tile_zoom(19.0, 2.0), 19);
        // out-of-range and nonsense input
        assert_eq!(tile_zoom(0.0, 1.0), MIN_ZOOM as u8);
        assert_eq!(tile_zoom(40.0, 1.0), MAX_ZOOM as u8);
        assert_eq!(tile_zoom(f64::NAN, f64::NAN), MIN_ZOOM as u8);
        assert_eq!(tile_zoom(12.2, 1.25), 12);
    }

    #[test]
    fn tile_range_covers_the_whole_viewport() {
        let (width, height) = (1024.0, 640.0);
        let (lat, lon, z) = (47.5, 19.05, 14u8);
        let range = tiles_for_view(lat, lon, z, width, height);
        let world = world_size(f64::from(z));
        let cx = lon_to_x(lon, f64::from(z));
        let cy = lat_to_y(lat, f64::from(z));

        // every corner of the viewport lies inside the requested range
        let corners = [
            (cx - width / 2.0, cy - height / 2.0),
            (cx + width / 2.0, cy - height / 2.0),
            (cx - width / 2.0, cy + height / 2.0),
            (cx + width / 2.0, cy + height / 2.0),
        ];
        for (px, py) in corners {
            assert!(px >= 0.0 && px <= world && py >= 0.0 && py <= world);
            let (tx, ty) = ((px / TILE_PX).floor() as u32, (py / TILE_PX).floor() as u32);
            assert!(
                range.contains_tile(tx, ty),
                "({tx},{ty}) missing from {range:?}"
            );
        }
        // and it is not absurdly larger than the visible area (4 + one spare ring per axis)
        assert!(range.count() >= 4 * 3, "{} tiles", range.count());
        assert!(range.count() <= 7 * 7, "{} tiles", range.count());
        assert_eq!(range.z, z);
        assert_eq!(range.iter().count(), range.count());
    }

    #[test]
    fn tile_range_is_clamped_at_the_edges_of_the_world() {
        // far west at zoom 3: only 8 tiles per axis, the range must stop at 0
        let range = tiles_for_view(0.0, -179.9, 3, 2400.0, 1400.0);
        assert_eq!(range.min_x, 0);
        assert_eq!(range.z, 3);
        assert!(range.max_x < 8 && range.max_y < 8);
        // zoom is clamped into the supported range
        assert_eq!(tiles_for_view(0.0, 0.0, 0, 800.0, 600.0).z, MIN_ZOOM as u8);
        assert_eq!(tiles_for_view(0.0, 0.0, 30, 800.0, 600.0).z, MAX_ZOOM as u8);
        // a degenerate viewport yields a small, sane range
        let tiny = tiles_for_view(47.5, 19.0, 12, 0.0, 0.0);
        assert!(tiny.count() >= 1 && tiny.count() <= 9, "{tiny:?}");
        // nonsense coordinates do not panic
        let broken = tiles_for_view(f64::NAN, f64::INFINITY, 10, f64::NAN, -5.0);
        assert!(broken.count() >= 1);
    }

    #[test]
    fn empty_and_single_tile_ranges() {
        let empty = TileRange {
            z: 4,
            min_x: 3,
            max_x: 2,
            min_y: 0,
            max_y: 1,
        };
        assert_eq!(empty.count(), 0);
        assert_eq!(empty.iter().count(), 0);
        let one = TileRange {
            z: 4,
            min_x: 1,
            max_x: 1,
            min_y: 2,
            max_y: 2,
        };
        assert_eq!(one.count(), 1);
        assert_eq!(one.iter().collect::<Vec<_>>(), vec![(1, 2)]);
        assert!(one.contains_tile(1, 2));
        assert!(!one.contains_tile(1, 3));
    }

    #[test]
    fn wheel_input_is_normalised_and_limited() {
        assert_eq!(wheel_pixels(0, 100.0), 100.0);
        assert_eq!(wheel_pixels(1, 3.0), 48.0);
        assert_eq!(wheel_pixels(2, 1.0), 800.0);
        assert_eq!(wheel_pixels(9, 10.0), 10.0);
        assert_eq!(wheel_pixels(0, f64::NAN), 0.0);

        assert!(close(wheel_zoom_delta(-100.0), 0.25, 1e-12));
        assert!(close(wheel_zoom_delta(100.0), -0.25, 1e-12));
        assert_eq!(wheel_zoom_delta(-10_000.0), MAX_WHEEL_ZOOM_STEP);
        assert_eq!(wheel_zoom_delta(10_000.0), -MAX_WHEEL_ZOOM_STEP);
        assert_eq!(wheel_zoom_delta(f64::NAN), 0.0);
        assert_eq!(wheel_zoom_delta(0.0), 0.0);
    }
}
