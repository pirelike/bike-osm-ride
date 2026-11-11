//! FTMS control: the Control Point safety filter, the command encoders and the
//! session state machine (handshake, one outstanding procedure, keep-alive, pause/stop
//! semantics, replay after reconnect).
//!
//! Everything in this module is pure (no I/O, time is passed in), so the whole protocol
//! behaviour is unit-tested without a trainer. `trainer.rs` drives it over a transport.
//!
//! The allow-list / clamp / bounded-queue structure is ported from OsmolControl
//! (`crates/bike-protocol/src/ftms_control.rs`, MIT, see NOTICE.md and
//! `third_party/OsmolControl/LICENSE`); what changed: the filter also covers the
//! simulation parameters (grade tied to the trainer's inclination range and +/-20 %), the
//! queue holds *intents* whose payload is computed at send time (so a slow trainer always
//! receives the latest value and ordering can never put a stale value after a newer one),
//! and the session adds procedure timeouts, keep-alive and reclaim logic.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ftms::{encode_control_request, ControlOptions, ControlRequest};
use ride_model::TrainerSim;

use crate::parsers::{parse_cp_response, CpResult, MachineStatus};
use ride_i18n::t;

/// FTMS Control Point op codes (FTMS v1.0 table 4.15). Only the ones we ever send.
pub mod op {
    pub const REQUEST_CONTROL: u8 = 0x00;
    /// Deliberately never sent: Reset drops the control permission.
    pub const RESET: u8 = 0x01;
    pub const SET_TARGET_POWER: u8 = 0x05;
    pub const START_OR_RESUME: u8 = 0x07;
    pub const STOP_OR_PAUSE: u8 = 0x08;
    pub const SET_SIMULATION: u8 = 0x11;
    pub const RESPONSE: u8 = 0x80;
}

/// Control information parameter of Stop/Pause (table 4.16).
pub const STOP: u8 = 0x01;
pub const PAUSE: u8 = 0x02;

/// No matter what the trainer allows, the simulated grade never leaves this window.
pub const MAX_GRADE_PCT: f64 = 20.0;

// ───────────────────────────── limits & filter ─────────────────────────────

/// What the trainer will accept, from its Supported Inclination / Power Range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Simulated grade window, percent.
    pub grade_pct: (f64, f64),
    /// ERG target window, watts.
    pub power_w: (i16, i16),
}

impl Limits {
    /// Conservative values for a trainer whose ranges could not be read.
    pub const FALLBACK: Limits = Limits {
        grade_pct: (-MAX_GRADE_PCT, MAX_GRADE_PCT),
        power_w: (0, 1500),
    };

    /// Intersect the trainer's inclination range (percent) with +/-20 %; an empty
    /// intersection (nonsensical range) falls back to +/-20 %.
    pub fn from_ranges(inclination_pct: Option<(f64, f64)>, power_w: Option<(i16, i16)>) -> Limits {
        let grade_pct = match inclination_pct {
            Some((lo, hi)) if lo.is_finite() && hi.is_finite() => {
                let (lo, hi) = (lo.max(-MAX_GRADE_PCT), hi.min(MAX_GRADE_PCT));
                if lo <= hi {
                    (lo, hi)
                } else {
                    Self::FALLBACK.grade_pct
                }
            }
            _ => Self::FALLBACK.grade_pct,
        };
        Limits {
            grade_pct,
            power_w: power_w.unwrap_or(Self::FALLBACK.power_w),
        }
    }

    /// Grade window in the wire unit (0.01 %).
    fn grade_raw(&self) -> (i16, i16) {
        let conv = |v: f64| (v * 100.0).round().clamp(-2000.0, 2000.0) as i16;
        (conv(self.grade_pct.0), conv(self.grade_pct.1))
    }
}

/// FTMS result codes used in synthetic rejections.
const RESULT_NOT_SUPPORTED: u8 = 0x02;
const RESULT_INVALID_PARAMETER: u8 = 0x03;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Write these bytes (possibly clamped).
    Forward(Vec<u8>),
    /// Do not write; this is the response the Control Point would have indicated.
    Reject([u8; 3]),
}

fn reject(opcode: u8, result: u8) -> Verdict {
    Verdict::Reject([op::RESPONSE, opcode, result])
}

/// The only gate between the app and the trainer's Control Point. Allow-list: Request
/// Control, Start/Resume, Stop/Pause (1|2), Set Target Power, Set Indoor Bike Simulation.
/// Targets are clamped to the trainer's limits. Anything else — including Reset, which
/// would drop the control permission — never reaches the device.
pub fn filter(command: &[u8], limits: &Limits) -> Verdict {
    let Some((&opcode, params)) = command.split_first() else {
        return reject(0, RESULT_INVALID_PARAMETER);
    };
    match (opcode, params.len()) {
        (op::REQUEST_CONTROL | op::START_OR_RESUME, 0) => Verdict::Forward(vec![opcode]),
        (op::STOP_OR_PAUSE, 1) if matches!(params[0], STOP | PAUSE) => {
            Verdict::Forward(command.to_vec())
        }
        (op::SET_TARGET_POWER, 2) => {
            let (lo, hi) = limits.power_w;
            let w = i16::from_le_bytes([params[0], params[1]]).clamp(lo, hi.max(lo));
            let [a, b] = w.to_le_bytes();
            Verdict::Forward(vec![opcode, a, b])
        }
        (op::SET_SIMULATION, 6) => {
            let (lo, hi) = limits.grade_raw();
            let grade = i16::from_le_bytes([params[2], params[3]]).clamp(lo, hi.max(lo));
            let mut out = command.to_vec();
            out[3..5].copy_from_slice(&grade.to_le_bytes());
            Verdict::Forward(out)
        }
        (op::REQUEST_CONTROL | op::START_OR_RESUME | op::STOP_OR_PAUSE, _)
        | (op::SET_TARGET_POWER | op::SET_SIMULATION, _) => {
            reject(opcode, RESULT_INVALID_PARAMETER)
        }
        _ => reject(opcode, RESULT_NOT_SUPPORTED),
    }
}

// ───────────────────────────── encoders ─────────────────────────────

fn encode(req: ControlRequest) -> Option<Vec<u8>> {
    let mut buf = [0u8; 11];
    let n = encode_control_request(req, ControlOptions::default(), &mut buf).ok()?;
    Some(buf[..n].to_vec())
}

fn plain(opcode: u8) -> Vec<u8> {
    encode(ControlRequest {
        opcode,
        operands: [0; 5],
        operand_count: 0,
    })
    .unwrap_or_else(|| vec![opcode])
}

pub fn encode_request_control() -> Vec<u8> {
    plain(op::REQUEST_CONTROL)
}

pub fn encode_start() -> Vec<u8> {
    plain(op::START_OR_RESUME)
}

/// Stop (`pause == false`) or Pause (`pause == true`).
pub fn encode_stop_pause(pause: bool) -> Vec<u8> {
    let action = if pause { PAUSE } else { STOP };
    encode(ControlRequest {
        opcode: op::STOP_OR_PAUSE,
        operands: [i32::from(action), 0, 0, 0, 0],
        operand_count: 1,
    })
    .unwrap_or_else(|| vec![op::STOP_OR_PAUSE, action])
}

/// Set Target Power; the value is saturated to the `sint16` wire range.
pub fn encode_target_power(watts: f64) -> Vec<u8> {
    let w = if watts.is_finite() {
        watts.round().clamp(-32768.0, 32767.0) as i16
    } else {
        0
    };
    encode(ControlRequest {
        opcode: op::SET_TARGET_POWER,
        operands: [i32::from(w), 0, 0, 0, 0],
        operand_count: 1,
    })
    .unwrap_or_else(|| {
        let [a, b] = w.to_le_bytes();
        vec![op::SET_TARGET_POWER, a, b]
    })
}

/// Indoor bike simulation parameters on the wire grid (FTMS v1.0 table 4.20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireSim {
    /// Wind speed, 0.001 m/s.
    pub wind: i16,
    /// Grade, 0.01 %.
    pub grade: i16,
    /// Coefficient of rolling resistance, 0.0001.
    pub crr: u8,
    /// Wind resistance coefficient, 0.01 kg/m.
    pub cw: u8,
}

fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

/// Quantise a `TrainerSim` to the wire grid. (The `ftms` crate rejects off-grid values with
/// `InvalidRange`, so rounding must happen first.) The grade is clamped to `grade_pct`.
pub fn quantise_sim(sim: &TrainerSim, grade_pct: (f64, f64)) -> WireSim {
    let grade = finite_or(sim.grade_pct, 0.0)
        .clamp(grade_pct.0.min(grade_pct.1), grade_pct.1.max(grade_pct.0));
    WireSim {
        wind: (finite_or(sim.wind_ms, 0.0) * 1000.0)
            .round()
            .clamp(-32768.0, 32767.0) as i16,
        grade: (grade * 100.0).round().clamp(-32768.0, 32767.0) as i16,
        crr: (finite_or(sim.crr, 0.004) * 10_000.0)
            .round()
            .clamp(0.0, 255.0) as u8,
        cw: (finite_or(sim.cw, 0.0) * 100.0).round().clamp(0.0, 255.0) as u8,
    }
}

pub fn encode_simulation(w: WireSim) -> Vec<u8> {
    encode(ControlRequest {
        opcode: op::SET_SIMULATION,
        operands: [
            i32::from(w.wind),
            i32::from(w.grade),
            i32::from(w.crr),
            i32::from(w.cw),
            0,
        ],
        operand_count: 4,
    })
    .unwrap_or_else(|| {
        let mut v = vec![op::SET_SIMULATION];
        v.extend_from_slice(&w.wind.to_le_bytes());
        v.extend_from_slice(&w.grade.to_le_bytes());
        v.push(w.crr);
        v.push(w.cw);
        v
    })
}

// ───────────────────────────── session state machine ─────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ControlConfig {
    /// A procedure without an indication after this long is abandoned.
    pub response_timeout: Duration,
    /// Minimum spacing between Control Point writes (at most 4 per second).
    pub min_write_gap: Duration,
    /// The active target is re-sent at least this often.
    pub keepalive: Duration,
    /// Request Control attempts before giving up (until the next reconnect / `start`).
    pub control_retries: u32,
    pub retry_delay: Duration,
    /// How often a trainer may take the control away again before we stop fighting for it.
    pub reclaim_limit: u32,
    /// The same warning is shown at most this often.
    pub notice_cooldown: Duration,
    /// A Fitness Machine Status op code repeating within this window is the device's steady
    /// state, not a fresh event (see [`Storm`]).
    pub status_storm_window: Duration,
    /// A status this soon after control was granted is firmware chatter, not a user action.
    pub grant_grace: Duration,
    /// A trainer that reports itself stopped while the ride runs is started again at most
    /// this often.
    pub restart_interval: Duration,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(3),
            min_write_gap: Duration::from_millis(250),
            keepalive: Duration::from_secs(5),
            control_retries: 3,
            retry_delay: Duration::from_secs(1),
            reclaim_limit: 3,
            notice_cooldown: Duration::from_secs(30),
            status_storm_window: STATUS_STORM_WINDOW,
            grant_grace: GRANT_GRACE,
            restart_interval: RESTART_INTERVAL,
        }
    }
}

/// A refused Start / grade / target write is retried this many times in a row.
const MAX_WRITE_RETRIES: u32 = 40;

/// A Fitness Machine Status op code that repeats within this window is the device's steady
/// state, not a fresh event: some FTMS firmware (a captured trainer does it) reports "Reset"
/// after every control write and every few hundred milliseconds.
const STATUS_STORM_WINDOW: Duration = Duration::from_secs(5);
/// A status this soon after control was granted is firmware chatter, not a user action.
const GRANT_GRACE: Duration = Duration::from_secs(2);
/// While the ride is running, a trainer that reports itself stopped is started again at
/// most this often (so a repeated status never turns into a write storm).
const RESTART_INTERVAL: Duration = Duration::from_secs(3);

/// Counts repetitions of one Fitness Machine Status op code.
///
/// A trainer that repeats a status every few hundred milliseconds is describing its idle
/// state (its firmware has no other way to say "not running"), not reporting a new event.
/// Reacting to every repetition would drop the control permission in an endless loop — the
/// target writes then never reach the device, which is exactly the "the trainer ignores ERG
/// and the hills" symptom such firmware produces.
#[derive(Debug, Default)]
struct Storm {
    last: Option<Instant>,
    /// The current run of repeats was already written to the BLE log.
    reported: bool,
}

impl Storm {
    /// Records the event; `true` when it repeats the previous one within `window`.
    fn repeats(&mut self, now: Instant, window: Duration) -> bool {
        let repeats = self
            .last
            .is_some_and(|at| now.saturating_duration_since(at) <= window);
        self.last = Some(now);
        if !repeats {
            self.reported = false;
        }
        repeats
    }

    /// `true` once per run of repeats, so a 4 Hz status storm writes one log line, not 240.
    fn report(&mut self) -> bool {
        !std::mem::replace(&mut self.reported, true)
    }

    fn clear(&mut self) {
        self.last = None;
        self.reported = false;
    }
}

/// What the connected trainer told us it can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerCaps {
    pub has_control_point: bool,
    /// `None` = the Feature characteristic could not be read (assume yes).
    pub supports_sim: Option<bool>,
    pub supports_power_target: Option<bool>,
}

impl TrainerCaps {
    pub const UNKNOWN: TrainerCaps = TrainerCaps {
        has_control_point: true,
        supports_sim: None,
        supports_power_target: None,
    };
}

/// Effects the driver must carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// Write to the Control Point (already passed through [`filter`]).
    Write(Vec<u8>),
    /// A Hungarian message for the user.
    Notice(String),
    ControlGranted,
    ControlLost,
    /// A repeated device status the session deliberately ignored: it belongs in the BLE log
    /// (the only evidence when a trainer misbehaves), not in a toast.
    StatusIgnored(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intent {
    RequestControl,
    Start,
    Sim,
    Target,
    Neutral,
    Pause,
    Stop,
}

impl Intent {
    fn opcode(self) -> u8 {
        match self {
            Intent::RequestControl => op::REQUEST_CONTROL,
            Intent::Start => op::START_OR_RESUME,
            Intent::Sim | Intent::Neutral => op::SET_SIMULATION,
            Intent::Target => op::SET_TARGET_POWER,
            Intent::Pause | Intent::Stop => op::STOP_OR_PAUSE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not connected, or the device has no Control Point.
    Detached,
    /// Connected; control must be (re)requested.
    NeedControl,
    /// Request Control is in flight.
    Requesting,
    Controlled,
    /// Gave up for now (see `start()` for a manual retry).
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    None,
    Sim,
    Power,
}

#[derive(Debug, Clone, Copy)]
struct Inflight {
    intent: Intent,
    opcode: u8,
    sent_at: Instant,
}

#[derive(Debug)]
pub struct ControlSession {
    cfg: ControlConfig,
    limits: Limits,
    caps: TrainerCaps,
    phase: Phase,
    queue: VecDeque<Intent>,
    inflight: Option<Inflight>,
    // desired state: survives reconnects so control can be replayed
    sim: Option<TrainerSim>,
    target_power: Option<f64>,
    mode: Mode,
    wants_run: bool,
    user_paused: bool,
    device_started: bool,
    device_blocked: bool,
    // bookkeeping
    last_write: Option<Instant>,
    last_target_sent: Option<Instant>,
    last_target_bytes: Option<Vec<u8>>,
    retry_at: Option<Instant>,
    control_attempts: u32,
    reclaims: u32,
    retried_start_for_sim: bool,
    /// When the control permission was last granted (a status right after it is not an event).
    granted_at: Option<Instant>,
    /// Repeating "Reset" (0x01) statuses.
    resets: Storm,
    /// Repeating "stopped / paused by the user" (0x02) statuses.
    stops: Storm,
    /// Earliest next automatic Start/Resume while the trainer reports itself stopped.
    restart_at: Option<Instant>,
    /// The trainer never answered Request Control with an indication: control is assumed and
    /// writes are not waited for (what other trainer apps do with such trainers).
    silent: bool,
    /// A pause / stop asked for while control was not ours: carried out on the next grant.
    pending_relax: Option<Intent>,
    /// Consecutive transport-level write failures of the current target (ATT "insufficient
    /// resources" from busy trainers); the write is retried while this stays small.
    failed_streak: u32,
    /// Shutting down: never ask for control any more.
    closing: bool,
    /// Last time each (static) warning text was shown, for the cooldown.
    warned: Vec<(&'static str, Instant)>,
}

impl ControlSession {
    pub fn new(cfg: ControlConfig) -> Self {
        Self {
            cfg,
            limits: Limits::FALLBACK,
            caps: TrainerCaps::UNKNOWN,
            phase: Phase::Detached,
            queue: VecDeque::new(),
            inflight: None,
            sim: None,
            target_power: None,
            mode: Mode::None,
            wants_run: false,
            user_paused: false,
            device_started: false,
            device_blocked: false,
            last_write: None,
            last_target_sent: None,
            last_target_bytes: None,
            retry_at: None,
            control_attempts: 0,
            reclaims: 0,
            retried_start_for_sim: false,
            granted_at: None,
            resets: Storm::default(),
            stops: Storm::default(),
            restart_at: None,
            silent: false,
            pending_relax: None,
            failed_streak: 0,
            closing: false,
            warned: Vec::new(),
        }
    }

    // ── connection lifecycle ──

    /// The trainer (re)connected and its capabilities are known. Control is requested on the
    /// next [`poll`](Self::poll); a remembered simulation / start request is replayed once
    /// control is granted.
    pub fn on_connected(&mut self, caps: TrainerCaps, limits: Limits, _now: Instant) -> Vec<Out> {
        self.caps = caps;
        self.limits = limits;
        self.queue.clear();
        self.inflight = None;
        self.device_started = false;
        self.device_blocked = false;
        self.last_write = None;
        self.last_target_sent = None;
        self.last_target_bytes = None;
        self.retry_at = None;
        self.control_attempts = 0;
        self.reclaims = 0;
        self.retried_start_for_sim = false;
        self.granted_at = None;
        self.resets.clear();
        self.stops.clear();
        self.restart_at = None;
        self.silent = false;
        self.failed_streak = 0;
        self.closing = false;
        let mut outs = Vec::new();
        if caps.has_control_point {
            self.phase = Phase::NeedControl;
        } else {
            self.phase = Phase::Detached;
            outs.push(Out::Notice(
                t("ble.control.a_trainer_nem_vezerelheto_ftms_en_keresztul_ezert_cs").to_string(),
            ));
        }
        outs
    }

    pub fn on_disconnected(&mut self) {
        self.phase = Phase::Detached;
        self.queue.clear();
        self.inflight = None;
        self.device_started = false;
    }

    // ── commands from the app ──

    pub fn set_simulation(&mut self, sim: TrainerSim, _now: Instant) {
        self.sim = Some(sim);
        self.mode = Mode::Sim;
        self.failed_streak = 0;
        self.after_target_change();
    }

    pub fn set_target_power(&mut self, watts: f64, _now: Instant) {
        if !watts.is_finite() {
            return;
        }
        self.target_power = Some(watts);
        self.mode = Mode::Power;
        self.failed_streak = 0;
        self.after_target_change();
    }

    /// `TrainerStart`: begin / resume the session on the trainer.
    pub fn start(&mut self, _now: Instant) {
        self.user_paused = false;
        self.device_blocked = false;
        self.wants_run = true;
        // a relax asked for while control was not ours is moot once the ride goes on
        self.pending_relax = None;
        if self.phase == Phase::Refused {
            self.phase = Phase::NeedControl;
            self.control_attempts = 0;
            // a manual retry is a fresh start: the losses before it no longer count
            self.reclaims = 0;
            self.retry_at = None;
        }
        if self.phase == Phase::Controlled {
            if !self.device_started {
                self.push_unique(Intent::Start);
            }
            self.ensure_target_intent();
        }
    }

    /// `TrainerPause`: relax the resistance (neutral grade) and pause the trainer. Updates
    /// stop until `start` is called again.
    pub fn pause(&mut self, _now: Instant) {
        self.user_paused = true;
        self.wants_run = false;
        self.queue.retain(|i| matches!(i, Intent::RequestControl));
        if self.phase == Phase::Controlled {
            if self.device_started || self.last_target_bytes.is_some() {
                self.push_unique(Intent::Neutral);
                self.push_unique(Intent::Pause);
            }
        } else {
            // Control is being (re)gained: the trainer may still hold the last target. The pause
            // is carried out as soon as control is ours again.
            self.pending_relax = Some(Intent::Pause);
        }
    }

    /// Hand the trainer back in a neutral state (before a deliberate disconnect / app exit).
    pub fn shutdown(&mut self, _now: Instant) {
        self.user_paused = true;
        self.wants_run = false;
        self.closing = true;
        self.queue.clear();
        if self.phase == Phase::Controlled {
            if self.device_started || self.last_target_bytes.is_some() {
                self.push_unique(Intent::Neutral);
                self.push_unique(Intent::Stop);
            }
        } else {
            self.pending_relax = Some(Intent::Stop);
            if self.phase == Phase::Refused {
                // One final bounded attempt can still hand the device back safely.
                self.phase = Phase::NeedControl;
                self.retry_at = None;
                self.control_attempts = self.cfg.control_retries.saturating_sub(1);
            }
        }
    }

    // ── input from the device ──

    /// A Control Point indication arrived.
    pub fn on_indication(&mut self, bytes: &[u8], now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();
        let Some(resp) = parse_cp_response(bytes) else {
            return outs;
        };
        self.failed_streak = 0;
        let Some(inflight) = self.inflight.take_if(|i| i.opcode == resp.request_opcode) else {
            return outs; // unsolicited / duplicate: ignore, keep waiting for ours
        };
        let ok = resp.result == CpResult::Success;
        match inflight.intent {
            Intent::RequestControl => self.handle_control_response(resp.result, now, &mut outs),
            Intent::Start => {
                if ok || resp.result == CpResult::OperationFailed {
                    // "operation failed" on Start means it is already running
                    self.device_started = true;
                    self.device_blocked = false;
                    self.last_target_bytes = None;
                    self.restart_at = None;
                    // the permission demonstrably works: earlier losses were spurious
                    self.reclaims = 0;
                } else if resp.result == CpResult::ControlNotPermitted {
                    self.lose_control(now, true, &mut outs);
                }
            }
            Intent::Sim | Intent::Target => {
                self.handle_target_response(inflight.intent, resp.result, now, &mut outs)
            }
            Intent::Neutral => {
                if resp.result == CpResult::ControlNotPermitted {
                    self.lose_control(now, true, &mut outs);
                }
            }
            Intent::Pause | Intent::Stop => {
                if ok {
                    self.device_started = false;
                }
            }
        }
        outs
    }

    /// A Fitness Machine Status notification arrived.
    pub fn on_machine_status(&mut self, status: MachineStatus, now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();
        match status {
            MachineStatus::ControlPermissionLost => {
                self.lose_control(now, true, &mut outs);
            }
            MachineStatus::Reset => {
                // Per the specification a Reset invalidates the control permission. Firmware
                // that repeats it after every control write is describing its idle state
                // instead (see [`Storm`]); dropping control for that would make ERG and the
                // grade simulation impossible on such a trainer, so only a first, isolated
                // Reset is taken as a permission loss.
                let storm = self.resets.repeats(now, self.cfg.status_storm_window);
                let just_granted = self
                    .granted_at
                    .is_some_and(|at| now.saturating_duration_since(at) <= self.cfg.grant_grace);
                if storm || just_granted {
                    self.keep_control_alive(now);
                    if self.resets.report() {
                        outs.push(Out::StatusIgnored(t(
                            "ble.control.a_trainer_ismetelten_reset_allapotot_jelez",
                        )));
                    }
                } else {
                    self.lose_control(now, false, &mut outs);
                }
            }
            MachineStatus::StoppedByUser | MachineStatus::PausedByUser => {
                if self.user_paused {
                    return outs;
                }
                // A repeated "stopped" is the device's steady state, not a new user action:
                // it must not latch the targets off for the rest of the ride.
                if self.stops.repeats(now, self.cfg.status_storm_window) {
                    self.device_blocked = false;
                    self.keep_control_alive(now);
                    if self.stops.report() {
                        outs.push(Out::StatusIgnored(t(
                            "ble.control.a_trainer_ismetelten_leallt_allapotot_jelez",
                        )));
                    }
                    return outs;
                }
                self.device_started = false;
                self.device_blocked = true;
                self.warn(
                    now,
                    t("ble.control.a_trainert_magan_a_keszuleken_allitottak_le_amig_ujr"),
                    &mut outs,
                );
            }
            MachineStatus::StoppedBySafetyKey => {
                if !self.user_paused {
                    self.device_started = false;
                    self.device_blocked = true;
                    self.warn(
                        now,
                        t("ble.control.a_trainer_a_biztonsagi_kulccsal_leallt_ha_ujraindito"),
                        &mut outs,
                    );
                }
            }
            MachineStatus::StartedOrResumed => {
                self.device_started = true;
                self.device_blocked = false;
                self.stops.clear();
                self.restart_at = None;
            }
            MachineStatus::Other(_) => {}
        }
        outs
    }

    /// The trainer says it is not running: keep the control permission, and — while the ride
    /// is running — ask it to start again at a bounded rate instead of giving up. The active
    /// target stays queued, so the next write carries the grade or the ERG wattage.
    fn keep_control_alive(&mut self, now: Instant) {
        if self.phase != Phase::Controlled
            || self.user_paused
            || self.device_blocked
            || self.closing
        {
            return;
        }
        let wants_to_ride = self.wants_run || self.mode != Mode::None;
        if wants_to_ride && !self.device_started && self.restart_at.is_none_or(|at| now >= at) {
            self.restart_at = Some(now + self.cfg.restart_interval);
            self.push_unique(Intent::Start);
        }
        self.ensure_target_intent();
    }

    /// A Control Point write failed at the transport level (not a trainer rejection). Returns
    /// the notices to show (a Request Control that keeps failing ends in `Refused`).
    pub fn on_write_failed(&mut self, now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();
        let Some(inflight) = self.inflight.take() else {
            return outs;
        };
        match inflight.intent {
            // counted like a timeout: `control_retries` bounds the attempts either way
            Intent::RequestControl => self.control_failed(now, &mut outs),
            // Busy trainers refuse writes now and then (ATT 0x11); try again after the write gap.
            // A relax (neutral, pause, stop) is retried as well: losing it would leave the
            // trainer on the last ERG target or grade.
            intent if self.failed_streak < MAX_WRITE_RETRIES => {
                self.failed_streak += 1;
                if matches!(intent, Intent::Sim | Intent::Target | Intent::Neutral) {
                    // the value never arrived: it must not count as sent
                    self.last_target_bytes = None;
                    self.last_target_sent = None;
                }
                if !self.queue.contains(&intent) {
                    self.queue.push_front(intent);
                }
            }
            Intent::Neutral | Intent::Pause | Intent::Stop => self.warn(
                now,
                t("ble.control.a_trainer_nem_valaszolt_idoben_egy_vezerloparancsra"),
                &mut outs,
            ),
            _ => {}
        }
        outs
    }

    // ── time ──

    /// Advance the state machine: handle timeouts, schedule keep-alives and return the next
    /// write if one is due. Call after every input and when [`next_deadline`](Self::next_deadline) passes.
    pub fn poll(&mut self, now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();

        if let Some(inf) = self.inflight {
            let timeout = if self.silent {
                self.cfg.min_write_gap
            } else {
                self.cfg.response_timeout
            };
            if now.saturating_duration_since(inf.sent_at) >= timeout {
                self.inflight = None;
                if inf.intent == Intent::RequestControl {
                    if self.control_attempts >= self.cfg.control_retries {
                        // Never an answer, not a refusal: drive the trainer without
                        // confirmations.
                        self.silent = true;
                        self.grant(now, &mut outs);
                    } else {
                        if !self.silent {
                            self.warn(
                                now,
                                t("ble.control.a_trainer_nem_valaszolt_idoben_egy_vezerloparancsra"),
                                &mut outs,
                            );
                        }
                        // also for a silent trainer: after a control loss it is asked again,
                        // and granted again once the retries are used up, as on connect
                        self.control_failed(now, &mut outs);
                    }
                } else if !self.silent {
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_valaszolt_idoben_egy_vezerloparancsra"),
                        &mut outs,
                    );
                }
            } else {
                return outs;
            }
        }

        if self.phase == Phase::NeedControl
            && (!self.closing || self.pending_relax.is_some())
            && self.retry_at.is_none_or(|t| now >= t)
            && !self.queue.contains(&Intent::RequestControl)
        {
            self.queue.push_front(Intent::RequestControl);
        }

        if self.keepalive_due(now) {
            self.ensure_target_intent();
        }

        if self
            .last_write
            .is_some_and(|t| now.saturating_duration_since(t) < self.cfg.min_write_gap)
        {
            return outs;
        }

        while let Some(intent) = self.queue.pop_front() {
            if let Some(bytes) = self.materialize(intent, now, &mut outs) {
                self.inflight = Some(Inflight {
                    intent,
                    opcode: intent.opcode(),
                    sent_at: now,
                });
                self.last_write = Some(now);
                outs.push(Out::Write(bytes));
                break;
            }
        }
        outs
    }

    /// The earliest instant at which [`poll`](Self::poll) has something to do (if any).
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let mut best: Option<Instant> = None;
        let mut consider = |t: Instant| best = Some(best.map_or(t, |b| b.min(t)));
        if let Some(inf) = self.inflight {
            // the same timeout `poll` applies
            let timeout = if self.silent {
                self.cfg.min_write_gap
            } else {
                self.cfg.response_timeout
            };
            consider(inf.sent_at + timeout);
        } else {
            let gap_end = self.last_write.map_or(now, |t| t + self.cfg.min_write_gap);
            if !self.queue.is_empty() {
                consider(gap_end.max(now));
            }
            if self.phase == Phase::NeedControl && (!self.closing || self.pending_relax.is_some()) {
                consider(self.retry_at.unwrap_or(now).max(gap_end).max(now));
            }
            if self.keepalive_armed() {
                let due = self
                    .last_target_sent
                    .map_or(now, |t| t + self.cfg.keepalive);
                consider(due.max(gap_end).max(now));
            }
        }
        best
    }

    /// Nothing queued and nothing in flight.
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.inflight.is_none()
    }

    /// Control permission is held (the trainer accepts our commands).
    pub fn has_control(&self) -> bool {
        self.phase == Phase::Controlled
    }

    // ── internals ──

    fn keepalive_armed(&self) -> bool {
        self.phase == Phase::Controlled
            && self.device_started
            && !self.user_paused
            && !self.device_blocked
            && self.mode != Mode::None
    }

    fn keepalive_due(&self, now: Instant) -> bool {
        self.keepalive_armed()
            && self
                .last_target_sent
                .is_none_or(|t| now.saturating_duration_since(t) >= self.cfg.keepalive)
    }

    fn push_unique(&mut self, intent: Intent) {
        if !self.queue.contains(&intent) {
            self.queue.push_back(intent);
        }
    }

    fn ensure_target_intent(&mut self) {
        let wanted = match self.mode {
            Mode::None => return,
            Mode::Sim => Intent::Sim,
            Mode::Power => Intent::Target,
        };
        // the two targets are mutually exclusive; the newest wins
        self.queue
            .retain(|i| !matches!(i, Intent::Sim | Intent::Target) || *i == wanted);
        self.push_unique(wanted);
    }

    fn after_target_change(&mut self) {
        if self.phase != Phase::Controlled || self.user_paused || self.device_blocked {
            return; // stored; replayed on control grant / start
        }
        if !self.device_started {
            self.push_unique(Intent::Start);
        }
        self.ensure_target_intent();
    }

    /// Emit `text` unless the same text was shown less than `notice_cooldown` ago.
    fn warn(&mut self, now: Instant, text: &'static str, outs: &mut Vec<Out>) {
        let cooldown = self.cfg.notice_cooldown;
        match self.warned.iter_mut().find(|(t, _)| *t == text) {
            Some((_, last)) => {
                if now.saturating_duration_since(*last) < cooldown {
                    return;
                }
                *last = now;
            }
            None => self.warned.push((text, now)),
        }
        outs.push(Out::Notice(text.to_string()));
    }

    /// Turn an intent into the bytes to write, or `None` if it is moot right now.
    fn materialize(
        &mut self,
        intent: Intent,
        now: Instant,
        outs: &mut Vec<Out>,
    ) -> Option<Vec<u8>> {
        let raw = match intent {
            Intent::RequestControl => {
                if !matches!(self.phase, Phase::NeedControl | Phase::Requesting) {
                    return None;
                }
                self.phase = Phase::Requesting;
                self.control_attempts += 1;
                encode_request_control()
            }
            Intent::Start => {
                if self.phase != Phase::Controlled {
                    return None;
                }
                encode_start()
            }
            Intent::Sim => {
                let sim = self.sim?;
                if self.phase != Phase::Controlled || self.mode != Mode::Sim {
                    return None;
                }
                if self.caps.supports_sim == Some(false) {
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_tamogatja_a_terepszimulaciot_ezert_az_"),
                        outs,
                    );
                    return None;
                }
                let bytes = encode_simulation(quantise_sim(&sim, self.limits.grade_pct));
                if self.is_redundant(&bytes, now) {
                    return None;
                }
                bytes
            }
            Intent::Target => {
                let watts = self.target_power?;
                if self.phase != Phase::Controlled || self.mode != Mode::Power {
                    return None;
                }
                if self.caps.supports_power_target == Some(false) {
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_tamogatja_a_celteljesitmeny_erg_modot"),
                        outs,
                    );
                    return None;
                }
                let bytes = encode_target_power(watts);
                if self.is_redundant(&bytes, now) {
                    return None;
                }
                bytes
            }
            Intent::Neutral => {
                if self.phase != Phase::Controlled || self.caps.supports_sim == Some(false) {
                    return None;
                }
                let base = self.sim.unwrap_or(TrainerSim {
                    grade_pct: 0.0,
                    crr: 0.004,
                    cw: 0.2,
                    wind_ms: 0.0,
                });
                let neutral = TrainerSim {
                    grade_pct: 0.0,
                    wind_ms: 0.0,
                    ..base
                };
                encode_simulation(quantise_sim(&neutral, self.limits.grade_pct))
            }
            Intent::Pause => {
                if self.phase != Phase::Controlled {
                    return None;
                }
                encode_stop_pause(true)
            }
            Intent::Stop => {
                if self.phase != Phase::Controlled {
                    return None;
                }
                encode_stop_pause(false)
            }
        };
        match filter(&raw, &self.limits) {
            Verdict::Forward(bytes) => {
                if matches!(intent, Intent::Sim | Intent::Target) {
                    self.last_target_sent = Some(now);
                    self.last_target_bytes = Some(bytes.clone());
                } else if intent == Intent::Neutral {
                    self.last_target_bytes = Some(bytes.clone());
                }
                Some(bytes)
            }
            Verdict::Reject(_) => None, // cannot happen for our own encoders (unit-tested)
        }
    }

    /// Same bytes as the last target and sent less than a keep-alive ago.
    fn is_redundant(&self, bytes: &[u8], now: Instant) -> bool {
        self.last_target_bytes.as_deref() == Some(bytes)
            && self
                .last_target_sent
                .is_some_and(|t| now.saturating_duration_since(t) < self.cfg.keepalive)
    }

    /// Control is ours (granted, or assumed for a silent trainer): replay what the app wanted
    /// before the (re)connect / control loss.
    fn grant(&mut self, now: Instant, outs: &mut Vec<Out>) {
        self.phase = Phase::Controlled;
        self.control_attempts = 0;
        self.retry_at = None;
        self.last_target_bytes = None;
        self.granted_at = Some(now);
        self.restart_at = None;
        outs.push(Out::ControlGranted);
        if let Some(relax) = self.pending_relax.take() {
            if self.user_paused {
                self.push_unique(Intent::Neutral);
                self.push_unique(relax);
                return;
            }
        }
        if !self.user_paused && !self.device_blocked && (self.wants_run || self.mode != Mode::None)
        {
            self.push_unique(Intent::Start);
            self.ensure_target_intent();
        }
    }

    fn handle_control_response(&mut self, result: CpResult, now: Instant, outs: &mut Vec<Out>) {
        if result == CpResult::Success {
            self.silent = false;
            self.grant(now, outs);
            return;
        }
        if result == CpResult::ControlNotPermitted {
            self.warn(
                now,
                t("ble.control.a_trainert_most_egy_masik_alkalmazas_vezerli_zard_be"),
                outs,
            );
        }
        self.control_failed(now, outs);
    }

    /// Gives up on control: always told, whatever the notice cooldown (the rider has to know
    /// that only data flows now and that `Start` tries again).
    fn refuse(&mut self, now: Instant, text: &'static str, outs: &mut Vec<Out>) {
        if self.phase == Phase::Refused {
            return; // already told
        }
        self.phase = Phase::Refused;
        self.warned.retain(|(known, _)| *known != text);
        self.warn(now, text, outs);
    }

    fn control_failed(&mut self, now: Instant, outs: &mut Vec<Out>) {
        if self.control_attempts >= self.cfg.control_retries {
            self.refuse(
                now,
                t("ble.control.a_trainer_nem_adta_at_a_vezerlest_ezert_csak_adatot_"),
                outs,
            );
        } else {
            self.phase = Phase::NeedControl;
            self.retry_at = Some(now + self.cfg.retry_delay);
        }
    }

    fn lose_control(&mut self, now: Instant, hostile: bool, outs: &mut Vec<Out>) {
        if self.user_paused {
            self.pending_relax = Some(if self.closing {
                Intent::Stop
            } else {
                Intent::Pause
            });
        }
        self.queue.clear();
        self.inflight = None;
        self.device_started = false;
        self.last_target_bytes = None;
        self.granted_at = None;
        self.restart_at = None;
        outs.push(Out::ControlLost);
        if hostile {
            self.reclaims += 1;
        }
        if self.reclaims > self.cfg.reclaim_limit {
            self.refuse(
                now,
                t("ble.control.tobbszor_is_elvesztettuk_a_trainer_vezerleset_talan_"),
                outs,
            );
        } else {
            self.phase = Phase::NeedControl;
            self.control_attempts = 0;
            self.retry_at = Some(now + self.cfg.retry_delay);
            if hostile {
                self.warn(
                    now,
                    t("ble.control.elvesztettuk_a_trainer_vezerleset_megprobaljuk_vissz"),
                    outs,
                );
            }
        }
    }

    fn handle_target_response(
        &mut self,
        intent: Intent,
        result: CpResult,
        now: Instant,
        outs: &mut Vec<Out>,
    ) {
        match result {
            CpResult::Success => {
                // a target was accepted: the control permission is real, not being fought over
                self.reclaims = 0;
            }
            CpResult::ControlNotPermitted => self.lose_control(now, true, outs),
            CpResult::OpCodeNotSupported => {
                if intent == Intent::Sim {
                    self.caps.supports_sim = Some(false);
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_tamogatja_a_terepszimulaciot_ezert_az_"),
                        outs,
                    );
                } else {
                    self.caps.supports_power_target = Some(false);
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_tamogatja_a_celteljesitmeny_erg_modot"),
                        outs,
                    );
                }
            }
            CpResult::InvalidParameter => {
                self.warn(
                    now,
                    t("ble.control.a_trainer_elutasitotta_a_megadott_parametereket"),
                    outs,
                );
            }
            CpResult::OperationFailed | CpResult::Other(_) => {
                // Some trainers refuse targets until they are started: keep the target queued
                // and ask for Start again — at a bounded rate, so a device that always refuses
                // does not turn into a write storm.
                if !self.retried_start_for_sim {
                    self.retried_start_for_sim = true;
                    self.device_started = false;
                    self.restart_at = None;
                    self.keep_control_alive(now);
                } else {
                    self.warn(
                        now,
                        t("ble.control.a_trainer_nem_tudta_vegrehajtani_a_vezerloparancsot"),
                        outs,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(grade: f64) -> TrainerSim {
        TrainerSim {
            grade_pct: grade,
            crr: 0.004,
            cw: 0.51,
            wind_ms: 0.0,
        }
    }

    fn ok(op: u8) -> Vec<u8> {
        vec![0x80, op, 0x01]
    }

    fn writes(outs: &[Out]) -> Vec<Vec<u8>> {
        outs.iter()
            .filter_map(|o| {
                if let Out::Write(b) = o {
                    Some(b.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Drive a connected, controlled session to the point where the simulation is active.
    fn running(t: Instant) -> (ControlSession, Instant) {
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.set_simulation(sim(5.0), t);
        let mut now = t;
        assert_eq!(writes(&s.poll(now)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), now);
        now += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(now)), vec![vec![0x07]]);
        s.on_indication(&ok(0x07), now);
        now += Duration::from_millis(300);
        let w = writes(&s.poll(now));
        assert_eq!(w, vec![vec![0x11, 0x00, 0x00, 0xF4, 0x01, 0x28, 0x33]]);
        s.on_indication(&ok(0x11), now);
        (s, now)
    }

    #[test]
    fn a_trainer_that_never_indicates_is_driven_without_confirmations() {
        // Some cheap FTMS trainers (e.g. a "VE200") accept commands but never indicate.
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.start(t);
        s.set_target_power(150.0, t);
        let mut sent = Vec::new();
        let mut notices = Vec::new();
        let mut at = t;
        while at < t + Duration::from_secs(20) {
            let outs = s.poll(at);
            sent.extend(writes(&outs));
            notices.extend(outs.into_iter().filter_map(|o| match o {
                Out::Notice(m) => Some(m),
                _ => None,
            }));
            at += Duration::from_millis(100);
        }
        let requests = sent.iter().filter(|w| **w == vec![0x00]).count();
        assert_eq!(
            requests, 3,
            "three unanswered Request Control attempts: {sent:?}"
        );
        assert!(sent.contains(&vec![0x07]), "then Start: {sent:?}");
        assert!(
            sent.contains(&vec![0x05, 150, 0]),
            "then the ERG target: {sent:?}"
        );
        assert!(
            !notices.iter().any(|m| m.contains("nem adta át")),
            "a silent trainer is not refused: {notices:?}"
        );
        assert!(s.has_control());
    }

    #[test]
    fn an_explicit_refusal_still_refuses() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        let mut at = t;
        for _ in 0..3 {
            assert_eq!(writes(&s.poll(at)), vec![vec![0x00]]);
            s.on_indication(&[0x80, 0x00, 0x05], at); // control not permitted
            at += Duration::from_millis(1100);
        }
        assert!(!s.has_control());
        assert!(writes(&s.poll(at + Duration::from_secs(10))).is_empty());
    }

    #[test]
    fn a_failed_target_write_is_retried_after_the_write_gap() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_secs(1);
        s.set_target_power(200.0, at);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x05, 200, 0]]);
        // the trainer answers the ATT write with "insufficient resources"
        s.on_write_failed(at);
        let again = at + Duration::from_millis(300);
        assert_eq!(
            writes(&s.poll(again)),
            vec![vec![0x05, 200, 0]],
            "retried instead of waiting for the keep-alive"
        );
    }

    // ── filter ──

    // Captured X2 Max ranges (OsmolControl docs/devices/x2-max.md): power 0-2000 W.
    fn x2() -> Limits {
        Limits::from_ranges(None, Some((0, 2000)))
    }

    #[test]
    fn filter_allows_the_five_commands_and_clamps() {
        let l = x2();
        assert_eq!(filter(&[0x00], &l), Verdict::Forward(vec![0x00]));
        assert_eq!(filter(&[0x07], &l), Verdict::Forward(vec![0x07]));
        assert_eq!(
            filter(&[0x08, 0x01], &l),
            Verdict::Forward(vec![0x08, 0x01])
        );
        assert_eq!(
            filter(&[0x08, 0x02], &l),
            Verdict::Forward(vec![0x08, 0x02])
        );
        assert_eq!(
            filter(&[0x05, 0xc8, 0x00], &l),
            Verdict::Forward(vec![0x05, 0xc8, 0x00])
        );
        // 3000 W is clamped to the advertised 2000 W; negative power to 0
        assert_eq!(
            filter(&[0x05, 0xb8, 0x0b], &l),
            Verdict::Forward(vec![0x05, 0xd0, 0x07])
        );
        assert_eq!(
            filter(&[0x05, 0xff, 0xff], &l),
            Verdict::Forward(vec![0x05, 0, 0])
        );
    }

    #[test]
    fn filter_clamps_simulation_grade_and_keeps_other_fields() {
        let l = x2();
        // wind 0, grade +50 %, crr 0x28, cw 0x33 -> grade clamped to +20 % = 2000
        let Verdict::Forward(out) = filter(&[0x11, 0, 0, 0x88, 0x13, 0x28, 0x33], &l) else {
            panic!()
        };
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), 2000);
        assert_eq!((out[5], out[6]), (0x28, 0x33));
        let Verdict::Forward(out) = filter(&[0x11, 0, 0, 0x78, 0xEC, 0x28, 0x33], &l) else {
            panic!()
        };
        assert_eq!(
            i16::from_le_bytes([out[3], out[4]]),
            -2000,
            "-50 % -> -20 %"
        );
        // in range stays untouched
        let ok = [0x11, 0, 0, 0xf4, 0x01, 0x28, 0x33];
        assert_eq!(filter(&ok, &l), Verdict::Forward(ok.to_vec()));
    }

    #[test]
    fn filter_respects_a_narrower_trainer_inclination_range() {
        let l = Limits::from_ranges(Some((-10.0, 15.0)), None);
        assert_eq!(l.grade_pct, (-10.0, 15.0));
        let steep = encode_simulation(quantise_sim(&sim(18.0), (-20.0, 20.0)));
        let Verdict::Forward(out) = filter(&steep, &l) else {
            panic!()
        };
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), 1500);
        let down = encode_simulation(quantise_sim(&sim(-18.0), (-20.0, 20.0)));
        let Verdict::Forward(out) = filter(&down, &l) else {
            panic!()
        };
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), -1000);
        // a range wider than +/-20 % never widens the window
        assert_eq!(
            Limits::from_ranges(Some((-40.0, 40.0)), None).grade_pct,
            (-20.0, 20.0)
        );
        // nonsense falls back
        assert_eq!(
            Limits::from_ranges(Some((30.0, -30.0)), None).grade_pct,
            (-20.0, 20.0)
        );
        assert_eq!(
            Limits::from_ranges(Some((f64::NAN, 5.0)), None).grade_pct,
            (-20.0, 20.0)
        );
    }

    #[test]
    fn filter_rejects_everything_else() {
        let l = x2();
        assert_eq!(
            filter(&[0x01], &l),
            Verdict::Reject([0x80, 0x01, RESULT_NOT_SUPPORTED]),
            "Reset drops control"
        );
        assert_eq!(
            filter(&[0x04, 100], &l),
            Verdict::Reject([0x80, 0x04, RESULT_NOT_SUPPORTED])
        );
        assert_eq!(
            filter(&[0x13, 1], &l),
            Verdict::Reject([0x80, 0x13, RESULT_NOT_SUPPORTED])
        );
        assert_eq!(
            filter(&[0x05, 1], &l),
            Verdict::Reject([0x80, 0x05, RESULT_INVALID_PARAMETER])
        );
        assert_eq!(
            filter(&[0x08, 9], &l),
            Verdict::Reject([0x80, 0x08, RESULT_INVALID_PARAMETER])
        );
        assert_eq!(
            filter(&[0x08, 0], &l),
            Verdict::Reject([0x80, 0x08, RESULT_INVALID_PARAMETER])
        );
        assert_eq!(
            filter(&[0x00, 1], &l),
            Verdict::Reject([0x80, 0x00, RESULT_INVALID_PARAMETER])
        );
        assert_eq!(
            filter(&[], &l),
            Verdict::Reject([0x80, 0, RESULT_INVALID_PARAMETER])
        );
        // exhaustive: only the allow-listed op codes ever come out, with the same length
        for opcode in 0..=u8::MAX {
            for len in 0..9 {
                let cmd: Vec<u8> = std::iter::once(opcode)
                    .chain(std::iter::repeat_n(0xff, len))
                    .collect();
                if let Verdict::Forward(out) = filter(&cmd, &l) {
                    assert!(
                        matches!(opcode, 0x00 | 0x05 | 0x07 | 0x08 | 0x11),
                        "{opcode:#x}"
                    );
                    assert_eq!(out.len(), cmd.len());
                }
            }
        }
    }

    // ── encoders ──

    #[test]
    fn encoders_produce_the_spec_bytes() {
        assert_eq!(encode_request_control(), vec![0x00]);
        assert_eq!(encode_start(), vec![0x07]);
        assert_eq!(encode_stop_pause(false), vec![0x08, 0x01]);
        assert_eq!(encode_stop_pause(true), vec![0x08, 0x02]);
        assert_eq!(encode_target_power(200.0), vec![0x05, 0xC8, 0x00]);
        assert_eq!(encode_target_power(200.4), vec![0x05, 0xC8, 0x00]);
        assert_eq!(encode_target_power(f64::NAN), vec![0x05, 0x00, 0x00]);
        assert_eq!(
            encode_target_power(1e9),
            vec![0x05, 0xFF, 0x7F],
            "saturates at i16::MAX"
        );
    }

    #[test]
    fn simulation_quantisation_and_negative_grade() {
        let w = quantise_sim(&sim(5.0), (-20.0, 20.0));
        assert_eq!(
            w,
            WireSim {
                wind: 0,
                grade: 500,
                crr: 40,
                cw: 51
            }
        );
        assert_eq!(
            encode_simulation(w),
            vec![0x11, 0x00, 0x00, 0xF4, 0x01, 0x28, 0x33]
        );
        // -3.5 % = -350 = 0xFEA2
        let w = quantise_sim(&sim(-3.5), (-20.0, 20.0));
        assert_eq!(w.grade, -350);
        assert_eq!(encode_simulation(w)[3..5], [0xA2, 0xFE]);
        // quantisation edges: 3.456 % -> 3.46 % (346), 0.004999 crr -> 0.0050, cw 0.196 -> 0.20
        let s = TrainerSim {
            grade_pct: 3.456,
            crr: 0.004_999,
            cw: 0.196,
            wind_ms: -1.2345,
        };
        let w = quantise_sim(&s, (-20.0, 20.0));
        assert_eq!(
            w,
            WireSim {
                wind: -1235,
                grade: 346,
                crr: 50,
                cw: 20
            }
        );
        // wire saturation and non-finite inputs
        let s = TrainerSim {
            grade_pct: f64::NAN,
            crr: f64::INFINITY,
            cw: 9.9,
            wind_ms: 1e9,
        };
        let w = quantise_sim(&s, (-20.0, 20.0));
        assert_eq!(
            w,
            WireSim {
                wind: i16::MAX,
                grade: 0,
                crr: 40,
                cw: 255
            }
        );
        // grade is clamped into the given window before quantising
        assert_eq!(quantise_sim(&sim(99.0), (-20.0, 20.0)).grade, 2000);
        assert_eq!(quantise_sim(&sim(-99.0), (-5.0, 8.0)).grade, -500);
    }

    #[test]
    fn every_generated_command_passes_the_filter_unchanged() {
        let l = Limits::FALLBACK;
        for g in (-3000..=3000).map(|v| f64::from(v) / 100.0) {
            let bytes = encode_simulation(quantise_sim(&sim(g), l.grade_pct));
            assert_eq!(
                filter(&bytes, &l),
                Verdict::Forward(bytes.clone()),
                "grade {g}"
            );
        }
        for w in [0.0, 1.0, 250.0, 1500.0, 99999.0] {
            let bytes = encode_target_power(w);
            let Verdict::Forward(out) = filter(&bytes, &l) else {
                panic!("{w}")
            };
            assert!(i16::from_le_bytes([out[1], out[2]]) <= 1500);
        }
    }

    // ── session ──

    #[test]
    fn handshake_order_and_replay_of_the_desired_simulation() {
        let t = Instant::now();
        let (s, _) = running(t);
        assert!(s.has_control());
        assert!(s.is_idle());
    }

    #[test]
    fn one_outstanding_procedure_and_write_pacing() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.set_simulation(sim(2.0), t);
        assert_eq!(writes(&s.poll(t)).len(), 1);
        // still waiting for the Request Control response
        assert!(s.poll(t + Duration::from_millis(100)).is_empty());
        assert!(s.poll(t + Duration::from_millis(2900)).is_empty());
        s.on_indication(&ok(0x00), t + Duration::from_millis(50));
        // response arrived, but the 250 ms write gap has not passed since the last write
        assert!(s.poll(t + Duration::from_millis(100)).is_empty());
        assert_eq!(
            writes(&s.poll(t + Duration::from_millis(260))),
            vec![vec![0x07]]
        );
    }

    #[test]
    fn a_response_for_another_op_code_is_ignored() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.start(t);
        assert_eq!(writes(&s.poll(t)), vec![vec![0x00]]);
        // wrong op code / garbage must not unblock the queue
        assert!(s.on_indication(&[0x80, 0x07, 0x01], t).is_empty());
        assert!(s.on_indication(&[0x01], t).is_empty());
        assert!(s.on_indication(&[], t).is_empty());
        assert!(
            s.poll(t + Duration::from_millis(500)).is_empty(),
            "still waiting"
        );
        let outs = s.on_indication(&ok(0x00), t + Duration::from_millis(500));
        assert!(outs.contains(&Out::ControlGranted));
    }

    #[test]
    fn keep_alive_resends_at_least_every_five_seconds() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let expect = vec![0x11, 0x00, 0x00, 0xF4, 0x01, 0x28, 0x33];
        assert!(s.poll(now + Duration::from_millis(4500)).is_empty());
        let w = writes(&s.poll(now + Duration::from_millis(5100)));
        assert_eq!(w, vec![expect.clone()], "same value re-sent as keep-alive");
        s.on_indication(&ok(0x11), now + Duration::from_millis(5200));
        assert!(s.poll(now + Duration::from_millis(9000)).is_empty());
        assert_eq!(
            writes(&s.poll(now + Duration::from_millis(10_200))),
            vec![expect]
        );
    }

    #[test]
    fn identical_updates_are_not_re_sent_but_changes_are() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_millis(400);
        s.set_simulation(sim(5.0), at); // same value
        assert!(
            s.poll(at).is_empty(),
            "redundant within the keep-alive window"
        );
        at += Duration::from_millis(400);
        s.set_simulation(sim(5.5), at);
        assert_eq!(
            writes(&s.poll(at)),
            vec![vec![0x11, 0x00, 0x00, 0x26, 0x02, 0x28, 0x33]]
        );
    }

    #[test]
    fn deadline_tracks_keepalive_and_timeouts() {
        let t = Instant::now();
        let (s, now) = running(t);
        let d = s.next_deadline(now).unwrap();
        assert_eq!(d, now + Duration::from_secs(5));
        let mut s2 = ControlSession::new(ControlConfig::default());
        s2.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        assert_eq!(
            s2.next_deadline(t),
            Some(t),
            "control request is due immediately"
        );
        assert_eq!(writes(&s2.poll(t)).len(), 1);
        assert_eq!(s2.next_deadline(t), Some(t + Duration::from_secs(3)));
        let idle = ControlSession::new(ControlConfig::default());
        assert_eq!(idle.next_deadline(t), None);
    }

    #[test]
    fn grade_is_clamped_on_the_wire() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        s.set_simulation(sim(35.0), now + Duration::from_millis(400));
        let w = writes(&s.poll(now + Duration::from_millis(400)));
        assert_eq!(w[0][3..5], 2000i16.to_le_bytes());
        s.on_indication(&ok(0x11), now + Duration::from_millis(450));
        s.set_simulation(sim(-25.0), now + Duration::from_millis(900));
        let w = writes(&s.poll(now + Duration::from_millis(900)));
        assert_eq!(w[0][3..5], (-2000i16).to_le_bytes());
    }

    #[test]
    fn response_timeout_unblocks_the_queue_and_warns_once() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        s.set_simulation(sim(6.0), now + Duration::from_millis(400));
        assert_eq!(writes(&s.poll(now + Duration::from_millis(400))).len(), 1);
        // no indication: nothing for 3 s, then a notice and the next command may go out
        s.set_simulation(sim(7.0), now + Duration::from_millis(900));
        assert!(s.poll(now + Duration::from_millis(2000)).is_empty());
        let outs = s.poll(now + Duration::from_millis(3500));
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("nem válaszolt"))));
        assert_eq!(
            writes(&outs).len(),
            1,
            "the newer value goes out right after the timeout"
        );
        // a second timeout inside the cooldown stays silent
        let outs = s.poll(now + Duration::from_millis(7000));
        assert!(!outs.iter().any(|o| matches!(o, Out::Notice(_))));
    }

    #[test]
    fn pause_sends_neutral_then_pause_and_start_resumes_with_the_latest_value() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_secs(1);
        s.pause(at);
        s.set_simulation(sim(8.0), at); // while paused: stored only
        assert_eq!(
            writes(&s.poll(at)),
            vec![vec![0x11, 0, 0, 0x00, 0x00, 0x28, 0x33]],
            "neutral grade first"
        );
        s.on_indication(&ok(0x11), at);
        at += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x08, 0x02]]);
        s.on_indication(&ok(0x08), at);
        at += Duration::from_secs(10);
        assert!(s.poll(at).is_empty(), "no keep-alive while paused");
        s.start(at);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x07]]);
        s.on_indication(&ok(0x07), at);
        at += Duration::from_millis(300);
        assert_eq!(
            writes(&s.poll(at)),
            vec![vec![0x11, 0, 0, 0x20, 0x03, 0x28, 0x33]],
            "8 % sent even though 5 % was the last value before the neutral grade"
        );
    }

    #[test]
    fn shutdown_is_neutral_then_stop() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_millis(500);
        s.shutdown(at);
        assert_eq!(
            writes(&s.poll(at)),
            vec![vec![0x11, 0, 0, 0x00, 0x00, 0x28, 0x33]]
        );
        s.on_indication(&ok(0x11), at);
        at += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x08, 0x01]]);
        s.on_indication(&ok(0x08), at);
        assert!(s.is_idle());
        // Even without control, shutdown requests it once more to relax the trainer.
        let mut fresh = ControlSession::new(ControlConfig::default());
        fresh.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        fresh.shutdown(t);
        assert_eq!(writes(&fresh.poll(t)), vec![vec![0x00]]);
        fresh.on_indication(&ok(0x00), t);
        assert_eq!(
            writes(&fresh.poll(t + Duration::from_millis(300)))[0][0],
            0x11
        );
    }

    #[test]
    fn reconnect_replays_control_start_and_simulation() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        s.on_disconnected();
        assert!(
            s.poll(now + Duration::from_secs(1)).is_empty(),
            "detached: nothing is sent"
        );
        let at = now + Duration::from_secs(5);
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, at);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), at);
        let at = at + Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x07]]);
        s.on_indication(&ok(0x07), at);
        let at = at + Duration::from_millis(300);
        assert_eq!(
            writes(&s.poll(at)),
            vec![vec![0x11, 0, 0, 0xF4, 0x01, 0x28, 0x33]]
        );
    }

    #[test]
    fn control_permission_lost_re_requests_with_a_limit() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_secs(1);
        let outs = s.on_machine_status(MachineStatus::ControlPermissionLost, at);
        assert!(outs.contains(&Out::ControlLost));
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("Elvesztettük"))));
        assert!(
            s.poll(at + Duration::from_millis(500)).is_empty(),
            "retry delay"
        );
        at += Duration::from_millis(1100);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), at);
        // keeps fighting at most `reclaim_limit` (3) times, then gives up with an explanation
        let mut all = Vec::new();
        for _ in 0..3 {
            at += Duration::from_secs(2);
            all.extend(s.on_machine_status(MachineStatus::ControlPermissionLost, at));
        }
        assert!(
            all.iter()
                .any(|o| matches!(o, Out::Notice(m) if m.contains("Többször"))),
            "{all:?}"
        );
        assert!(s.poll(at + Duration::from_secs(60)).is_empty(), "gave up");
    }

    #[test]
    fn not_permitted_on_a_target_means_control_was_lost() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_millis(500);
        s.set_simulation(sim(1.0), at);
        assert_eq!(writes(&s.poll(at)).len(), 1);
        s.on_indication(&[0x80, 0x11, 0x05], at);
        assert!(!s.has_control());
        let later = at + Duration::from_millis(1200);
        assert_eq!(writes(&s.poll(later)), vec![vec![0x00]]);
    }

    #[test]
    fn request_control_failure_retries_then_refuses_until_start() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        let mut at = t;
        let mut all = Vec::new();
        for attempt in 1..=3 {
            assert_eq!(writes(&s.poll(at)), vec![vec![0x00]], "attempt {attempt}");
            all.extend(s.on_indication(&[0x80, 0x00, 0x04], at)); // operation failed
            at += Duration::from_millis(1100);
        }
        assert!(
            all.iter()
                .any(|o| matches!(o, Out::Notice(m) if m.contains("nem adta át"))),
            "{all:?}"
        );
        assert!(s.poll(at).is_empty());
        assert!(s.poll(at + Duration::from_secs(30)).is_empty(), "refused");
        // the rider restarting the ride is a manual retry
        s.start(at + Duration::from_secs(31));
        assert_eq!(
            writes(&s.poll(at + Duration::from_secs(31))),
            vec![vec![0x00]]
        );
    }

    #[test]
    fn control_not_permitted_explains_who_holds_it() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.poll(t);
        let outs = s.on_indication(&[0x80, 0x00, 0x05], t);
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("másik alkalmazás"))));
    }

    #[test]
    fn unsupported_simulation_is_reported_once_and_never_written() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        let caps = TrainerCaps {
            has_control_point: true,
            supports_sim: Some(false),
            supports_power_target: Some(true),
        };
        s.on_connected(caps, Limits::FALLBACK, t);
        s.set_simulation(sim(4.0), t);
        let mut at = t;
        assert_eq!(writes(&s.poll(at)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), at);
        at += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x07]]);
        s.on_indication(&ok(0x07), at);
        at += Duration::from_millis(300);
        let outs = s.poll(at);
        assert!(writes(&outs).is_empty());
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("terepszimuláció"))));
        s.set_simulation(sim(5.0), at);
        assert!(!s
            .poll(at + Duration::from_secs(1))
            .iter()
            .any(|o| matches!(o, Out::Write(_))));
    }

    #[test]
    fn op_code_not_supported_response_disables_the_simulation() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_millis(400);
        s.set_simulation(sim(6.0), at);
        assert_eq!(writes(&s.poll(at)).len(), 1);
        let outs = s.on_indication(&[0x80, 0x11, 0x02], at);
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("terepszimuláció"))));
        s.set_simulation(sim(7.0), at + Duration::from_secs(1));
        assert!(writes(&s.poll(at + Duration::from_secs(1))).is_empty());
    }

    #[test]
    fn start_reported_as_failed_means_it_was_already_running() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.set_simulation(sim(1.0), t);
        s.poll(t);
        s.on_indication(&ok(0x00), t);
        let mut at = t + Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x07]]);
        s.on_indication(&[0x80, 0x07, 0x04], at); // "operation failed"
        at += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)).len(), 1, "the simulation follows");
    }

    #[test]
    fn target_refused_before_start_triggers_exactly_one_start_retry() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_millis(400);
        s.set_simulation(sim(2.0), at);
        assert_eq!(writes(&s.poll(at)).len(), 1);
        s.device_started = false; // the trainer silently stopped meanwhile
        s.on_indication(&[0x80, 0x11, 0x04], at); // operation failed
        at += Duration::from_millis(300);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x07]]);
        s.on_indication(&ok(0x07), at);
        at += Duration::from_millis(300);
        assert_eq!(
            writes(&s.poll(at)).len(),
            1,
            "simulation re-sent after the start"
        );
        s.device_started = false;
        s.on_indication(&[0x80, 0x11, 0x04], at);
        at += Duration::from_millis(300);
        assert!(writes(&s.poll(at)).is_empty(), "no endless start loop");
    }

    #[test]
    fn user_stop_on_the_device_blocks_auto_restart_until_start() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let mut at = now + Duration::from_secs(1);
        let outs = s.on_machine_status(MachineStatus::PausedByUser, at);
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("állították le"))));
        s.set_simulation(sim(9.0), at);
        at += Duration::from_secs(10);
        assert!(
            s.poll(at).is_empty(),
            "we do not fight the rider's own stop button"
        );
        s.on_machine_status(MachineStatus::StartedOrResumed, at);
        s.set_simulation(sim(9.5), at);
        assert_eq!(
            writes(&s.poll(at)).len(),
            1,
            "device runs again, updates flow"
        );
    }

    #[test]
    fn our_own_pause_echo_is_not_treated_as_a_user_stop() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        s.pause(now + Duration::from_millis(500));
        let outs = s.on_machine_status(
            MachineStatus::PausedByUser,
            now + Duration::from_millis(600),
        );
        assert!(outs.is_empty());
    }

    // ── status storms (captured firmware behaviour) ──

    /// A status right after the grant is firmware chatter, not a permission loss.
    #[test]
    fn a_reset_right_after_the_grant_keeps_the_control() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let outs = s.on_machine_status(MachineStatus::Reset, now + Duration::from_millis(10));
        assert!(!outs.contains(&Out::ControlLost), "{outs:?}");
        assert!(s.has_control());
    }

    /// The captured trainer sends `Machine Status 0x01 (Reset)` four times a second, also
    /// right after every control grant. Honouring each one as a permission loss produced an
    /// endless Request Control loop in which no target was ever written: 55 requests and not
    /// a single simulation or target write in one session (ble.log, 2026-11-01).
    #[test]
    fn a_reset_storm_does_not_stop_the_targets() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        // A grade change is queued …
        let at0 = now + Duration::from_millis(1200);
        s.set_simulation(sim(9.0), at0);
        let mut wrote: Vec<Vec<u8>> = Vec::new();
        let mut at = at0;
        for _ in 0..40 {
            // … while the trainer chatters: 4 statuses per second
            s.on_machine_status(MachineStatus::Reset, at);
            wrote.extend(writes(&s.poll(at)));
            at += Duration::from_millis(250);
        }
        assert!(s.has_control(), "the control permission survives the storm");
        assert!(
            wrote.iter().any(|b| b.first() == Some(&op::SET_SIMULATION)),
            "the grade reaches the trainer: {wrote:?}"
        );
        assert!(
            !wrote
                .iter()
                .any(|b| b.first() == Some(&op::REQUEST_CONTROL)),
            "no Request Control ping-pong: {wrote:?}"
        );
        assert_eq!(
            s.cfg.grant_grace,
            Duration::from_secs(2),
            "the production grace is seconds, not milliseconds"
        );
    }

    /// A lone Reset far from any grant is still the permission loss the specification means.
    #[test]
    fn a_lone_reset_still_loses_the_control() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_secs(6);
        let outs = s.on_machine_status(MachineStatus::Reset, at);
        assert!(outs.contains(&Out::ControlLost), "{outs:?}");
        assert!(!s.has_control());
        // … and the permission is asked for again, so the ride recovers by itself
        assert_eq!(
            writes(&s.poll(at + Duration::from_secs(2))),
            vec![vec![0x00]]
        );
    }

    /// If the chatter pauses long enough for a Reset to count as a real loss, the app recovers
    /// by itself: it asks for the permission again and replays Start and the active grade.
    #[test]
    fn the_permission_is_reclaimed_and_the_grade_replayed_after_a_reset() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_secs(3);
        let outs = s.on_machine_status(MachineStatus::Reset, at);
        assert!(outs.contains(&Out::ControlLost), "{outs:?}");

        let ask = at + Duration::from_secs(2);
        assert_eq!(writes(&s.poll(ask)), vec![vec![0x00]], "asks again");
        s.on_indication(&ok(0x00), ask);
        assert_eq!(
            writes(&s.poll(ask + Duration::from_millis(300))),
            vec![vec![0x07]],
            "Start is replayed"
        );
        s.on_indication(&ok(0x07), ask + Duration::from_millis(300));
        assert_eq!(
            writes(&s.poll(ask + Duration::from_millis(600))),
            vec![vec![0x11, 0x00, 0x00, 0xF4, 0x01, 0x28, 0x33]],
            "and so is the 5 % grade"
        );
        s.on_indication(&ok(0x11), ask + Duration::from_millis(600));

        // The chatter returns; it is ignored and a new grade still reaches the trainer.
        let mut at = ask + Duration::from_millis(900);
        s.set_simulation(sim(7.5), at);
        let mut wrote = Vec::new();
        for _ in 0..8 {
            s.on_machine_status(MachineStatus::Reset, at);
            wrote.extend(writes(&s.poll(at)));
            at += Duration::from_millis(250);
        }
        assert!(s.has_control(), "the permission is kept");
        assert!(
            wrote.iter().any(|b| b.first() == Some(&op::SET_SIMULATION)),
            "the new grade is written: {wrote:?}"
        );
    }

    /// A device that keeps repeating "stopped by the user" is describing its idle state; it
    /// must not latch the targets off for the rest of the ride.
    #[test]
    fn a_stop_status_storm_does_not_latch_the_targets_off() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_millis(1200);
        // the first one is the rider's own stop button: respected
        let outs = s.on_machine_status(MachineStatus::StoppedByUser, at);
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("állították le"))));
        // … a repeat 200 ms later is not: no warning, no permission loss, only a log line
        let at = at + Duration::from_millis(200);
        let outs = s.on_machine_status(MachineStatus::StoppedByUser, at);
        assert!(!outs.contains(&Out::ControlLost), "{outs:?}");
        assert!(
            !outs.iter().any(|o| matches!(o, Out::Notice(_))),
            "{outs:?}"
        );
        assert!(s.has_control());
        // the machine is started again (the ride is running) …
        assert_eq!(
            writes(&s.poll(at + Duration::from_millis(300))),
            vec![vec![0x07]]
        );
        s.on_indication(&ok(0x07), at + Duration::from_millis(300));
        // … and the grade follows instead of waiting for the keep-alive
        assert_eq!(
            writes(&s.poll(at + Duration::from_millis(600))),
            vec![vec![0x11, 0x00, 0x00, 0xF4, 0x01, 0x28, 0x33]],
            "the 5 % grade is re-sent after the restart"
        );
    }

    #[test]
    fn no_control_point_means_read_only() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        let caps = TrainerCaps {
            has_control_point: false,
            supports_sim: None,
            supports_power_target: None,
        };
        let outs = s.on_connected(caps, Limits::FALLBACK, t);
        assert!(outs
            .iter()
            .any(|o| matches!(o, Out::Notice(m) if m.contains("csak adatot"))));
        s.set_simulation(sim(3.0), t);
        s.start(t);
        s.set_target_power(150.0, t);
        assert!(s.poll(t + Duration::from_secs(1)).is_empty());
        assert_eq!(s.next_deadline(t), None);
    }

    #[test]
    fn erg_target_replaces_the_simulation_and_is_kept_alive() {
        let t = Instant::now();
        let (mut s, now) = running(t);
        let at = now + Duration::from_millis(500);
        s.set_target_power(180.0, at);
        assert_eq!(writes(&s.poll(at)), vec![vec![0x05, 0xB4, 0x00]]);
        s.on_indication(&ok(0x05), at);
        assert_eq!(
            writes(&s.poll(at + Duration::from_millis(5100))),
            vec![vec![0x05, 0xB4, 0x00]]
        );
        // NaN is ignored
        s.set_target_power(f64::NAN, at);
    }

    #[test]
    fn queue_never_grows_beyond_the_distinct_intents() {
        // under control (the queue only fills then), hammered without ever polling
        let (mut s, t) = running(Instant::now());
        let mut longest = 0;
        for i in 0..10_000 {
            s.set_simulation(sim(f64::from(i % 20)), t);
            s.set_target_power(f64::from(i % 400), t);
            s.start(t);
            longest = longest.max(s.queue.len());
            s.pause(t);
            longest = longest.max(s.queue.len());
        }
        assert!(longest >= 1, "the queue was exercised");
        assert!(longest <= 7, "{longest} queued intents");
        // a NaN target is not a target
        let before = s.target_power;
        s.set_target_power(f64::NAN, t);
        assert_eq!(s.target_power, before);
    }

    /// Polls every 100 ms for `secs`, returning the writes and the notices.
    fn drive(s: &mut ControlSession, from: Instant, secs: u64) -> (Vec<Vec<u8>>, Vec<String>) {
        let (mut sent, mut notices) = (Vec::new(), Vec::new());
        let mut at = from;
        while at < from + Duration::from_secs(secs) {
            let outs = s.poll(at);
            sent.extend(writes(&outs));
            notices.extend(outs.into_iter().filter_map(|o| match o {
                Out::Notice(m) => Some(m),
                _ => None,
            }));
            at += Duration::from_millis(100);
        }
        (sent, notices)
    }

    #[test]
    fn a_silent_trainer_gets_control_back_after_a_loss() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.start(t);
        s.set_target_power(150.0, t);
        drive(&mut s, t, 20);
        assert!(s.has_control() && s.silent, "driven without confirmations");
        let at = t + Duration::from_secs(20);
        s.on_machine_status(MachineStatus::ControlPermissionLost, at);
        assert!(!s.has_control());
        let (sent, _) = drive(&mut s, at, 20);
        assert!(
            s.has_control(),
            "control is assumed again, not stuck in Requesting"
        );
        assert!(
            sent.iter().any(|w| w.first() == Some(&0x05)),
            "the ERG target goes out again"
        );
    }

    #[test]
    fn a_failing_request_control_write_ends_in_refused() {
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        s.start(t);
        let mut at = t;
        let mut notices = Vec::new();
        let mut writes_seen = 0;
        for _ in 0..200 {
            let outs = s.poll(at);
            if !writes(&outs).is_empty() {
                writes_seen += 1;
                for o in s.on_write_failed(at) {
                    if let Out::Notice(m) = o {
                        notices.push(m);
                    }
                }
            }
            at += Duration::from_millis(100);
        }
        assert_eq!(s.phase, Phase::Refused);
        assert!(
            writes_seen <= 3,
            "{writes_seen} writes, not an endless loop"
        );
        assert_eq!(notices.len(), 1, "{notices:?}");
    }

    #[test]
    fn a_failed_pause_write_is_retried() {
        let (mut s, t) = running(Instant::now());
        let at = t + Duration::from_millis(300);
        s.pause(at);
        let first = writes(&s.poll(at));
        assert_eq!(first.len(), 1, "the neutral grade goes out first");
        s.on_write_failed(at);
        let mut now = at;
        let mut sent = Vec::new();
        for _ in 0..10 {
            now += Duration::from_millis(300);
            let w = writes(&s.poll(now));
            if let Some(op) = w.first().and_then(|w| w.first()).copied() {
                s.on_indication(&ok(op), now);
            }
            sent.extend(w);
        }
        assert!(
            sent.iter().any(|w| w.first() == Some(&0x11)),
            "neutral again"
        );
        assert!(sent.iter().any(|w| w == &vec![0x08, 0x02]), "and the pause");
    }

    #[test]
    fn a_pause_during_a_control_loss_is_carried_out_on_the_grant() {
        let (mut s, t) = running(Instant::now());
        let at = t + Duration::from_millis(300);
        s.on_machine_status(MachineStatus::ControlPermissionLost, at);
        s.pause(at);
        let mut now = at + Duration::from_secs(2);
        assert_eq!(writes(&s.poll(now)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), now);
        let mut sent = Vec::new();
        for _ in 0..10 {
            now += Duration::from_millis(300);
            let w = writes(&s.poll(now));
            if let Some(op) = w.first().and_then(|w| w.first()).copied() {
                s.on_indication(&ok(op), now);
            }
            sent.extend(w);
        }
        assert!(sent.iter().any(|w| w.first() == Some(&0x11)), "{sent:?}");
        assert!(sent.iter().any(|w| w == &vec![0x08, 0x02]), "{sent:?}");
        assert!(
            !sent.iter().any(|w| w == &vec![0x07]),
            "no Start while paused"
        );
    }

    #[test]
    fn a_control_loss_after_pause_preserves_the_relax_commands() {
        let (mut s, t) = running(Instant::now());
        let at = t + Duration::from_millis(300);
        s.pause(at);
        s.on_machine_status(MachineStatus::ControlPermissionLost, at);
        let mut now = at + Duration::from_secs(2);
        assert_eq!(writes(&s.poll(now)), vec![vec![0x00]]);
        s.on_indication(&ok(0x00), now);
        let mut sent = Vec::new();
        for _ in 0..10 {
            now += Duration::from_millis(300);
            let w = writes(&s.poll(now));
            if let Some(op) = w.first().and_then(|w| w.first()).copied() {
                s.on_indication(&ok(op), now);
            }
            sent.extend(w);
        }
        assert!(sent.iter().any(|w| w.first() == Some(&0x11)), "{sent:?}");
        assert!(sent.contains(&vec![0x08, 0x02]), "{sent:?}");
        assert!(!sent.contains(&vec![0x07]));
    }

    #[test]
    fn a_manual_retry_resets_the_reclaim_budget_and_refusals_are_always_told() {
        let (mut s, t) = running(Instant::now());
        let mut at = t;
        let mut refused_notices = 0;
        for _ in 0..10 {
            at += Duration::from_millis(100);
            for o in s.on_machine_status(MachineStatus::ControlPermissionLost, at) {
                if matches!(&o, Out::Notice(m) if m.contains("Többször")) {
                    refused_notices += 1;
                }
            }
        }
        assert_eq!(s.phase, Phase::Refused);
        assert_eq!(refused_notices, 1);
        s.start(at);
        assert_eq!(s.reclaims, 0);
        assert_eq!(s.phase, Phase::NeedControl);
    }
}
