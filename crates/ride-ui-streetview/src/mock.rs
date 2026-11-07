//! A tiny local HTTP server that imitates the two Google endpoints. It exists so that the
//! metadata client, the prefetcher, the panel and the QA screenshots can all be exercised
//! without a real API key (point `RIDE_STREETVIEW_BASE=http://127.0.0.1:<port>` at it).
//!
//! Std only: one accept thread, one thread per connection, `Connection: close`.
//!
//! Behaviour of [`MockServer::streetview`]:
//!
//! * `GET /maps/api/streetview/metadata?location=lat,lng&key=K` - the key selects the reply:
//!   `BADKEY` gives `REQUEST_DENIED`, `QUOTAKEY` `OVER_QUERY_LIMIT`, `HTTP500KEY` an HTTP 500,
//!   `GARBAGEKEY` an HTML page; anything else is answered from the coverage predicate
//!   (`OK` + a deterministic pano id, or `ZERO_RESULTS`).
//! * `GET /maps/embed/v1/streetview?...` - a small HTML page that draws a fake street scene and
//!   prints the pano id / heading it was asked for (so a screenshot shows what was loaded).

use ride_i18n::t;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// A real 640×480 JPEG, served by the mock's image endpoint (and used by its tests).
pub const MOCK_JPEG: &[u8] = include_bytes!("../assets/mock.jpg");

/// One parsed request.
#[derive(Debug, Clone)]
pub struct MockRequest {
    pub request_line: String,
    pub path: String,
    /// Percent-decoded query pairs in order.
    pub query: Vec<(String, String)>,
}

impl MockRequest {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// One canned reply.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    /// Binary body (an image); takes precedence over `body` when set.
    pub body_bytes: Option<Vec<u8>>,
    /// Sleep before answering (to make requests overlap).
    pub delay_ms: u64,
    pub headers: Vec<(String, String)>,
}

impl MockResponse {
    pub fn json(status: u16, body: &str) -> MockResponse {
        MockResponse {
            status,
            content_type: "application/json; charset=utf-8",
            body: body.to_string(),
            body_bytes: None,
            delay_ms: 0,
            headers: Vec::new(),
        }
    }

    pub fn html(body: String) -> MockResponse {
        MockResponse {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
            body_bytes: None,
            delay_ms: 0,
            headers: Vec::new(),
        }
    }

    pub fn jpeg(status: u16, bytes: Vec<u8>) -> MockResponse {
        MockResponse {
            status,
            content_type: "image/jpeg",
            body: String::new(),
            body_bytes: Some(bytes),
            delay_ms: 0,
            headers: Vec::new(),
        }
    }
}

#[derive(Default)]
struct Shared {
    lines: Mutex<Vec<String>>,
    count: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    stop: AtomicBool,
}

/// Running mock server; stops when dropped.
pub struct MockServer {
    /// `http://127.0.0.1:<port>`
    pub base: String,
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

type Handler = dyn Fn(&MockRequest) -> MockResponse + Send + Sync + 'static;

impl MockServer {
    /// Starts on a free port with `handler` answering every request.
    pub fn start(
        handler: impl Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    ) -> MockServer {
        Self::start_on("127.0.0.1:0", handler).expect("bind a loopback port")
    }

    /// Starts on a given address (QA: a fixed port).
    pub fn start_on(
        addr: &str,
        handler: impl Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    ) -> std::io::Result<MockServer> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let base = format!("http://{}", listener.local_addr()?);
        let shared = Arc::new(Shared::default());
        let handler: Arc<Handler> = Arc::new(handler);
        let sh = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            while !sh.stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let sh = Arc::clone(&sh);
                        let handler = Arc::clone(&handler);
                        std::thread::spawn(move || serve(stream, &sh, &*handler));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(MockServer {
            base,
            shared,
            handle: Some(handle),
        })
    }

    /// The Google imitation described in the module docs.
    pub fn streetview(opts: StreetviewMock) -> MockServer {
        Self::start(move |req| opts.reply(req))
    }

    /// Like [`MockServer::streetview`] on a fixed address.
    pub fn streetview_on(addr: &str, opts: StreetviewMock) -> std::io::Result<MockServer> {
        Self::start_on(addr, move |req| opts.reply(req))
    }

    /// Request lines (`GET /path?query HTTP/1.1`) in arrival order.
    pub fn request_lines(&self) -> Vec<String> {
        self.shared
            .lines
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default()
    }

    pub fn request_count(&self) -> usize {
        self.shared.count.load(Ordering::SeqCst)
    }

    /// Highest number of requests that were being answered at the same time.
    pub fn max_in_flight(&self) -> usize {
        self.shared.max_in_flight.load(Ordering::SeqCst)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn serve(mut stream: TcpStream, sh: &Shared, handler: &Handler) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buf).into_owned();
    let line = head.lines().next().unwrap_or("").to_string();
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target.to_string(), Vec::new()),
    };
    if let Ok(mut l) = sh.lines.lock() {
        l.push(line.clone());
    }
    sh.count.fetch_add(1, Ordering::SeqCst);
    let now = sh.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
    sh.max_in_flight.fetch_max(now, Ordering::SeqCst);

    let resp = handler(&MockRequest {
        request_line: line,
        path,
        query,
    });
    if resp.delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(resp.delay_ms));
    }
    let payload: &[u8] = resp.body_bytes.as_deref().unwrap_or(resp.body.as_bytes());
    let mut out = format!(
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        resp.status,
        resp.content_type,
        payload.len()
    );
    for (k, v) in &resp.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(payload);
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
    sh.in_flight.fetch_sub(1, Ordering::SeqCst);
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(v) = h {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (pct_decode(k), pct_decode(v)),
            None => (pct_decode(p), String::new()),
        })
        .collect()
}

/// Options of the Google imitation.
#[derive(Clone)]
pub struct StreetviewMock {
    /// Added to every metadata reply.
    pub delay_ms: u64,
    /// Is there a panorama near `lat,lng`?
    pub covered: Arc<dyn Fn(f64, f64) -> bool + Send + Sync>,
}

impl StreetviewMock {
    /// The reply to one request (lets a caller serve extra pages next to the two endpoints).
    pub fn reply(&self, req: &MockRequest) -> MockResponse {
        streetview_reply(self, req)
    }
}

impl Default for StreetviewMock {
    fn default() -> Self {
        StreetviewMock {
            delay_ms: 0,
            covered: Arc::new(|_, _| true),
        }
    }
}

/// Deterministic pano id for a query point (about 11 m grid).
pub fn mock_pano_id(lat: f64, lng: f64) -> String {
    format!(
        "MOCK{}_{}",
        (lat * 1e4).round() as i64,
        (lng * 1e4).round() as i64
    )
}

fn parse_latlng(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn streetview_reply(opts: &StreetviewMock, req: &MockRequest) -> MockResponse {
    let key = req.param("key").unwrap_or("");
    match req.path.as_str() {
        "/maps/api/streetview" => {
            // The Static API image endpoint: the key selects an error, otherwise a JPEG.
            let mut r = match key {
                "BADKEY" => MockResponse::json(403, r#"{"status":"REQUEST_DENIED"}"#),
                "QUOTAKEY" => MockResponse::json(429, r#"{"status":"OVER_QUERY_LIMIT"}"#),
                _ => MockResponse::jpeg(200, MOCK_JPEG.to_vec()),
            };
            r.delay_ms = opts.delay_ms;
            r
        }
        "/maps/api/streetview/metadata" => {
            let mut r = match key {
                "BADKEY" => MockResponse::json(
                    200,
                    r#"{"error_message":"The provided API key is invalid.","status":"REQUEST_DENIED"}"#,
                ),
                "QUOTAKEY" => MockResponse::json(200, r#"{"status":"OVER_QUERY_LIMIT"}"#),
                "HTTP500KEY" => MockResponse::json(500, "{}"),
                "GARBAGEKEY" => MockResponse::html("<html>nope</html>".to_string()),
                _ => match req.param("location").and_then(parse_latlng) {
                    None => MockResponse::json(200, r#"{"status":"INVALID_REQUEST"}"#),
                    Some((lat, lng)) if (opts.covered)(lat, lng) => MockResponse::json(
                        200,
                        &format!(
                            r#"{{"copyright":"© Mock","date":"2021-06","location":{{"lat":{lat:.6},"lng":{lng:.6}}},"pano_id":"{}","status":"OK"}}"#,
                            mock_pano_id(lat, lng)
                        ),
                    ),
                    Some(_) => MockResponse::json(200, r#"{"status":"ZERO_RESULTS"}"#),
                },
            };
            r.delay_ms = opts.delay_ms;
            r
        }
        "/maps/embed/v1/streetview" => {
            let pano = req.param("pano").unwrap_or("(location)");
            let heading = req.param("heading").unwrap_or("0");
            let loc = req.param("location").unwrap_or("");
            MockResponse::html(embed_page(pano, heading, loc, key == "BADKEY"))
        }
        _ => MockResponse::json(404, "{}"),
    }
}

fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '&' => "&amp;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// A fake street scene; the hue follows the pano id so a change of panorama is visible.
fn embed_page(pano: &str, heading: &str, loc: &str, denied: bool) -> String {
    let hue = pano
        .bytes()
        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32))
        % 360;
    let h: f64 = heading.parse().unwrap_or(0.0);
    let shift = -((h % 360.0) / 360.0 * 100.0);
    let body = if denied {
        t("ui_streetview.mock.div_class_deny_ez_az_oldal_nem_tudta_megfeleloen_bet").to_string()
    } else {
        format!(
            "<div class=\"sky\"></div><div class=\"hills\"></div><div class=\"road\"></div>\
             <div class=\"tag\">MOCK Street View<br>{}<br>{} {}°<br>{}</div>",
            esc(pano),
            esc(t("ui_streetview.mock.irany")),
            esc(heading),
            esc(loc)
        )
    };
    format!(
        "<!doctype html><meta charset=utf-8><title>mock</title><style>\
         html,body{{margin:0;height:100%;overflow:hidden;font:12px/1.4 Segoe UI,sans-serif;color:#fff}}\
         body{{position:relative;background:hsl({hue} 30% 20%)}}\
         .sky{{position:absolute;inset:0 0 45% 0;background:linear-gradient(hsl({hue} 55% 62%),hsl({hue} 45% 80%))}}\
         .hills{{position:absolute;left:{shift}%;width:300%;bottom:40%;height:16%;\
           background:repeating-linear-gradient(90deg,hsl({hue} 25% 30%) 0 90px,hsl({hue} 22% 38%) 90px 180px)}}\
         .road{{position:absolute;inset:55% 0 0 0;background:hsl({hue} 8% 26%);\
           clip-path:polygon(0 0,100% 0,100% 100%,0 100%)}}\
         .road::after{{content:\"\";position:absolute;left:49%;width:2%;top:0;bottom:0;\
           background:repeating-linear-gradient(#e8e0b0 0 14%,transparent 14% 28%)}}\
         .tag{{position:absolute;left:8px;top:8px;padding:4px 6px;background:rgba(0,0,0,.55)}}\
         .deny{{position:absolute;inset:0;display:grid;place-items:center;background:#202124;\
           color:#e8eaed;text-align:center;padding:16px}}\
         </style>{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(base: &str, path_query: &str) -> (u16, String) {
        let addr = base.trim_start_matches("http://");
        let mut s = TcpStream::connect(addr).expect("connect");
        write!(s, "GET {path_query} HTTP/1.1\r\nHost: x\r\n\r\n").expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).expect("read");
        let code = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (code, body)
    }

    #[test]
    fn serves_metadata_and_embed_pages() {
        let m = MockServer::streetview(StreetviewMock::default());
        let (c, b) = get(
            &m.base,
            "/maps/api/streetview/metadata?location=47.5%2C19.0&key=K",
        );
        assert_eq!(c, 200);
        assert!(
            b.contains("\"status\":\"OK\"") && b.contains("MOCK475000_190000"),
            "{b}"
        );
        let (_, b) = get(&m.base, "/maps/api/streetview/metadata?key=K");
        assert!(b.contains("INVALID_REQUEST"));
        let (c, b) = get(
            &m.base,
            "/maps/embed/v1/streetview?key=K&pano=ABC&heading=90",
        );
        assert_eq!(c, 200);
        assert!(b.contains("ABC") && b.contains("irány 90"));
        let (c, _) = get(&m.base, "/nope");
        assert_eq!(c, 404);
        assert_eq!(m.request_count(), 4);
    }

    #[test]
    fn query_decoding() {
        assert_eq!(pct_decode("47.5%2C19.0"), "47.5,19.0");
        assert_eq!(pct_decode("a%2"), "a%2");
        assert_eq!(pct_decode("%zz"), "%zz");
        assert_eq!(pct_decode("a+b"), "a b");
        let q = parse_query("a=1&b=%26&c");
        assert_eq!(q[1], ("b".to_string(), "&".to_string()));
        assert_eq!(q[2], ("c".to_string(), String::new()));
    }
}
