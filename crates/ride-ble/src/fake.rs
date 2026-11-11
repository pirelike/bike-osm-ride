//! In-memory implementation of the transport traits for tests: scripted GATT servers with
//! failure injection, so connect/reconnect, the FTMS handshake, keep-alive and the whole
//! hub run through their real code paths without Bluetooth hardware.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::{BoxStream, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::error::BleError;
use crate::scan::{SERVICE_CSC, SERVICE_CYCLING_POWER, SERVICE_FTMS, SERVICE_HEART_RATE};
use crate::transport::{
    sig_uuid, AdapterEvent, Advertisement, BleAdapter, CharProps, GattChar, GattPeripheral,
    Notification,
};
use crate::zwift;
use ride_i18n::t;

#[derive(Default)]
struct FakeState {
    connected: bool,
    chars: Vec<GattChar>,
    reads: HashMap<Uuid, Vec<u8>>,
    writes: Vec<(Uuid, Vec<u8>)>,
    notif_tx: Option<mpsc::UnboundedSender<Notification>>,
    subscribed: Vec<Uuid>,
    /// Fail this many upcoming `connect` calls.
    connect_failures: u32,
    connects: u32,
    disconnects: u32,
    /// Calls of `disconnect`, whether or not a link was up.
    disconnect_calls: u32,
    /// `disconnect` takes this long (a link that is slow to be handed back).
    disconnect_delay: Duration,
    /// `connect` never answers (Android's direct connect can pend for tens of seconds).
    hang_connects: bool,
    /// `connect` takes this long before it answers (a slow Android connect).
    connect_delay: Duration,
    /// Link operations in the order they happened: `connect`, `disconnect:begin`, `disconnect:end`.
    ops: Vec<&'static str>,
    /// Answer Control Point writes automatically with these result codes (default: success).
    auto_respond: bool,
    cp_results: HashMap<u8, u8>,
    /// Never answer Control Point writes of these op codes.
    cp_silent: HashSet<u8>,
    /// Report `Machine Status 0x01 (Reset)` after every Control Point write, the way some
    /// FTMS firmware does (captured from a real trainer on 2026-11-01).
    reset_status_after_write: bool,
    adv: Advertisement,
}

#[derive(Clone)]
pub struct FakePeripheral {
    id: String,
    st: Arc<Mutex<FakeState>>,
}

fn ch(service: u16, short: u16, props: CharProps) -> GattChar {
    GattChar {
        service: sig_uuid(service),
        uuid: sig_uuid(short),
        props,
    }
}

const NOTIFY: CharProps = CharProps {
    read: false,
    write: false,
    write_without_response: false,
    notify: true,
    indicate: false,
};
const INDICATE_WRITE: CharProps = CharProps {
    read: false,
    write: true,
    write_without_response: false,
    notify: false,
    indicate: true,
};
const WRITE: CharProps = CharProps {
    read: false,
    write: true,
    write_without_response: false,
    notify: false,
    indicate: false,
};
const READ: CharProps = CharProps {
    read: true,
    write: false,
    write_without_response: false,
    notify: false,
    indicate: false,
};

impl FakePeripheral {
    fn new(id: &str, name: &str, services: &[Uuid], chars: Vec<GattChar>) -> Self {
        let st = FakeState {
            chars,
            auto_respond: true,
            adv: Advertisement {
                name: Some(name.to_string()),
                services: services.to_vec(),
                rssi: Some(-55),
                ..Default::default()
            },
            ..Default::default()
        };
        Self {
            id: id.to_string(),
            st: Arc::new(Mutex::new(st)),
        }
    }

    /// A Zwift Click (type 9) or Play controller: the Zwift service with notify (0002), control
    /// point (0003) and response (0004). It answers a plain `RideOn` hello in plain mode.
    pub fn shifter(id: &str, name: &str, controller_type: Option<u8>) -> Self {
        let zwift_char = |uuid: Uuid, props: CharProps| GattChar {
            service: zwift::SERVICE_UUID,
            uuid,
            props,
        };
        let chars = vec![
            zwift_char(zwift::MEASUREMENT_UUID, NOTIFY),
            zwift_char(zwift::CONTROL_UUID, WRITE),
            zwift_char(zwift::RESPONSE_UUID, NOTIFY),
        ];
        let p = Self::new(id, name, &[zwift::SERVICE_UUID], chars);
        p.st.lock().unwrap().adv.zwift_controller = controller_type;
        p
    }

    /// An FTMS trainer like the ThinkRider X2 Max: Indoor Bike Data, Control Point, Machine
    /// Status, Feature (sim + power target), power range 0-2000 W, inclination -10..+20 %.
    pub fn trainer(id: &str, name: &str) -> Self {
        let chars = vec![
            ch(0x1826, 0x2AD2, NOTIFY),
            ch(0x1826, 0x2AD9, INDICATE_WRITE),
            ch(0x1826, 0x2ADA, NOTIFY),
            ch(0x1826, 0x2ACC, READ),
            ch(0x1826, 0x2AD5, READ),
            ch(0x1826, 0x2AD8, READ),
        ];
        let p = Self::new(id, name, &[SERVICE_FTMS], chars);
        {
            let mut st = p.st.lock().unwrap();
            // Fitness Machine Feature 034000000ca00000 (captured on an X2 Max)
            st.reads.insert(
                sig_uuid(0x2ACC),
                vec![0x03, 0x40, 0x00, 0x00, 0x0c, 0xa0, 0x00, 0x00],
            );
            st.reads
                .insert(sig_uuid(0x2AD8), vec![0x00, 0x00, 0xd0, 0x07, 0x01, 0x00]);
            // inclination range -10.0 .. +20.0 %, step 0.1
            st.reads
                .insert(sig_uuid(0x2AD5), vec![0x9C, 0xFF, 0xC8, 0x00, 0x01, 0x00]);
        }
        p
    }

    /// A Zwift Hub: an FTMS trainer that also advertises and exposes the Zwift service (with its
    /// controller byte), so the Shifter role is reachable from the advertisement alone.
    pub fn zwift_hub(id: &str, name: &str) -> Self {
        let p = Self::trainer(id, name);
        {
            let mut st = p.st.lock().unwrap();
            for (uuid, props) in [
                (zwift::MEASUREMENT_UUID, NOTIFY),
                (zwift::CONTROL_UUID, WRITE),
                (zwift::RESPONSE_UUID, NOTIFY),
            ] {
                st.chars.push(GattChar {
                    service: zwift::SERVICE_UUID,
                    uuid,
                    props,
                });
            }
            st.adv.services.push(zwift::SERVICE_UUID);
            st.adv.zwift_controller = Some(9);
        }
        p
    }

    /// An FTMS trainer that also exposes Cycling Power, CSC and a bridged Heart Rate service
    /// — one connection, four usable roles (several modern smart trainers do this).
    pub fn bridge_trainer(id: &str, name: &str) -> Self {
        let chars = vec![
            ch(0x1826, 0x2AD2, NOTIFY),
            ch(0x1826, 0x2AD9, INDICATE_WRITE),
            ch(0x1826, 0x2ADA, NOTIFY),
            ch(0x1826, 0x2ACC, READ),
            ch(0x1826, 0x2AD5, READ),
            ch(0x1826, 0x2AD8, READ),
            ch(0x1818, 0x2A63, NOTIFY),
            ch(0x1816, 0x2A5B, NOTIFY),
            ch(0x180D, 0x2A37, NOTIFY),
        ];
        let p = Self::new(
            id,
            name,
            &[
                SERVICE_FTMS,
                SERVICE_CYCLING_POWER,
                SERVICE_CSC,
                SERVICE_HEART_RATE,
            ],
            chars,
        );
        {
            let mut st = p.st.lock().unwrap();
            st.reads.insert(
                sig_uuid(0x2ACC),
                vec![0x03, 0x40, 0x00, 0x00, 0x0c, 0xa0, 0x00, 0x00],
            );
            st.reads
                .insert(sig_uuid(0x2AD8), vec![0x00, 0x00, 0xd0, 0x07, 0x01, 0x00]);
            st.reads
                .insert(sig_uuid(0x2AD5), vec![0x9C, 0xFF, 0xC8, 0x00, 0x01, 0x00]);
        }
        p
    }

    pub fn heart_rate(id: &str, name: &str) -> Self {
        Self::new(
            id,
            name,
            &[SERVICE_HEART_RATE],
            vec![ch(0x180D, 0x2A37, NOTIFY)],
        )
    }

    pub fn power_meter(id: &str, name: &str) -> Self {
        Self::new(
            id,
            name,
            &[SERVICE_CYCLING_POWER],
            vec![ch(0x1818, 0x2A63, NOTIFY)],
        )
    }

    pub fn cadence(id: &str, name: &str) -> Self {
        Self::new(id, name, &[SERVICE_CSC], vec![ch(0x1816, 0x2A5B, NOTIFY)])
    }

    /// A peripheral that connects fine but lacks the service its role needs.
    pub fn wrong_device(id: &str, name: &str) -> Self {
        Self::new(id, name, &[], vec![ch(0x180F, 0x2A19, NOTIFY)])
    }

    // ── test controls ──

    /// Inject a notification / indication from the device.
    pub fn push(&self, short: u16, bytes: &[u8]) {
        let st = self.st.lock().unwrap();
        if let Some(tx) = &st.notif_tx {
            let _ = tx.send(Notification {
                uuid: sig_uuid(short),
                value: bytes.to_vec(),
            });
        }
    }

    /// Inject a notification from a characteristic of any service (by full UUID).
    pub fn push_uuid(&self, uuid: Uuid, bytes: &[u8]) {
        let st = self.st.lock().unwrap();
        if let Some(tx) = &st.notif_tx {
            let _ = tx.send(Notification {
                uuid,
                value: bytes.to_vec(),
            });
        }
    }

    /// Everything written to the characteristic `uuid` so far.
    pub fn writes_to(&self, uuid: Uuid) -> Vec<Vec<u8>> {
        self.st
            .lock()
            .unwrap()
            .writes
            .iter()
            .filter(|(u, _)| *u == uuid)
            .map(|(_, b)| b.clone())
            .collect()
    }

    /// Everything written to the Control Point so far.
    pub fn cp_writes(&self) -> Vec<Vec<u8>> {
        self.st
            .lock()
            .unwrap()
            .writes
            .iter()
            .filter(|(u, _)| *u == sig_uuid(0x2AD9))
            .map(|(_, b)| b.clone())
            .collect()
    }

    pub fn clear_writes(&self) {
        self.st.lock().unwrap().writes.clear();
    }

    pub fn is_connected_now(&self) -> bool {
        self.st.lock().unwrap().connected
    }

    pub fn connects(&self) -> u32 {
        self.st.lock().unwrap().connects
    }

    pub fn disconnects(&self) -> u32 {
        self.st.lock().unwrap().disconnects
    }

    pub fn subscribed(&self) -> Vec<Uuid> {
        self.st.lock().unwrap().subscribed.clone()
    }

    pub fn fail_next_connects(&self, n: u32) {
        self.st.lock().unwrap().connect_failures = n;
    }

    /// Calls of `disconnect` so far, whether or not a link was up.
    pub fn disconnect_calls(&self) -> u32 {
        self.st.lock().unwrap().disconnect_calls
    }

    /// Make `disconnect` take `delay`.
    pub fn set_disconnect_delay(&self, delay: Duration) {
        self.st.lock().unwrap().disconnect_delay = delay;
    }

    /// Every `connect` takes this long.
    pub fn set_connect_delay(&self, delay: Duration) {
        self.st.lock().unwrap().connect_delay = delay;
    }

    /// While set, `connect` never answers.
    pub fn hang_connects(&self, hang: bool) {
        self.st.lock().unwrap().hang_connects = hang;
    }

    /// The link operations in order: `connect`, `disconnect:begin`, `disconnect:end`.
    pub fn ops(&self) -> Vec<&'static str> {
        self.st.lock().unwrap().ops.clone()
    }

    /// Simulate the link dropping: the notification stream ends and `is_connected` turns false.
    pub fn drop_link(&self) {
        let mut st = self.st.lock().unwrap();
        st.connected = false;
        st.notif_tx = None;
    }

    /// Answer writes of `opcode` with this result code instead of success.
    pub fn set_cp_result(&self, opcode: u8, result: u8) {
        self.st.lock().unwrap().cp_results.insert(opcode, result);
    }

    /// Never answer writes of `opcode`.
    pub fn silence_cp(&self, opcode: u8) {
        self.st.lock().unwrap().cp_silent.insert(opcode);
    }

    /// Report `Machine Status 0x01 (Reset)` after every Control Point write, like the FTMS
    /// firmware of a captured trainer.
    pub fn report_reset_after_every_write(&self) {
        self.st.lock().unwrap().reset_status_after_write = true;
    }

    pub fn set_rssi(&self, rssi: i16) {
        self.st.lock().unwrap().adv.rssi = Some(rssi);
    }
}

impl GattPeripheral for FakePeripheral {
    fn id(&self) -> String {
        self.id.clone()
    }

    async fn connect(&self) -> Result<(), BleError> {
        let (hang, delay) = {
            let mut st = self.st.lock().unwrap();
            st.connects += 1;
            st.ops.push("connect");
            (st.hang_connects, st.connect_delay)
        };
        if hang {
            std::future::pending::<()>().await;
        }
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let mut st = self.st.lock().unwrap();
        if st.connect_failures > 0 {
            st.connect_failures -= 1;
            return Err(BleError::Gatt(
                t("ble.fake.a_kapcsolodas_sikertelen").into(),
            ));
        }
        st.connected = true;
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        let delay = {
            let mut st = self.st.lock().unwrap();
            st.disconnect_calls += 1;
            st.ops.push("disconnect:begin");
            st.disconnect_delay
        };
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let mut st = self.st.lock().unwrap();
        if st.connected {
            st.disconnects += 1;
        }
        st.connected = false;
        st.notif_tx = None;
        st.ops.push("disconnect:end");
        Ok(())
    }

    async fn is_connected(&self) -> Result<bool, BleError> {
        Ok(self.st.lock().unwrap().connected)
    }

    async fn discover_services(&self) -> Result<(), BleError> {
        if self.st.lock().unwrap().connected {
            Ok(())
        } else {
            Err(BleError::NotConnected)
        }
    }

    fn characteristics(&self) -> Vec<GattChar> {
        self.st.lock().unwrap().chars.clone()
    }

    async fn read(&self, c: &GattChar) -> Result<Vec<u8>, BleError> {
        let st = self.st.lock().unwrap();
        if !st.connected {
            return Err(BleError::NotConnected);
        }
        st.reads
            .get(&c.uuid)
            .cloned()
            .ok_or_else(|| BleError::Gatt(t("ble.fake.nincs_ertek").into()))
    }

    async fn write(&self, c: &GattChar, data: &[u8], _with_response: bool) -> Result<(), BleError> {
        let mut st = self.st.lock().unwrap();
        if !st.connected {
            return Err(BleError::NotConnected);
        }
        st.writes.push((c.uuid, data.to_vec()));
        if c.uuid == zwift::CONTROL_UUID && data.starts_with(zwift::crypto::RIDE_ON) {
            // plain-mode answer: the bare greeting on the response characteristic
            if let Some(tx) = &st.notif_tx {
                let _ = tx.send(Notification {
                    uuid: zwift::RESPONSE_UUID,
                    value: zwift::crypto::RIDE_ON.to_vec(),
                });
            }
        }
        if c.uuid == sig_uuid(0x2AD9) && st.auto_respond {
            if let Some(&op) = data.first() {
                if !st.cp_silent.contains(&op) {
                    let result = st.cp_results.get(&op).copied().unwrap_or(0x01);
                    if let Some(tx) = &st.notif_tx {
                        let _ = tx.send(Notification {
                            uuid: c.uuid,
                            value: vec![0x80, op, result],
                        });
                    }
                }
            }
        }
        if c.uuid == sig_uuid(0x2AD9) && st.reset_status_after_write {
            if let Some(tx) = &st.notif_tx {
                // Machine Status 0x01 = Reset, sent right after the answer (as captured)
                let _ = tx.send(Notification {
                    uuid: sig_uuid(0x2ADA),
                    value: vec![0x01],
                });
            }
        }
        Ok(())
    }

    async fn subscribe(&self, c: &GattChar) -> Result<(), BleError> {
        let mut st = self.st.lock().unwrap();
        if !st.connected {
            return Err(BleError::NotConnected);
        }
        st.subscribed.push(c.uuid);
        Ok(())
    }

    async fn unsubscribe(&self, c: &GattChar) -> Result<(), BleError> {
        let mut st = self.st.lock().unwrap();
        st.subscribed.retain(|u| *u != c.uuid);
        Ok(())
    }

    async fn notifications(&self) -> Result<BoxStream<'static, Notification>, BleError> {
        let mut st = self.st.lock().unwrap();
        if !st.connected {
            return Err(BleError::NotConnected);
        }
        let (tx, rx) = mpsc::unbounded_channel();
        st.notif_tx = Some(tx);
        Ok(
            futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|v| (v, rx)) })
                .boxed(),
        )
    }
}

struct AdapterState {
    peripherals: Vec<FakePeripheral>,
    events: Option<mpsc::UnboundedSender<AdapterEvent<FakePeripheral>>>,
    scanning: bool,
    scan_starts: u32,
    scan_stops: u32,
    start_error: Option<BleError>,
}

#[derive(Clone)]
pub struct FakeAdapter {
    st: Arc<Mutex<AdapterState>>,
}

impl FakeAdapter {
    pub fn new(peripherals: Vec<FakePeripheral>) -> Self {
        Self {
            st: Arc::new(Mutex::new(AdapterState {
                peripherals,
                events: None,
                scanning: false,
                scan_starts: 0,
                scan_stops: 0,
                start_error: None,
            })),
        }
    }

    /// Simulate an advertisement from `p` reaching the adapter.
    pub fn announce(&self, p: &FakePeripheral) {
        let st = self.st.lock().unwrap();
        if let Some(tx) = &st.events {
            let _ = tx.send(AdapterEvent::Seen(p.clone()));
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.st.lock().unwrap().scanning
    }

    pub fn scan_starts(&self) -> u32 {
        self.st.lock().unwrap().scan_starts
    }

    pub fn scan_stops(&self) -> u32 {
        self.st.lock().unwrap().scan_stops
    }

    pub fn fail_scan(&self, e: Option<BleError>) {
        self.st.lock().unwrap().start_error = e;
    }
}

impl BleAdapter for FakeAdapter {
    type Peripheral = FakePeripheral;

    async fn start_scan(&self) -> Result<(), BleError> {
        let mut st = self.st.lock().unwrap();
        if let Some(e) = st.start_error.clone() {
            return Err(e);
        }
        st.scanning = true;
        st.scan_starts += 1;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<(), BleError> {
        let mut st = self.st.lock().unwrap();
        st.scanning = false;
        st.scan_stops += 1;
        Ok(())
    }

    async fn events(&self) -> Result<BoxStream<'static, AdapterEvent<FakePeripheral>>, BleError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.st.lock().unwrap().events = Some(tx);
        Ok(
            futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|v| (v, rx)) })
                .boxed(),
        )
    }

    async fn peripherals(&self) -> Result<Vec<FakePeripheral>, BleError> {
        Ok(self.st.lock().unwrap().peripherals.clone())
    }

    async fn advertisement(&self, p: &FakePeripheral) -> Result<Option<Advertisement>, BleError> {
        Ok(Some(p.st.lock().unwrap().adv.clone()))
    }
}
