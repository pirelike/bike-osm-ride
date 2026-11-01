//! Elevation fallback for tracks without (usable) `<ele>`: the Open-Meteo Elevation API.
//!
//! `GET https://api.open-meteo.com/v1/elevation?latitude=a,b,…&longitude=x,y,…`
//! returns `{"elevation":[…]}` (Copernicus DEM, 90 m grid, CC BY 4.0 – attribution is in
//! the README / about dialog). At most 100 coordinates per request.
//!
//! The server is public, so the load is kept low: the track is sampled about every
//! [`SAMPLE_SPACING_M`] (the DEM is not finer anyway, never more than [`MAX_SAMPLES`]
//! samples in total), requests run one after the other with a short pause, and the
//! elevation of the points in between is interpolated over distance.

use std::time::Duration;

use crate::geo;
use crate::gpx::RawPoint;
use crate::{err, http, Result};

pub const OPEN_METEO_URL: &str = "https://api.open-meteo.com/v1/elevation";
pub const MAX_COORDS_PER_REQUEST: usize = 100;
pub const SAMPLE_SPACING_M: f64 = 100.0;
pub const MAX_SAMPLES: usize = 3000;
const PAUSE_BETWEEN_REQUESTS: Duration = Duration::from_millis(150);
const RETRY_DELAY: Duration = Duration::from_millis(1500);

/// Which raw points get an elevation lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplePlan {
    /// Indices into the raw point slice (strictly increasing, first and last included).
    pub indices: Vec<usize>,
    /// Cumulative distance of every raw point, metres.
    pub cum_dist: Vec<f64>,
}

/// Choose the sample points of `points`.
pub fn plan_samples(points: &[RawPoint]) -> SamplePlan {
    let mut cum_dist = Vec::with_capacity(points.len());
    let mut acc = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            let q = &points[i - 1];
            let d = geo::haversine_m(q.lat, q.lon, p.lat, p.lon);
            if d.is_finite() {
                acc += d;
            }
        }
        cum_dist.push(acc);
    }
    let n = points.len();
    if n == 0 {
        return SamplePlan {
            indices: Vec::new(),
            cum_dist,
        };
    }
    let spacing = SAMPLE_SPACING_M.max(acc / (MAX_SAMPLES as f64 - 1.0));
    let mut indices = vec![0];
    let mut last_d = 0.0;
    for (i, &d) in cum_dist.iter().enumerate().skip(1) {
        if d - last_d >= spacing {
            indices.push(i);
            last_d = d;
        }
    }
    if indices.last() != Some(&(n - 1)) {
        indices.push(n - 1);
    }
    SamplePlan { indices, cum_dist }
}

/// Request URL for one chunk of `(lat, lon)` coordinates (five decimals ≈ 1 m).
pub fn request_url(base: &str, coords: &[(f64, f64)]) -> String {
    let lats = coords
        .iter()
        .map(|c| format!("{:.5}", c.0))
        .collect::<Vec<_>>()
        .join(",");
    let lons = coords
        .iter()
        .map(|c| format!("{:.5}", c.1))
        .collect::<Vec<_>>()
        .join(",");
    format!("{base}?latitude={lats}&longitude={lons}")
}

/// Parse `{"elevation":[…]}`; `null` entries become `None`.
pub fn parse_response(body: &str, expected: usize) -> Result<Vec<Option<f64>>> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return err("Open-Meteo: a válasz nem értelmezhető.");
    };
    if v.get("error").and_then(serde_json::Value::as_bool) == Some(true) {
        let reason = v
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ismeretlen hiba");
        return err(format!(
            "Open-Meteo: {}",
            reason.chars().take(160).collect::<String>()
        ));
    }
    let Some(arr) = v.get("elevation").and_then(serde_json::Value::as_array) else {
        return err("Open-Meteo: a válaszból hiányzik a magassági adat.");
    };
    if arr.len() != expected {
        return err(format!(
            "Open-Meteo: {} magassági érték érkezett {} helyett.",
            arr.len(),
            expected
        ));
    }
    Ok(arr
        .iter()
        .map(|x| x.as_f64().filter(|e| e.is_finite()))
        .collect())
}

/// Write elevations into `points`: looked-up values at the planned indices, linear
/// interpolation over distance in between (ends held). Fails when too many lookups are
/// missing to be trustworthy (less than half).
pub fn apply(points: &mut [RawPoint], plan: &SamplePlan, elevations: &[Option<f64>]) -> Result<()> {
    if plan.indices.len() != elevations.len() || plan.cum_dist.len() != points.len() {
        return err("Open-Meteo: a magassági adatok száma nem egyezik a kért pontokéval.");
    }
    let known: Vec<(f64, f64)> = plan
        .indices
        .iter()
        .zip(elevations)
        .filter_map(|(&i, e)| Some((*plan.cum_dist.get(i)?, (*e)?)))
        .collect();
    if known.len() * 2 < plan.indices.len() || known.is_empty() {
        return err("Open-Meteo: a pontok többségéhez nem érhető el magassági adat.");
    }
    let mut j = 0usize;
    for (p, &d) in points.iter_mut().zip(&plan.cum_dist) {
        while j + 1 < known.len() && known[j + 1].0 < d {
            j += 1;
        }
        let e = if d <= known[0].0 {
            known[0].1
        } else if j + 1 >= known.len() {
            known[known.len() - 1].1
        } else {
            let ((d0, e0), (d1, e1)) = (known[j], known[j + 1]);
            if d1 > d0 {
                e0 + (e1 - e0) * ((d - d0) / (d1 - d0)).clamp(0.0, 1.0)
            } else {
                e0
            }
        };
        p.ele = Some(e);
    }
    Ok(())
}

async fn fetch_chunk(
    client: &reqwest::Client,
    base: &str,
    coords: &[(f64, f64)],
) -> Result<Vec<Option<f64>>> {
    let url = request_url(base, coords);
    let mut last_err = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAY).await;
        }
        match http::get_text(client, &url, "Open-Meteo").await {
            Ok(f) if f.status == 200 => return parse_response(&f.body, coords.len()),
            Ok(f) if f.status == 429 || f.status >= 500 => {
                last_err = format!(
                    "Open-Meteo: a szerver most nem elérhető (HTTP {}).",
                    f.status
                );
            }
            Ok(f) => {
                // 4xx: our request is wrong, retrying will not help; surface the reason.
                return match parse_response(&f.body, coords.len()) {
                    Err(e) => Err(e),
                    Ok(_) => err(format!("Open-Meteo: váratlan válasz (HTTP {}).", f.status)),
                };
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    err(last_err)
}

/// Fetch elevations for every point of `points` from `base` (see [`OPEN_METEO_URL`]).
pub async fn fill_from(base: &str, points: &mut [RawPoint]) -> Result<()> {
    let plan = plan_samples(points);
    if plan.indices.len() < 2 {
        return err("Az útvonalnak legalább két pontja kell legyen a magasságadatok lekéréséhez.");
    }
    let coords: Vec<(f64, f64)> = plan
        .indices
        .iter()
        .map(|&i| (points[i].lat, points[i].lon))
        .collect();
    let client = http::client(Duration::from_secs(20))?;
    let mut elevations: Vec<Option<f64>> = Vec::with_capacity(coords.len());
    for (n, chunk) in coords.chunks(MAX_COORDS_PER_REQUEST).enumerate() {
        if n > 0 {
            tokio::time::sleep(PAUSE_BETWEEN_REQUESTS).await;
        }
        elevations.extend(fetch_chunk(&client, base, chunk).await?);
    }
    apply(points, &plan, &elevations)
}

/// Fetch elevations from the public Open-Meteo API.
pub async fn fill(points: &mut [RawPoint]) -> Result<()> {
    fill_from(OPEN_METEO_URL, points).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{Canned, Server};

    fn line(n: usize, step_deg: f64) -> Vec<RawPoint> {
        (0..n)
            .map(|i| RawPoint {
                lat: 47.5,
                lon: 19.0 + i as f64 * step_deg,
                ele: None,
            })
            .collect()
    }

    #[test]
    fn plan_includes_ends_and_respects_spacing() {
        // ~75 m between points -> every second point (>= 100 m).
        let pts = line(41, 0.001);
        let plan = plan_samples(&pts);
        assert_eq!(plan.indices.first(), Some(&0));
        assert_eq!(plan.indices.last(), Some(&40));
        assert!(plan.indices.windows(2).all(|w| w[1] > w[0]));
        for w in plan.indices.windows(2).take(plan.indices.len() - 2) {
            assert!(plan.cum_dist[w[1]] - plan.cum_dist[w[0]] >= SAMPLE_SPACING_M);
        }
        assert!(plan.indices.len() < 30);
        // single point / empty
        assert_eq!(plan_samples(&line(1, 0.001)).indices, vec![0]);
        assert!(plan_samples(&[]).indices.is_empty());
    }

    #[test]
    fn plan_is_capped_for_very_long_tracks() {
        // 400 km with a point every ~7.5 m.
        let pts = line(53_334, 0.0001);
        let plan = plan_samples(&pts);
        assert!(
            plan.indices.len() <= MAX_SAMPLES + 2,
            "{}",
            plan.indices.len()
        );
        assert_eq!(plan.indices.last(), Some(&(pts.len() - 1)));
    }

    #[test]
    fn url_format() {
        let url = request_url(
            "https://api.open-meteo.com/v1/elevation",
            &[(47.5, 19.0), (47.512_345_6, 19.1)],
        );
        assert_eq!(url, "https://api.open-meteo.com/v1/elevation?latitude=47.50000,47.51235&longitude=19.00000,19.10000");
    }

    #[test]
    fn response_parsing() {
        assert_eq!(
            parse_response(r#"{"elevation":[120.0,121.5,null]}"#, 3).unwrap(),
            vec![Some(120.0), Some(121.5), None]
        );
        assert!(parse_response(r#"{"elevation":[1.0]}"#, 2)
            .unwrap_err()
            .to_string()
            .contains("1 magassági"));
        assert!(parse_response("not json", 1).is_err());
        assert!(parse_response(r#"{"nothing":1}"#, 1).is_err());
        let e = parse_response(r#"{"error":true,"reason":"Too many coordinates"}"#, 1)
            .unwrap_err()
            .to_string();
        assert!(e.contains("Too many coordinates"), "{e}");
    }

    #[test]
    fn apply_interpolates_over_distance_and_tolerates_a_few_gaps() {
        let mut pts = line(11, 0.001);
        let plan = SamplePlan {
            indices: vec![0, 5, 10],
            cum_dist: plan_samples(&pts).cum_dist,
        };
        apply(&mut pts, &plan, &[Some(100.0), None, Some(200.0)]).unwrap();
        // the middle lookup is missing: straight line between the ends
        assert!((pts[5].ele.unwrap() - 150.0).abs() < 0.01);
        assert_eq!(pts[0].ele, Some(100.0));
        assert!((pts[10].ele.unwrap() - 200.0).abs() < 1e-9);
        // too many gaps -> error
        let mut pts = line(11, 0.001);
        let plan = SamplePlan {
            indices: vec![0, 5, 10],
            cum_dist: plan_samples(&pts).cum_dist,
        };
        assert!(apply(&mut pts, &plan, &[None, None, Some(1.0)]).is_err());
        // length mismatch -> error
        assert!(apply(&mut pts, &plan, &[Some(1.0)]).is_err());
    }

    #[tokio::test]
    async fn fill_from_local_server_in_chunks() {
        // 250 samples (every 100 m) -> 3 requests of 100/100/50 coordinates.
        let mut pts = line(251, 0.001_4);
        let plan = plan_samples(&pts);
        let total = plan.indices.len();
        assert!(total > 200 && total <= 300, "{total}");
        let chunk_json = |from: usize, to: usize| {
            let v: Vec<String> = (from..to)
                .map(|i| format!("{:.1}", 100.0 + i as f64))
                .collect();
            format!(r#"{{"elevation":[{}]}}"#, v.join(","))
        };
        let server = Server::start(vec![
            Canned {
                status: 200,
                content_type: "application/json",
                body: chunk_json(0, 100),
            },
            Canned {
                status: 200,
                content_type: "application/json",
                body: chunk_json(100, 200),
            },
            Canned {
                status: 200,
                content_type: "application/json",
                body: chunk_json(200, total),
            },
        ]);
        fill_from(&format!("{}/v1/elevation", server.base), &mut pts)
            .await
            .unwrap();
        assert_eq!(server.request_lines().len(), 3);
        assert!(
            server.request_lines()[0].starts_with("GET /v1/elevation?latitude=47.50000,"),
            "{:?}",
            server.request_lines()[0]
        );
        assert_eq!(pts[0].ele, Some(100.0));
        assert!((pts[250].ele.unwrap() - (100.0 + (total - 1) as f64)).abs() < 1e-9);
        assert!(pts.iter().all(|p| p.ele.is_some()));
        let mid = pts[125].ele.unwrap();
        assert!(mid > 150.0 && mid < 250.0, "{mid}");
    }

    #[tokio::test]
    async fn server_errors_surface_the_reason_without_retrying_client_errors() {
        let mut pts = line(30, 0.001);
        let server = Server::start(vec![Canned {
            status: 400,
            content_type: "application/json",
            body: r#"{"error":true,"reason":"Latitude must be in range of -90 to 90"}"#.into(),
        }]);
        let e = fill_from(&format!("{}/v1/elevation", server.base), &mut pts)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("Latitude must be"), "{e}");
        assert!(
            pts.iter().all(|p| p.ele.is_none()),
            "points untouched on failure"
        );
    }
}
