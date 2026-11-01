//! Small geodesy and hashing helpers shared by the route pipeline.

/// Mean Earth radius in metres (IUGG), used by the haversine formula.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Great-circle distance between two WGS-84 points in metres (haversine).
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

/// Squared distance in m² on a local equirectangular plane. Good enough (error well
/// below 1 %) for "which of these points is nearest" questions inside a route.
pub fn approx_dist2_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let mid = ((lat1 + lat2) / 2.0).to_radians().cos();
    let mut dlon = lon2 - lon1;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    let dx = dlon.to_radians() * mid * EARTH_RADIUS_M;
    let dy = (lat2 - lat1).to_radians() * EARTH_RADIUS_M;
    dx * dx + dy * dy
}

/// `true` for finite coordinates inside the WGS-84 range.
pub fn is_valid_coord(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

/// Linear interpolation of a longitude that stays correct across the antimeridian.
pub fn lerp_lon(a: f64, b: f64, t: f64) -> f64 {
    let mut d = b - a;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    let mut r = a + d * t;
    if r > 180.0 {
        r -= 360.0;
    } else if r < -180.0 {
        r += 360.0;
    }
    r
}

/// 64-bit FNV-1a hash (public domain algorithm), used for stable route ids.
#[derive(Debug, Clone, Copy)]
pub struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_known_values() {
        // One degree of latitude is ~111.2 km.
        let d = haversine_m(47.0, 19.0, 48.0, 19.0);
        assert!((d - 111_195.0).abs() < 100.0, "got {d}");
        assert_eq!(haversine_m(47.5, 19.0, 47.5, 19.0), 0.0);
        // Budapest (Deák tér) - Vienna (Stephansplatz) is about 214 km.
        let d = haversine_m(47.4979, 19.0540, 48.2082, 16.3738);
        assert!((d - 214_000.0).abs() < 3_000.0, "got {d}");
    }

    #[test]
    fn approx_matches_haversine_locally() {
        let exact = haversine_m(47.50, 19.00, 47.52, 19.03);
        let approx = approx_dist2_m(47.50, 19.00, 47.52, 19.03).sqrt();
        assert!((exact - approx).abs() / exact < 0.005);
    }

    #[test]
    fn coordinate_validation() {
        assert!(is_valid_coord(47.5, 19.0));
        assert!(!is_valid_coord(91.0, 19.0));
        assert!(!is_valid_coord(47.5, 181.0));
        assert!(!is_valid_coord(f64::NAN, 19.0));
        assert!(!is_valid_coord(47.5, f64::INFINITY));
    }

    #[test]
    fn lerp_lon_crosses_antimeridian() {
        let m = lerp_lon(179.0, -179.0, 0.5);
        assert!((m.abs() - 180.0).abs() < 1e-9, "got {m}");
        assert!((lerp_lon(10.0, 20.0, 0.25) - 12.5).abs() < 1e-12);
    }

    #[test]
    fn fnv1a_reference_vectors() {
        // Published FNV-1a 64-bit test vectors.
        let mut h = Fnv1a::new();
        assert_eq!(h.finish(), 0xcbf2_9ce4_8422_2325);
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
        let mut h = Fnv1a::new();
        h.write(b"foobar");
        assert_eq!(h.finish(), 0x8594_4171_f739_67e8);
    }
}
