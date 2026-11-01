//! URL builders for the two Google endpoints, the API key newtype and the endpoint
//! override used by tests / QA.
//!
//! Parameter names checked against Google's documentation (developers.google.com):
//!
//! * Street View Static API *metadata*: `https://maps.googleapis.com/maps/api/streetview/metadata`
//!   with `location=lat,lng` (or `pano=`), `radius` (metres, default 50), `source`
//!   (`default` | `outdoor`), `key`; replies JSON `{status, pano_id, location{lat,lng}, date,
//!   copyright}`; metadata requests are free of charge.
//! * Maps Embed API, Street View mode: `https://www.google.com/maps/embed/v1/streetview` with
//!   `key` (required), `pano` or `location` (one is required; `location` given next to `pano`
//!   is used only when the panorama id is not found), `heading` (-180..360), `pitch`
//!   (-90..90, default 0), `fov` (10..100, default 90), `radius` (default 50), `source`.

use std::fmt;

use reqwest::Url;

/// Production host of the metadata endpoint.
pub const GOOGLE_API_BASE: &str = "https://maps.googleapis.com";
/// Production host of the embed endpoint.
pub const GOOGLE_EMBED_BASE: &str = "https://www.google.com";
/// Path of the metadata endpoint below the API base.
pub const METADATA_PATH: &str = "/maps/api/streetview/metadata";
/// Path of the Street View Static API image endpoint below the API base.
pub const STATIC_PATH: &str = "/maps/api/streetview";
/// Path of the embed endpoint below the embed base.
pub const EMBED_PATH: &str = "/maps/embed/v1/streetview";
/// Environment variable that points both endpoints at a local mock (loopback only).
pub const BASE_ENV: &str = "RIDE_STREETVIEW_BASE";

/// Longest key we accept; real keys have 39 characters.
const MAX_KEY_LEN: usize = 256;

/// The user's Google Maps Platform API key. `Debug` is redacted and there is no `Display`,
/// so formatting a struct that contains one cannot leak it. The only way to the text is
/// [`ApiKey::expose`], used by the URL builders.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// `None` for an empty / blank key or one with whitespace or control characters inside.
    pub fn new(raw: &str) -> Option<ApiKey> {
        let k = raw.trim();
        let ok = !k.is_empty()
            && k.len() <= MAX_KEY_LEN
            && k.chars().all(|c| !c.is_whitespace() && !c.is_control());
        ok.then(|| ApiKey(k.to_string()))
    }

    /// The raw key. Only for building request URLs.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<elrejtve>)")
    }
}

/// A finished embed URL (it contains the key). `Debug` is redacted; the panel hands
/// [`EmbedUrl::as_str`] to the iframe and nowhere else.
#[derive(Clone, PartialEq, Eq)]
pub struct EmbedUrl(String);

impl EmbedUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EmbedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EmbedUrl(<elrejtve>)")
    }
}

/// Camera of the embedded panorama.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Degrees clockwise from north.
    pub heading: f64,
    /// Degrees, positive = up.
    pub pitch: f64,
    /// Horizontal field of view in degrees.
    pub fov: f64,
}

impl View {
    /// Heading wrapped to `0..360`, pitch to `-90..=90`, fov to `10..=100`; NaN becomes the
    /// default (0 / 0 / 90).
    pub fn normalized(self) -> View {
        let fix = |v: f64, d: f64| if v.is_finite() { v } else { d };
        View {
            heading: fix(self.heading, 0.0).rem_euclid(360.0),
            pitch: fix(self.pitch, 0.0).clamp(-90.0, 90.0),
            fov: fix(self.fov, 90.0).clamp(10.0, 100.0),
        }
    }
}

/// What the embed iframe should show.
#[derive(Debug, Clone, PartialEq)]
pub enum EmbedTarget {
    /// A known panorama. `fallback` (route point) is sent as `location` too: Google only
    /// uses it when the id is no longer found, which keeps cached ids safe against re-IDs.
    Pano {
        id: String,
        fallback: Option<(f64, f64)>,
    },
    /// Let Google pick the nearest outdoor panorama.
    Location { lat: f64, lng: f64, radius_m: u32 },
}

/// Is `id` something we are willing to put in a URL / a cache file? (Google ids are
/// URL-safe base64-like strings.)
pub fn valid_pano_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Percent-encodes everything except the RFC 3986 unreserved characters.
pub fn pct_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len() + 8);
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}

/// `lat,lng` with six decimals (about 0.1 m), always with a dot.
fn latlng(lat: f64, lng: f64) -> String {
    format!("{lat:.6},{lng:.6}")
}

/// Is this an http(s) URL pointing at the local machine?
pub fn is_loopback_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
    })
}

/// The two base addresses requests go to. Production = Google; a loopback override lets
/// tests and QA point both at a local mock server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    api: String,
    embed: String,
    local: bool,
}

impl Endpoints {
    /// The real Google endpoints.
    pub fn google() -> Endpoints {
        Endpoints {
            api: GOOGLE_API_BASE.to_string(),
            embed: GOOGLE_EMBED_BASE.to_string(),
            local: false,
        }
    }

    /// Both endpoints below one local base such as `http://127.0.0.1:9300`. `None` unless
    /// the base is a loopback http(s) address: the key must never reach any other host.
    pub fn local(base: &str) -> Option<Endpoints> {
        let b = base.trim().trim_end_matches('/');
        is_loopback_url(b).then(|| Endpoints {
            api: b.to_string(),
            embed: b.to_string(),
            local: true,
        })
    }

    /// `RIDE_STREETVIEW_BASE` if it is a valid loopback base, otherwise Google.
    pub fn from_env() -> Endpoints {
        std::env::var(BASE_ENV)
            .ok()
            .and_then(|v| Endpoints::local(&v))
            .unwrap_or_else(Endpoints::google)
    }

    /// Is this the local-mock configuration?
    pub fn is_local(&self) -> bool {
        self.local
    }

    /// May the key be sent to this URL? True for `https://maps.googleapis.com/…` and
    /// `https://www.google.com/…`; in local mode also for loopback addresses.
    pub fn trusts(&self, url: &str) -> bool {
        let Ok(u) = Url::parse(url) else {
            return false;
        };
        if u.scheme() == "https"
            && matches!(u.host_str(), Some("maps.googleapis.com" | "www.google.com"))
            && u.port().is_none()
        {
            return true;
        }
        self.local && is_loopback_url(url)
    }

    /// Metadata request for the panorama nearest to `lat,lng` within `radius_m`
    /// (`source=outdoor`: no indoor / photo-sphere results).
    pub fn metadata_url(&self, key: &ApiKey, lat: f64, lng: f64, radius_m: u32) -> String {
        format!(
            "{}{}?location={}&radius={}&source=outdoor&key={}",
            self.api,
            METADATA_PATH,
            pct_encode(&latlng(lat, lng)),
            radius_m,
            pct_encode(key.expose())
        )
    }

    /// Embed iframe URL. Unusable panorama ids are replaced by a plain location lookup when
    /// a fallback point exists (never put into the URL unvalidated).
    pub fn embed_url(&self, key: &ApiKey, target: &EmbedTarget, view: View) -> EmbedUrl {
        let v = view.normalized();
        let mut q = format!("key={}", pct_encode(key.expose()));
        match target {
            EmbedTarget::Pano { id, fallback } if valid_pano_id(id) => {
                q.push_str("&pano=");
                q.push_str(&pct_encode(id));
                if let Some((lat, lng)) = fallback {
                    q.push_str("&location=");
                    q.push_str(&pct_encode(&latlng(*lat, *lng)));
                }
            }
            EmbedTarget::Pano {
                fallback: Some((lat, lng)),
                ..
            } => {
                q.push_str("&location=");
                q.push_str(&pct_encode(&latlng(*lat, *lng)));
                q.push_str("&radius=50&source=outdoor");
            }
            EmbedTarget::Pano { .. } => {}
            EmbedTarget::Location { lat, lng, radius_m } => {
                q.push_str("&location=");
                q.push_str(&pct_encode(&latlng(*lat, *lng)));
                q.push_str(&format!("&radius={radius_m}&source=outdoor"));
            }
        }
        q.push_str(&format!(
            "&heading={:.0}&pitch={:.0}&fov={:.0}",
            v.heading, v.pitch, v.fov
        ));
        EmbedUrl(format!("{}{}?{}", self.embed, EMBED_PATH, q))
    }

    /// Street View Static API image URL (a square JPEG). Used only by the opt-in offline
    /// cache; the key is in the query string, so the result must never be logged or shown.
    /// Unusable panorama ids fall back to a plain location lookup when one is known.
    pub fn static_url(
        &self,
        key: &ApiKey,
        target: &EmbedTarget,
        view: View,
        size_px: u32,
    ) -> String {
        let v = view.normalized();
        let size = size_px.clamp(64, 640);
        let mut q = format!("size={size}x{size}");
        match target {
            EmbedTarget::Pano { id, .. } if valid_pano_id(id) => {
                q.push_str("&pano=");
                q.push_str(&pct_encode(id));
            }
            EmbedTarget::Pano {
                fallback: Some((lat, lng)),
                ..
            } => {
                q.push_str("&location=");
                q.push_str(&pct_encode(&latlng(*lat, *lng)));
                q.push_str("&radius=50&source=outdoor");
            }
            EmbedTarget::Pano { .. } => {}
            EmbedTarget::Location { lat, lng, radius_m } => {
                q.push_str("&location=");
                q.push_str(&pct_encode(&latlng(*lat, *lng)));
                q.push_str(&format!("&radius={radius_m}&source=outdoor"));
            }
        }
        q.push_str(&format!(
            "&heading={:.0}&pitch={:.0}&fov={:.0}&key={}",
            v.heading,
            v.pitch,
            v.fov,
            pct_encode(key.expose())
        ));
        format!("{}{}?{}", self.api, STATIC_PATH, q)
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints::google()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ApiKey {
        ApiKey::new("AIzaSy-TEST_key.123").expect("key")
    }

    #[test]
    fn key_validation_and_redaction() {
        assert!(ApiKey::new("").is_none());
        assert!(ApiKey::new("   ").is_none());
        assert!(ApiKey::new("ab cd").is_none());
        assert!(ApiKey::new("ab\ncd").is_none());
        assert!(ApiKey::new(&"a".repeat(300)).is_none());
        let k = ApiKey::new("  AIzaSecretSecret  ").expect("trimmed");
        assert_eq!(k.expose(), "AIzaSecretSecret");
        assert!(!format!("{k:?}").contains("Secret"));
        assert!(!format!("{k:#?}").contains("Secret"));
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(pct_encode("abc-._~XYZ019"), "abc-._~XYZ019");
        assert_eq!(pct_encode("47.5,19.0"), "47.5%2C19.0");
        assert_eq!(
            pct_encode("a b&c=d?e#f/g+h%"),
            "a%20b%26c%3Dd%3Fe%23f%2Fg%2Bh%25"
        );
        assert_eq!(pct_encode("é"), "%C3%A9");
        assert_eq!(pct_encode(""), "");
    }

    #[test]
    fn metadata_url_matches_the_documented_shape() {
        let u = Endpoints::google().metadata_url(&key(), 47.497_912, 19.040_235, 50);
        assert_eq!(
            u,
            "https://maps.googleapis.com/maps/api/streetview/metadata\
             ?location=47.497912%2C19.040235&radius=50&source=outdoor&key=AIzaSy-TEST_key.123"
        );
        // negative coordinates keep the sign, always a dot decimal separator
        let u = Endpoints::google().metadata_url(&key(), -33.5, -70.25, 30);
        assert!(
            u.contains("location=-33.500000%2C-70.250000&radius=30"),
            "{u}"
        );
    }

    #[test]
    fn hostile_key_text_is_encoded_not_injected() {
        let k = ApiKey::new("x&pano=evil&key=y#z").expect("key");
        let u = Endpoints::google().metadata_url(&k, 47.0, 19.0, 50);
        assert!(u.ends_with("&key=x%26pano%3Devil%26key%3Dy%23z"), "{u}");
        assert_eq!(u.matches("&key=").count(), 1);
        let e = Endpoints::google().embed_url(
            &k,
            &EmbedTarget::Location {
                lat: 1.0,
                lng: 2.0,
                radius_m: 50,
            },
            View {
                heading: 0.0,
                pitch: 0.0,
                fov: 90.0,
            },
        );
        assert!(e.as_str().contains("key=x%26pano%3Devil%26key%3Dy%23z"));
        assert!(!e.as_str().contains("pano=evil"));
    }

    #[test]
    fn embed_url_with_pano_adds_location_fallback_and_clamps_the_view() {
        let ep = Endpoints::google();
        let u = ep.embed_url(
            &key(),
            &EmbedTarget::Pano {
                id: "CAoSLEFGMVFpcE9f".into(),
                fallback: Some((47.5, 19.04)),
            },
            View {
                heading: -90.0,
                pitch: 2.0,
                fov: 250.0,
            },
        );
        assert_eq!(
            u.as_str(),
            "https://www.google.com/maps/embed/v1/streetview?key=AIzaSy-TEST_key.123\
             &pano=CAoSLEFGMVFpcE9f&location=47.500000%2C19.040000&heading=270&pitch=2&fov=100"
        );
        // without fallback: only the pano
        let u = ep.embed_url(
            &key(),
            &EmbedTarget::Pano {
                id: "abc_DEF-1".into(),
                fallback: None,
            },
            View {
                heading: 359.6,
                pitch: f64::NAN,
                fov: f64::NAN,
            },
        );
        assert!(
            u.as_str().contains("&pano=abc_DEF-1&heading=360&"),
            "{}",
            u.as_str()
        );
        assert!(u.as_str().ends_with("&pitch=0&fov=90"));
    }

    #[test]
    fn embed_url_for_location_and_for_a_bad_pano_id() {
        let ep = Endpoints::google();
        let v = View {
            heading: 10.0,
            pitch: 2.0,
            fov: 90.0,
        };
        let u = ep.embed_url(
            &key(),
            &EmbedTarget::Location {
                lat: 47.5,
                lng: 19.0,
                radius_m: 60,
            },
            v,
        );
        assert!(u
            .as_str()
            .contains("&location=47.500000%2C19.000000&radius=60&source=outdoor&heading=10"));
        // an id with characters that do not belong in an id never reaches the URL
        let u = ep.embed_url(
            &key(),
            &EmbedTarget::Pano {
                id: "bad&id=1".into(),
                fallback: Some((47.5, 19.0)),
            },
            v,
        );
        assert!(!u.as_str().contains("pano="), "{}", u.as_str());
        assert!(u.as_str().contains("location="));
        assert!(valid_pano_id("AF1Qip-_x9"));
        assert!(!valid_pano_id("") && !valid_pano_id("a b") && !valid_pano_id(&"a".repeat(129)));
    }

    #[test]
    fn view_normalisation() {
        let v = View {
            heading: 725.0,
            pitch: -120.0,
            fov: 3.0,
        }
        .normalized();
        assert_eq!((v.heading, v.pitch, v.fov), (5.0, -90.0, 10.0));
    }

    #[test]
    fn local_override_is_loopback_only() {
        assert!(Endpoints::local("http://127.0.0.1:9300").is_some());
        assert!(Endpoints::local("http://localhost:1/").is_some());
        assert!(Endpoints::local("http://[::1]:5").is_some());
        assert!(Endpoints::local("https://evil.example.com").is_none());
        assert!(Endpoints::local("http://127.0.0.1.evil.com").is_none());
        assert!(Endpoints::local("ftp://127.0.0.1").is_none());
        assert!(Endpoints::local("").is_none());
        let e = Endpoints::local("http://127.0.0.1:9300/").expect("local");
        assert!(e.is_local());
        assert_eq!(
            e.metadata_url(&key(), 1.0, 2.0, 50).split('?').next(),
            Some("http://127.0.0.1:9300/maps/api/streetview/metadata")
        );
    }

    #[test]
    fn trust_check_only_allows_google_and_local_mock() {
        let g = Endpoints::google();
        assert!(g.trusts("https://maps.googleapis.com/maps/api/streetview/metadata?x=1"));
        assert!(g.trusts("https://www.google.com/maps/embed/v1/streetview?x=1"));
        assert!(!g.trusts("http://maps.googleapis.com/x"));
        assert!(!g.trusts("https://maps.googleapis.com.evil.com/x"));
        assert!(!g.trusts("https://evil.com/maps.googleapis.com"));
        assert!(!g.trusts("https://user@evil.com@maps.googleapis.com.evil.com/"));
        assert!(
            !g.trusts("http://127.0.0.1:9300/x"),
            "loopback needs the override"
        );
        assert!(!g.trusts("garbage"));
        let l = Endpoints::local("http://127.0.0.1:9300").expect("local");
        assert!(l.trusts("http://127.0.0.1:9300/x"));
        assert!(!l.trusts("https://example.com/x"));
    }

    #[test]
    fn static_image_url_is_built_for_the_offline_cache() {
        let ep = Endpoints::google();
        let v = View {
            heading: 359.6,
            pitch: 2.0,
            fov: 90.0,
        };
        let u = ep.static_url(
            &key(),
            &EmbedTarget::Pano {
                id: "PANO-1".into(),
                fallback: Some((47.5, 19.0)),
            },
            v,
            640,
        );
        assert_eq!(
            u,
            "https://maps.googleapis.com/maps/api/streetview?size=640x640&pano=PANO-1\
             &heading=360&pitch=2&fov=90&key=AIzaSy-TEST_key.123"
        );
        // the size is clamped and a hostile id never reaches the URL
        let u = ep.static_url(
            &key(),
            &EmbedTarget::Pano {
                id: "bad&id=1".into(),
                fallback: Some((47.5, 19.0)),
            },
            v,
            4000,
        );
        assert!(u.contains("size=640x640&location=47.500000%2C19.000000&radius=50"));
        assert!(!u.contains("pano="));
        // a location target keeps its radius
        let u = ep.static_url(
            &key(),
            &EmbedTarget::Location {
                lat: 1.0,
                lng: 2.0,
                radius_m: 60,
            },
            v,
            128,
        );
        assert!(u.contains("size=128x128&location=1.000000%2C2.000000&radius=60"));
        // loopback mode (tests) uses the same path on the mock
        let local = Endpoints::local("http://127.0.0.1:9300").expect("local");
        let u = local.static_url(
            &key(),
            &EmbedTarget::Pano {
                id: "A".into(),
                fallback: None,
            },
            v,
            640,
        );
        assert!(u.starts_with("http://127.0.0.1:9300/maps/api/streetview?"));
        assert!(local.trusts(&u));
    }

    #[test]
    fn default_endpoints_are_google() {
        let e = Endpoints::default();
        assert!(!e.is_local());
        assert!(e.trusts(&e.metadata_url(&key(), 1.0, 2.0, 50)));
        assert!(e.trusts(
            e.embed_url(
                &key(),
                &EmbedTarget::Location {
                    lat: 1.0,
                    lng: 2.0,
                    radius_m: 50
                },
                View {
                    heading: 0.0,
                    pitch: 0.0,
                    fov: 90.0
                }
            )
            .as_str()
        ));
    }
}
