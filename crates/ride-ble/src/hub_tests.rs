//! Behavioural tests for the hub, driven end-to-end through the in-memory fake transport
//! (`fake.rs`): scanning, connecting, the FTMS handshake, the safety filter, keep-alive,
//! pause/stop semantics, reconnects and the merged sample priorities.
//!
//! Time is real but compressed: `HubConfig::fast()` shrinks every timeout to a few
//! milliseconds, and the tests wait on observable state instead of sleeping whenever
//! possible. The production defaults themselves are asserted where the contract is a
//! specific number (3 s procedure timeout, 5 s keep-alive, 3 s staleness, ~10 Hz samples).

use std::time::Duration;

use futures::future::BoxFuture;
use ride_model::{
    ConnState, DeviceInfo, DeviceKind, SensorCommand, SensorEvent, SensorSample, TrainerSim,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::control::ControlConfig;
use crate::error::BleError;
use crate::fake::{FakeAdapter, FakePeripheral};
use crate::hexlog::HexLog;
use crate::hub::{self, SIM_ID, SIM_NAME};
use crate::link::HubConfig;
use crate::merge::STALE_AFTER;
use crate::transport::short_id;
use crate::zwift;

const TRAINER_ID: &str = "t1";

// ───────────────────────────── harness ─────────────────────────────

struct Harness {
    cmd: UnboundedSender<SensorCommand>,
    evt: UnboundedReceiver<SensorEvent>,
    handle: JoinHandle<()>,
}

impl Harness {
    fn spawn(adapter: FakeAdapter, cfg: HubConfig) -> Self {
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let (evt_tx, evt_rx) = unbounded_channel();
        let init_adapter = adapter.clone();
        let init: hub::InitFn<FakeAdapter> = Box::new(move || {
            let adapter = init_adapter.clone();
            let fut: BoxFuture<'static, Result<FakeAdapter, BleError>> =
                Box::pin(async move { Ok(adapter) });
            fut
        });
        let handle = tokio::spawn(hub::run_hub(
            init,
            cfg,
            HexLog::disabled(),
            0x5EED,
            cmd_rx,
            evt_tx,
        ));
        Self {
            cmd: cmd_tx,
            evt: evt_rx,
            handle,
        }
    }

    fn send(&self, cmd: SensorCommand) {
        self.cmd.send(cmd).expect("the hub command channel is open");
    }

    async fn next_event(&mut self) -> SensorEvent {
        tokio::time::timeout(Duration::from_secs(3), self.evt.recv())
            .await
            .expect("timed out waiting for a hub event")
            .expect("the hub closed its event channel")
    }

    /// Wait for a `State` event of `kind` with exactly `want`.
    async fn wait_state(&mut self, kind: DeviceKind, want: &ConnState) -> Option<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !left.is_zero(),
                "timed out waiting for {kind:?} to become {want:?}"
            );
            let event = tokio::time::timeout(left, self.evt.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {kind:?} to become {want:?}"))
                .expect("the hub closed its event channel");
            if let SensorEvent::State {
                kind: k,
                state,
                name,
            } = event
            {
                if k == kind && state == *want {
                    return name;
                }
            }
        }
    }

    async fn wait_discovered(&mut self, pred: impl Fn(&[DeviceInfo]) -> bool) -> Vec<DeviceInfo> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!left.is_zero(), "timed out waiting for a discovery list");
            let event = tokio::time::timeout(left, self.evt.recv())
                .await
                .expect("timed out waiting for a discovery list")
                .expect("the hub closed its event channel");
            if let SensorEvent::Discovered(list) = event {
                if pred(&list) {
                    return list;
                }
            }
        }
    }

    /// Wait for a `Sample` matching `pred`, reporting the last sample seen on timeout.
    async fn wait_sample(
        &mut self,
        what: &str,
        mut pred: impl FnMut(&SensorSample) -> bool,
    ) -> SensorSample {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut last: Option<SensorSample> = None;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !left.is_zero(),
                "timed out waiting for {what}; last sample: {last:?}"
            );
            let event = tokio::time::timeout(left, self.evt.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}; last sample: {last:?}"))
                .expect("the hub closed its event channel");
            if let SensorEvent::Sample(sample) = event {
                if pred(&sample) {
                    return sample;
                }
                last = Some(sample);
            }
        }
    }

    async fn wait_notice(&mut self, needle: &str) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !left.is_zero(),
                "timed out waiting for a notice containing {needle:?}"
            );
            let event = tokio::time::timeout(left, self.evt.recv())
                .await
                .expect("timed out waiting for a notice")
                .expect("the hub closed its event channel");
            if let SensorEvent::Notice(text) = event {
                if text.contains(needle) {
                    return text;
                }
            }
        }
    }

    async fn events_for(&mut self, duration: Duration) -> Vec<SensorEvent> {
        let deadline = tokio::time::Instant::now() + duration;
        let mut out = Vec::new();
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return out;
            }
            match tokio::time::timeout(deadline - now, self.evt.recv()).await {
                Ok(Some(event)) => out.push(event),
                Ok(None) => panic!("the hub closed its event channel"),
                Err(_) => return out,
            }
        }
    }

    async fn samples_for(&mut self, duration: Duration) -> Vec<SensorSample> {
        self.events_for(duration)
            .await
            .into_iter()
            .filter_map(|event| match event {
                SensorEvent::Sample(sample) => Some(sample),
                _ => None,
            })
            .collect()
    }

    /// Poll `cond` until it holds, without consuming events (the hub runs concurrently).
    async fn wait_until(&self, what: &str, mut cond: impl FnMut() -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if cond() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}

// ───────────────────────────── helpers ─────────────────────────────

fn sim(grade: f64) -> TrainerSim {
    TrainerSim {
        grade_pct: grade,
        crr: 0.004,
        cw: 0.39,
        wind_ms: 0.0,
    }
}

fn writes_of(peripheral: &FakePeripheral, opcode: u8) -> Vec<Vec<u8>> {
    peripheral
        .cp_writes()
        .into_iter()
        .filter(|bytes| bytes.first() == Some(&opcode))
        .collect()
}

fn grade_of(bytes: &[u8]) -> i16 {
    assert_eq!(
        bytes.first(),
        Some(&0x11),
        "not a simulation write: {bytes:02X?}"
    );
    i16::from_le_bytes([bytes[3], bytes[4]])
}

fn subscribed_shorts(peripheral: &FakePeripheral) -> Vec<u16> {
    peripheral
        .subscribed()
        .iter()
        .filter_map(short_id)
        .collect()
}

/// The captured TryRider-style frames the fake trainer emits on request.
fn trainer_indoor_bike_power(power: i16) -> Vec<u8> {
    // bit 0 set (no speed), bit 6 set (instantaneous power).
    let mut v = vec![0x41, 0x00];
    v.extend_from_slice(&power.to_le_bytes());
    v
}

fn cp_crank_packet(power: i16, revs: u16, time: u16) -> Vec<u8> {
    let mut v = vec![0x20, 0x00]; // crank revolution data present
    v.extend_from_slice(&power.to_le_bytes());
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

fn csc_crank_packet(revs: u16, time: u16) -> Vec<u8> {
    let mut v = vec![0x02]; // crank revolution data present
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

// ───────────────────────────── scanning ─────────────────────────────

#[tokio::test]
async fn scan_lists_the_simulated_rider_first() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.set_rssi(-49);
    let adapter = FakeAdapter::new(vec![trainer]);
    let mut hub = Harness::spawn(adapter.clone(), HubConfig::fast());

    // Before any scan, the hub already offers the simulated rider — even with no adapter at
    // all, and always as the first entry.
    let first = match hub.next_event().await {
        SensorEvent::Discovered(list) => list,
        other => panic!("the first event must be Discovered, got {other:?}"),
    };
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, SIM_ID);
    assert_eq!(first[0].name, SIM_NAME, "Szimulált tekerő");
    assert_eq!(first[0].rssi, None);
    assert_eq!(
        first[0].kinds,
        vec![
            DeviceKind::Trainer,
            DeviceKind::PowerMeter,
            DeviceKind::Cadence,
            DeviceKind::HeartRate
        ]
    );

    hub.send(SensorCommand::StartScan);
    hub.wait_until("the scan to start", || adapter.is_scanning())
        .await;
    assert!(adapter.scan_starts() >= 1, "the adapter scan was started");
    let list = hub
        .wait_discovered(|list| list.iter().any(|d| d.id == TRAINER_ID))
        .await;
    assert_eq!(list[0].id, SIM_ID, "the simulated rider is always first");
    let trainer = list
        .iter()
        .find(|d| d.id == TRAINER_ID)
        .expect("the trainer is listed");
    assert_eq!(trainer.kinds, vec![DeviceKind::Trainer]);
    assert_eq!(trainer.rssi, Some(-49), "the advertised RSSI is reported");

    // A device that only shows up in a live advertisement (not in the adapter's initial
    // peripheral list) is discovered too.
    let late = FakePeripheral::heart_rate("h9", "Polar H10");
    late.set_rssi(-44);
    adapter.announce(&late);
    let list = hub
        .wait_discovered(|list| list.iter().any(|d| d.id == "h9"))
        .await;
    let strap = list
        .iter()
        .find(|d| d.id == "h9")
        .expect("the announced strap is listed");
    assert_eq!(strap.kinds, vec![DeviceKind::HeartRate]);
    assert_eq!(strap.rssi, Some(-44));
    assert_eq!(list[0].id, SIM_ID);

    hub.send(SensorCommand::StopScan);
    hub.wait_until("the scan to stop", || {
        !adapter.is_scanning() && adapter.scan_stops() >= 1
    })
    .await;
}

#[tokio::test]
async fn a_scan_that_cannot_start_is_reported() {
    let adapter = FakeAdapter::new(Vec::new());
    adapter.fail_scan(Some(BleError::Unavailable(
        "nincs Bluetooth-adapter".into(),
    )));
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::StartScan);
    let notice = hub.wait_notice("A keresés nem indítható").await;
    assert!(notice.contains("Bluetooth"), "{notice}");
}

#[tokio::test]
async fn connecting_stops_the_scan_first() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer]);
    let mut hub = Harness::spawn(adapter.clone(), HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::StartScan);
    hub.wait_until("the scan to start", || adapter.is_scanning())
        .await;
    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;

    // `connect_ble` awaits `stop_scan` before it touches the peripheral (WinRT connects
    // far more reliably with the scanner off).
    assert!(
        !adapter.is_scanning(),
        "the scan is stopped before the GATT connection"
    );
    assert!(adapter.scan_stops() >= 1);
}

// ───────────────────────────── FTMS handshake ─────────────────────────────

#[tokio::test]
async fn connect_subscribes_before_request_control() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("Request Control", || !trainer.cp_writes().is_empty())
        .await;

    // Control Point indications, Machine Status and Indoor Bike Data, in that order.
    let subscribed = subscribed_shorts(&trainer);
    assert!(
        subscribed.starts_with(&[0x2AD9, 0x2ADA, 0x2AD2]),
        "{subscribed:04X?}"
    );
    let writes = trainer.cp_writes();
    assert_eq!(
        writes[0],
        vec![0x00],
        "Request Control is the first Control Point write"
    );

    // The fake answered `[0x80, 0x00, 0x01]`; a Start/Resume must follow, not another retry.
    hub.wait_until("Start/Resume", || writes_of(&trainer, 0x07).len() == 1)
        .await;
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    assert!(
        trainer.is_connected_now(),
        "the GATT link is up after Connected"
    );
    assert_eq!(
        writes_of(&trainer, 0x00).len(),
        1,
        "success means no control retry"
    );
}

#[tokio::test]
async fn success_indication_unlocks_the_start() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.silence_cp(0x00); // do not auto-answer Request Control
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("Request Control", || writes_of(&trainer, 0x00).len() == 1)
        .await;
    // A few milliseconds with no indication: the one outstanding procedure blocks the queue
    // (the fast config still leaves 150 ms before the timeout).
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        writes_of(&trainer, 0x07).is_empty(),
        "Start must wait for the indication"
    );
    assert_eq!(
        writes_of(&trainer, 0x00).len(),
        1,
        "still the first attempt"
    );

    // Deliver exactly the response the FTMS specification defines.
    trainer.push(0x2AD9, &[0x80, 0x00, 0x01]);
    hub.wait_until("Start after the indication", || {
        writes_of(&trainer, 0x07).len() == 1
    })
    .await;
}

#[tokio::test]
async fn one_outstanding_procedure_with_the_response_timeout() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.silence_cp(0x11); // never answer simulation writes
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("Start/Resume", || writes_of(&trainer, 0x07).len() == 1)
        .await;
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.wait_until("the first simulation", || {
        writes_of(&trainer, 0x11).len() == 1
    })
    .await;
    let first = writes_of(&trainer, 0x11)[0].clone();
    assert_eq!(grade_of(&first), 500);

    // A newer value waits: at most one Control Point procedure may be outstanding.
    hub.send(SensorCommand::SetSimulation(sim(6.0)));
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert_eq!(
        writes_of(&trainer, 0x11).len(),
        1,
        "one outstanding procedure"
    );
    assert_eq!(
        writes_of(&trainer, 0x11)[0],
        first,
        "the unanswered value is not replaced"
    );

    // The 150 ms fast timeout abandons it, warns, and sends the latest intent only.
    let notice = hub.wait_notice("nem válaszolt").await;
    assert!(notice.contains("nem válaszolt időben"), "{notice}");
    hub.wait_until("the newer simulation after the timeout", || {
        writes_of(&trainer, 0x11).len() == 2
    })
    .await;
    let second = writes_of(&trainer, 0x11)[1].clone();
    assert_eq!(grade_of(&second), 600);
    assert_eq!(second[5], 40, "crr 0.004");
    assert_eq!(second[6], 39, "cw 0.39");

    // The production values behind the behaviour: a 3 s procedure timeout and a 250 ms
    // write-pacing gap.
    let defaults = ControlConfig::default();
    assert_eq!(defaults.response_timeout, Duration::from_secs(3));
    assert_eq!(
        HubConfig::default().control.response_timeout,
        Duration::from_secs(3)
    );
    assert!(defaults.min_write_gap >= Duration::from_millis(200));
}

#[tokio::test]
async fn a_trainer_that_reports_reset_after_every_write_still_gets_the_grade() {
    // Captured firmware behaviour (ble.log, 2026-11-01): the trainer answers every Control
    // Point write and then sends Machine Status 0x01 (Reset). Treating that as a permission
    // loss produced an endless Request Control loop and the trainer never received a single
    // simulation or target write, so ERG and the hills did nothing.
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.report_reset_after_every_write();
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.send(SensorCommand::TrainerStart);

    hub.wait_until("the simulation despite the reset storm", || {
        !writes_of(&trainer, 0x11).is_empty()
    })
    .await;
    assert_eq!(grade_of(&writes_of(&trainer, 0x11)[0]), 500);

    // A new grade reaches the trainer too, and the control permission is not fought over.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let requests = writes_of(&trainer, 0x00).len();
    assert!(
        requests <= 1,
        "no Request Control loop: {requests} requests"
    );
    hub.send(SensorCommand::SetSimulation(sim(7.0)));
    hub.wait_until("the new grade", || {
        writes_of(&trainer, 0x11).iter().any(|b| grade_of(b) == 700)
    })
    .await;
    assert_eq!(
        writes_of(&trainer, 0x00).len(),
        requests,
        "still the original grant"
    );
}

#[tokio::test]
async fn simulation_keepalive_repeats_the_target() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("the first simulation", || {
        writes_of(&trainer, 0x11).len() == 1
    })
    .await;
    let first = writes_of(&trainer, 0x11)[0].clone();

    // Without any further command the same value is re-sent within the keep-alive window.
    hub.wait_until("a keep-alive re-send", || {
        writes_of(&trainer, 0x11).len() >= 2
    })
    .await;
    let second = writes_of(&trainer, 0x11)[1].clone();
    assert_eq!(first, second, "the keep-alive repeats the active target");

    // The production contract is at most every 5 s.
    assert!(ControlConfig::default().keepalive <= Duration::from_secs(5));
    assert!(HubConfig::default().control.keepalive <= Duration::from_secs(5));
}

#[tokio::test]
async fn simulation_grade_is_clamped_on_the_wire() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(50.0)));
    hub.send(SensorCommand::TrainerStart);

    // +50 % is clamped to +20 % (and the trainer's own +20 % limit).
    hub.wait_until("the +20 % clamp", || {
        writes_of(&trainer, 0x11)
            .iter()
            .any(|b| grade_of(b) == 2000)
    })
    .await;
    trainer.clear_writes();

    // The fake trainer advertises -10.0 .. +20.0 %, so the negative side is tighter.
    hub.send(SensorCommand::SetSimulation(sim(-30.0)));
    hub.wait_until("the -10 % clamp", || {
        writes_of(&trainer, 0x11)
            .iter()
            .any(|b| grade_of(b) == -1000)
    })
    .await;
    let clamped = writes_of(&trainer, 0x11)
        .into_iter()
        .find(|b| grade_of(b) == -1000)
        .expect("a clamped negative simulation write");
    assert_eq!(
        (clamped[5], clamped[6]),
        (40, 39),
        "crr 0.004 and cw 0.39 on the wire grid"
    );
}

#[tokio::test]
async fn rejected_simulation_is_reported_and_not_retried() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.set_cp_result(0x11, 0x02); // "Op Code Not Supported"
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("the rejected simulation", || {
        writes_of(&trainer, 0x11).len() == 1
    })
    .await;
    let notice = hub.wait_notice("terepszimuláció").await;
    assert!(
        notice.contains("nem támogatja a terepszimulációt"),
        "{notice}"
    );

    let before = writes_of(&trainer, 0x11).len();
    hub.send(SensorCommand::SetSimulation(sim(6.0)));
    let _ = hub.events_for(Duration::from_millis(150)).await;
    assert_eq!(
        writes_of(&trainer, 0x11).len(),
        before,
        "a trainer that rejected simulation parameters is not spammed with them"
    );
}

// ───────────────────────────── pause / stop ─────────────────────────────

#[tokio::test]
async fn pause_sends_a_neutral_grade_then_stop_pause() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("the running simulation", || {
        !writes_of(&trainer, 0x07).is_empty() && !writes_of(&trainer, 0x11).is_empty()
    })
    .await;
    let before = trainer.cp_writes().len();

    hub.send(SensorCommand::TrainerPause);
    hub.wait_until("Stop/Pause", || {
        trainer.cp_writes().iter().any(|b| b == &[0x08, 0x02])
    })
    .await;

    let since = &trainer.cp_writes()[before..];
    let neutral = since
        .iter()
        .position(|b| b.first() == Some(&0x11) && grade_of(b) == 0)
        .expect("a neutral grade before the pause");
    let pause = since
        .iter()
        .position(|b| b == &[0x08, 0x02])
        .expect("Stop/Pause(2)");
    assert!(
        neutral < pause,
        "neutral grade first, then Pause: {since:02X?}"
    );

    // Paused means no keep-alive traffic.
    let count = trainer.cp_writes().len();
    let _ = hub.events_for(Duration::from_millis(200)).await;
    assert_eq!(
        trainer.cp_writes().len(),
        count,
        "no keep-alive while paused"
    );
}

#[tokio::test]
async fn disconnect_hands_the_trainer_back_neutral_then_stop() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter.clone(), HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(5.0)));
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("the running simulation", || {
        !writes_of(&trainer, 0x07).is_empty() && !writes_of(&trainer, 0x11).is_empty()
    })
    .await;
    let before = trainer.cp_writes().len();

    hub.send(SensorCommand::Disconnect(DeviceKind::Trainer));
    hub.wait_until("the device to disconnect", || {
        trainer.cp_writes().iter().any(|b| b == &[0x08, 0x01])
    })
    .await;
    hub.wait_until("the link to close", || trainer.disconnects() >= 1)
        .await;
    hub.wait_state(DeviceKind::Trainer, &ConnState::Disconnected)
        .await;
    assert!(!trainer.is_connected_now(), "the GATT link was closed");

    let since = &trainer.cp_writes()[before..];
    let neutral = since
        .iter()
        .position(|b| b.first() == Some(&0x11) && grade_of(b) == 0)
        .expect("a neutral grade on hand-back");
    let stop = since.iter().position(|b| b == &[0x08, 0x01]).expect("Stop");
    assert!(
        neutral < stop,
        "neutral grade first, then Stop: {since:02X?}"
    );
}

#[tokio::test]
async fn shutting_the_hub_down_hands_the_trainer_back() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.send(SensorCommand::SetSimulation(sim(4.0)));
    hub.send(SensorCommand::TrainerStart);
    hub.wait_until("the running simulation", || {
        !writes_of(&trainer, 0x07).is_empty() && !writes_of(&trainer, 0x11).is_empty()
    })
    .await;
    let before = trainer.cp_writes().len();

    // Dropping the command channel is what happens when `SensorLink` is dropped on exit.
    let Harness { cmd, handle, .. } = hub;
    drop(cmd);
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("the hub task must finish after its command channel closes")
        .expect("the hub task panicked");

    let since = &trainer.cp_writes()[before..];
    assert!(
        since
            .iter()
            .any(|b| b.first() == Some(&0x11) && grade_of(b) == 0),
        "a neutral grade while shutting down: {since:02X?}"
    );
    assert!(
        since.iter().any(|b| b == &[0x08, 0x01]),
        "Stop while shutting down: {since:02X?}"
    );
}

// ───────────────────────────── reconnects ─────────────────────────────

#[tokio::test]
async fn link_loss_reconnects_and_repeats_the_handshake() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    let connects = trainer.connects();
    assert_eq!(writes_of(&trainer, 0x00).len(), 1);

    // The link drops without a hub command; the supervisor retries after its backoff.
    trainer.drop_link();
    hub.wait_until("the reconnect attempt", || trainer.connects() > connects)
        .await;
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    assert_eq!(
        writes_of(&trainer, 0x00).len(),
        2,
        "Request Control is re-done on the new link"
    );

    let subscribed = subscribed_shorts(&trainer);
    let sequences = subscribed
        .as_slice()
        .windows(3)
        .filter(|w| w.starts_with(&[0x2AD9, 0x2ADA, 0x2AD2]))
        .count();
    assert!(
        sequences >= 2,
        "subscriptions are repeated after the reconnect: {subscribed:04X?}"
    );
}

#[tokio::test]
async fn reconnect_gives_up_after_three_retries_with_a_notice() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    trainer.fail_next_connects(4); // the initial attempt plus the three retries all fail
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    let mut retry_notices = 0;
    let failure = loop {
        match hub.next_event().await {
            SensorEvent::Notice(text) if text.contains("újrapróbálom") => retry_notices += 1,
            SensorEvent::State {
                kind: DeviceKind::Trainer,
                state: ConnState::Failed(message),
                ..
            } => {
                break message;
            }
            _ => {}
        }
    };
    assert_eq!(retry_notices, 3, "3 retries, each reported");
    assert!(
        failure.contains("nem sikerült (újra)csatlakozni"),
        "{failure}"
    );
    assert_eq!(trainer.connects(), 4, "one attempt plus 3 retries");
}

#[tokio::test]
async fn a_wrong_device_fails_once_without_retrying() {
    let wrong = FakePeripheral::wrong_device("w1", "Battery only");
    let adapter = FakeAdapter::new(vec![wrong.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: "w1".to_string(),
        kind: DeviceKind::Trainer,
    });
    let failure = loop {
        if let SensorEvent::State {
            kind: DeviceKind::Trainer,
            state: ConnState::Failed(message),
            ..
        } = hub.next_event().await
        {
            break message;
        }
    };
    assert!(
        failure.contains("nem kínálja a szükséges szolgáltatást"),
        "{failure}"
    );
    assert_eq!(
        wrong.connects(),
        1,
        "a missing service is permanent; retrying cannot help"
    );
}

// ───────────────────────────── merged samples ─────────────────────────────

#[tokio::test]
async fn merged_sample_priorities_and_fallbacks() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Trainer");
    let strap = FakePeripheral::heart_rate("h1", "Strap");
    let meter = FakePeripheral::power_meter("p1", "Meter");
    let cadence = FakePeripheral::cadence("c1", "Cadence");
    let adapter = FakeAdapter::new(vec![
        trainer.clone(),
        strap.clone(),
        meter.clone(),
        cadence.clone(),
    ]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    for (id, kind) in [
        (TRAINER_ID, DeviceKind::Trainer),
        ("h1", DeviceKind::HeartRate),
        ("p1", DeviceKind::PowerMeter),
        ("c1", DeviceKind::Cadence),
    ] {
        hub.send(SensorCommand::Connect {
            id: id.to_string(),
            kind,
        });
        hub.wait_state(kind, &ConnState::Connected).await;
    }

    // The trainer reports speed, cadence, power and its own HR...
    // flags: bit 0 clear (speed present), bit 2 (cadence), bit 6 (power), bit 9 (HR).
    let mut ibd = vec![0x44, 0x02];
    ibd.extend_from_slice(&2500u16.to_le_bytes()); // 25.00 km/h
    ibd.extend_from_slice(&124u16.to_le_bytes()); // 62.0 rpm
    ibd.extend_from_slice(&200i16.to_le_bytes()); // 200 W
    ibd.push(120); // trainer-reported HR
    trainer.push(0x2AD2, &ibd);
    // Wait until the trainer's frame is actually in the merger before the others arrive.
    let trainer_sample = hub
        .wait_sample("the trainer's own power and speed", |s| {
            s.power_w == Some(200.0) && s.speed_ms.is_some()
        })
        .await;
    assert_eq!(trainer_sample.cadence_rpm, Some(62.0));

    // ...a strap, a power meter and a cadence sensor all report their own values.
    strap.push(0x2A37, &[0x00, 140]);
    meter.push(0x2A63, &cp_crank_packet(210, 0, 0));
    meter.push(0x2A63, &cp_crank_packet(210, 1, 1024)); // 60 rpm
    cadence.push(0x2A5B, &csc_crank_packet(0, 0));
    cadence.push(0x2A5B, &csc_crank_packet(2, 1024)); // 120 rpm

    // power: meter > trainer; cadence: CSC > power-meter cranks > trainer; HR: strap > trainer.
    let sample = hub
        .wait_sample("the merged priority sample", |s| {
            s.power_w == Some(210.0)
                && s.cadence_rpm == Some(120.0)
                && s.hr_bpm == Some(140.0)
                && s.speed_ms.is_some()
        })
        .await;
    assert_eq!(
        sample.hr_bpm,
        Some(140.0),
        "the strap beats the trainer-reported HR"
    );
    let speed = sample.speed_ms.expect("the trainer reports speed");
    assert!((speed - 25.0 / 3.6).abs() < 1e-9, "{speed}");

    // CSC gone: the power-meter cranks take over.
    hub.send(SensorCommand::Disconnect(DeviceKind::Cadence));
    let sample = hub
        .wait_sample("the power-meter crank cadence", |s| {
            s.cadence_rpm == Some(60.0)
        })
        .await;
    assert_eq!(sample.power_w, Some(210.0), "the meter still owns power");

    // Power meter gone: the trainer's power and cadence are the fallback.
    hub.send(SensorCommand::Disconnect(DeviceKind::PowerMeter));
    let sample = hub
        .wait_sample("the trainer fallback", |s| {
            s.power_w == Some(200.0) && s.cadence_rpm == Some(62.0)
        })
        .await;
    assert_eq!(sample.hr_bpm, Some(140.0));

    // Strap gone: the trainer-reported HR is the fallback.
    hub.send(SensorCommand::Disconnect(DeviceKind::HeartRate));
    let sample = hub
        .wait_sample("the trainer HR fallback", |s| s.hr_bpm == Some(120.0))
        .await;
    assert_eq!(sample.power_w, Some(200.0));
}

#[tokio::test]
async fn values_go_stale_after_the_window() {
    let mut cfg = HubConfig::fast();
    cfg.stale_after = Duration::from_millis(60);
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, cfg);
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    trainer.push(0x2AD2, &trainer_indoor_bike_power(250));
    let sample = hub
        .wait_sample("the fresh trainer power", |s| s.power_w == Some(250.0))
        .await;
    assert_eq!(sample.power_w, Some(250.0));

    // No refresh: the merged sample loses the value instead of repeating it forever.
    let sample = hub
        .wait_sample("the stale sample", |s| s.power_w.is_none())
        .await;
    assert_eq!(sample.power_w, None);
    assert_eq!(
        STALE_AFTER,
        Duration::from_secs(3),
        "the shipped staleness window"
    );
}

// ───────────────────────────── simulated rider + rate ─────────────────────────────

#[tokio::test]
async fn simulated_rider_connects_and_the_sample_rate_is_bounded() {
    let adapter = FakeAdapter::new(Vec::new());
    let mut cfg = HubConfig::fast();
    cfg.emit_min_interval = Duration::from_millis(40);
    let mut hub = Harness::spawn(adapter, cfg);
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: SIM_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    let name = hub
        .wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    assert_eq!(name.as_deref(), Some(SIM_NAME), "Szimulált tekerő");
    hub.send(SensorCommand::SetVirtualPower(180.0));

    let samples = hub.samples_for(Duration::from_millis(200)).await;
    assert!(!samples.is_empty(), "the simulated rider emits samples");
    assert!(
        samples.len() <= 200 / 40 + 2,
        "{} samples in 200 ms with a 40 ms gate",
        samples.len()
    );
    for sample in &samples {
        if let Some(power) = sample.power_w {
            assert!(
                power.is_finite() && (0.0..=2000.0).contains(&power),
                "{power}"
            );
        }
    }
    assert!(
        samples.iter().filter_map(|s| s.power_w).any(|p| p > 0.0),
        "the simulated rider starts producing power"
    );

    // The shipped configuration keeps the emission rate at about 10 Hz.
    let defaults = HubConfig::default();
    assert_eq!(defaults.tick, Duration::from_millis(100));
    assert!(defaults.emit_min_interval >= Duration::from_millis(90));
    assert!(
        defaults.tick >= defaults.emit_min_interval,
        "never more than one sample per tick"
    );
}

// ───────────────────────────── one device, several roles ─────────────────────────────

#[tokio::test]
async fn one_trainer_fills_all_roles_it_advertises_on_a_single_link() {
    let trainer = FakePeripheral::bridge_trainer("b1", "Bridge Trainer");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: "b1".to_string(),
        kind: DeviceKind::Trainer,
    });
    for kind in [
        DeviceKind::Trainer,
        DeviceKind::PowerMeter,
        DeviceKind::Cadence,
        DeviceKind::HeartRate,
    ] {
        hub.wait_state(kind, &ConnState::Connected).await;
    }
    assert_eq!(trainer.connects(), 1, "one GATT connection for four roles");
    assert!(trainer.is_connected_now());

    // Indoor Bike Data: power only (200 W).
    trainer.push(0x2AD2, &trainer_indoor_bike_power(200));
    // Cycling Power: 210 W and 60 rpm from the crank counters.
    trainer.push(0x2A63, &cp_crank_packet(210, 0, 0));
    trainer.push(0x2A63, &cp_crank_packet(210, 1, 1024));
    // CSC crank: 120 rpm.
    trainer.push(0x2A5B, &csc_crank_packet(0, 0));
    trainer.push(0x2A5B, &csc_crank_packet(2, 1024));
    // Bridged heart rate: 140 bpm.
    trainer.push(0x2A37, &[0x00, 140]);

    // power: power meter (the trainer's own CPS) > trainer; cadence: CSC > CPS > trainer;
    // HR: the bridged HR service is the strap source.
    let sample = hub
        .wait_sample("power, cadence and HR from one device", |s| {
            s.power_w == Some(210.0) && s.cadence_rpm == Some(120.0) && s.hr_bpm == Some(140.0)
        })
        .await;
    assert_eq!(sample.hr_bpm, Some(140.0));
    assert_eq!(sample.power_w, Some(210.0));
    assert_eq!(sample.cadence_rpm, Some(120.0));

    // Disconnecting any of its roles disconnects the whole device (the UI's "Bontás").
    hub.send(SensorCommand::Disconnect(DeviceKind::HeartRate));
    hub.wait_state(DeviceKind::Trainer, &ConnState::Disconnected)
        .await;
    hub.wait_until("the link to close", || trainer.disconnects() >= 1)
        .await;
    assert!(!trainer.is_connected_now());
}

#[tokio::test]
async fn a_dedicated_strap_takes_only_the_heart_rate_role() {
    let trainer = FakePeripheral::bridge_trainer("b1", "Bridge Trainer");
    let strap = FakePeripheral::heart_rate("h1", "Strap");
    let adapter = FakeAdapter::new(vec![trainer.clone(), strap.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: "b1".to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::HeartRate, &ConnState::Connected)
        .await;

    // A real strap replaces the bridged HR; the trainer keeps its other three roles.
    hub.send(SensorCommand::Connect {
        id: "h1".to_string(),
        kind: DeviceKind::HeartRate,
    });
    hub.wait_state(DeviceKind::HeartRate, &ConnState::Connected)
        .await;
    assert_eq!(trainer.connects(), 1, "the trainer was not reconnected");
    assert!(trainer.is_connected_now(), "its link stays up");

    // The bridged HR value is no longer used: the strap's value wins even if the trainer
    // keeps sending its own later.
    strap.push(0x2A37, &[0x00, 132]);
    trainer.push(0x2A37, &[0x00, 150]);
    let sample = hub
        .wait_sample("the strap's heart rate", |s| s.hr_bpm == Some(132.0))
        .await;
    assert_eq!(sample.hr_bpm, Some(132.0));

    // The other roles are untouched and still update. Once the CPS values went stale the
    // Indoor Bike Data power takes over.
    tokio::time::sleep(Duration::from_millis(320)).await;
    trainer.push(0x2AD2, &trainer_indoor_bike_power(190));
    let sample = hub
        .wait_sample("the trainer still owns power", |s| s.power_w == Some(190.0))
        .await;
    assert_eq!(sample.power_w, Some(190.0));
    hub.wait_state(DeviceKind::PowerMeter, &ConnState::Connected)
        .await;
}

#[tokio::test]
async fn advertised_roles_are_claimed_even_without_a_scan() {
    // No StartScan: the hub still learns the roles from the peripheral's advertisement.
    let trainer = FakePeripheral::bridge_trainer("b1", "Bridge Trainer");
    let adapter = FakeAdapter::new(vec![trainer]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: "b1".to_string(),
        kind: DeviceKind::Trainer,
    });
    for kind in [
        DeviceKind::Trainer,
        DeviceKind::PowerMeter,
        DeviceKind::Cadence,
        DeviceKind::HeartRate,
    ] {
        hub.wait_state(kind, &ConnState::Connected).await;
    }
}

#[tokio::test]
async fn a_real_trainer_replaces_the_simulated_rider_in_the_same_role() {
    let trainer = FakePeripheral::trainer(TRAINER_ID, "Think X2 Max");
    let adapter = FakeAdapter::new(vec![trainer.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: SIM_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    hub.send(SensorCommand::TrainerStart);
    hub.send(SensorCommand::SetVirtualPower(180.0));
    let sample = hub
        .wait_sample("simulated power", |s| s.power_w.unwrap_or(0.0) > 0.0)
        .await;
    assert!(sample.power_w.unwrap_or(0.0) > 0.0);

    // The real trainer takes the Trainer role; the simulator stops feeding it.
    hub.send(SensorCommand::Connect {
        id: TRAINER_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    trainer.push(0x2AD2, &trainer_indoor_bike_power(150));
    let sample = hub
        .wait_sample("trainer power after the hand-over", |s| {
            s.power_w == Some(150.0)
        })
        .await;
    assert_eq!(sample.power_w, Some(150.0));

    // ... and the simulator takes it back, closing the trainer's link.
    hub.send(SensorCommand::Connect {
        id: SIM_ID.to_string(),
        kind: DeviceKind::Trainer,
    });
    let name = hub
        .wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    assert_eq!(name.as_deref(), Some(SIM_NAME));
    hub.wait_until("the trainer link to close", || trainer.disconnects() >= 1)
        .await;
    assert!(!trainer.is_connected_now());
}

/// Wait for the next `Shift` event (other events are skipped).
async fn next_shift(hub: &mut Harness) -> i32 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!left.is_zero(), "timed out waiting for a gear step");
        let event = tokio::time::timeout(left, hub.evt.recv())
            .await
            .expect("timed out waiting for a gear step")
            .expect("the hub closed its event channel");
        if let SensorEvent::Shift(delta) = event {
            return delta;
        }
    }
}

#[tokio::test]
async fn a_zwift_click_steps_the_gearbox_over_one_plain_link() {
    let click = FakePeripheral::shifter("c1", "Zwift Click 7F3A", Some(9));
    let adapter = FakeAdapter::new(vec![click.clone()]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered

    hub.send(SensorCommand::Connect {
        id: "c1".to_string(),
        kind: DeviceKind::Shifter,
    });
    hub.wait_state(DeviceKind::Shifter, &ConnState::Connected)
        .await;
    // the notifications and the response characteristic are subscribed
    let subscribed = click.subscribed();
    assert!(subscribed.contains(&zwift::MEASUREMENT_UUID));
    assert!(subscribed.contains(&zwift::RESPONSE_UUID));
    // the hello went out; the fake answers it in plain mode before anything else arrives
    hub.wait_until("the Zwift hello", || {
        click
            .writes_to(zwift::CONTROL_UUID)
            .iter()
            .any(|w| w.starts_with(zwift::crypto::RIDE_ON))
    })
    .await;

    // Plus pressed: one gear harder, then released; Minus pressed: one gear easier
    click.push_uuid(zwift::MEASUREMENT_UUID, &[0x37, 0x08, 0x00]);
    assert_eq!(next_shift(&mut hub).await, 1);
    click.push_uuid(zwift::MEASUREMENT_UUID, &[0x37, 0x08, 0x01]);
    click.push_uuid(zwift::MEASUREMENT_UUID, &[0x37, 0x10, 0x00]);
    assert_eq!(next_shift(&mut hub).await, -1);

    // every shift is acknowledged with a haptic command (plain: opcode 0x12 first)
    hub.wait_until("the haptic acknowledgement", || {
        click
            .writes_to(zwift::CONTROL_UUID)
            .iter()
            .any(|w| w.first() == Some(&0x12))
    })
    .await;

    // a held button is one step only (the press is after the 180 ms window of the last one)
    tokio::time::sleep(Duration::from_millis(200)).await;
    click.push_uuid(zwift::MEASUREMENT_UUID, &[0x37, 0x08, 0x00]);
    assert_eq!(next_shift(&mut hub).await, 1);
    for _ in 0..3 {
        click.push_uuid(zwift::MEASUREMENT_UUID, &[0x37, 0x08, 0x00]);
    }
    let extra = hub.events_for(Duration::from_millis(60)).await;
    assert!(
        !extra.iter().any(|e| matches!(e, SensorEvent::Shift(_))),
        "holding plus does not repeat"
    );
}

#[tokio::test]
async fn a_zwift_hub_with_ftms_never_becomes_a_shifter() {
    // a Hub exposes the Zwift service as well as FTMS: the trainer role wins, no Shifter
    let trainer = FakePeripheral::trainer("h1", "Zwift Hub");
    let adapter = FakeAdapter::new(vec![trainer]);
    let mut hub = Harness::spawn(adapter, HubConfig::fast());
    let _ = hub.next_event().await; // Discovered
    hub.send(SensorCommand::Connect {
        id: "h1".to_string(),
        kind: DeviceKind::Trainer,
    });
    hub.wait_state(DeviceKind::Trainer, &ConnState::Connected)
        .await;
    let events = hub.events_for(Duration::from_millis(60)).await;
    assert!(
        !events.iter().any(|e| matches!(
            e,
            SensorEvent::State {
                kind: DeviceKind::Shifter,
                ..
            }
        )),
        "no shifter role on a trainer"
    );
}
