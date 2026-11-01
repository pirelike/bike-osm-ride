//! A local imitation of the two Google endpoints for QA (no key, no network):
//!
//!   cargo run -p ride-ui-streetview --example mock_server -- 9300
//!   set RIDE_STREETVIEW_BASE=http://127.0.0.1:9300     (then start the app)
//!
//! Any key works except the special ones `BADKEY` (REQUEST_DENIED), `QUOTAKEY`
//! (OVER_QUERY_LIMIT), `HTTP500KEY` (HTTP 500) and `GARBAGEKEY` (HTML instead of JSON).
//! Optional second argument: artificial delay per metadata reply in ms. Third argument
//! `gap`: roughly every third kilometre of longitude has no panorama (to see the
//! "not available" state). Prints every request line (the key is masked).

use std::sync::Arc;

use ride_ui_streetview::mock::{MockServer, StreetviewMock};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().and_then(|a| a.parse().ok()).unwrap_or(9300);
    let delay_ms: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    let gaps = args.next().is_some_and(|a| a == "gap");
    let opts = StreetviewMock {
        delay_ms,
        covered: if gaps {
            // ~ every third 0.01 degree of longitude (about 750 m) has no panorama
            Arc::new(|_, lng| ((lng * 100.0).floor() as i64).rem_euclid(3) != 0)
        } else {
            Arc::new(|_, _| true)
        },
    };
    let server = MockServer::streetview_on(&format!("127.0.0.1:{port}"), opts)?;
    println!("mock Street View on {} (Ctrl+C to stop)", server.base);
    let mut shown = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let lines = server.request_lines();
        for l in lines.iter().skip(shown) {
            // never print the key, even though this is only the mock
            let masked = match l.find("key=") {
                Some(i) => {
                    let end = l[i..].find(['&', ' ']).map_or(l.len(), |e| i + e);
                    format!("{}key=***{}", &l[..i], &l[end..])
                }
                None => l.clone(),
            };
            println!("{masked}");
        }
        shown = lines.len();
    }
}
