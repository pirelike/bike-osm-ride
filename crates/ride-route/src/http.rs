//! Minimal HTTP helper (reqwest + rustls) shared by BRouter and Open-Meteo.
//!
//! Public servers: one request at a time, a descriptive User-Agent, timeouts, a hard
//! cap on the response size and Hungarian error messages.

use std::time::Duration;

use crate::{err, Result};

pub(crate) const USER_AGENT: &str = "bike-osm-ride/0.1 (desktop app)";
/// Largest response body we accept (a 1000 km BRouter GPX is far below this).
pub(crate) const MAX_BODY_BYTES: usize = 48 * 1024 * 1024;

pub(crate) fn client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .or_else(|e| err(format!("A hálózati kapcsolat nem hozható létre: {e}")))
}

fn describe(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "időtúllépés: a szerver nem válaszolt időben".to_string()
    } else if e.is_connect() {
        "nem sikerült kapcsolódni (nincs internet, vagy a szerver nem érhető el)".to_string()
    } else if e.is_decode() || e.is_body() {
        "a válasz nem olvasható".to_string()
    } else {
        // The library's own text is English and may contain the (long) URL: keep it out.
        "hálózati hiba".to_string()
    }
}

/// Result of a GET: status code and the (size-capped) body as lossy UTF-8.
pub(crate) struct Fetched {
    pub status: u16,
    pub body: String,
}

/// GET `url`. `what` names the service in error messages ("BRouter", "Open-Meteo").
pub(crate) async fn get_text(client: &reqwest::Client, url: &str, what: &str) -> Result<Fetched> {
    let mut resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => return err(format!("{what}: {}.", describe(e))),
    };
    let status = resp.status().as_u16();
    if resp
        .content_length()
        .is_some_and(|l| l > MAX_BODY_BYTES as u64)
    {
        return err(format!("{what}: a válasz túl nagy."));
    }
    let mut body: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_BODY_BYTES {
                    return err(format!("{what}: a válasz túl nagy."));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => return err(format!("{what}: a letöltés megszakadt ({}).", describe(e))),
        }
    }
    Ok(Fetched {
        status,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// First line of a server error text, trimmed to something toast-sized.
pub(crate) fn short_server_text(body: &str) -> String {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let line: String = line.chars().filter(|c| !c.is_control()).take(160).collect();
    line
}

#[cfg(test)]
pub(crate) mod testserver {
    //! Tiny one-shot-per-connection HTTP/1.1 server for tests (std only).

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    pub struct Canned {
        pub status: u16,
        pub content_type: &'static str,
        pub body: String,
    }

    pub struct Server {
        pub base: String,
        pub requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl Server {
        /// Answer the next `responses.len()` connections in order, then stop.
        pub fn start(responses: Vec<Canned>) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let log = Arc::clone(&requests);
            let handle = std::thread::spawn(move || {
                for canned in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        return;
                    };
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&buf);
                    let line = head.lines().next().unwrap_or("").to_string();
                    if let Ok(mut l) = log.lock() {
                        l.push(line);
                    }
                    let reply = format!(
                        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        canned.status,
                        canned.content_type,
                        canned.body.len(),
                        canned.body
                    );
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                }
            });
            Server {
                base,
                requests,
                handle: Some(handle),
            }
        }

        pub fn request_lines(&self) -> Vec<String> {
            self.requests.lock().map(|l| l.clone()).unwrap_or_default()
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            // Unblock a listener that is still waiting for connections.
            if let Some(h) = self.handle.take() {
                if h.is_finished() {
                    let _ = h.join();
                }
            }
        }
    }
}
