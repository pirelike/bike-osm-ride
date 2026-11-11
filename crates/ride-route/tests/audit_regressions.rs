//! Regression tests for the confirmed findings of the 2026-11-10 audit (`BUGS.md`, BUG-241…251).

use std::io::Cursor;
use std::sync::Arc;

use ride_model::{RideRecord, RiderSettings, Route, RouteSource, SensorSample};
use ride_route::store::Store;
use ride_route::{activity, backup, build, climbs, gpx, import};

fn route(id: &str, len: f64) -> Route {
    let mut route = ride_model::testing::straight_route(&[(len, 0.0)], 100.0);
    route.id = id.into();
    route
}

fn recorded(route: &Route, seconds: u32) -> RideRecord {
    let mut engine = ride_sim::RideEngine::new(
        Arc::new(route.clone()),
        RiderSettings {
            use_trainer_speed: true,
            ..RiderSettings::default()
        },
    );
    engine.start(1_700_000_000.0);
    for t in 1..=seconds {
        let sample = SensorSample {
            power_w: Some(200.0),
            speed_ms: Some(10.0),
            ..SensorSample::default()
        };
        engine.step(1.0, &sample, 1_700_000_000.0 + f64::from(t));
    }
    engine.finish();
    RideRecord {
        version: ride_model::RIDE_RECORD_VERSION,
        route_id: route.id.clone(),
        route_name: route.name.clone(),
        simulated: false,
        summary: engine.summary(),
        points: engine.history().to_vec(),
        source: ride_model::RideSource::Recorded,
        power_estimated: false,
        estimate: None,
        measured_power: Vec::new(),
    }
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    (dir, store)
}

fn edit_index(dir: &std::path::Path, route_id: &str, edit: impl FnOnce(&mut serde_json::Value)) {
    let index = dir.join("rides").join(route_id).join("index.json");
    let mut metas: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
    edit(&mut metas);
    std::fs::write(&index, serde_json::to_vec(&metas).unwrap()).unwrap();
}

#[test]
fn a_wrong_length_curve_is_recomputed() {
    let (dir, store) = store();
    let route = route("audit-route", 10_000.0);
    store.save_recent(&route).unwrap();
    store
        .save_ride(&recorded(&route, 61), &route)
        .unwrap()
        .unwrap();
    edit_index(dir.path(), &route.id, |m| {
        m[0]["mmp"] = serde_json::json!([999.0])
    });
    let pending = |s: &Store| {
        ride_sim::power_curve::analyze(&s.list_all_rides(), 1_700_000_100.0).rides_pending
    };
    assert_eq!(pending(&store), 1);
    assert_eq!(store.backfill_analysis(), 1);
    assert_eq!(pending(&store), 0);
}

#[test]
fn an_unknown_record_version_is_ignored() {
    let (_dir, store) = store();
    let route = route("audit-route", 10_000.0);
    store.save_recent(&route).unwrap();
    let mut old = recorded(&route, 61);
    old.version = 0;
    let meta = store.save_ride(&old, &route).unwrap().unwrap();
    assert!(store.load_ride(&route.id, &meta.stem).is_none());
}

#[test]
fn a_huge_index_time_does_not_panic_the_analysis() {
    let (dir, store) = store();
    let route = route("audit-route", 10_000.0);
    store.save_recent(&route).unwrap();
    store
        .save_ride(&recorded(&route, 61), &route)
        .unwrap()
        .unwrap();
    edit_index(dir.path(), &route.id, |m| {
        m[0]["started_unix_s"] = serde_json::json!(1e300)
    });
    let all = store.list_all_rides();
    assert!(
        std::panic::catch_unwind(|| ride_sim::power_curve::analyze(&all, 1_700_000_100.0)).is_ok()
    );
}

#[test]
fn cleanup_keeps_the_temp_file_of_a_live_process() {
    let dir = tempfile::tempdir().unwrap();
    // the test process itself is alive, but the cleanup protects its own PID: use the parent
    // (cargo) when there is one, else any other live process id is not available portably.
    let path = dir
        .path()
        .join(format!("settings.json.{}.tmp", std::process::id() + 1));
    std::fs::write(&path, b"another instance is writing this").unwrap();
    ride_route::store::remove_stale_temp_files(dir.path());
    assert!(
        path.exists(),
        "a fresh temp file of another process was deleted"
    );
}

#[test]
fn signed_timestamp_fields_are_rejected() {
    for text in [
        "2026-11-10T-1:00:00Z",
        "2026-11-10T00:-1:00Z",
        "2026-11-10T00:00:-1Z",
        "2026-11-10T00:00:00+-1:00",
    ] {
        assert_eq!(gpx::parse_iso8601(text), None, "{text}");
    }
    assert!(gpx::parse_iso8601("2026-11-10T00:00:00Z").is_some());
    assert!(gpx::parse_iso8601("2026-11-10T00:00:00+01:00").is_some());
}

fn gpx_with(comment: &str, ele: impl Fn(usize) -> f64) -> String {
    let mut xml = format!("<gpx><!-- {comment} --><trk><trkseg>");
    for i in 0..20 {
        xml.push_str(&format!(
            "<trkpt lat=\"{}\" lon=\"19\"><ele>{}</ele><time>2026-11-10T00:00:{i:02}Z</time></trkpt>",
            47.5 + i as f64 * 0.0001,
            ele(i)
        ));
    }
    xml.push_str("</trkseg></trk></gpx>");
    xml
}

#[test]
fn a_comment_does_not_make_a_gpx_a_tcx() {
    let xml = gpx_with("TrainingCenterDatabase export converted to GPX", |i| {
        100.0 + i as f64
    });
    let parsed = activity::from_bytes(xml.as_bytes()).expect("a GPX stays a GPX");
    assert_eq!(parsed.samples.len(), 20);
}

#[test]
fn a_huge_elevation_never_yields_a_route_that_cannot_be_read_back() {
    let (_dir, store) = store();
    let xml = gpx_with("x", |i| 1e308 + i as f64 * 1e306);
    let built =
        gpx::parse(&xml).and_then(|data| build::build_route(&data, "huge", RouteSource::GpxFile));
    if let Ok(mut route) = built {
        assert!(route.points.iter().all(|p| p.ele.is_finite()));
        route.id = "huge-elevation".into();
        store.save_recent(&route).unwrap();
        assert!(store.load_recent(&route.id).is_some());
    }
}

fn mixed_activity(route: &Route) -> activity::Activity {
    let samples = (0..=120)
        .map(|i| {
            let p = route.locate(f64::from(i) * 8.0);
            activity::ActivitySample {
                time: 1_701_000_000.0 + f64::from(i),
                lat: p.lat,
                lon: p.lon,
                ele: None,
                hr: None,
                cadence: None,
                power: if (30..40).contains(&i) {
                    Some(900.0)
                } else {
                    None
                },
            }
        })
        .collect();
    activity::Activity {
        name: "mixed".into(),
        format: activity::ActivityFormat::Gpx,
        samples,
        skipped: 0,
    }
}

#[test]
fn reestimate_keeps_the_measured_watts() {
    let route = route("mixed-power", 5000.0);
    let rider = RiderSettings::default();
    let original = import::import_activity(&mixed_activity(&route), &route, &rider, None)
        .unwrap()
        .record;
    assert_eq!(original.points[35].power_w, Some(900.0));
    let params = ride_sim::estimate::custom_params(&rider, 95.0, 11.0, 0.007);
    let updated = import::reestimate(&original, params, &rider).unwrap();
    assert_eq!(updated.points[35].power_w, Some(900.0));
    assert_ne!(updated.points[5].power_w, original.points[5].power_w);
}

#[test]
fn reestimate_updates_the_ride_in_place() {
    let (_dir, store) = store();
    let route = route("mixed-power", 5000.0);
    let rider = RiderSettings::default();
    let original = import::import_activity(&mixed_activity(&route), &route, &rider, None)
        .unwrap()
        .record;
    let first = store.save_ride(&original, &route).unwrap().unwrap();
    let params = ride_sim::estimate::custom_params(&rider, 95.0, 11.0, 0.007);
    let updated = import::reestimate(&original, params, &rider).unwrap();
    let meta = store
        .replace_ride(&updated, &route, &first.stem)
        .unwrap()
        .unwrap();
    assert_eq!(meta.stem, first.stem);
    assert_eq!(store.list_rides(&route.id).len(), 1);
    let loaded = store.load_ride(&route.id, &first.stem).unwrap();
    assert_eq!(loaded.estimate, updated.estimate);
}

fn backup_of(
    route: &Route,
    record: &RideRecord,
    store_dir: &std::path::Path,
) -> (Store, Vec<u8>, ride_model::RideMeta) {
    let src = Store::at(store_dir.join("source"));
    src.save_recent(route).unwrap();
    let mut other = route.clone();
    other.id = "second-route".into();
    src.save_recent(&other).unwrap();
    let meta = src.save_ride(record, route).unwrap().unwrap();
    let mut bytes = Cursor::new(Vec::new());
    backup::export(&src, &Default::default(), "audit", 1, &mut bytes).unwrap();
    (src, bytes.into_inner(), meta)
}

#[test]
fn restore_repairs_an_empty_local_record() {
    let dir = tempfile::tempdir().unwrap();
    let route = route("audit-route", 10_000.0);
    let record = recorded(&route, 61);
    let (_src, bytes, meta) = backup_of(&route, &record, dir.path());
    let dst = Store::at(dir.path().join("broken-local"));
    dst.save_ride(&record, &route).unwrap().unwrap();
    let path = dst
        .dir()
        .join(format!("rides/{}/{}.json", route.id, meta.stem));
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["points"] = serde_json::json!([]);
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(dst.load_ride(&route.id, &meta.stem).is_none());
    let report = backup::restore(&dst, Cursor::new(bytes)).unwrap();
    assert_eq!(report.rides, 1);
    assert!(dst.load_ride(&route.id, &meta.stem).is_some());
}

#[test]
fn restore_refuses_a_budget_it_cannot_finish_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let route = route("audit-route", 10_000.0);
    let record = recorded(&route, 61);
    let (src, bytes, _meta) = backup_of(&route, &record, dir.path());
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut total = 0;
    for i in 0..zip.len() {
        total += zip.by_index(i).unwrap().size();
    }
    let first_len = std::fs::metadata(src.dir().join(format!("routes/{}.json", route.id)))
        .unwrap()
        .len();
    let target = Store::at(dir.path().join("budget"));
    let limits = backup::Limits {
        max_total_bytes: total + first_len + 5,
        ..Default::default()
    };
    let result = backup::restore_with(&target, Cursor::new(&bytes), &limits);
    let written = target
        .dir()
        .join(format!("routes/{}.json", route.id))
        .exists();
    // either the whole restore fits and succeeds, or nothing was written
    assert!(
        result.is_ok() || !written,
        "partial restore: first route written, error {:?}",
        result.err()
    );
}

#[test]
fn climb_detection_scales_linearly_with_rejected_runs() {
    let build = |n: usize| -> Vec<ride_model::RoutePoint> {
        (0..n)
            .map(|i| ride_model::RoutePoint {
                dist: i as f64 * 10.0,
                lat: 47.5,
                lon: 19.0,
                ele: 100.0 + 10.0 * ((i % 40) as f64 * std::f64::consts::PI / 20.0).sin(),
                grade: if i % 40 < 10 { 4.0 } else { -4.0 },
            })
            .collect()
    };
    // best of three: a busy machine slows a run down, it never speeds one up
    let time = |n| {
        let points = build(n);
        (0..3)
            .map(|_| {
                let start = std::time::Instant::now();
                let _ = climbs::detect(&points);
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let (small, large) = (time(20_000).max(1e-4), time(80_000));
    // four times the points: linear is ~4x, the quadratic scan was ~16x
    assert!(large / small < 9.0, "20k: {small:.4}s, 80k: {large:.4}s");
}

#[test]
fn sparse_slow_samples_keep_their_real_elapsed_time() {
    let route = route("slow-import", 2_000.0);
    let samples = (0..100)
        .map(|i| {
            let pos = route.locate(i as f64 * 13.5);
            activity::ActivitySample {
                time: 1_700_000_000.0 + i as f64 * 15.0,
                lat: pos.lat,
                lon: pos.lon,
                ele: Some(pos.ele),
                hr: None,
                cadence: None,
                power: None,
            }
        })
        .collect();
    let mut act = activity::Activity {
        name: "Slow climb".into(),
        format: activity::ActivityFormat::Gpx,
        samples,
        skipped: 0,
    };
    let imported = import::import_activity(&act, &route, &RiderSettings::default(), None).unwrap();
    assert!((imported.record.summary.elapsed_s - 1485.0).abs() < 1.0);
    assert!((imported.record.summary.avg_speed_ms - 0.9).abs() < 0.05);
    assert_eq!(imported.paused_s, 0.0);
    for (i, sample) in act.samples.iter_mut().enumerate() {
        sample.time = 1_700_000_000.0 + i as f64 * 2000.0;
    }
    assert!(import::import_activity(&act, &route, &RiderSettings::default(), None).is_err());
}

#[test]
fn a_short_reverse_track_remains_route_geometry() {
    let mut xml = String::from("<gpx><trk><trkseg>");
    for i in 0..=300 {
        let lat = 47.5 + i as f64 * 10.0 / 110_540.0;
        xml.push_str(&format!("<trkpt lat=\"{lat:.7}\" lon=\"19\"/>"));
    }
    xml.push_str("</trkseg></trk><trk><trkseg>");
    for i in 0..=100 {
        let lat = 47.5 + (3000.0 - i as f64 * 10.0) / 110_540.0;
        xml.push_str(&format!("<trkpt lat=\"{lat:.7}\" lon=\"19\"/>"));
    }
    xml.push_str("</trkseg></trk></gpx>");
    let parsed = gpx::parse(&xml).unwrap();
    assert!(parsed.segment_tracks.is_empty());
    let route = build::build_route(&parsed, "reverse", RouteSource::GpxFile).unwrap();
    assert!(
        (route.total_dist - 4000.0).abs() < 30.0,
        "{}",
        route.total_dist
    );
}
