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

use crate::link::{stopped, HubConfig, Internal};
use crate::transport::{sig_uuid, AdapterEvent, Advertisement, BleAdapter, GattPeripheral};
use crate::zwift::ControllerKind;

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
    last_seen: Instant,
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
            last_seen: now,
        });
        e.last_seen = now;
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

    fn visible_signature(&self, id: &str) -> Option<(Option<String>, usize, bool)> {
        let e = self.entries.get(id)?;
        let services: Vec<Uuid> = e.services.iter().copied().collect();
        let kinds = classify_with(e.name.as_deref(), &services, e.zwift);
        let visible = Self::is_visible(e, &kinds);
        visible.then(|| (e.name.clone(), kinds.len(), e.rssi.is_some()))
    }

    fn is_visible(e: &Entry, kinds: &[DeviceKind]) -> bool {
        !kinds.is_empty() || (e.name.is_some() && e.rssi.is_some_and(|r| r >= NEARBY_RSSI_DBM))
    }

    /// Drop devices not heard from for `forget_after`. Returns whether anything went.
    pub fn prune(&mut self, now: Instant) -> bool {
        let before = self.entries.len();
        let keep = self.forget_after;
        self.entries
            .retain(|_, e| now.saturating_duration_since(e.last_seen) < keep);
        self.entries.len() != before
    }

    /// The list for the UI: recognised devices first, then by signal strength, then by name.
    pub fn snapshot(&self) -> Vec<DeviceInfo> {
        let mut out: Vec<(bool, DeviceInfo)> = self
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
                let name = e
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Névtelen eszköz ({tail})"));
                Some((
                    !kinds.is_empty(),
                    DeviceInfo {
                        id: id.clone(),
                        name,
                        kinds,
                        rssi: e.rssi,
                    },
                ))
            })
            .collect();
        out.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| {
                    b.1.rssi
                        .unwrap_or(i16::MIN)
                        .cmp(&a.1.rssi.unwrap_or(i16::MIN))
                })
                .then_with(|| a.1.name.cmp(&b.1.name))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        out.into_iter().map(|(_, d)| d).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The scan task: runs until `stop` fires, publishing `Internal::Discovered`.
pub(crate) async fn run_scan<A: BleAdapter>(
    adapter: Arc<A>,
    tx: mpsc::UnboundedSender<Internal>,
    cfg: Arc<HubConfig>,
    mut stop: watch::Receiver<bool>,
) {
    let notice = |text: String| {
        let _ = tx.send(Internal::Notice(text));
    };
    let mut events = match adapter.events().await {
        Ok(e) => e,
        Err(e) => {
            notice(format!("A keresés nem indítható: {e}"));
            return;
        }
    };
    if let Err(e) = adapter.start_scan().await {
        notice(format!(
            "A keresés nem indítható: {e}. Ellenőrizd, hogy be van-e kapcsolva a Bluetooth."
        ));
        return;
    }

    let mut table = DiscoveryTable::new(cfg.scan_forget_after);
    let mut dirty = true;
    // devices the OS already knows about show up immediately
    if let Ok(known) = adapter.peripherals().await {
        for p in known {
            if let Ok(Some(adv)) = adapter.advertisement(&p).await {
                dirty |= table.update(p.id(), &adv, Instant::now());
            }
        }
    }

    let mut last_event = Instant::now();
    let mut ticker = tokio::time::interval(cfg.scan_emit_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => break,
            ev = events.next() => match ev {
                Some(AdapterEvent::Seen(p)) => {
                    last_event = Instant::now();
                    if let Ok(Some(adv)) = adapter.advertisement(&p).await {
                        dirty |= table.update(p.id(), &adv, last_event);
                    }
                }
                Some(AdapterEvent::Lost(_)) => {}
                None => match adapter.events().await {
                    Ok(e) => events = e,
                    Err(e) => {
                        notice(format!("A keresés megszakadt: {e}"));
                        break;
                    }
                },
            },
            _ = ticker.tick() => {
                let now = Instant::now();
                dirty |= table.prune(now);
                if dirty {
                    dirty = false;
                    if tx.send(Internal::Discovered(table.snapshot())).is_err() {
                        break;
                    }
                }
                if now.saturating_duration_since(last_event) > cfg.scan_silence_restart {
                    // a scan that heard nothing for a long time is probably stuck: restart it
                    let _ = adapter.stop_scan().await;
                    if let Err(e) = adapter.start_scan().await {
                        notice(format!("A keresés újraindítása nem sikerült: {e}"));
                        break;
                    }
                    last_event = now;
                }
            }
        }
    }
    let _ = adapter.stop_scan().await;
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
}
