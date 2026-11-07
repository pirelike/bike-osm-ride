//! Plumbing shared by every connected device: timing configuration, the messages device
//! tasks send to the hub, and the supervisor that implements connect / reconnect / give-up.
//!
//! Reconnect policy (docs/ARCHITECTURE.md): 3 tries with backoff and a `Notice` per try;
//! a link that stayed up for [`HubConfig::stable_after`] resets the budget. The re-scan
//! before retrying a vanished device follows iron-heart's lesson (MIT, see NOTICE.md):
//! on Windows a device that dropped off needs the scanner to see it again.
//!
//! One physical device may serve **several roles** (a trainer that also exposes cycling
//! power, cadence and a heart-rate bridge). [`Ctx`] therefore carries the device id and the
//! set of roles it currently serves; every state message names one role, so the hub can
//! update the four slots independently.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use ride_model::{ConnState, DeviceInfo, DeviceKind};
use tokio::sync::{mpsc, watch};

use crate::control::ControlConfig;
use crate::error::{bounded, BleError};
use crate::hexlog::HexLog;
use crate::merge::{Field, Source};
use crate::transport::GattPeripheral;
use ride_i18n::{t, tf};

/// Hungarian slot names (match the UI's device cards).
pub fn kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Trainer => "Trainer",
        DeviceKind::HeartRate => t("ble.link.pulzusmero"),
        DeviceKind::PowerMeter => t("ble.link.teljesitmenymero"),
        DeviceKind::Cadence => t("ble.link.kadenciaszenzor"),
        DeviceKind::Shifter => t("ble.link.valto"),
    }
}

/// All timing knobs in one place so tests can run the real code paths in milliseconds.
#[derive(Debug, Clone)]
pub struct HubConfig {
    /// Hub tick: simulated rider step, staleness expiry, sample emission.
    pub tick: Duration,
    /// Values not refreshed for this long are dropped from the merged sample.
    pub stale_after: Duration,
    /// Minimum spacing of `Sample` events (rate limit, ~10 Hz).
    pub emit_min_interval: Duration,
    pub connect_timeout: Duration,
    /// Discovery, subscribe, unsubscribe, disconnect, control writes.
    pub io_timeout: Duration,
    /// Optional capability reads (Feature, ranges).
    pub read_timeout: Duration,
    /// Poll `is_connected` this often to notice silent disconnects.
    pub health_interval: Duration,
    /// Trainer: no notification at all for this long = link lost.
    pub trainer_watchdog: Duration,
    /// HR / power / CSC sensors: same, but straps may legitimately fall silent.
    pub sensor_watchdog: Duration,
    /// Coasting check of cadence trackers.
    pub idle_tick: Duration,
    /// Delay before reconnect try 1, 2, 3 …; the length is the number of tries.
    pub reconnect_backoff: Vec<Duration>,
    /// A connection that lasted this long resets the reconnect budget.
    pub stable_after: Duration,
    /// How long a graceful stop may spend handing the trainer back (neutral grade, stop).
    pub shutdown_flush: Duration,
    /// How long the short re-scan before a reconnect try runs.
    pub rescan_burst: Duration,
    /// A running scan that saw no event for this long is restarted (WinRT quirk).
    pub scan_silence_restart: Duration,
    /// `Discovered` events are sent at most this often.
    pub scan_emit_interval: Duration,
    /// A device not seen for this long drops off the list.
    pub scan_forget_after: Duration,
    pub control: ControlConfig,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            tick: Duration::from_millis(100),
            stale_after: crate::merge::STALE_AFTER,
            emit_min_interval: Duration::from_millis(90),
            // Android's own direct connect gives up after about 30 s (and btleplug retries a failed
            // attempt up to twice), and discovering the services of a trainer takes seconds there:
            // the desktop limits cut a link that was about to come up.
            connect_timeout: Duration::from_secs(if cfg!(target_os = "android") { 30 } else { 12 }),
            io_timeout: Duration::from_secs(if cfg!(target_os = "android") { 12 } else { 6 }),
            read_timeout: Duration::from_secs(3),
            health_interval: Duration::from_secs(2),
            trainer_watchdog: Duration::from_secs(10),
            sensor_watchdog: Duration::from_secs(30),
            idle_tick: Duration::from_secs(1),
            reconnect_backoff: vec![
                Duration::from_millis(1500),
                Duration::from_secs(4),
                Duration::from_secs(10),
            ],
            stable_after: Duration::from_secs(30),
            shutdown_flush: Duration::from_secs(2),
            rescan_burst: Duration::from_secs(2),
            scan_silence_restart: Duration::from_secs(30),
            scan_emit_interval: Duration::from_millis(500),
            scan_forget_after: Duration::from_secs(60),
            control: ControlConfig::default(),
        }
    }
}

#[cfg(test)]
impl HubConfig {
    /// Everything shrunk to milliseconds for the fake-transport tests.
    pub(crate) fn fast() -> Self {
        let ms = Duration::from_millis;
        Self {
            tick: ms(10),
            stale_after: ms(300),
            emit_min_interval: ms(8),
            connect_timeout: ms(500),
            io_timeout: ms(300),
            read_timeout: ms(100),
            health_interval: ms(20),
            trainer_watchdog: ms(400),
            sensor_watchdog: ms(400),
            idle_tick: ms(20),
            reconnect_backoff: vec![ms(10), ms(20), ms(30)],
            stable_after: ms(2000),
            shutdown_flush: ms(300),
            rescan_burst: ms(5),
            scan_silence_restart: ms(100),
            scan_emit_interval: ms(10),
            scan_forget_after: ms(500),
            control: ControlConfig {
                response_timeout: ms(150),
                min_write_gap: ms(5),
                keepalive: ms(80),
                control_retries: 3,
                retry_delay: ms(10),
                reclaim_limit: 3,
                notice_cooldown: ms(200),
                // The status-storm rules run at the same scale as the rest of the fast config.
                status_storm_window: ms(400),
                grant_grace: ms(100),
                restart_interval: ms(150),
            },
        }
    }
}

/// Messages from device tasks and the scanner to the hub loop.
#[derive(Debug)]
pub(crate) enum Internal {
    State {
        /// Physical device (BLE id) the state belongs to.
        device: String,
        kind: DeviceKind,
        generation: u64,
        state: ConnState,
        name: Option<String>,
    },
    /// The roles a freshly connected device actually serves (after service discovery), plus
    /// the requested roles whose service is missing (with a Hungarian explanation).
    Roles {
        device: String,
        generation: u64,
        roles: Vec<DeviceKind>,
        missing: Vec<(DeviceKind, String)>,
    },
    Notice(String),
    /// One virtual gear step from the shifting controller of `device`.
    Shift {
        device: String,
        generation: u64,
        delta: i32,
    },
    Data {
        device: String,
        generation: u64,
        source: Source,
        values: Vec<(Field, f64)>,
    },
    /// The device task ended (stopped, failed, or gave up).
    Finished {
        device: String,
        generation: u64,
    },
    /// Current scan results (without the simulated rider).
    Discovered(Vec<DeviceInfo>),
}

pub(crate) type Rescan = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// Everything a device task needs to talk to the hub.
#[derive(Clone)]
pub(crate) struct Ctx {
    /// Primary role, used for human-readable labels and error messages.
    pub kind: DeviceKind,
    /// Physical device id (`Connect.id`).
    pub device: String,
    pub generation: u64,
    pub name: String,
    pub tx: mpsc::UnboundedSender<Internal>,
    pub cfg: Arc<HubConfig>,
    pub log: HexLog,
    pub rescan: Option<Rescan>,
    /// Roles the hub wants from this device (the session narrows this after discovery).
    roles: Arc<Mutex<Vec<DeviceKind>>>,
    connected_once: Arc<AtomicBool>,
}

impl Ctx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: DeviceKind,
        device: String,
        generation: u64,
        name: String,
        roles: Vec<DeviceKind>,
        tx: mpsc::UnboundedSender<Internal>,
        cfg: Arc<HubConfig>,
        log: HexLog,
        rescan: Option<Rescan>,
    ) -> Self {
        Self {
            kind,
            device,
            generation,
            name,
            tx,
            cfg,
            log,
            rescan,
            roles: Arc::new(Mutex::new(roles)),
            connected_once: Arc::new(AtomicBool::new(false)),
        }
    }

    fn role_list(&self) -> MutexGuard<'_, Vec<DeviceKind>> {
        self.roles.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The roles this device serves right now (the wanted set until discovery finishes).
    pub fn roles(&self) -> Vec<DeviceKind> {
        self.role_list().clone()
    }

    pub fn serves(&self, kind: DeviceKind) -> bool {
        self.role_list().contains(&kind)
    }

    /// Records what discovery found and tells the hub (which updates the slot claims).
    pub fn set_roles(&self, roles: &[DeviceKind], missing: Vec<(DeviceKind, String)>) {
        *self.role_list() = roles.to_vec();
        let _ = self.tx.send(Internal::Roles {
            device: self.device.clone(),
            generation: self.generation,
            roles: roles.to_vec(),
            missing,
        });
    }

    pub fn state(&self, kind: DeviceKind, state: ConnState) {
        let _ = self.tx.send(Internal::State {
            device: self.device.clone(),
            kind,
            generation: self.generation,
            state,
            name: Some(self.name.clone()),
        });
    }

    /// Emits `state` for every role of the device.
    pub fn state_all(&self, state: ConnState) {
        for kind in self.roles() {
            self.state(kind, state.clone());
        }
    }

    /// The link is up and subscribed.
    pub fn connected(&self) {
        self.connected_once.store(true, Ordering::Relaxed);
        self.state_all(ConnState::Connected);
    }

    pub fn was_connected(&self) -> bool {
        self.connected_once.load(Ordering::Relaxed)
    }

    pub fn notice(&self, text: impl Into<String>) {
        let _ = self.tx.send(Internal::Notice(text.into()));
    }

    /// One gear step (`+1` harder, `-1` easier) from the shifting controller.
    pub fn shift(&self, delta: i32) {
        let _ = self.tx.send(Internal::Shift {
            device: self.device.clone(),
            generation: self.generation,
            delta,
        });
    }

    pub fn data(&self, source: Source, values: Vec<(Field, f64)>) {
        if !values.is_empty() {
            let _ = self.tx.send(Internal::Data {
                device: self.device.clone(),
                generation: self.generation,
                source,
                values,
            });
        }
    }

    fn finished(&self) {
        let _ = self.tx.send(Internal::Finished {
            device: self.device.clone(),
            generation: self.generation,
        });
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SessionEnd {
    /// The hub asked us to stop; the link was handed back cleanly.
    Stopped,
    /// The link dropped; the supervisor decides whether to retry.
    LinkLost(String),
}

/// One connection attempt's worth of work for a device role.
pub(crate) trait DeviceSession<P: GattPeripheral>: Send + 'static {
    fn run(
        &mut self,
        p: &P,
        ctx: &Ctx,
        stop: &mut watch::Receiver<bool>,
    ) -> impl Future<Output = Result<SessionEnd, BleError>> + Send;
}

/// Resolves once a stop was requested (or the hub dropped its sender).
pub(crate) async fn stopped(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

enum Final {
    Stopped,
    Failed(String),
}

/// Run `session` against `p` until stopped or out of retries, reporting through `ctx`.
pub(crate) async fn supervise<P, S>(p: P, mut session: S, ctx: Ctx, mut stop: watch::Receiver<bool>)
where
    P: GattPeripheral,
    S: DeviceSession<P>,
{
    let label = kind_label(ctx.kind);
    let max = ctx.cfg.reconnect_backoff.len() as u32;
    let mut attempt = 0u32;
    let outcome = loop {
        if *stop.borrow() {
            break Final::Stopped;
        }
        ctx.state_all(ConnState::Connecting);
        let started = Instant::now();
        let result = session.run(&p, &ctx, &mut stop).await;
        // whatever happened, do not leave a half-open link behind
        let _ = bounded(
            t("ble.link.lekapcsolodas"),
            ctx.cfg.io_timeout,
            p.disconnect(),
        )
        .await;
        if *stop.borrow() {
            break Final::Stopped;
        }
        let (why, rescan) = match result {
            Ok(SessionEnd::Stopped) => break Final::Stopped,
            Ok(SessionEnd::LinkLost(why)) => (why, true),
            Err(e) if e.is_permanent() => {
                break Final::Failed(tf(
                    "ble.link.label_nem_sikerult_ujra_csatlakozni_why",
                    &[("label", label), ("why", &e.to_string())],
                ))
            }
            Err(e) => (e.to_string(), e.is_link_lost()),
        };
        ctx.log.comment(&format!("{label}: {why}"));
        if started.elapsed() >= ctx.cfg.stable_after {
            attempt = 0;
        }
        attempt += 1;
        if attempt > max {
            break Final::Failed(tf(
                "ble.link.label_nem_sikerult_ujra_csatlakozni_why",
                &[("label", label), ("why", &why)],
            ));
        }
        let text = if ctx.was_connected() {
            tf(
                "ble.link.label_a_kapcsolat_megszakadt_ujracsatlakozas_attempt",
                &[
                    ("label", label),
                    ("attempt", &attempt.to_string()),
                    ("max", &max.to_string()),
                ],
            )
        } else {
            tf(
                "ble.link.label_a_csatlakozas_nem_sikerult_ujraprobalom_attemp",
                &[
                    ("label", label),
                    ("attempt", &attempt.to_string()),
                    ("max", &max.to_string()),
                ],
            )
        };
        ctx.notice(text);
        if rescan {
            if let Some(r) = &ctx.rescan {
                tokio::select! {
                    _ = r() => {}
                    _ = stopped(&mut stop) => break Final::Stopped,
                }
            }
        }
        let delay = ctx
            .cfg
            .reconnect_backoff
            .get(attempt as usize - 1)
            .copied()
            .unwrap_or(Duration::from_secs(10));
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stopped(&mut stop) => break Final::Stopped,
        }
    };
    match outcome {
        Final::Stopped => ctx.state_all(ConnState::Disconnected),
        Final::Failed(message) => {
            ctx.notice(message.clone());
            ctx.state_all(ConnState::Failed(message));
        }
    }
    ctx.finished();
}
