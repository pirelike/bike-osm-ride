//! Street View Static API *metadata* client.
//!
//! Only JSON metadata (status, panorama id, panorama position) is requested - never an
//! image, which Google's terms forbid to download, store or pre-fetch. The key goes only to
//! URLs that [`Endpoints::trusts`] accepts (Google, or a loopback mock in tests), redirects
//! are not followed, and no error value or `Debug` output contains the key.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::error::SvError;
use crate::url::{valid_pano_id, ApiKey, EmbedTarget, EmbedUrl, Endpoints, View};

const USER_AGENT: &str = "bike-osm-ride/0.1 (desktop app)";
/// Metadata replies are tiny; anything larger is not from the API.
const MAX_BODY_BYTES: usize = 64 * 1024;
/// Search radius around the query point in metres (Google's default is 50).
pub const DEFAULT_RADIUS_M: u32 = 50;
/// Requests in flight at any moment from one client, whatever the callers do.
const MAX_IN_FLIGHT: usize = 6;

/// A panorama found by the metadata endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct PanoInfo {
    pub pano_id: String,
    /// Position of the panorama itself.
    pub lat: f64,
    pub lng: f64,
    /// Capture date as Google reports it (`2019-05` or `2019`), for display only - it is
    /// never stored.
    pub date: Option<String>,
}

#[derive(Deserialize)]
struct RawMeta {
    status: Option<String>,
    pano_id: Option<String>,
    location: Option<RawLatLng>,
    date: Option<String>,
}

#[derive(Deserialize)]
struct RawLatLng {
    lat: f64,
    lng: f64,
}

/// Turns a metadata reply into a [`PanoInfo`] or the mapped error. `query` is used for the
/// position when the reply has none. Pure, so the status table is unit-tested.
pub fn parse_metadata(
    body: &str,
    http_status: u16,
    query: (f64, f64),
) -> Result<PanoInfo, SvError> {
    let raw: Option<RawMeta> = serde_json::from_str(body).ok();
    let status = raw.as_ref().and_then(|r| r.status.as_deref());
    match status {
        Some(s) => {
            if let Some(e) = SvError::from_status(s) {
                return Err(e);
            }
        }
        None => {
            return Err(if (200..300).contains(&http_status) {
                SvError::BadResponse
            } else {
                SvError::from_http(http_status)
            });
        }
    }
    let raw = raw.ok_or(SvError::BadResponse)?;
    let id = raw.pano_id.filter(|id| valid_pano_id(id));
    let Some(pano_id) = id else {
        return Err(SvError::BadResponse);
    };
    let (lat, lng) = raw
        .location
        .filter(|l| l.lat.is_finite() && l.lng.is_finite())
        .map_or(query, |l| (l.lat, l.lng));
    let date = raw
        .date
        .map(|d| {
            d.chars()
                .filter(|c| c.is_ascii_digit() || *c == '-')
                .take(10)
                .collect()
        })
        .filter(|d: &String| !d.is_empty());
    Ok(PanoInfo {
        pano_id,
        lat,
        lng,
        date,
    })
}

/// Cheap to clone (shares the connection pool and the concurrency gate).
#[derive(Clone)]
pub struct MetadataClient {
    http: reqwest::Client,
    endpoints: Endpoints,
    key: ApiKey,
    gate: Arc<Semaphore>,
}

impl fmt::Debug for MetadataClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetadataClient")
            .field("endpoints", &self.endpoints)
            .field("key", &self.key)
            .finish()
    }
}

impl MetadataClient {
    pub fn new(endpoints: Endpoints, key: ApiKey) -> Result<MetadataClient, SvError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(5))
            // The key is in the query string: never follow a redirect to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| SvError::Network)?;
        Ok(MetadataClient {
            http,
            endpoints,
            key,
            gate: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
        })
    }

    /// Client for the key typed into the settings, with the endpoints from the environment
    /// (`RIDE_STREETVIEW_BASE` for a local mock, otherwise Google).
    pub fn from_settings_key(raw_key: &str) -> Result<MetadataClient, SvError> {
        let key = ApiKey::new(raw_key).ok_or(SvError::MissingKey)?;
        MetadataClient::new(Endpoints::from_env(), key)
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// The embed iframe URL for `target` (contains the key; keep it out of logs).
    pub fn embed_url(&self, target: &EmbedTarget, view: View) -> EmbedUrl {
        self.endpoints.embed_url(&self.key, target, view)
    }

    /// Nearest outdoor panorama within `radius_m` of `lat,lng`.
    pub async fn lookup(&self, lat: f64, lng: f64, radius_m: u32) -> Result<PanoInfo, SvError> {
        if !lat.is_finite() || !lng.is_finite() {
            return Err(SvError::InvalidRequest);
        }
        let url = self.endpoints.metadata_url(&self.key, lat, lng, radius_m);
        if !self.endpoints.trusts(&url) {
            return Err(SvError::UntrustedEndpoint);
        }
        let _permit = self.gate.acquire().await.map_err(|_| SvError::Network)?;
        let mut resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SvError::from_transport(&e))?;
        let status = resp.status().as_u16();
        if resp
            .content_length()
            .is_some_and(|l| l > MAX_BODY_BYTES as u64)
        {
            return Err(SvError::BadResponse);
        }
        let mut body: Vec<u8> = Vec::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_BODY_BYTES {
                        return Err(SvError::BadResponse);
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(SvError::from_transport(&e)),
            }
        }
        parse_metadata(&String::from_utf8_lossy(&body), status, (lat, lng))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockResponse, MockServer};

    const Q: (f64, f64) = (47.5, 19.0);

    #[test]
    fn parses_a_documented_ok_reply() {
        let body = r#"{
           "copyright" : "© Google",
           "date" : "2019-05",
           "location" : { "lat" : 47.49791, "lng" : 19.04024 },
           "pano_id" : "CAoSLEFGMVFpcE5v-_x",
           "status" : "OK"
        }"#;
        let p = parse_metadata(body, 200, Q).expect("ok");
        assert_eq!(p.pano_id, "CAoSLEFGMVFpcE5v-_x");
        assert_eq!((p.lat, p.lng), (47.49791, 19.04024));
        assert_eq!(p.date.as_deref(), Some("2019-05"));
    }

    #[test]
    fn maps_every_documented_status() {
        let st = |s: &str, code: u16| {
            parse_metadata(&format!(r#"{{"status":"{s}"}}"#), code, Q).unwrap_err()
        };
        assert_eq!(st("ZERO_RESULTS", 200), SvError::NoCoverage);
        assert_eq!(st("REQUEST_DENIED", 200), SvError::KeyRejected);
        assert_eq!(st("OVER_QUERY_LIMIT", 200), SvError::QuotaExceeded);
        assert_eq!(st("INVALID_REQUEST", 200), SvError::InvalidRequest);
        assert_eq!(st("UNKNOWN_ERROR", 200), SvError::Server);
        assert_eq!(st("NOT_FOUND", 200), SvError::NoCoverage);
        // a REQUEST_DENIED body wins over a generic HTTP code, error_message is ignored
        let e = parse_metadata(
            r#"{"error_message":"The provided API key is invalid.","status":"REQUEST_DENIED"}"#,
            403,
            Q,
        )
        .unwrap_err();
        assert_eq!(e, SvError::KeyRejected);
    }

    #[test]
    fn unusable_replies_are_bad_responses_or_http_errors() {
        assert_eq!(
            parse_metadata("", 200, Q).unwrap_err(),
            SvError::BadResponse
        );
        assert_eq!(
            parse_metadata("<html>", 200, Q).unwrap_err(),
            SvError::BadResponse
        );
        assert_eq!(
            parse_metadata("<html>", 429, Q).unwrap_err(),
            SvError::QuotaExceeded
        );
        assert_eq!(parse_metadata("{}", 503, Q).unwrap_err(), SvError::Server);
        // OK without a pano id, or with an id that is not URL safe
        assert_eq!(
            parse_metadata(r#"{"status":"OK"}"#, 200, Q).unwrap_err(),
            SvError::BadResponse
        );
        assert_eq!(
            parse_metadata(r#"{"status":"OK","pano_id":"a&b=c"}"#, 200, Q).unwrap_err(),
            SvError::BadResponse
        );
        // missing location falls back to the query point; weird date text is trimmed
        let p = parse_metadata(
            r#"{"status":"OK","pano_id":"abc","date":"<b>2020-01</b>"}"#,
            200,
            Q,
        )
        .expect("ok");
        assert_eq!((p.lat, p.lng), Q);
        assert_eq!(p.date.as_deref(), Some("2020-01"));
    }

    #[tokio::test]
    async fn lookup_against_the_mock_server() {
        let server = MockServer::streetview(Default::default());
        let ep = Endpoints::local(&server.base).expect("local");
        let key = ApiKey::new("TESTKEY123").expect("key");
        let client = MetadataClient::new(ep, key).expect("client");
        let p = client.lookup(47.5, 19.0, 50).await.expect("pano");
        assert!(valid_pano_id(&p.pano_id), "{}", p.pano_id);
        let lines = server.request_lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("GET /maps/api/streetview/metadata?"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("location=47.500000%2C19.000000"));
        assert!(lines[0].contains("radius=50"));
        assert!(lines[0].contains("source=outdoor"));
        assert!(lines[0].contains("key=TESTKEY123"));
    }

    #[tokio::test]
    async fn lookup_maps_server_side_failures_without_leaking_the_key() {
        let cases: [(&str, SvError); 4] = [
            ("BADKEY", SvError::KeyRejected),
            ("QUOTAKEY", SvError::QuotaExceeded),
            ("HTTP500KEY", SvError::Server),
            ("GARBAGEKEY", SvError::BadResponse),
        ];
        let server = MockServer::streetview(Default::default());
        for (k, expected) in cases {
            let client = MetadataClient::new(
                Endpoints::local(&server.base).expect("local"),
                ApiKey::new(k).expect("key"),
            )
            .expect("client");
            let err = client.lookup(47.5, 19.0, 50).await.unwrap_err();
            assert_eq!(err, expected, "{k}");
            assert!(!format!("{err:?}{err}{}", err.title()).contains(k));
            assert!(!format!("{client:?}").contains(k));
        }
    }

    #[tokio::test]
    async fn no_coverage_is_reported_as_such() {
        let server = MockServer::streetview(crate::mock::StreetviewMock {
            covered: std::sync::Arc::new(|lat, _| lat < 47.6),
            ..Default::default()
        });
        let client = MetadataClient::new(
            Endpoints::local(&server.base).expect("local"),
            ApiKey::new("K").expect("key"),
        )
        .expect("client");
        assert!(client.lookup(47.5, 19.0, 50).await.is_ok());
        assert_eq!(
            client.lookup(47.7, 19.0, 50).await.unwrap_err(),
            SvError::NoCoverage
        );
    }

    #[tokio::test]
    async fn connection_failures_and_redirects_are_safe() {
        // nothing listens on this port
        let ep = Endpoints::local("http://127.0.0.1:1").expect("local");
        let client =
            MetadataClient::new(ep, ApiKey::new("SECRETKEY").expect("key")).expect("client");
        let err = client.lookup(47.5, 19.0, 50).await.unwrap_err();
        assert_eq!(err, SvError::Network);
        assert!(!format!("{err:?}{err}").contains("SECRETKEY"));

        // a redirect to another host is not followed (the key would travel with it)
        let target = MockServer::start(|_| MockResponse::json(200, r#"{"status":"ZERO_RESULTS"}"#));
        let target_base = target.base.clone();
        let redirector = MockServer::start(move |_| MockResponse {
            status: 302,
            content_type: "text/plain",
            body: String::new(),
            body_bytes: None,
            delay_ms: 0,
            headers: vec![("Location".into(), format!("{target_base}/elsewhere"))],
        });
        let client = MetadataClient::new(
            Endpoints::local(&redirector.base).expect("local"),
            ApiKey::new("SECRETKEY").expect("key"),
        )
        .expect("client");
        let err = client.lookup(47.5, 19.0, 50).await.unwrap_err();
        assert_eq!(err, SvError::BadResponse);
        assert_eq!(
            target.request_count(),
            0,
            "redirect target must not be contacted"
        );
    }

    #[tokio::test]
    async fn invalid_coordinates_and_untrusted_endpoints_send_nothing() {
        let server = MockServer::streetview(Default::default());
        let client = MetadataClient::new(
            Endpoints::local(&server.base).expect("local"),
            ApiKey::new("K").expect("key"),
        )
        .expect("client");
        assert_eq!(
            client.lookup(f64::NAN, 19.0, 50).await.unwrap_err(),
            SvError::InvalidRequest
        );
        assert_eq!(server.request_count(), 0);
        assert_eq!(
            MetadataClient::from_settings_key("   ").unwrap_err(),
            SvError::MissingKey
        );
    }
}
