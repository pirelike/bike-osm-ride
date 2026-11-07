//! GPX / bikerouter import → processed [`Route`], plus local persistence.
//!
//! Modules: [`gpx`] (tolerant parser), [`geo`] (haversine, hashing), [`build`] (clean →
//! resample ~10 m → smooth elevation → grade → ascent/descent → bounds/waypoints/id),
//! [`climbs`] (climb detection and categories), [`bikerouter`] (link parsing, BRouter
//! API; the exact supported link formats are documented there), [`elevation`]
//! (Open-Meteo fallback for tracks without elevation), [`demo`] (built-in route),
//! [`store`] (settings / recent routes under `%LOCALAPPDATA%\bike-osm-ride`, or
//! `$RIDE_DATA_DIR`), [`activity`] / [`matching`] / [`import`] (a recorded GPX or TCX activity
//! projected onto a route and stored as a ride) and [`rides`] (the ride log: one record per finished ride, with the
//! segment times the ghost and the PRs are built from).
//!
//! The async entry points below are what `ride-app` calls. Network access happens only
//! in [`load_route_from_bikerouter`] and, when requested and needed, for missing
//! elevation. CPU-heavy work runs on tokio's blocking pool when called inside a
//! runtime. All error messages are Hungarian and safe to show to the user.

use std::path::{Path, PathBuf};

use ride_i18n::{t, tf};
use ride_model::{Route, RouteSource};

pub mod activity;
pub mod backup;
pub mod bikerouter;
pub mod build;
pub mod climbs;
pub mod demo;
pub mod elevation;
pub mod geo;
pub mod gpx;
pub mod http;
pub mod import;
pub mod matching;
pub mod rides;
pub mod store;

pub use demo::demo_route;

/// Error with a Hungarian, user-presentable message (shown in a toast / on the welcome screen).
#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, RouteError>;

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(RouteError::Message(msg.into()))
}

/// Files larger than this are refused before reading (a 100 km track is ~8 MB).
pub const MAX_GPX_BYTES: u64 = 64 * 1024 * 1024;

/// Run CPU-bound work on the blocking pool when inside a tokio runtime, else inline.
async fn run_blocking<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        match tokio::task::spawn_blocking(f).await {
            Ok(r) => r,
            Err(_) => err(t("route.az_utvonal_feldolgozasa_megszakadt")),
        }
    } else {
        f()
    }
}

fn file_error(path: &Path, e: &std::io::Error) -> RouteError {
    let shown = path.display();
    let msg = match e.kind() {
        std::io::ErrorKind::NotFound => tf(
            "route.a_fajl_nem_talalhato_shown",
            &[("shown", &shown.to_string())],
        ),
        std::io::ErrorKind::PermissionDenied => tf(
            "route.nincs_jogosultsag_a_fajl_olvasasahoz_shown",
            &[("shown", &shown.to_string())],
        ),
        _ => tf(
            "route.a_fajl_nem_olvashato_e_shown",
            &[("e", &e.to_string()), ("shown", &shown.to_string())],
        ),
    };
    RouteError::Message(msg)
}

/// Parse + build; if the result has no usable elevation and `elevation_api` is given,
/// look the elevation up there and rebuild. A failed lookup is not an error: the
/// route is returned with `has_elevation = false`.
async fn build_from_bytes(
    bytes: Vec<u8>,
    hint: String,
    source: RouteSource,
    elevation_api: Option<&str>,
) -> Result<Route> {
    let hint_first = hint.clone();
    let (data, route) = run_blocking(move || {
        let data = gpx::parse_bytes(&bytes)?;
        let route = build::build_route(&data, &hint_first, source)?;
        Ok((data, route))
    })
    .await?;
    let Some(api) = elevation_api.filter(|_| !route.has_elevation) else {
        return Ok(route);
    };

    let mut data = data;
    if elevation::fill_from(api, &mut data.points).await.is_err() {
        return Ok(route);
    }
    match run_blocking(move || build::build_route(&data, &hint, source)).await {
        Ok(better) if better.has_elevation => Ok(better),
        _ => Ok(route),
    }
}

/// Read a GPX file from disk and build a route. If the file has no (or mostly no)
/// elevation and `fetch_missing_elevation` is true, elevation is fetched online
/// (Open-Meteo); on failure the route is still returned with `has_elevation = false`.
pub async fn load_route_from_path(path: PathBuf, fetch_missing_elevation: bool) -> Result<Route> {
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| file_error(&path, &e))?;
    if !meta.is_file() {
        return err(t("route.a_kivalasztott_elem_nem_fajl"));
    }
    if meta.len() > MAX_GPX_BYTES {
        return err(tf(
            "route.a_fajl_tul_nagy_legfeljebb_mb_engedelyezett",
            &[("mb", &(MAX_GPX_BYTES / (1024 * 1024)).to_string())],
        ));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| file_error(&path, &e))?;
    let hint = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let api = fetch_missing_elevation.then_some(elevation::OPEN_METEO_URL);
    build_from_bytes(bytes, hint, RouteSource::GpxFile, api).await
}

/// Build a route from raw file bytes (UTF-8/UTF-16, BOM tolerated), e.g. from a
/// drag-and-drop payload. `name_hint` is used when the GPX has no name.
pub async fn load_route_from_bytes(
    bytes: Vec<u8>,
    name_hint: &str,
    source: RouteSource,
    fetch_missing_elevation: bool,
) -> Result<Route> {
    if bytes.len() as u64 > MAX_GPX_BYTES {
        return err(tf(
            "route.a_fajl_tul_nagy_legfeljebb_mb_engedelyezett",
            &[("mb", &(MAX_GPX_BYTES / (1024 * 1024)).to_string())],
        ));
    }
    let api = fetch_missing_elevation.then_some(elevation::OPEN_METEO_URL);
    build_from_bytes(bytes, name_hint.to_string(), source, api).await
}

/// Build a route from GPX text. `name_hint` is used when the GPX has no name.
pub async fn load_route_from_text(
    text: &str,
    name_hint: &str,
    source: RouteSource,
    fetch_missing_elevation: bool,
) -> Result<Route> {
    load_route_from_bytes(
        text.as_bytes().to_vec(),
        name_hint,
        source,
        fetch_missing_elevation,
    )
    .await
}

/// `29,4` style kilometres for names (the language's decimal separator).
fn km_hu(metres: f64) -> String {
    ride_i18n::decimal(metres / 1000.0, 1)
}

/// Accepts a bikerouter.de / brouter-web share link (or a raw BRouter API URL),
/// calls the BRouter server (`https://brouter.de/brouter?...&format=gpx`) and builds the route.
///
/// See [`bikerouter`] for the exact link formats.
pub async fn load_route_from_bikerouter(link: &str) -> Result<Route> {
    let name = t("route.bikerouter_utvonal");
    let request = bikerouter::parse_link(link)?;
    let gpx_text = bikerouter::fetch_gpx(&request).await?;
    let mut route = build_from_bytes(
        gpx_text.into_bytes(),
        name.to_string(),
        RouteSource::Bikerouter,
        Some(elevation::OPEN_METEO_URL),
    )
    .await?;
    if route.name == name {
        route.name = tf(
            "route.bikerouter.name_with_km",
            &[("name", name), ("km", &km_hu(route.total_dist))],
        );
    }
    Ok(route)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{Canned, Server};

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    #[tokio::test]
    async fn loads_the_short_brouter_fixture_from_disk() {
        let r = load_route_from_path(fixture("brouter_budapest_short.gpx"), false)
            .await
            .unwrap();
        assert_eq!(r.source, RouteSource::GpxFile);
        assert_eq!(
            r.name, "brouter_budapest_short",
            "generic BRouter track name falls back to the file name"
        );
        assert!(r.has_elevation);
        assert!((r.total_dist - 3006.0).abs() < 60.0, "{}", r.total_dist);
        assert_eq!(r.points[0].dist, 0.0);
        assert!(r
            .points
            .windows(2)
            .all(|w| w[1].dist > w[0].dist && w[1].dist - w[0].dist <= 10.0 + 1e-9));
    }

    #[tokio::test]
    async fn file_errors_are_hungarian() {
        let e = load_route_from_path(PathBuf::from("Z:/nincs/ilyen/fajl.gpx"), false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("nem található"), "{e}");
        let dir = tempfile::tempdir().unwrap();
        let e = load_route_from_path(dir.path().to_path_buf(), false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("nem fájl"), "{e}");
        let bad = dir.path().join("x.gpx");
        std::fs::write(&bad, "ez nem gpx").unwrap();
        let e = load_route_from_path(bad, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("GPX"), "{e}");
        let e = load_route_from_text("", "x", RouteSource::GpxFile, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("üres"), "{e}");
    }

    #[tokio::test]
    async fn text_loading_uses_gpx_name_or_hint() {
        let gpx = |name: &str| {
            format!(
                r#"<gpx>{name}<trk><trkseg>
                <trkpt lat="47.5" lon="19.0"><ele>100</ele></trkpt>
                <trkpt lat="47.502" lon="19.0"><ele>110</ele></trkpt>
                <trkpt lat="47.504" lon="19.0"><ele>120</ele></trkpt></trkseg></trk></gpx>"#
            )
        };
        let r = load_route_from_text(&gpx(""), "Saját név", RouteSource::GpxFile, false)
            .await
            .unwrap();
        assert_eq!(r.name, "Saját név");
        let r = load_route_from_text(
            &gpx("<metadata><name>Gpx név</name></metadata>"),
            "Saját név",
            RouteSource::Bikerouter,
            false,
        )
        .await
        .unwrap();
        assert_eq!(r.name, "Gpx név");
        assert_eq!(r.source, RouteSource::Bikerouter);
    }

    const NO_ELE: &str = r#"<gpx><trk><trkseg>
        <trkpt lat="47.5" lon="19.0"/><trkpt lat="47.502" lon="19.0"/><trkpt lat="47.504" lon="19.0"/>
        <trkpt lat="47.506" lon="19.0"/><trkpt lat="47.508" lon="19.0"/></trkseg></trk></gpx>"#;

    #[tokio::test]
    async fn missing_elevation_is_fetched_when_allowed() {
        let server = Server::start(vec![Canned {
            status: 200,
            content_type: "application/json",
            body: r#"{"elevation":[100.0,110.0,120.0,130.0,140.0]}"#.into(),
        }]);
        let api = format!("{}/v1/elevation", server.base);
        let r = build_from_bytes(
            NO_ELE.as_bytes().to_vec(),
            "x".into(),
            RouteSource::GpxFile,
            Some(&api),
        )
        .await
        .unwrap();
        assert!(r.has_elevation);
        assert!((r.max_ele - 140.0).abs() < 0.5 && (r.min_ele - 100.0).abs() < 0.5);
        assert!((r.ascent - 40.0).abs() < 1.5, "{}", r.ascent);
        assert_eq!(server.request_lines().len(), 1);
    }

    #[tokio::test]
    async fn failed_elevation_lookup_keeps_a_flat_route() {
        let server = Server::start(vec![Canned {
            status: 400,
            content_type: "application/json",
            body: r#"{"error":true,"reason":"nope"}"#.into(),
        }]);
        let api = format!("{}/v1/elevation", server.base);
        let r = build_from_bytes(
            NO_ELE.as_bytes().to_vec(),
            "x".into(),
            RouteSource::GpxFile,
            Some(&api),
        )
        .await
        .unwrap();
        assert!(!r.has_elevation);
        assert!(r.total_dist > 800.0);
        // not requested: no request at all
        let r = load_route_from_text(NO_ELE, "x", RouteSource::GpxFile, false)
            .await
            .unwrap();
        assert!(!r.has_elevation);
    }

    #[tokio::test]
    async fn bikerouter_link_end_to_end_against_a_local_server() {
        let gpx = std::fs::read_to_string(fixture("brouter_budapest_short.gpx")).unwrap();
        let server = Server::start(vec![Canned {
            status: 200,
            content_type: "application/gpx+xml; charset=utf-8",
            body: gpx,
        }]);
        let link = format!("{}/brouter?lonlats=19.040199,47.497899|19.0467,47.5008&profile=trekking&alternativeidx=0&format=gpx", server.base);
        let r = load_route_from_bikerouter(&link).await.unwrap();
        assert_eq!(r.source, RouteSource::Bikerouter);
        assert_eq!(r.name, "Bikerouter útvonal – 3,0 km");
        assert!(r.has_elevation);
        assert_eq!(server.request_lines().len(), 1);
        let e = load_route_from_bikerouter("https://bikerouter.de/")
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("nincs útvonal"), "{e}");
    }

    #[test]
    fn public_futures_are_send() {
        // The app may drive these with `tokio::spawn`; this only has to compile.
        fn is_send<T: Send>(_: &T) {}
        is_send(&load_route_from_path(PathBuf::new(), false));
        is_send(&load_route_from_text("", "", RouteSource::GpxFile, false));
        is_send(&load_route_from_bytes(
            Vec::new(),
            "",
            RouteSource::GpxFile,
            false,
        ));
        is_send(&load_route_from_bikerouter(""));
    }

    #[test]
    fn km_formatting_uses_a_decimal_comma() {
        assert_eq!(km_hu(29_404.0), "29,4");
        assert_eq!(km_hu(940.0), "0,9");
    }

    #[test]
    fn works_on_a_current_thread_runtime() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let r = rt
            .block_on(load_route_from_path(
                fixture("brouter_budapest_short.gpx"),
                false,
            ))
            .unwrap();
        assert!(r.total_dist > 2900.0);
    }
}
