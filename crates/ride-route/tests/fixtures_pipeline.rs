//! End-to-end tests of the public API on the files in `tests/fixtures/`.
//!
//! Run with `cargo test -p ride-route --test fixtures_pipeline -- --nocapture` to see the
//! numbers the verification report quotes.

use std::path::PathBuf;
use std::time::Instant;

use ride_model::{Route, RouteSource};
use ride_route::gpx;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

async fn load(name: &str) -> ride_route::Result<Route> {
    ride_route::load_route_from_path(fixture(name), false).await
}

fn assert_route_invariants(r: &Route) {
    assert!(r.points.len() >= 2);
    assert_eq!(r.points[0].dist, 0.0);
    assert!((r.points.last().unwrap().dist - r.total_dist).abs() < 1e-9);
    for w in r.points.windows(2) {
        let gap = w[1].dist - w[0].dist;
        assert!(gap > 0.0 && gap <= 10.0 + 1e-9, "gap {gap}");
    }
    assert!(r.points.iter().all(|p| p.grade.abs() <= 25.0
        && p.lat.is_finite()
        && p.lon.is_finite()
        && p.ele.is_finite()));
    assert!(r.min_ele <= r.max_ele);
    assert!(r
        .points
        .iter()
        .all(|p| p.ele >= r.min_ele - 1e-9 && p.ele <= r.max_ele + 1e-9));
    assert!(r.points.iter().all(|p| p.lat >= r.bounds.south
        && p.lat <= r.bounds.north
        && p.lon >= r.bounds.west
        && p.lon <= r.bounds.east));
    for c in &r.climbs {
        assert!(
            c.length >= 500.0 && c.avg_grade >= 3.0 && c.gain >= 25.0,
            "{c:?}"
        );
        assert!(c.start_dist >= 0.0 && c.end_dist <= r.total_dist);
    }
    assert!(r
        .climbs
        .windows(2)
        .all(|w| w[0].end_dist <= w[1].start_dist));
}

#[tokio::test]
async fn dobogoko_climb_detection_and_ascent_against_the_brouter_header() {
    let bytes = std::fs::read(fixture("brouter_budapest_dobogoko.gpx")).unwrap();
    let header = gpx::parse_bytes(&bytes)
        .unwrap()
        .brouter
        .expect("BRouter header comment");
    let (track_len, filtered, plain) = (
        header.track_length_m.unwrap(),
        header.filtered_ascend_m.unwrap(),
        header.plain_ascend_m.unwrap(),
    );

    let r = load("brouter_budapest_dobogoko.gpx").await.unwrap();
    assert_route_invariants(&r);
    println!(
        "DOBOGOKO header: track-length={track_len} m, filtered ascend={filtered} m, plain-ascend={plain} m\n\
         DOBOGOKO ours:   total_dist={:.0} m, ascent={:.1} m, descent={:.1} m, net={:.1} m, min={:.1} m, max={:.1} m, points={}, climbs={}",
        r.total_dist,
        r.ascent,
        r.descent,
        r.ascent - r.descent,
        r.min_ele,
        r.max_ele,
        r.points.len(),
        r.climbs.len()
    );
    println!(
        "DOBOGOKO deltas: length {:+.2} %, ascent vs filtered {:+.1} m ({:+.1} %), net vs plain-ascend {:+.1} m",
        100.0 * (r.total_dist - track_len) / track_len,
        r.ascent - filtered,
        100.0 * (r.ascent - filtered) / filtered,
        (r.ascent - r.descent) - plain
    );
    // BRouter (RoutingEngine.recalcTrack) discounts dips of up to 10 m, i.e. it behaves like a
    // ~10 m hysteresis; our default is the 3 m of the project brief. Show both.
    let ele: Vec<f64> = r.points.iter().map(|p| p.ele).collect();
    let (a5, _) = ride_route::build::ascent_descent(&ele, 5.0);
    let (a10, _) = ride_route::build::ascent_descent(&ele, 10.0);
    println!("DOBOGOKO ascent by hysteresis: 3 m -> {:.1}, 5 m -> {a5:.1}, 10 m -> {a10:.1} (BRouter filtered {filtered})", r.ascent);
    for (i, c) in r.climbs.iter().enumerate() {
        println!(
            "  climb {}: km {:.2}-{:.2} len {:.0} m gain {:.0} m avg {:.1} % max {:.1} % ele {:.0}->{:.0} {:?}",
            i + 1,
            c.start_dist / 1000.0,
            c.end_dist / 1000.0,
            c.length,
            c.gain,
            c.avg_grade,
            c.max_grade,
            c.start_ele,
            c.end_ele,
            c.category
        );
    }

    // Distance: haversine over the same points BRouter measured its track-length on.
    assert!(
        (r.total_dist - track_len).abs() / track_len < 0.01,
        "length {} vs {}",
        r.total_dist,
        track_len
    );
    // Ascent: BRouter's "filtered ascend" is its own noise-filtered figure. We use a
    // different filter (50 m smoothing + 3 m hysteresis), so require agreement within 10 %.
    assert!(
        (r.ascent - filtered).abs() / filtered < 0.10,
        "ascent {} vs filtered {}",
        r.ascent,
        filtered
    );
    // The net change is geometry, not filtering: it must match the start/end elevations.
    assert!(
        ((r.ascent - r.descent) - plain).abs() < 4.0,
        "net {} vs plain {}",
        r.ascent - r.descent,
        plain
    );
    assert!(
        (a10 - filtered).abs() / filtered < 0.06,
        "ascent(10 m) {a10} vs filtered {filtered}"
    );
    assert!(r.ascent >= a10 && a10 >= 0.9 * r.ascent);
    assert!(r.has_elevation && r.max_ele > 650.0 && r.min_ele < 270.0);

    // Hűvösvölgy -> Dobogókő has a long sustained climb; the biggest one must be a real
    // categorised climb and together the climbs must carry most of the ascent.
    assert!(!r.climbs.is_empty());
    let biggest = r
        .climbs
        .iter()
        .max_by(|a, b| a.gain.total_cmp(&b.gain))
        .unwrap();
    assert!(
        biggest.length > 3000.0 && biggest.gain > 200.0 && biggest.category.is_some(),
        "{biggest:?}"
    );
    let climbed: f64 = r.climbs.iter().map(|c| c.gain).sum();
    assert!(
        climbed > 0.6 * r.ascent,
        "climbs gain {climbed} of ascent {}",
        r.ascent
    );
    // every climb gain is consistent with the elevations at its ends
    for c in &r.climbs {
        let a = r.locate(c.start_dist).ele;
        let b = r.locate(c.end_dist).ele;
        assert!((b - a - c.gain).abs() < 0.5, "{c:?} {a} {b}");
    }
}

#[tokio::test]
async fn short_brouter_fixture_matches_its_header() {
    let bytes = std::fs::read(fixture("brouter_budapest_short.gpx")).unwrap();
    let header = gpx::parse_bytes(&bytes).unwrap().brouter.unwrap();
    let r = load("brouter_budapest_short.gpx").await.unwrap();
    assert_route_invariants(&r);
    let ele: Vec<f64> = r.points.iter().map(|p| p.ele).collect();
    let a10 = ride_route::build::ascent_descent(&ele, 10.0).0;
    println!(
        "SHORT header: len {:?} filtered {:?}; ours: len {:.0} ascent(3 m) {:.1} ascent(10 m) {a10:.1} descent {:.1}",
        header.track_length_m, header.filtered_ascend_m, r.total_dist, r.ascent, r.descent
    );
    assert!((r.total_dist - header.track_length_m.unwrap()).abs() < 30.0);
    // BRouter's filter tolerates 10 m dips, so compare against our 10 m figure; the default
    // 3 m hysteresis counts a few more small wiggles on such a short, almost flat track.
    assert!(
        (a10 - header.filtered_ascend_m.unwrap()).abs() < 8.0,
        "ascent(10 m) {a10}"
    );
    assert!(r.ascent >= a10);
    assert!(
        (r.ascent - r.descent - (-3.0)).abs() < 5.0,
        "net {} vs plain-ascend -3",
        r.ascent - r.descent
    );
}

#[tokio::test]
async fn rte_and_wpt_fixture() {
    let r = load("rte_wpt.gpx").await.unwrap();
    assert_route_invariants(&r);
    assert_eq!(r.name, "Moszkva tér - János-hegy");
    assert_eq!(r.source, RouteSource::GpxFile);
    assert!(r.has_elevation);
    assert!((r.max_ele - 527.0).abs() < 1.0 && (r.min_ele - 152.0).abs() < 1.0);
    // 4 waypoints in the file, the one at the Balaton is far from the route and dropped
    assert_eq!(r.waypoints.len(), 3, "{:?}", r.waypoints);
    let names: Vec<&str> = r.waypoints.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"Libegő & Zugligeti út"), "{names:?}");
    assert!(r
        .waypoints
        .windows(2)
        .all(|w| w[0].dist_on_route <= w[1].dist_on_route));
    let last = r.waypoints.last().unwrap();
    assert!(
        (last.dist_on_route - r.total_dist).abs() < 80.0,
        "summit wpt at {} of {}",
        last.dist_on_route,
        r.total_dist
    );
    assert_eq!(
        r.waypoints
            .iter()
            .find(|w| w.name == "Normafa")
            .unwrap()
            .ele,
        Some(455.0)
    );
}

#[tokio::test]
async fn multiple_segments_are_joined_and_extensions_ignored() {
    let bytes = std::fs::read(fixture("multi_segment.gpx")).unwrap();
    let data = gpx::parse_bytes(&bytes).unwrap();
    assert_eq!(data.segments, 3);
    assert_eq!(data.points.len(), 257);
    assert!(
        data.points.iter().all(|p| p.ele.is_some()),
        "hr/cad extension values must not leak into <ele>"
    );
    let r = load("multi_segment.gpx").await.unwrap();
    assert_route_invariants(&r);
    assert_eq!(r.name, "Három szakasz - János-hegy");
    assert!(
        r.total_dist > 5000.0 && r.total_dist < 7500.0,
        "{}",
        r.total_dist
    );
    assert!(r.ascent > 330.0 && r.ascent < 420.0, "ascent {}", r.ascent);
}

#[tokio::test]
async fn track_without_elevation_is_flat_and_flagged() {
    let r = load("no_elevation.gpx").await.unwrap();
    assert_route_invariants(&r);
    assert!(!r.has_elevation);
    assert_eq!(
        (r.ascent, r.descent, r.min_ele, r.max_ele),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert!(r.climbs.is_empty());
    assert!(r.total_dist > 3000.0);
}

#[tokio::test]
async fn broken_file_is_a_clear_error() {
    let e = load("broken_truncated.gpx").await.unwrap_err().to_string();
    assert!(e.contains("GPX"), "{e}");
    println!("BROKEN -> {e}");
}

#[tokio::test]
async fn bom_crlf_and_prefixed_namespace() {
    let bytes = std::fs::read(fixture("bom_namespaced.gpx")).unwrap();
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "fixture really starts with a UTF-8 BOM"
    );
    let r = load("bom_namespaced.gpx").await.unwrap();
    assert_route_invariants(&r);
    assert_eq!(r.name, "BOM és előtag");
    assert!(r.has_elevation);
}

#[tokio::test]
async fn same_geometry_gives_the_same_id_from_file_and_text() {
    let text = std::fs::read_to_string(fixture("brouter_budapest_short.gpx")).unwrap();
    let a = load("brouter_budapest_short.gpx").await.unwrap();
    let b = ride_route::load_route_from_text(&text, "x", RouteSource::Bikerouter, false)
        .await
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_ne!(a.id, load("rte_wpt.gpx").await.unwrap().id);
}

#[tokio::test]
async fn demo_route_roundtrips_through_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = ride_route::store::Store::at(dir.path());
    let demo = ride_route::demo_route();
    store.save_recent(&demo).unwrap();
    let back = store.load_recent(&demo.id).unwrap();
    assert_eq!(back, demo);
    assert_eq!(store.list_recent()[0].name, demo.name);
}

/// Generate a GPX of `n` points along ~`km` km with several hills and ~1 m of noise.
fn big_gpx(n: usize, km: f64) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(n * 64 + 200);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"perf\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>perf</name><trkseg>\n");
    let step_m = km * 1000.0 / (n as f64 - 1.0);
    let m_per_deg_lat = 111_195.0;
    let (mut lat, mut lon) = (47.0f64, 17.0f64);
    for i in 0..n {
        let d = i as f64 * step_m;
        let bearing = (70.0 + 25.0 * (d / 9_000.0).sin()).to_radians();
        lat += step_m * bearing.cos() / m_per_deg_lat;
        lon += step_m * bearing.sin() / (m_per_deg_lat * lat.to_radians().cos());
        let ele = 200.0
            + 150.0 * (d / 7_000.0).sin()
            + 60.0 * (d / 1_300.0).sin()
            + ((i * 7919) % 100) as f64 / 50.0;
        let _ = writeln!(
            s,
            "<trkpt lat=\"{lat:.6}\" lon=\"{lon:.6}\"><ele>{ele:.2}</ele></trkpt>"
        );
    }
    s.push_str("</trkseg></trk></gpx>\n");
    s
}

#[tokio::test]
async fn performance_100_km_100k_points_under_thirty_seconds() {
    let text = big_gpx(100_000, 100.0);
    let mb = text.len() as f64 / 1e6;
    // phase breakdown (single thread, same code the async API runs on the blocking pool)
    let t0 = Instant::now();
    let data = gpx::parse(&text).unwrap();
    let t_parse = t0.elapsed();
    let t1 = Instant::now();
    let _ = ride_route::build::build_route(&data, "perf", RouteSource::GpxFile).unwrap();
    let t_build = t1.elapsed();
    println!(
        "PERF phases: parse {:.0} ms, build {:.0} ms",
        t_parse.as_secs_f64() * 1000.0,
        t_build.as_secs_f64() * 1000.0
    );

    let t = Instant::now();
    let r = ride_route::load_route_from_text(&text, "perf", RouteSource::GpxFile, false)
        .await
        .unwrap();
    let elapsed = t.elapsed();
    println!(
        "PERF {} points / {mb:.1} MB GPX -> {} route points, {:.1} km, ascent {:.0} m, {} climbs in {:.0} ms ({})",
        100_000,
        r.points.len(),
        r.total_dist / 1000.0,
        r.ascent,
        r.climbs.len(),
        elapsed.as_secs_f64() * 1000.0,
        if cfg!(debug_assertions) { "dev profile" } else { "release profile" }
    );
    assert_route_invariants(&r);
    assert!(
        (r.total_dist - 100_000.0).abs() < 1_500.0,
        "{}",
        r.total_dist
    );
    // A wall-clock bound measures the machine, not the code: this one only catches a blow-up
    // (a quadratic step on 100 000 points takes minutes), not a slow or busy CI runner.
    assert!(elapsed.as_secs_f64() < 30.0, "took {elapsed:?}");
}

// ---- manual network checks: `cargo test -p ride-route --test fixtures_pipeline -- --ignored --nocapture`

#[tokio::test]
#[ignore = "talks to the public BRouter server; run manually"]
async fn live_bikerouter_share_link() {
    let link = "https://bikerouter.de/#map=14/47.5030/19.0420/standard&lonlats=19.040199,47.497899;19.0467,47.5008&profile=trekking";
    let r = ride_route::load_route_from_bikerouter(link).await.unwrap();
    println!(
        "LIVE bikerouter: {} -> {:.0} m, {} points, ascent {:.1} m, elevation {}",
        r.name,
        r.total_dist,
        r.points.len(),
        r.ascent,
        r.has_elevation
    );
    assert_eq!(r.source, RouteSource::Bikerouter);
    assert!(r.has_elevation && r.total_dist > 200.0 && r.total_dist < 2000.0);
}

#[tokio::test]
#[ignore = "talks to the public Open-Meteo API; run manually"]
async fn live_open_meteo_fills_missing_elevation() {
    let r = ride_route::load_route_from_path(fixture("no_elevation.gpx"), true)
        .await
        .unwrap();
    println!(
        "LIVE open-meteo: has_elevation={} min {:.0} max {:.0} ascent {:.1}",
        r.has_elevation, r.min_ele, r.max_ele, r.ascent
    );
    assert!(r.has_elevation);
    // Pest side of Budapest: roughly 95-200 m above sea level.
    assert!(r.min_ele > 80.0 && r.max_ele < 250.0);
}
