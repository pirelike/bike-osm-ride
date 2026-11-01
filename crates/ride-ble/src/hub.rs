//! The hub: one task that owns the command loop, the merger and the simulated rider, and
//! supervises one task per connected device (plus the scan task).
//!
//! ```text
//!  SensorCommand ──► Hub ──► device tasks (one per physical device, any role mix), scan task
//!                     ▲  ◄── Internal messages (state, roles, data, notices, scan results)
//!                     └──► SensorEvent (State, Discovered, Notice, Sample ≤ 10 Hz)
//! ```
//!
//! A physical device is connected **once** and may fill several slots at the same time: a
//! trainer that also exposes Cycling Power, CSC and a heart-rate bridge is claimed as
//! Trainer + PowerMeter + Cadence + HeartRate, all on one GATT link. `slots` is keyed by the
//! device id; `role_owner` maps each of the four roles to the device serving it. Requesting
//! an already-claimed role replaces the previous owner.
//!
//! The simulated rider (`sim:rider`) is a normal device: it can fill any slot, is always
//! offered in `Discovered`, and works even when no Bluetooth adapter exists.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::FutureExt;
use ride_model::{ConnState, DeviceInfo, DeviceKind, SensorCommand, SensorEvent, TrainerSim};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::error::BleError;
use crate::hexlog::HexLog;
use crate::link::{supervise, Ctx, HubConfig, Internal, Rescan};
use crate::merge::{EmitGate, Field, Merger, Source};
use crate::multi::MultiSession;
use crate::scan::{classify_with, run_scan};
use crate::sim::SimRider;
use crate::trainer::TrainerCmd;
use crate::transport::{BleAdapter, GattPeripheral};
use crate::zwift::ControllerKind;

/// Id of the simulated rider in `Discovered` / `Connect`.
pub const SIM_ID: &str = "sim:rider";
/// Display name of the simulated rider.
pub const SIM_NAME: &str = "Szimulált tekerő";

/// Canonical slot order (the UI shows the cards in this order).
const SLOT_ORDER: [DeviceKind; 5] = [
    DeviceKind::Trainer,
    DeviceKind::HeartRate,
    DeviceKind::PowerMeter,
    DeviceKind::Cadence,
    DeviceKind::Shifter,
];

fn sim_device() -> DeviceInfo {
    DeviceInfo {
        id: SIM_ID.to_string(),
        name: SIM_NAME.to_string(),
        kinds: vec![
            DeviceKind::Trainer,
            DeviceKind::PowerMeter,
            DeviceKind::Cadence,
            DeviceKind::HeartRate,
        ],
        rssi: None,
    }
}

fn source_of(kind: DeviceKind) -> Source {
    match kind {
        DeviceKind::Trainer => Source::Trainer,
        DeviceKind::HeartRate => Source::HrStrap,
        DeviceKind::PowerMeter => Source::PowerMeter,
        DeviceKind::Cadence => Source::Csc,
        DeviceKind::Shifter => Source::Shifter,
    }
}

/// The role a merger source belongs to (a device may only feed sources it owns).
fn source_role(source: Source) -> DeviceKind {
    match source {
        Source::Trainer => DeviceKind::Trainer,
        Source::HrStrap => DeviceKind::HeartRate,
        Source::PowerMeter => DeviceKind::PowerMeter,
        Source::Csc => DeviceKind::Cadence,
        Source::Shifter => DeviceKind::Shifter,
    }
}

fn sort_roles(roles: &mut Vec<DeviceKind>) {
    roles.sort_by_key(|k| SLOT_ORDER.iter().position(|s| s == k).unwrap_or(9));
    roles.dedup();
}

/// Creates the platform adapter on demand (and again after a failure, e.g. once the user
/// switched Bluetooth on).
pub(crate) type InitFn<A> = Box<dyn Fn() -> BoxFuture<'static, Result<A, BleError>> + Send + Sync>;

/// What the app last asked of the trainer, replayed to a trainer that connects later.
#[derive(Debug, Default, Clone, Copy)]
struct TrainerIntent {
    target: Option<TrainerTarget>,
    /// `Some(true)` after `TrainerStart`, `Some(false)` after `TrainerPause`.
    running: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
enum TrainerTarget {
    Sim(TrainerSim),
    Power(f64),
}

struct Slot {
    generation: u64,
    /// Roles this device serves (the wanted set until discovery narrows it).
    kinds: Vec<DeviceKind>,
    stop: watch::Sender<bool>,
    handle: JoinHandle<()>,
    trainer_tx: Option<UnboundedSender<TrainerCmd>>,
    /// Stop was requested; the task is shutting down.
    stopping: bool,
}

struct ScanHandle {
    stop: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

struct Hub<A: BleAdapter> {
    init: InitFn<A>,
    adapter: Option<Arc<A>>,
    cfg: Arc<HubConfig>,
    log: HexLog,
    evt_tx: UnboundedSender<SensorEvent>,
    int_tx: UnboundedSender<Internal>,
    merger: Merger,
    gate: EmitGate,
    sim: SimRider,
    sim_slots: Vec<DeviceKind>,
    last_tick: Instant,
    /// One connection per physical device, keyed by device id.
    slots: HashMap<String, Slot>,
    /// Which device serves each role.
    role_owner: HashMap<DeviceKind, String>,
    /// Newest generation ever started per device (older State messages are stale).
    latest_generation: HashMap<String, u64>,
    generation: u64,
    scan: Option<ScanHandle>,
    scan_results: Vec<DeviceInfo>,
    intent: TrainerIntent,
}

/// Run the hub until the command channel closes.
pub(crate) async fn run_hub<A: BleAdapter>(
    init: InitFn<A>,
    cfg: HubConfig,
    log: HexLog,
    seed: u64,
    mut cmd_rx: UnboundedReceiver<SensorCommand>,
    evt_tx: UnboundedSender<SensorEvent>,
) {
    let (int_tx, mut int_rx) = mpsc::unbounded_channel();
    let cfg = Arc::new(cfg);
    let mut hub = Hub {
        init,
        adapter: None,
        merger: Merger::new(cfg.stale_after),
        gate: EmitGate::new(cfg.emit_min_interval),
        cfg: cfg.clone(),
        log,
        evt_tx,
        int_tx,
        sim: SimRider::new(seed),
        sim_slots: Vec::new(),
        last_tick: Instant::now(),
        slots: HashMap::new(),
        role_owner: HashMap::new(),
        latest_generation: HashMap::new(),
        generation: 0,
        scan: None,
        scan_results: Vec::new(),
        intent: TrainerIntent::default(),
    };
    hub.emit_discovered();

    let mut tick = tokio::time::interval(cfg.tick);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(c) => hub.handle_command(c).await,
                None => break,
            },
            Some(msg) = int_rx.recv() => hub.handle_internal(msg),
            _ = tick.tick() => hub.on_tick(),
        }
    }
    hub.shutdown().await;
}

impl<A: BleAdapter> Hub<A> {
    fn emit(&self, ev: SensorEvent) {
        let _ = self.evt_tx.send(ev);
    }

    fn notice(&self, text: impl Into<String>) {
        self.emit(SensorEvent::Notice(text.into()));
    }

    fn emit_state(&self, kind: DeviceKind, state: ConnState, name: Option<String>) {
        self.emit(SensorEvent::State { kind, state, name });
    }

    fn emit_discovered(&self) {
        let mut list = Vec::with_capacity(self.scan_results.len() + 1);
        list.push(sim_device());
        list.extend(self.scan_results.iter().filter(|d| d.id != SIM_ID).cloned());
        self.emit(SensorEvent::Discovered(list));
    }

    fn emit_sample_now(&mut self) {
        let sample = self.merger.sample(Instant::now());
        if let Some(s) = self.gate.offer(sample, Instant::now()) {
            self.emit(SensorEvent::Sample(s));
        }
    }

    /// Clears every value a slot's roles contributed.
    fn clear_slot_sources(&mut self, kinds: &[DeviceKind]) {
        for kind in kinds {
            self.merger.clear_source(source_of(*kind));
        }
    }

    // ───────────────────────────── commands ─────────────────────────────

    async fn handle_command(&mut self, cmd: SensorCommand) {
        match cmd {
            SensorCommand::StartScan => self.start_scan().await,
            SensorCommand::StopScan => self.stop_scan().await,
            SensorCommand::Connect { id, kind } => {
                if id == SIM_ID {
                    self.connect_sim(kind).await;
                } else {
                    self.connect_ble(id, kind).await;
                }
            }
            SensorCommand::Disconnect(kind) => self.disconnect(kind).await,
            SensorCommand::SetSimulation(sim) => {
                self.intent.target = Some(TrainerTarget::Sim(sim));
                self.sim.set_terrain(sim);
                self.to_trainer(TrainerCmd::Sim(sim));
            }
            SensorCommand::SetTargetPower(w) => {
                if w.is_finite() {
                    self.intent.target = Some(TrainerTarget::Power(w));
                    // in ERG mode the simulated rider simply holds that power
                    self.sim.set_power(w);
                    self.to_trainer(TrainerCmd::Power(w));
                }
            }
            SensorCommand::TrainerStart => {
                self.intent.running = Some(true);
                self.sim.set_active(true);
                self.to_trainer(TrainerCmd::Start);
            }
            SensorCommand::TrainerPause => {
                self.intent.running = Some(false);
                self.sim.set_active(false);
                self.to_trainer(TrainerCmd::Pause);
            }
            SensorCommand::SetVirtualPower(w) => self.sim.set_power(w),
        }
    }

    fn to_trainer(&self, cmd: TrainerCmd) {
        if let Some(tx) = self
            .slots
            .values()
            .find(|s| s.kinds.contains(&DeviceKind::Trainer) && s.trainer_tx.is_some())
            .and_then(|s| s.trainer_tx.as_ref())
        {
            let _ = tx.send(cmd);
        }
    }

    /// The adapter, creating it on first use. A failure is reported once per attempt and
    /// leaves the simulated rider fully usable.
    async fn ensure_adapter(&mut self) -> Option<Arc<A>> {
        if let Some(a) = &self.adapter {
            return Some(a.clone());
        }
        match (self.init)().await {
            Ok(a) => {
                let a = Arc::new(a);
                self.adapter = Some(a.clone());
                Some(a)
            }
            Err(e) => {
                self.notice(format!(
                    "{e}. A szimulált tekerő Bluetooth nélkül is használható."
                ));
                None
            }
        }
    }

    async fn start_scan(&mut self) {
        self.emit_discovered();
        if self.scan.is_some() {
            return;
        }
        let Some(adapter) = self.ensure_adapter().await else {
            return;
        };
        let (stop_tx, stop_rx) = watch::channel(false);
        let handle = tokio::spawn(run_scan(
            adapter,
            self.int_tx.clone(),
            self.cfg.clone(),
            stop_rx,
        ));
        self.scan = Some(ScanHandle {
            stop: stop_tx,
            handle,
        });
    }

    async fn stop_scan(&mut self) {
        if let Some(scan) = self.scan.take() {
            let _ = scan.stop.send(true);
            let limit = self.cfg.io_timeout * 2;
            if tokio::time::timeout(limit, scan.handle).await.is_err() {
                // the task is dropped by the timeout future; nothing else to do
            }
        }
    }

    async fn connect_sim(&mut self, kind: DeviceKind) {
        if let Some(owner) = self.role_owner.get(&kind).cloned() {
            let handle = self.stop_device(&owner);
            if let Some(h) = handle {
                self.wait_handle(h).await;
            }
        }
        self.sim_slots.retain(|k| *k != kind);
        self.sim_slots.push(kind);
        self.emit_state(kind, ConnState::Connected, Some(SIM_NAME.to_string()));
        // show the first values immediately
        self.on_tick();
    }

    /// Connects `id` once, claiming `kind` plus every other free role the device advertises.
    async fn connect_ble(&mut self, id: String, kind: DeviceKind) {
        let name = self
            .scan_results
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| id.clone());

        // Roles seen in the scan (a scan may not have run: the advertisement below helps).
        let mut advertised: Vec<DeviceKind> = self
            .scan_results
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.kinds.clone())
            .unwrap_or_default();
        advertised.push(kind);

        let Some(adapter) = self.ensure_adapter().await else {
            let msg = "A Bluetooth nem érhető el".to_string();
            for k in &advertised {
                self.emit_state(*k, ConnState::Failed(msg.clone()), Some(name.clone()));
            }
            return;
        };
        // stop scanning before connecting (Windows connects far more reliably that way)
        self.stop_scan().await;

        let peripheral = match adapter.peripherals().await {
            Ok(list) => list.into_iter().find(|p| p.id() == id),
            Err(e) => {
                for k in &advertised {
                    self.emit_state(*k, ConnState::Failed(e.to_string()), Some(name.clone()));
                }
                return;
            }
        };
        let Some(peripheral) = peripheral else {
            let msg = BleError::NotFound.to_string();
            for k in &advertised {
                self.emit_state(*k, ConnState::Failed(msg.clone()), Some(name.clone()));
            }
            return;
        };
        // The advertisement tells us the rest without a full scan (names included), and which
        // Zwift controller side it is (a Play report only means something for a known side).
        let mut controller_side = None;
        if let Ok(Some(adv)) = adapter.advertisement(&peripheral).await {
            for k in classify_with(adv.name.as_deref(), &adv.services, adv.zwift_controller) {
                if !advertised.contains(&k) {
                    advertised.push(k);
                }
            }
            controller_side = adv
                .zwift_controller
                .and_then(ControllerKind::from_advertised);
        }

        // Roles of the previous connection of this device are kept (a reconnect), plus the
        // requested role and every advertised role nobody else serves.
        let mut wanted: Vec<DeviceKind> = self
            .slots
            .get(&id)
            .map(|s| s.kinds.clone())
            .unwrap_or_default();
        if !wanted.contains(&kind) {
            wanted.push(kind);
        }
        for role in advertised {
            if !wanted.contains(&role) && !self.role_owner.contains_key(&role) {
                wanted.push(role);
            }
        }
        sort_roles(&mut wanted);

        // Free the requested roles from the simulated rider and from other devices: only the
        // role moves, the other device keeps its remaining roles and its link.
        for role in &wanted {
            if self.sim_slots.contains(role)
                || self.role_owner.get(role).is_some_and(|owner| owner != &id)
            {
                self.take_role(*role);
            }
        }
        // Restart our own device first, so only one connection exists at a time.
        if let Some(handle) = self.stop_device(&id) {
            self.wait_handle(handle).await;
        }
        self.sim_slots.retain(|k| *k != kind);

        self.generation += 1;
        let generation = self.generation;
        self.latest_generation.insert(id.clone(), generation);
        let rescan = self.make_rescan(&adapter);
        let ctx = Ctx::new(
            kind,
            id.clone(),
            generation,
            name.clone(),
            wanted.clone(),
            self.int_tx.clone(),
            self.cfg.clone(),
            self.log.clone(),
            rescan,
        );
        let (stop_tx, stop_rx) = watch::channel(false);

        // One command channel per device: the connection may turn out to be a trainer even if
        // the advertisement was silent about FTMS, so control is always possible.
        let (trainer_tx, ctl_rx) = {
            let (tx, rx) = mpsc::unbounded_channel();
            // replay what the app already asked for, so a late-connecting trainer follows the ride
            match self.intent.target {
                Some(TrainerTarget::Sim(s)) => {
                    let _ = tx.send(TrainerCmd::Sim(s));
                }
                Some(TrainerTarget::Power(w)) => {
                    let _ = tx.send(TrainerCmd::Power(w));
                }
                None => {}
            }
            match self.intent.running {
                Some(true) => {
                    let _ = tx.send(TrainerCmd::Start);
                }
                Some(false) => {
                    let _ = tx.send(TrainerCmd::Pause);
                }
                None => {}
            }
            (Some(tx), Some(rx))
        };
        let session = MultiSession::new(ctl_rx, self.cfg.control, controller_side);
        let handle = tokio::spawn(supervise(peripheral, session, ctx, stop_rx));

        for role in &wanted {
            self.role_owner.insert(*role, id.clone());
        }
        self.slots.insert(
            id,
            Slot {
                generation,
                kinds: wanted.clone(),
                stop: stop_tx,
                handle,
                trainer_tx,
                stopping: false,
            },
        );
        for role in &wanted {
            self.emit_state(*role, ConnState::Connecting, Some(name.clone()));
        }
    }

    /// Moves one role to another device: the previous owner (a BLE device or the simulated
    /// rider) keeps its link and its other roles, and only the role's values are dropped.
    fn take_role(&mut self, role: DeviceKind) {
        if self.sim_slots.contains(&role) {
            self.sim_slots.retain(|k| *k != role);
            self.merger.clear_source(source_of(role));
            self.emit_state(role, ConnState::Disconnected, None);
            self.emit_sample_now();
        }
        let Some(old) = self.role_owner.get(&role).cloned() else {
            return;
        };
        let now_empty = self
            .slots
            .get_mut(&old)
            .map(|slot| {
                slot.kinds.retain(|k| *k != role);
                slot.kinds.is_empty()
            })
            .unwrap_or(true);
        self.merger.clear_source(source_of(role));
        self.role_owner.remove(&role);
        self.emit_state(role, ConnState::Disconnected, None);
        self.emit_sample_now();
        if now_empty {
            // No roles left: the task is stopping; the hub does not need its handle.
            let _ = self.stop_device(&old);
        }
    }

    fn make_rescan(&self, adapter: &Arc<A>) -> Option<Rescan> {
        let adapter = adapter.clone();
        let burst = self.cfg.rescan_burst;
        Some(Arc::new(move || {
            let adapter = adapter.clone();
            async move {
                if adapter.start_scan().await.is_ok() {
                    tokio::time::sleep(burst).await;
                    let _ = adapter.stop_scan().await;
                }
            }
            .boxed()
        }))
    }

    /// Stops one device (all of its roles) and returns its task handle for an optional wait.
    fn stop_device(&mut self, id: &str) -> Option<JoinHandle<()>> {
        let Some(mut slot) = self.slots.remove(id) else {
            // No live slot, but clean up any stale role claims anyway.
            self.role_owner.retain(|_, owner| owner != id);
            return None;
        };
        if !slot.stopping {
            slot.stopping = true;
            let _ = slot.stop.send(true);
        }
        for kind in &slot.kinds {
            if self.role_owner.get(kind).is_some_and(|owner| owner == id) {
                self.role_owner.remove(kind);
            }
        }
        // Values disappear at once; the session's own Disconnected messages are ignored
        // because the slot is gone.
        self.clear_slot_sources(&slot.kinds);
        for kind in &slot.kinds {
            self.emit_state(*kind, ConnState::Disconnected, None);
        }
        self.emit_sample_now();
        Some(slot.handle)
    }

    async fn wait_handle(&self, handle: JoinHandle<()>) {
        let limit = self.cfg.shutdown_flush + self.cfg.io_timeout * 3;
        let _ = tokio::time::timeout(limit, handle).await;
    }

    /// Ask the device serving `kind` (or the simulated rider) to stop.
    async fn disconnect(&mut self, kind: DeviceKind) {
        if self.sim_slots.contains(&kind) {
            self.sim_slots.retain(|k| *k != kind);
            self.merger.clear_source(source_of(kind));
            self.emit_state(kind, ConnState::Disconnected, None);
            self.emit_sample_now();
        }
        let owner = self.role_owner.get(&kind).cloned().or_else(|| {
            self.slots
                .iter()
                .find(|(_, s)| s.kinds.contains(&kind))
                .map(|(id, _)| id.clone())
        });
        if let Some(owner) = owner {
            let _ = self.stop_device(&owner);
        }
    }

    // ───────────────────────────── internal messages ─────────────────────────────

    fn is_current(&self, device: &str, generation: u64) -> bool {
        self.latest_generation
            .get(device)
            .is_none_or(|g| generation >= *g)
    }

    fn handle_internal(&mut self, msg: Internal) {
        match msg {
            Internal::State {
                device,
                kind,
                generation,
                state,
                name,
            } => {
                let live = self
                    .slots
                    .get(&device)
                    .is_some_and(|s| s.generation == generation && !s.stopping);
                if !live || !self.is_current(&device, generation) {
                    return;
                }
                if !self
                    .slots
                    .get(&device)
                    .is_some_and(|s| s.kinds.contains(&kind))
                {
                    return; // the role was dropped after discovery
                }
                if !matches!(state, ConnState::Connected) {
                    // no live link, no values
                    self.merger.clear_source(source_of(kind));
                }
                self.emit_state(kind, state, name);
                self.emit_sample_now();
            }
            Internal::Roles {
                device,
                generation,
                roles,
                missing,
            } => {
                let Some(previous) = self
                    .slots
                    .get(&device)
                    .filter(|s| s.generation == generation)
                    .map(|s| s.kinds.clone())
                else {
                    return;
                };
                // A role another device took over meanwhile stays with that device.
                let mut accepted = Vec::with_capacity(roles.len());
                for role in roles {
                    if self
                        .role_owner
                        .get(&role)
                        .is_some_and(|owner| owner != &device)
                    {
                        continue;
                    }
                    accepted.push(role);
                }
                sort_roles(&mut accepted);
                let dropped: Vec<DeviceKind> = previous
                    .into_iter()
                    .filter(|k| !accepted.contains(k))
                    .collect();
                for kind in &dropped {
                    if self.role_owner.get(kind).is_some_and(|o| o == &device) {
                        self.role_owner.remove(kind);
                    }
                }
                if let Some(slot) = self.slots.get_mut(&device) {
                    slot.kinds = accepted.clone();
                }
                for role in &accepted {
                    self.role_owner.insert(*role, device.clone());
                }
                for (kind, reason) in missing {
                    if self
                        .role_owner
                        .get(&kind)
                        .is_none_or(|owner| owner == &device)
                    {
                        self.emit_state(kind, ConnState::Failed(reason), None);
                    }
                }
            }
            Internal::Notice(text) => self.notice(text),
            Internal::Shift {
                device,
                generation,
                delta,
            } => {
                // a shift counts only from the live connection that owns the Shifter role
                let live = self.slots.get(&device).is_some_and(|s| {
                    s.generation == generation
                        && !s.stopping
                        && s.kinds.contains(&DeviceKind::Shifter)
                });
                let owns = self
                    .role_owner
                    .get(&DeviceKind::Shifter)
                    .is_some_and(|owner| owner == &device);
                if live && owns && self.is_current(&device, generation) {
                    self.emit(SensorEvent::Shift(delta));
                }
            }
            Internal::Data {
                device,
                generation,
                source,
                values,
            } => {
                let live = self
                    .slots
                    .get(&device)
                    .is_some_and(|s| s.generation == generation && !s.stopping);
                let owns_source = self
                    .role_owner
                    .get(&source_role(source))
                    .is_some_and(|owner| owner == &device);
                if live && owns_source {
                    let now = Instant::now();
                    for (field, value) in values {
                        self.merger.set(source, field, value, now);
                    }
                }
            }
            Internal::Finished { device, generation } => {
                if self
                    .slots
                    .get(&device)
                    .is_some_and(|s| s.generation == generation)
                {
                    if let Some(slot) = self.slots.remove(&device) {
                        for kind in &slot.kinds {
                            if self.role_owner.get(kind).is_some_and(|o| o == &device) {
                                self.role_owner.remove(kind);
                            }
                        }
                        self.clear_slot_sources(&slot.kinds);
                        for kind in &slot.kinds {
                            self.emit_state(*kind, ConnState::Disconnected, None);
                        }
                        self.emit_sample_now();
                    }
                }
            }
            Internal::Discovered(list) => {
                self.scan_results = list;
                self.emit_discovered();
            }
        }
    }

    // ───────────────────────────── tick ─────────────────────────────

    fn on_tick(&mut self) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let out = self.sim.step(dt);
        let sim_slots = self.sim_slots.clone();
        for kind in sim_slots {
            let src = source_of(kind);
            match kind {
                DeviceKind::Trainer => {
                    self.merger.set(src, Field::Power, out.power_w, now);
                    self.merger.set(src, Field::Cadence, out.cadence_rpm, now);
                    self.merger.set(src, Field::HeartRate, out.hr_bpm, now);
                    self.merger.set(src, Field::Speed, out.speed_ms, now);
                }
                DeviceKind::PowerMeter => {
                    self.merger.set(src, Field::Power, out.power_w, now);
                    self.merger.set(src, Field::Cadence, out.cadence_rpm, now);
                }
                DeviceKind::Cadence => self.merger.set(src, Field::Cadence, out.cadence_rpm, now),
                DeviceKind::HeartRate => self.merger.set(src, Field::HeartRate, out.hr_bpm, now),
                DeviceKind::Shifter => {}
            }
        }
        let sample = self.merger.sample(now);
        if let Some(s) = self.gate.offer(sample, now) {
            self.emit(SensorEvent::Sample(s));
        }
    }

    // ───────────────────────────── shutdown ─────────────────────────────

    /// Stop everything and give the trainer a chance to return to a neutral state.
    async fn shutdown(&mut self) {
        self.stop_scan().await;
        let ids: Vec<String> = self.slots.keys().cloned().collect();
        let mut handles = Vec::new();
        for id in ids {
            if let Some(h) = self.stop_device(&id) {
                handles.push(h);
            }
        }
        let limit = self.cfg.shutdown_flush + self.cfg.io_timeout * 3;
        let deadline = tokio::time::Instant::now() + limit;
        for h in handles {
            let _ = tokio::time::timeout_at(deadline, h).await;
        }
        let _ = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    // Hub behaviour is covered by the fake-transport tests in `hub_tests.rs`.
}
