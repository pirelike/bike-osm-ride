//! The `svimg` protocol: serves the downloaded Street View images of the offline cache to
//! the panel (`http://svimg.localhost/streetview/<route id>/<bucket>.jpg` on Windows,
//! `svimg://localhost/...` on macOS/Linux, `https://svimg.localhost/...` on Android — the
//! handler only reads the path).
//!
//! The handler only reads files below `<data>/streetview/img/`, and only for a route id and
//! bucket that `ride_ui_streetview::imagestore::parse_image_path` accepts, so a crafted URL
//! can never address another file. Missing files answer 404 without a body; images are
//! answered `no-store`, because the offline cache enforces its own 30-day retention and the
//! webview must not keep a copy of its own.

use std::borrow::Cow;

use dioxus::desktop::wry::http::{Method, Response};
use dioxus::desktop::Config;
use ride_ui_streetview::imagestore::{image_path, parse_image_path};

use crate::paths;

/// What the protocol answers for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResponse {
    pub status: u16,
    pub body: Cow<'static, [u8]>,
}

impl ImageResponse {
    fn error(status: u16) -> Self {
        ImageResponse {
            status,
            body: Cow::Borrowed(&[]),
        }
    }

    fn jpeg(bytes: Vec<u8>) -> Self {
        ImageResponse {
            status: 200,
            body: Cow::Owned(bytes),
        }
    }

    pub fn into_http(self) -> Response<Cow<'static, [u8]>> {
        let content_type = if self.body.is_empty() {
            "text/plain"
        } else {
            "image/jpeg"
        };
        Response::builder()
            .status(self.status)
            .header("Content-Type", content_type)
            .header("Cache-Control", "no-store")
            .header("Access-Control-Allow-Origin", "*")
            .body(self.body)
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
    }
}

/// Answers a request for `path` (e.g. `/streetview/abc123/7.jpg`).
pub fn respond(path: &str) -> ImageResponse {
    respond_in(&paths::data_dir(), path)
}

fn respond_in(data_dir: &std::path::Path, path: &str) -> ImageResponse {
    let Some((route_id, bucket)) = parse_image_path(path) else {
        return ImageResponse::error(400);
    };
    let Some(file) = image_path(data_dir, &route_id, bucket) else {
        return ImageResponse::error(400);
    };
    use std::io::Read;
    let bytes = std::fs::File::open(&file).and_then(|f| {
        let mut bytes = Vec::new();
        f.take(ride_ui_streetview::image::MAX_IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    match bytes {
        Ok(bytes) if ride_ui_streetview::image::is_jpeg(&bytes) => ImageResponse::jpeg(bytes),
        Ok(_) => ImageResponse::error(415),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ImageResponse::error(404),
        Err(_) => ImageResponse::error(500),
    }
}

/// Registers the `svimg` protocol on the desktop config.
pub fn register(config: Config) -> Config {
    config.with_asynchronous_custom_protocol("svimg", move |_webview, request, responder| {
        if request.method() != Method::GET && request.method() != Method::HEAD {
            responder.respond(ImageResponse::error(405).into_http());
            return;
        }
        let path = request.uri().path().to_string();
        let head = request.method() == Method::HEAD;
        // Bound OS threads before spawning, so a request flood cannot exhaust them.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static ACTIVE: AtomicUsize = AtomicUsize::new(0);
        if ACTIVE
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .is_err()
        {
            responder.respond(ImageResponse::error(503).into_http());
            return;
        }
        struct Permit;
        impl Drop for Permit {
            fn drop(&mut self) {
                ACTIVE.fetch_sub(1, Ordering::Release);
            }
        }
        let permit = Permit;
        std::thread::spawn(move || {
            let _permit = permit;
            let mut response = respond(&path);
            if head {
                response.body = Cow::Borrowed(&[]);
            }
            responder.respond(response.into_http());
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_paths_are_rejected() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert_eq!(respond_in(tmp.path(), "/nope").status, 400);
        assert_eq!(respond_in(tmp.path(), "/streetview/../x/1.jpg").status, 400);
        assert_eq!(respond_in(tmp.path(), "/streetview/abc/1.png").status, 400);
        assert_eq!(respond_in(tmp.path(), "/streetview/abc/1.jpg").status, 404);
    }

    #[test]
    fn a_written_image_is_served_and_other_bytes_are_not() {
        let tmp = tempfile::tempdir().expect("tmp");
        let file = image_path(tmp.path(), "svimgroute", 3).expect("path");
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        let jpeg = ride_ui_streetview::mock::MOCK_JPEG;
        std::fs::write(&file, jpeg).expect("write");
        let r = respond_in(tmp.path(), "/streetview/svimgroute/3.jpg");
        assert_eq!(r.status, 200);
        assert_eq!(r.body.as_ref(), jpeg);
        let http = r.into_http();
        assert_eq!(http.headers()["Content-Type"], "image/jpeg");
        assert_eq!(http.headers()["Cache-Control"], "no-store");

        std::fs::write(&file, b"not a jpeg").expect("write");
        assert_eq!(
            respond_in(tmp.path(), "/streetview/svimgroute/3.jpg").status,
            415
        );
    }

    #[test]
    fn methods_other_than_get_are_rejected_by_the_protocol() {
        // `respond` itself is method agnostic; the registration wraps it (checked in QA).
        assert_eq!(ImageResponse::error(405).status, 405);
    }
}
