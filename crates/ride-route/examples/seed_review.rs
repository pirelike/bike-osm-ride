//! Fills a data folder with rides for trying out the review page:
//! `cargo run -p ride-route --example seed_review -- <folder> [route.gpx]`, then start the app
//! with `RIDE_DATA_DIR=<folder> RIDE_REVIEW=1` (and `RIDE_GPX=<route.gpx>` when one was given).
//!
//! The route (the built-in demo route unless a GPX is given) gets two rides made by the ride engine at different power, and one
//! more that arrives the way a Strava GPX does: positions, time stamps, heart rate and cadence,
//! no watts (so the page shows estimated ones).

use std::fmt::Write as _;
use std::sync::Arc;

use ride_model::{
    RecordPoint, RideRecord, RideSource, RideState, RiderSettings, SensorSample,
    RIDE_RECORD_VERSION,
};
use ride_route::store::Store;
use ride_route::{activity, import};
use ride_sim::RideEngine;

const START: f64 = 1_790_000_000.0;
/// One simulated day.
const DAY: f64 = 86_400.0;

fn watts_at(base: f64, t: f64) -> f64 {
    // a steady effort with a slow surge and a little jitter, the same every run
    base + 45.0 * (t / 240.0).sin() + 12.0 * (t / 7.0).sin()
}

fn ride(route: &Arc<ride_model::Route>, base: f64, started: f64) -> RideRecord {
    let mut engine = RideEngine::new(route.clone(), RiderSettings::default());
    engine.start(started);
    let mut t = 0.0;
    while engine.state() == RideState::Riding && t < 6.0 * 3600.0 {
        let watts = watts_at(base, t);
        let sample = SensorSample {
            power_w: Some(watts),
            cadence_rpm: Some(78.0 + watts / 14.0),
            hr_bpm: Some(105.0 + watts / 3.6 + t / 120.0),
            ..SensorSample::default()
        };
        engine.step(0.5, &sample, started + t);
        t += 0.5;
    }
    let summary = engine.summary();
    RideRecord {
        version: RIDE_RECORD_VERSION,
        route_id: route.id.clone(),
        route_name: route.name.clone(),
        simulated: false,
        summary,
        points: engine.history().to_vec(),
        source: RideSource::Recorded,
        power_estimated: false,
        estimate: None,
        measured_power: Vec::new(),
    }
}

/// The ride as a GPX of another app would have it: 5 m beside the line, no watts.
fn as_gpx(points: &[RecordPoint], started: f64) -> String {
    let mut gpx = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx creator=\"StravaGPX\" xmlns=\"http://www.topografix.com/GPX/1/1\" xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\">\n<trk><name>Esti kör</name><type>cycling</type><trkseg>\n",
    );
    for (i, p) in points.iter().enumerate() {
        // every other second only: a 2 s recording interval
        if i % 2 == 1 {
            continue;
        }
        let secs = (started + p.t) as i64;
        let (h, m, s) = (secs % 86_400 / 3600, secs % 3600 / 60, secs % 60);
        let days = secs / 86_400;
        let (year, month, day) = civil(days);
        let _ = writeln!(
            gpx,
            "<trkpt lat=\"{:.6}\" lon=\"{:.6}\"><ele>{:.1}</ele><time>{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z</time><extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>{:.0}</gpxtpx:hr><gpxtpx:cad>{:.0}</gpxtpx:cad></gpxtpx:TrackPointExtension></extensions></trkpt>",
            p.lat + 5.0 / 110_540.0,
            p.lon,
            p.ele + 2.0,
            p.hr_bpm.unwrap_or(130.0),
            p.cadence_rpm.unwrap_or(80.0),
        );
    }
    gpx.push_str("</trkseg></trk></gpx>\n");
    gpx
}

/// Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: seed_review <folder>");
        std::process::exit(2);
    };
    let store = Store::at(&dir);
    let route = match std::env::args().nth(2) {
        Some(gpx) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            let loaded = runtime
                .block_on(ride_route::load_route_from_path(gpx.into(), false))
                .expect("load the GPX");
            Arc::new(loaded)
        }
        None => Arc::new(ride_route::demo_route()),
    };
    store.save_recent(&route).expect("save the route");

    for (days_ago, base) in [(9.0, 215.0), (2.0, 250.0)] {
        let record = ride(&route, base, START - days_ago * DAY);
        let meta = store
            .save_ride(&record, &route)
            .expect("save a ride")
            .expect("long enough");
        println!(
            "recorded {} at {base} W: {:.1} km in {:.0} s",
            meta.stem,
            record.summary.dist / 1000.0,
            record.summary.elapsed_s
        );
    }

    let source = ride(&route, 235.0, START - 5.0 * DAY);
    let gpx = as_gpx(&source.points, START - 5.0 * DAY);
    let parsed = activity::parse_gpx(&gpx).expect("the synthetic GPX parses");
    let done = import::import_activity(&parsed, &route, &RiderSettings::default(), None)
        .expect("the import works");
    let meta = store
        .save_ride(&done.record, &route)
        .expect("save the import")
        .expect("long enough");
    println!(
        "imported {}: {:.1} km, estimated {:.0} W (the engine used 235 W)",
        meta.stem,
        done.record.summary.dist / 1000.0,
        done.record.summary.avg_power_w.unwrap_or(0.0)
    );
    for warning in &done.warnings {
        println!("  note: {warning}");
    }
}
