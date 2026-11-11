//! Regression tests for the confirmed findings of the 2026-11-10 audit (`BUGS.md`, BUG-230…240).

use std::sync::Arc;

use ride_model::{RecordPoint, RideMeta, RiderSettings, SensorSample};
use ride_sim::{analysis, compare, power_curve, segments, tcx, RideEngine};

fn point(t: f64, w: f64) -> RecordPoint {
    RecordPoint {
        t,
        unix_s: 1_700_000_000.0 + t,
        dist: 10.0 * t,
        lat: 47.5,
        lon: 19.0,
        ele: 100.0,
        speed_ms: 10.0,
        power_w: Some(w),
        cadence_rpm: None,
        hr_bpm: None,
        grade: 0.0,
    }
}

fn engine(len: f64) -> RideEngine {
    RideEngine::new(
        Arc::new(ride_model::testing::straight_route(&[(len, 0.0)], 100.0)),
        RiderSettings {
            use_trainer_speed: true,
            ..RiderSettings::default()
        },
    )
}

fn sample(w: f64) -> SensorSample {
    SensorSample {
        power_w: Some(w),
        speed_ms: Some(10.0),
        ..SensorSample::default()
    }
}

#[test]
fn finish_line_stops_the_accounting() {
    let mut e = engine(10.0);
    e.start(1000.0);
    e.step(60.0, &sample(200.0), 1060.0);
    let s = e.summary();
    assert!((s.dist - 10.0).abs() < 1e-6);
    assert!(s.elapsed_s < 1.5, "elapsed {}", s.elapsed_s);
    assert!(s.work_kj < 0.4, "work {}", s.work_kj);
}

#[test]
fn manual_finish_after_pause_keeps_the_wall_clock_monotonic() {
    let mut e = engine(10_000.0);
    e.start(1000.0);
    e.step(1.0, &sample(200.0), 1001.0);
    e.pause();
    e.step(60.0, &sample(0.0), 1061.0);
    e.resume();
    e.step(1.0, &sample(200.0), 1062.0);
    e.step(0.25, &sample(200.0), 1062.25);
    e.finish();
    let h = e.history();
    assert!(
        h.windows(2).all(|w| w[1].unix_s >= w[0].unix_s),
        "{:?}",
        h.iter().map(|p| (p.t, p.unix_s)).collect::<Vec<_>>()
    );
    assert!(h.last().is_some_and(|p| p.unix_s >= 1062.0));
}

#[test]
fn same_second_overwrite_feeds_the_next_gap() {
    // the last point is an endpoint, not a cell of its own
    let dense = power_curve::dense_power(&[
        point(0.0, 100.0),
        point(0.4, 300.0),
        point(2.0, 200.0),
        point(3.0, 200.0),
    ]);
    assert_eq!(dense, vec![300.0, 300.0, 200.0]);
}

#[test]
fn a_ride_shorter_than_the_window_has_no_value() {
    let points: Vec<_> = (0..5).map(|t| point(f64::from(t), 200.0)).collect();
    assert_eq!(power_curve::mean_max(&points, &[5]), vec![None]);
    let points: Vec<_> = (0..6).map(|t| point(f64::from(t), 200.0)).collect();
    assert_eq!(power_curve::mean_max(&points, &[5]), vec![Some(200.0)]);
}

#[test]
fn live_and_recalculated_np_agree_after_a_forced_last_point() {
    let mut e = engine(10_000.0);
    e.step(0.1, &sample(200.0), 1000.0);
    e.start(1000.0);
    e.step(1.0, &sample(200.0), 1001.0);
    e.step(0.4, &sample(800.0), 1001.4);
    e.finish();
    let saved = analysis::summarize("x", "y", e.history(), &RiderSettings::default()).unwrap();
    let live = e.summary().normalized_power_w.unwrap();
    let saved = saved.normalized_power_w.unwrap();
    assert!((live - saved).abs() < 1.0, "live {live} saved {saved}");
}

#[test]
fn huge_timestamps_do_not_overflow_the_trend() {
    for period in [power_curve::Period::Week, power_curve::Period::Month] {
        let meta = RideMeta {
            started_unix_s: 1e300,
            ..RideMeta::default()
        };
        let result =
            std::panic::catch_unwind(|| power_curve::bucket(&[&meta], period, 12, 1_700_000_000.0));
        assert!(result.is_ok(), "{period:?} panicked");
    }
}

#[test]
fn a_corrupt_segment_key_does_not_overflow() {
    let result =
        std::panic::catch_unwind(|| segments::same_segment("0-0", "0--9223372036854775808"));
    assert_eq!(result.ok(), Some(false));
}

#[test]
fn trainer_speed_is_not_held_through_a_dropout() {
    let mut e = engine(10_000.0);
    e.start(1000.0);
    e.step(1.0, &sample(200.0), 1001.0);
    let before = e.snapshot().dist;
    e.step(10.0, &SensorSample::default(), 1011.0);
    assert!(
        e.snapshot().dist - before < 1.0,
        "{}",
        e.snapshot().dist - before
    );
}

#[test]
fn section_np_counts_the_dropout_seconds() {
    let mut points: Vec<_> = (0..=120).map(|t| point(f64::from(t), 200.0)).collect();
    for p in &mut points[30..60] {
        p.power_w = None;
    }
    let section = compare::section_stats(&points, 0.0, 1200.0).unwrap();
    let summary = analysis::summarize("x", "y", &points, &RiderSettings::default()).unwrap();
    let (a, b) = (
        section.normalized_power_w.unwrap(),
        summary.normalized_power_w.unwrap(),
    );
    assert!((a - b).abs() < 1.0, "section {a} summary {b}");
}

#[test]
fn a_huge_smoothing_window_does_not_panic() {
    let grid =
        compare::DistSeries::from_points(&[point(0.0, 200.0), point(10.0, 200.0)], 10.0).unwrap();
    assert!(std::panic::catch_unwind(|| grid.smoothed(1e300)).is_ok());
}

#[test]
fn tcx_cadence_stays_inside_the_schema_range() {
    let mut e = engine(10_000.0);
    e.start(1_700_000_000.0);
    let mut s = sample(200.0);
    s.cadence_rpm = Some(260.0);
    e.step(1.0, &s, 1_700_000_001.0);
    e.finish();
    let xml = tcx::to_tcx(&e.summary(), e.history());
    for value in xml
        .split("<Cadence>")
        .skip(1)
        .chain(xml.split("<AverageCadence>").skip(1))
    {
        let n: u32 = value.split('<').next().unwrap().trim().parse().unwrap();
        assert!(n <= 255, "cadence {n} is not an unsignedByte");
    }
}
