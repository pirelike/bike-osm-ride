//! Device discovery.
//!
//! **Windows (WinRT) scanning: service-UUID scan filters are unreliable**, so the scan is
//! always unfiltered. Devices are classified afterwards from the services they advertise
//! and, only when the advertisement is silent about services, from their name. The services
//! are verified again after connecting (a wrong guess becomes a clear error, not a hang).
//!
//! Pattern notes (iron-heart, MIT, see NOTICE.md): the scan is driven by the adapter's
//! event stream, a scan that went silent for 30 s is restarted, and a failing `start_scan`
//! is reported instead of retried in a loop.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use ride_model::{DeviceInfo, DeviceKind};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::error::bounded;
use crate::link::{stopped, HubConfig, Internal};
use crate::transport::{sig_uuid, AdapterEvent, Advertisement, BleAdapter, ConnectedDevice, GattPeripheral};
use crate::zwift::ControllerKind;
use ride_i18n::{t, tf};

pub const SERVICE_HEART_RATE: Uuid = sig_uuid(0x180D);
pub const SERVICE_CYCLING_POWER: Uuid = sig_uuid(0x1818);
pub const SERVICE_CSC: Uuid = sig_uuid(0x1816);
pub const SERVICE_FTMS: Uuid = sig_uuid(0x1826);
/// Zwift's custom service: Click, Play and Hub.
pub const SERVICE_ZWIFT: Uuid = crate::zwift::SERVICE_UUID;

/// A named device without any recognised service is still listed when it is this close
/// (so a trainer with a silent advertisement can be picked by the person sitting on it).
const NEARBY_RSSI_DBM: i16 = -65;

/// Name prefixes (lower case) that identify a role when no service is advertised.
const TRAINER_NAMES: &[&str] = &[
    "think x",
    "kickr",
    "wahoo kickr",
    "tacx",
    "zwift hub",
    "elite direto",
    "elite suito",
    "elite zumo",
    "saris",
    "jetblack",
    "wattbike",
    "bkool",
    "magene t",
    "xplova",
    "rouvy",
];
const HEART_RATE_NAMES: &[&str] = &[
    "polar h",
    "polar oh",
    "hrm",
    "tickr",
    "wahoo tickr",
    "coospo h",
    "magene h",
    "scosche",
    "myzone",
    "heart",
    "pulse",
];
const POWER_NAMES: &[&str] = &[
    "assioma", "favero", "stages", "4iiii", "quarq", "powertap", "rally", "srm", "vector",
];
const CADENCE_NAMES: &[&str] = &["cadence", "rpm", "magene s3", "gsc", "csc"];

/// Name prefixes (lower case) of the Zwift virtual-shifting controllers.
const ZWIFT_CONTROLLER_NAMES: &[&str] = &["zwift click", "zwift play"];
/// Name prefixes (lower case) of the Zwift Hub trainer, which also exposes the Zwift service.
const ZWIFT_HUB_NAMES: &[&str] = &["zwift hub"];

/// Kinds a device can serve, from its advertised services; the name is only consulted when
/// the services say nothing. Order: Trainer, PowerMeter, Cadence, HeartRate, Shifter.
pub fn classify(name: Option<&str>, services: &[Uuid]) -> Vec<DeviceKind> {
    classify_with(name, services, None)
}

/// [`classify`] with the Zwift controller type byte from the manufacturer data, if any.
pub fn classify_with(
    name: Option<&str>,
    services: &[Uuid],
    zwift_controller: Option<u8>,
) -> Vec<DeviceKind> {
    let mut kinds = Vec::new();
    let has = |u: &Uuid| services.contains(u);
    if has(&SERVICE_FTMS) {
        kinds.push(DeviceKind::Trainer);
    }
    if has(&SERVICE_CYCLING_POWER) {
        kinds.push(DeviceKind::PowerMeter);
    }
    if has(&SERVICE_CSC) {
        kinds.push(DeviceKind::Cadence);
    }
    if has(&SERVICE_HEART_RATE) {
        kinds.push(DeviceKind::HeartRate);
    }
    let lower_name = name.map(|n| n.trim().to_lowercase()).unwrap_or_default();
    let starts = |list: &[&str]| list.iter().any(|p| lower_name.starts_with(p));
    // A Zwift Hub advertises FTMS and is a trainer; only a controller without FTMS shifts.
    let controller = has(&SERVICE_ZWIFT)
        || zwift_controller
            .and_then(ControllerKind::from_advertised)
            .is_some()
        || starts(ZWIFT_CONTROLLER_NAMES);
    if controller && !has(&SERVICE_FTMS) && !starts(ZWIFT_HUB_NAMES) {
        kinds.push(DeviceKind::Shifter);
    }
    if kinds.is_empty() && !lower_name.is_empty() {
        if starts(TRAINER_NAMES) {
            kinds.push(DeviceKind::Trainer);
        } else if starts(HEART_RATE_NAMES) {
            kinds.push(DeviceKind::HeartRate);
        } else if starts(POWER_NAMES) {
            kinds.push(DeviceKind::PowerMeter);
        } else if starts(CADENCE_NAMES) {
            kinds.push(DeviceKind::Cadence);
        }
    }
    kinds
}

#[derive(Debug, Clone)]
struct Entry {
    name: Option<String>,
    services: HashSet<Uuid>,
    rssi: Option<i16>,
    /// Zwift controller type byte from the manufacturer data, once seen.
    zwift: Option<u8>,
    /// The OS reports the device as connected (seeded by `seed_connected`).
    connected: bool,
    /// The hub has a GATT handle for the device (`seed_connected` decides; any real
    /// advertisement re-enables it). `false`: another app owns the link, Android refuses
    /// a second client — the row is information, not a connect target.
    connectable: bool,
    last_seen: Option<Instant>,
}

/// What the list shows of a device: its name, its roles (the set, not just how many: a
/// Zwift Hub first classified as a Shifter turns into a Trainer), whether it is heard,
/// and the connected/connectable marks the row renders. The signature of the *visible*
/// list: any change in it must re-emit the list.
#[derive(Debug, Clone, PartialEq)]
struct Visible {
    name: Option<String>,
    kinds: Vec<DeviceKind>,
    heard: bool,
    connected: bool,
    connectable: bool,
}

/// What the scan has seen so far, aged and classified.
#[derive(Debug, Clone)]
pub struct DiscoveryTable {
    entries: HashMap<String, Entry>,
    forget_after: Duration,
}

impl DiscoveryTable {
    pub fn new(forget_after: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            forget_after,
        }
    }

    /// Merge one advertisement. Returns whether the *visible list* may have changed.
    /// (Advertisement data and scan responses arrive separately, so services and names are
    /// accumulated; RSSI is only reported when it moved by ≥ 3 dBm to avoid UI churn.)
    pub fn update(&mut self, id: String, adv: &Advertisement, now: Instant) -> bool {
        let before = self.visible_signature(&id);
        let e = self.entries.entry(id.clone()).or_insert_with(|| Entry {
            name: None,
            services: HashSet::new(),
            rssi: None,
            zwift: None,
            connected: false,
            connectable: true,
            last_seen: None,
        });
        e.last_seen = Some(now);
        // A real advertisement means the adapter has the device: it can be connected to.
        e.connectable = true;
        if let Some(n) = adv.name.as_ref().filter(|n| !n.trim().is_empty()) {
            e.name = Some(n.clone());
        }
        e.services.extend(adv.services.iter().copied());
        if adv.zwift_controller.is_some() {
            e.zwift = adv.zwift_controller;
        }
        let rssi_moved = match (e.rssi, adv.rssi) {
            (Some(a), Some(b)) => (a - b).abs() >= 3,
            (None, Some(_)) => true,
            _ => false,
        };
        if rssi_moved {
            e.rssi = adv.rssi;
        }
        before != self.visible_signature(&id) || rssi_moved
    }

    /// Merge what the OS knows about a **connected** device (it usually does not advertise,
    /// so `update` would never hear of it). `connectable` says whether the hub can open a
    /// GATT link to it; while another app owns the link it cannot. Returns whether the
    /// *visible list* may have changed.
    pub fn seed_connected(
        &mut self,
        id: String,
        adv: &Advertisement,
        connectable: bool,
    ) -> bool {
        let before = self.visible_signature(&id);
        let e = self.entries.entry(id.clone()).or_insert_with(|| Entry {
            name: None,
            services: HashSet::new(),
            rssi: None,
            zwift: None,
            connected: false,
            connectable: true,
            last_seen: None,
        });
        e.connected = true;
        // The owner of the link can change between scans, so the fresh value wins. A real
        // advertisement arriving later re-enables connecting (see `update`).
        e.connectable = connectable;
        if let Some(n) = adv.name.as_ref().filter(|n| !n.trim().is_empty()) {
            e.name = Some(n.clone());
        }
        e.services.extend(adv.services.iter().copied());
        before != self.visible_signature(&id)
    }

    /// Replace the OS connection snapshot while keeping independently heard advertisements.
    fn refresh_connected<P>(&mut self, devices: Vec<ConnectedDevice<P>>) -> bool {
        let present: HashSet<_> = devices.iter().map(|d| d.id.as_str()).collect();
        let mut dirty = false;
        self.entries.retain(|id, entry| {
            if entry.connected && !present.contains(id.as_str()) {
                dirty = true;
                entry.connected = false;
                entry.connectable = true;
                return entry.last_seen.is_some();
            }
            true
        });
        for device in devices {
            dirty |= self.seed_connected(device.id, &device.adv, device.peripheral.is_some());
        }
        dirty
    }

    fn visible_signature(&self, id: &str) -> Option<Visible> {
        let e = self.entries.get(id)?;
        let services: Vec<Uuid> = e.services.iter().copied().collect();
        let mut kinds = classify_with(e.name.as_deref(), &services, e.zwift);
        let visible = Self::is_visible(e, &kinds);
        kinds.sort_by_key(|k| *k as u8);
        visible.then(|| Visible {
            name: e.name.clone(),
            kinds,
            heard: e.rssi.is_some(),
            connected: e.connected,
            connectable: e.connectable,
        })
    }

    fn is_visible(e: &Entry, kinds: &[DeviceKind]) -> bool {
        // A connected device is always worth showing: it is usually the trainer itself,
        // sitting on a link the scan cannot see.
        e.connected
            || !kinds.is_empty()
            || (e.name.is_some() && e.rssi.is_some_and(|r| r >= NEARBY_RSSI_DBM))
    }

    /// Drop devices not heard from for `forget_after`. Returns whether anything went.
    /// Devices still connected at the OS level are kept between connection snapshots.
    pub fn prune(&mut self, now: Instant) -> bool {
        let before = self.entries.len();
        let keep = self.forget_after;
        self.entries
            .retain(|_, e| {
                e.connected || e.last_seen.is_some_and(|seen| now.saturating_duration_since(seen) < keep)
            });
        self.entries.len() != before
    }

    /// The list for the UI: recognised devices first, connected ones next (a trainer on a
    /// live link is what the rider is usually looking for), then by signal strength and name.
    pub fn snapshot(&self) -> Vec<DeviceInfo> {
        let mut out: Vec<(bool, bool, DeviceInfo)> = self
            .entries
            .iter()
            .filter_map(|(id, e)| {
                let services: Vec<Uuid> = e.services.iter().copied().collect();
                let kinds = classify_with(e.name.as_deref(), &services, e.zwift);
                if !Self::is_visible(e, &kinds) {
                    return None;
                }
                let tail: String = id
                    .chars()
                    .rev()
                    .take(5)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let name = e.name.clone().unwrap_or_else(|| {
                    tf(
                        "ble.scan.nevtelen_eszkoz_tail",
                        &[("tail", &tail.to_string())],
                    )
                });
                Some((
                    !kinds.is_empty(),
                    e.connected,
                    DeviceInfo {
                        id: id.clone(),
                        name,
                        kinds,
                        rssi: e.rssi,
                        connected: e.connected,
                        connectable: e.connectable,
                    },
                ))
            })
            .collect();
        out.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| {
                    b.2.rssi
                        .unwrap_or(i16::MIN)
                        .cmp(&a.2.rssi.unwrap_or(i16::MIN))
                })
                .then_with(|| a.2.name.cmp(&b.2.name))
                .then_with(|| a.2.id.cmp(&b.2.id))
        });
        out.into_iter().map(|(_, _, d)| d).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Shown once when a scan that started without error has not heard a single device.
fn silent_scan_hint() -> &'static str {
    t("ble.scan.a_kereses_nem_lat_egyetlen_bluetooth_eszkozt_sem_ell")
}

pub(crate) async fn run_scan<A: BleAdapter>(
    adapter: Arc<A>,
    tx: mpsc::UnboundedSender<Internal>,
    cfg: Arc<HubConfig>,
    mut stop: watch::Receiver<bool>,
    generation: u64,
) {
    let notice = |text: String| {
        let _ = tx.send(Internal::Notice(text));
    };
    let events = || bounded(t("ble.hub.eszkozlista"), cfg.io_timeout, adapter.events());
    let start = || bounded(t("ble.hub.eszkozlista"), cfg.io_timeout, adapter.start_scan());
    let mut events_stream = match events().await {
        Ok(e) => e,
        Err(e) => {
            notice(tf(
                "ble.scan.a_kereses_nem_indithato_e",
                &[("e", &e.to_string())],
            ));
            return;
        }
    };
    if let Err(e) = start().await {
        notice(tf(
            "ble.scan.a_kereses_nem_indithato_e_ellenorizd_hogy_be_van_e_k",
            &[("e", &e.to_string())],
        ));
        return;
    }
    let _ = tx.send(Internal::ScanStarted(generation));

    let mut table = DiscoveryTable::new(cfg.scan_forget_after);
    let mut dirty = true;
    // devices the OS already knows about show up immediately
    let known = async {
        let mut known = Vec::new();
        if let Ok(peripherals) = adapter.peripherals().await {
            for p in peripherals {
                if let Ok(Some(adv)) = adapter.advertisement(&p).await {
                    known.push((p.id(), adv));
                }
            }
        }
        known
    };
    let mut known_pending = true;
    let known = tokio::time::timeout(cfg.io_timeout, known);
    tokio::pin!(known);
    // Refresh even if advertisements are arriving, and poll concurrently with them.
    let adapter_ref = adapter.as_ref();
    let io_timeout = cfg.io_timeout;
    let connected = |delay| async move {
        tokio::time::sleep(delay).await;
        tokio::time::timeout(io_timeout, adapter_ref.connected_devices()).await
    };
    let refresh = connected(Duration::ZERO);
    tokio::pin!(refresh);

    let mut last_event = Instant::now();
    // The system can start a scan without error and still hand over nothing (Android without the
    // right permission or with the location switch off): say so once instead of staying silent.
    let mut heard_anything = false;
    let mut told_silence = false;
    let mut ticker = tokio::time::interval(cfg.scan_emit_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = stopped(&mut stop) => break,
            result = &mut known, if known_pending => {
                known_pending = false;
                for (id, adv) in result.unwrap_or_default() {
                    dirty |= table.update(id, &adv, Instant::now());
                }
            }
            result = &mut refresh => {
                if let Ok(Ok(devices)) = result {
                    dirty |= table.refresh_connected(devices);
                }
                refresh.set(connected(cfg.scan_connected_interval));
            }
            ev = events_stream.next() => match ev {
                Some(AdapterEvent::Seen(p)) => {
                    heard_anything = true;
                    last_event = Instant::now();
                    if let Ok(Some(adv)) = adapter.advertisement(&p).await {
                        dirty |= table.update(p.id(), &adv, last_event);
                    }
                }
                Some(AdapterEvent::Lost(_)) => {}
                None => match events().await {
                    Ok(e) => events_stream = e,
                    Err(e) => {
                        notice(tf(
                            "ble.scan.a_kereses_megszakadt_e",
                            &[("e", &e.to_string())],
                        ));
                        break;
                    }
                },
            },
            _ = ticker.tick() => {
                let now = Instant::now();
                dirty |= table.prune(now);
                if dirty {
                    dirty = false;
                    if tx.send(Internal::Discovered(generation, table.snapshot())).is_err() {
                        break;
                    }
                }
                if now.saturating_duration_since(last_event) > cfg.scan_silence_restart {
                    if !heard_anything && !told_silence {
                        told_silence = true;
                        notice(silent_scan_hint().to_string());
                    }
                    // a scan that heard nothing for a long time is probably stuck: restart it
                    let _ = tokio::time::timeout(cfg.io_timeout, adapter.stop_scan()).await;
                    if let Err(e) = start().await {
                        notice(tf(
                            "ble.scan.a_kereses_ujrainditasa_nem_sikerult_e",
                            &[("e", &e.to_string())],
                        ));
                        break;
                    }
                    last_event = now;
                }
            }
        }
    }
    let _ = tokio::time::timeout(cfg.io_timeout, adapter.stop_scan()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adv(name: Option<&str>, services: &[Uuid], rssi: Option<i16>) -> Advertisement {
        Advertisement {
            name: name.map(str::to_string),
            services: services.to_vec(),
            rssi,
            ..Default::default()
        }
    }

    #[test]
    fn services_decide_the_kind() {
        assert_eq!(classify(None, &[SERVICE_FTMS]), vec![DeviceKind::Trainer]);
        assert_eq!(
            classify(None, &[SERVICE_HEART_RATE]),
            vec![DeviceKind::HeartRate]
        );
        assert_eq!(
            classify(None, &[SERVICE_CYCLING_POWER]),
            vec![DeviceKind::PowerMeter]
        );
        assert_eq!(classify(None, &[SERVICE_CSC]), vec![DeviceKind::Cadence]);
        // a smart trainer advertises several: trainer first
        assert_eq!(
            classify(
                Some("X"),
                &[SERVICE_CSC, SERVICE_CYCLING_POWER, SERVICE_FTMS]
            ),
            vec![
                DeviceKind::Trainer,
                DeviceKind::PowerMeter,
                DeviceKind::Cadence
            ]
        );
        // services beat a misleading name
        assert_eq!(
            classify(Some("Polar H10"), &[SERVICE_FTMS]),
            vec![DeviceKind::Trainer]
        );
        assert!(
            classify(None, &[sig_uuid(0x180F)]).is_empty(),
            "battery service alone says nothing"
        );
    }

    #[test]
    fn names_are_a_fallback_for_silent_advertisements() {
        assert_eq!(
            classify(Some("Think X222- 0001234"), &[]),
            vec![DeviceKind::Trainer]
        );
        assert_eq!(
            classify(Some("KICKR CORE 1AB2"), &[]),
            vec![DeviceKind::Trainer]
        );
        assert_eq!(
            classify(Some("Polar H10 12345678"), &[]),
            vec![DeviceKind::HeartRate]
        );
        assert_eq!(
            classify(Some("  TICKR X 4F2A "), &[]),
            vec![DeviceKind::HeartRate]
        );
        assert_eq!(
            classify(Some("ASSIOMA DUO"), &[]),
            vec![DeviceKind::PowerMeter]
        );
        assert_eq!(
            classify(Some("Wahoo RPM"), &[]),
            Vec::<DeviceKind>::new(),
            "prefix match only"
        );
        assert_eq!(classify(Some("RPM Sensor"), &[]), vec![DeviceKind::Cadence]);
        assert!(
            classify(Some("Think VS01-0000"), &[]).is_empty(),
            "the V100 remote is not a trainer"
        );
        assert!(classify(Some("Bose QC"), &[]).is_empty());
        assert!(classify(Some(""), &[]).is_empty());
        assert!(classify(None, &[]).is_empty());
    }

    #[test]
    fn zwift_controllers_are_shifters() {
        let zwift = [SERVICE_ZWIFT];
        assert_eq!(classify(None, &zwift), vec![DeviceKind::Shifter]);
        // manufacturer data alone, both controller sides and the Click
        assert_eq!(classify_with(None, &[], Some(9)), vec![DeviceKind::Shifter]);
        assert_eq!(classify_with(None, &[], Some(2)), vec![DeviceKind::Shifter]);
        assert_eq!(classify_with(None, &[], Some(3)), vec![DeviceKind::Shifter]);
        // name prefixes, for stacks that hide both of the above
        assert_eq!(
            classify(Some("Zwift Click 7F3A"), &[]),
            vec![DeviceKind::Shifter]
        );
        assert_eq!(classify(Some("Zwift Play"), &[]), vec![DeviceKind::Shifter]);
        // an unknown manufacturer type is not a controller
        assert!(classify_with(None, &[], Some(1)).is_empty());
        assert!(classify_with(Some("Zwift Ride"), &[], None).is_empty());
    }

    #[test]
    fn zwift_hub_with_ftms_is_only_a_trainer() {
        assert_eq!(
            classify(Some("Zwift Hub"), &[SERVICE_FTMS, SERVICE_ZWIFT]),
            vec![DeviceKind::Trainer]
        );
        assert_eq!(
            classify_with(None, &[SERVICE_ZWIFT, SERVICE_FTMS], Some(9)),
            vec![DeviceKind::Trainer],
            "FTMS wins over controller hints"
        );
        // a silent Zwift Hub by name is still a trainer, never a shifter
        assert_eq!(
            classify(Some("Zwift Hub 4420"), &[SERVICE_ZWIFT]),
            vec![DeviceKind::Trainer]
        );
    }

    #[test]
    fn table_remembers_the_controller_type_from_manufacturer_data() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        // scan response first: name only, far away, hidden
        t.update("Z:1".into(), &adv(Some("Zwift"), &[], Some(-80)), t0);
        assert!(t.snapshot().is_empty());
        let mut with_type = adv(None, &[], Some(-60));
        with_type.zwift_controller = Some(9);
        assert!(t.update("Z:1".into(), &with_type, t0));
        let shown = t.snapshot();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].kinds, vec![DeviceKind::Shifter]);
        assert_eq!(shown[0].name, "Zwift");
    }

    #[test]
    fn table_accumulates_advertisement_and_scan_response() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        // first the scan response with only a name: not recognisable, far away: hidden
        t.update(
            "AA:01".into(),
            &adv(Some("Some Gadget"), &[], Some(-80)),
            t0,
        );
        assert!(t.snapshot().is_empty());
        // then the advertisement with the service
        assert!(t.update(
            "AA:01".into(),
            &adv(None, &[SERVICE_HEART_RATE], Some(-80)),
            t0
        ));
        let s = t.snapshot();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "Some Gadget");
        assert_eq!(s[0].kinds, vec![DeviceKind::HeartRate]);
        assert_eq!(s[0].rssi, Some(-80));
    }

    #[test]
    fn nearby_named_unknown_devices_are_offered_distant_ones_are_not() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "N:1".into(),
            &adv(Some("Mystery Trainer 9"), &[], Some(-50)),
            t0,
        );
        t.update("N:2".into(), &adv(Some("Neighbour TV"), &[], Some(-90)), t0);
        t.update("N:3".into(), &adv(None, &[], Some(-40)), t0);
        let s = t.snapshot();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "N:1");
        assert!(s[0].kinds.is_empty());
    }

    #[test]
    fn unnamed_recognised_devices_get_a_placeholder_name() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "11:22:33:44:55:66".into(),
            &adv(None, &[SERVICE_FTMS], None),
            t0,
        );
        let s = t.snapshot();
        assert_eq!(s[0].name, "Névtelen eszköz (55:66)");
    }

    #[test]
    fn sorting_recognised_first_then_by_signal() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "A".into(),
            &adv(Some("Far HR"), &[SERVICE_HEART_RATE], Some(-85)),
            t0,
        );
        t.update(
            "B".into(),
            &adv(Some("Near Trainer"), &[SERVICE_FTMS], Some(-45)),
            t0,
        );
        t.update("C".into(), &adv(Some("Close Mystery"), &[], Some(-30)), t0);
        t.update(
            "D".into(),
            &adv(Some("Mid Power"), &[SERVICE_CYCLING_POWER], Some(-60)),
            t0,
        );
        let ids: Vec<String> = t.snapshot().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["B", "D", "A", "C"]);
    }

    #[test]
    fn rssi_jitter_does_not_report_changes_but_real_movement_does() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        let id = "X".to_string();
        assert!(t.update(
            id.clone(),
            &adv(Some("HR"), &[SERVICE_HEART_RATE], Some(-60)),
            t0
        ));
        assert!(
            !t.update(id.clone(), &adv(None, &[], Some(-61)), t0),
            "1 dBm is noise"
        );
        assert!(!t.update(id.clone(), &adv(None, &[], Some(-62)), t0));
        assert!(
            t.update(id.clone(), &adv(None, &[], Some(-70)), t0),
            "10 dBm is real"
        );
        assert_eq!(t.snapshot()[0].rssi, Some(-70));
    }

    #[test]
    fn stale_devices_are_forgotten() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "A".into(),
            &adv(Some("HR"), &[SERVICE_HEART_RATE], Some(-60)),
            t0,
        );
        t.update(
            "B".into(),
            &adv(Some("HR2"), &[SERVICE_HEART_RATE], Some(-60)),
            t0 + Duration::from_secs(50),
        );
        assert!(!t.prune(t0 + Duration::from_secs(59)));
        assert!(t.prune(t0 + Duration::from_secs(61)));
        assert_eq!(t.len(), 1);
        assert_eq!(t.snapshot()[0].id, "B");
        assert!(t.prune(t0 + Duration::from_secs(200)));
        assert!(t.is_empty());
    }

    fn named_adv(name: &str) -> Advertisement {
        Advertisement {
            name: Some(name.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn a_connected_device_is_listed_without_any_advertisement() {
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        // a trainer holding a link: no services, no RSSI, only what the OS says
        assert!(t.seed_connected("VE:1".into(), &named_adv("VE200"), false));
        let s = t.snapshot();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "VE:1");
        assert_eq!(s[0].name, "VE200");
        assert!(s[0].kinds.is_empty(), "the name says nothing here");
        assert!(s[0].connected);
        assert!(!s[0].connectable, "another app owns the link");
        // seeding again with a GATT handle makes it connectable
        assert!(t.seed_connected("VE:1".into(), &named_adv("VE200"), true));
        assert!(t.snapshot()[0].connectable);
    }

    #[test]
    fn a_connected_device_survives_pruning() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "A".into(),
            &adv(Some("HR"), &[SERVICE_HEART_RATE], Some(-60)),
            t0,
        );
        t.seed_connected("VE:1".into(), &named_adv("VE200"), false);
        assert!(t.prune(t0 + Duration::from_secs(120)));
        assert_eq!(t.len(), 1, "only the connected device stays");
        assert_eq!(t.snapshot()[0].id, "VE:1");
    }

    #[test]
    fn a_real_advertisement_makes_a_connected_device_connectable() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.seed_connected("VE:1".into(), &named_adv("VE200"), false);
        // the other app let go of it and the trainer advertises again
        t.update(
            "VE:1".into(),
            &adv(Some("VE200"), &[SERVICE_FTMS], Some(-50)),
            t0,
        );
        let s = t.snapshot();
        assert_eq!(s[0].kinds, vec![DeviceKind::Trainer]);
        assert!(s[0].connectable);
        assert!(s[0].connected, "the OS still reports the link");
    }

    #[test]
    fn connected_devices_rank_above_plain_named_ones() {
        let t0 = Instant::now();
        let mut t = DiscoveryTable::new(Duration::from_secs(60));
        t.update(
            "A".into(),
            &adv(Some("HR"), &[SERVICE_HEART_RATE], Some(-60)),
            t0,
        );
        t.seed_connected("C:9".into(), &named_adv("VE200"), false);
        t.update("N".into(), &adv(Some("Neighbour TV"), &[], Some(-30)), t0);
        let ids: Vec<String> = t.snapshot().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["A", "C:9", "N"]);
    }
}
