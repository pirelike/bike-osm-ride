//! Street View Static API *image* client for the opt-in offline cache.
//!
//! This is the only place in the app that downloads Street View imagery. It is used
//! exclusively by an explicit "download this route" action ([`crate::download`]); nothing is
//! fetched in the background while riding. The key goes only to URLs that
//! [`Endpoints::trusts`] accepts, redirects are not followed (they would carry the key to
//! another host), the reply must be a JPEG of a plausible size, and no error text ever
//! contains the URL (and so the key).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::error::SvError;
use crate::url::{ApiKey, EmbedTarget, Endpoints, View};

const USER_AGENT: &str = "bike-osm-ride/0.1 (desktop app)";
/// Larger answers are not Street View images.
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// Image requests in flight at any moment from one fetcher.
const MAX_IN_FLIGHT: usize = 4;

/// Is this the start of a JPEG file?
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Cheap to clone (shares the connection pool and the concurrency gate).
#[derive(Clone)]
pub struct ImageFetcher {
    http: reqwest::Client,
    endpoints: Endpoints,
    key: ApiKey,
    gate: Arc<Semaphore>,
}

impl fmt::Debug for ImageFetcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageFetcher")
            .field("endpoints", &self.endpoints)
            .field("key", &self.key)
            .finish()
    }
}

impl ImageFetcher {
    pub fn new(endpoints: Endpoints, key: ApiKey) -> Result<ImageFetcher, SvError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| SvError::Network)?;
        Ok(ImageFetcher {
            http,
            endpoints,
            key,
            gate: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
        })
    }

    /// Fetcher for the key typed into the settings, with the endpoints from the environment.
    pub fn from_settings_key(raw_key: &str) -> Result<ImageFetcher, SvError> {
        let key = ApiKey::new(raw_key).ok_or(SvError::MissingKey)?;
        ImageFetcher::new(Endpoints::from_env(), key)
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// Downloads the image of one panorama (or of a location when the id is unusable).
    pub async fn fetch(
        &self,
        target: &EmbedTarget,
        view: View,
        size_px: u32,
    ) -> Result<Vec<u8>, SvError> {
        let url = self.endpoints.static_url(&self.key, target, view, size_px);
        if !self.endpoints.trusts(&url) {
            return Err(SvError::UntrustedEndpoint);
        }
        let _permit = self.gate.acquire().await.map_err(|_| SvError::Network)?;
        let response = self
            .http
            .get(&url)
            .header("Accept", "image/jpeg,image/*;q=0.8")
            .send()
            .await
            .map_err(|e| SvError::from_transport(&e))?;
        let status = response.status();
        if !status.is_success() {
            return Err(SvError::from_http(status.as_u16()));
        }
        if let Some(content_type) = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        {
            if !content_type.starts_with("image/") {
                return Err(SvError::BadResponse);
            }
        }
        if response
            .content_length()
            .is_some_and(|len| len > MAX_IMAGE_BYTES as u64 || len == 0)
        {
            return Err(SvError::BadResponse);
        }
        let mut body: Vec<u8> = Vec::new();
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_IMAGE_BYTES {
                        return Err(SvError::BadResponse);
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(SvError::from_transport(&e)),
            }
        }
        if !is_jpeg(&body) {
            return Err(SvError::BadResponse);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockServer, StreetviewMock, MOCK_JPEG};

    fn fetcher(server: &MockServer, key: &str) -> ImageFetcher {
        ImageFetcher::new(
            Endpoints::local(&server.base).expect("local"),
            ApiKey::new(key).expect("key"),
        )
        .expect("fetcher")
    }

    fn target() -> EmbedTarget {
        EmbedTarget::Pano {
            id: "PANO-1".into(),
            fallback: Some((47.5, 19.0)),
        }
    }

    fn view() -> View {
        View {
            heading: 90.0,
            pitch: 2.0,
            fov: 90.0,
        }
    }

    #[tokio::test]
    async fn downloads_a_jpeg_from_the_static_endpoint() {
        let server = MockServer::streetview(StreetviewMock::default());
        let f = fetcher(&server, "GOODKEY");
        let bytes = f.fetch(&target(), view(), 640).await.expect("image");
        assert!(is_jpeg(&bytes));
        assert_eq!(bytes, MOCK_JPEG);
        let lines = server.request_lines();
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert!(line.starts_with("GET /maps/api/streetview?"), "{line}");
        assert!(line.contains("size=640x640"));
        assert!(line.contains("pano=PANO-1"));
        assert!(line.contains("heading=90"));
        assert!(line.contains("key=GOODKEY"));
        assert!(!format!("{f:?}").contains("GOODKEY"));
    }

    #[tokio::test]
    async fn key_and_quota_errors_are_mapped_without_leaking_the_key() {
        let server = MockServer::streetview(StreetviewMock::default());
        for (key, expected) in [
            ("BADKEY", SvError::KeyRejected),
            ("QUOTAKEY", SvError::QuotaExceeded),
        ] {
            let f = fetcher(&server, key);
            let err = f.fetch(&target(), view(), 640).await.unwrap_err();
            assert_eq!(err, expected);
            assert!(!format!("{err:?}{err}").contains(key));
        }
    }

    #[tokio::test]
    async fn non_jpeg_and_empty_answers_are_rejected() {
        let server = MockServer::start(|req| {
            if req.path == "/maps/api/streetview" {
                crate::mock::MockResponse::html("<html>captive portal</html>".to_string())
            } else {
                crate::mock::MockResponse::json(404, "{}")
            }
        });
        let f = fetcher(&server, "K");
        assert_eq!(
            f.fetch(&target(), view(), 640).await.unwrap_err(),
            SvError::BadResponse
        );
        // nothing listens: a transport error, no key text
        let f = ImageFetcher::new(
            Endpoints::local("http://127.0.0.1:1").expect("local"),
            ApiKey::new("SECRET").expect("key"),
        )
        .expect("f");
        let err = f.fetch(&target(), view(), 640).await.unwrap_err();
        assert_eq!(err, SvError::Network);
        assert!(!format!("{err:?}").contains("SECRET"));
    }

    #[test]
    fn jpeg_magic() {
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!is_jpeg(&[0x89, 0x50, 0x4E, 0x47]));
        assert!(!is_jpeg(&[]));
    }
}
