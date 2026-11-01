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
        }
    }
}

/// A refused Start / grade / target write is retried this many times in a row.
const MAX_WRITE_RETRIES: u32 = 40;

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
    /// The trainer never answered Request Control with an indication: control is assumed and
    /// writes are not waited for (what other trainer apps do with such trainers).
    silent: bool,
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
            silent: false,
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
        self.silent = false;
        self.failed_streak = 0;
        self.closing = false;
        let mut outs = Vec::new();
        if caps.has_control_point {
            self.phase = Phase::NeedControl;
        } else {
            self.phase = Phase::Detached;
            outs.push(Out::Notice(
                "A trainer nem vezérelhető FTMS-en keresztül, ezért csak adatot olvasunk róla; az emelkedők nem hatnak az ellenállásra."
                    .to_string(),
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
        if self.phase == Phase::Refused {
            self.phase = Phase::NeedControl;
            self.control_attempts = 0;
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
        if self.phase == Phase::Controlled
            && (self.device_started || self.last_target_bytes.is_some())
        {
            self.push_unique(Intent::Neutral);
            self.push_unique(Intent::Pause);
        }
    }

    /// Hand the trainer back in a neutral state (before a deliberate disconnect / app exit).
    pub fn shutdown(&mut self, _now: Instant) {
        self.user_paused = true;
        self.wants_run = false;
        self.closing = true;
        self.queue.clear();
        if self.phase == Phase::Controlled
            && (self.device_started || self.last_target_bytes.is_some())
        {
            self.push_unique(Intent::Neutral);
            self.push_unique(Intent::Stop);
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
                    self.last_target_bytes = None;
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
                // a Reset (e.g. from the trainer's own UI) invalidates the control permission
                self.lose_control(now, false, &mut outs);
            }
            MachineStatus::StoppedByUser
            | MachineStatus::PausedByUser
            | MachineStatus::StoppedBySafetyKey => {
                if !self.user_paused {
                    self.device_started = false;
                    self.device_blocked = true;
                    let text = if status == MachineStatus::StoppedBySafetyKey {
                        "A trainer a biztonsági kulccsal leállt; ha újraindítod a tekerést, a vezérlés is folytatódik."
                    } else {
                        "A trainert magán a készüléken állították le; amíg újra nem indítod a tekerést, nem avatkozunk bele."
                    };
                    self.warn(now, text, &mut outs);
                }
            }
            MachineStatus::StartedOrResumed => {
                self.device_started = true;
                self.device_blocked = false;
            }
            MachineStatus::Other(_) => {}
        }
        outs
    }

    /// A Control Point write failed at the transport level (not a trainer rejection).
    pub fn on_write_failed(&mut self, now: Instant) {
        let Some(inflight) = self.inflight.take() else {
            return;
        };
        match inflight.intent {
            Intent::RequestControl => {
                self.phase = Phase::NeedControl;
                self.retry_at = Some(now + self.cfg.retry_delay);
            }
            // Busy trainers refuse writes now and then (ATT 0x11); try again after the write gap.
            Intent::Start | Intent::Sim | Intent::Target
                if self.failed_streak < MAX_WRITE_RETRIES =>
            {
                self.failed_streak += 1;
                if matches!(inflight.intent, Intent::Sim | Intent::Target) {
                    // the value never arrived: it must not count as sent
                    self.last_target_bytes = None;
                    self.last_target_sent = None;
                }
                if !self.queue.contains(&inflight.intent) {
                    self.queue.push_front(inflight.intent);
                }
            }
            _ => {}
        }
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
                if inf.intent == Intent::RequestControl
                    && self.control_attempts >= self.cfg.control_retries
                {
                    // Never an answer, not a refusal: drive the trainer without confirmations.
                    self.silent = true;
                    self.grant(&mut outs);
                } else if !self.silent {
                    self.warn(
                        now,
                        "A trainer nem válaszolt időben egy vezérlőparancsra.",
                        &mut outs,
                    );
                    if inf.intent == Intent::RequestControl {
                        self.control_failed(now, &mut outs);
                    }
                }
            } else {
                return outs;
            }
        }

        if self.phase == Phase::NeedControl
            && !self.closing
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
            consider(inf.sent_at + self.cfg.response_timeout);
        } else {
            let gap_end = self.last_write.map_or(now, |t| t + self.cfg.min_write_gap);
            if !self.queue.is_empty() {
                consider(gap_end.max(now));
            }
            if self.phase == Phase::NeedControl && !self.closing {
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
                    self.warn(now,
                        "A trainer nem támogatja a terepszimulációt, ezért az emelkedők nem hatnak az ellenállásra.",
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
                        "A trainer nem támogatja a célteljesítmény (ERG) módot.",
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
    fn grant(&mut self, outs: &mut Vec<Out>) {
        self.phase = Phase::Controlled;
        self.control_attempts = 0;
        self.retry_at = None;
        self.last_target_bytes = None;
        outs.push(Out::ControlGranted);
        if !self.user_paused && !self.device_blocked && (self.wants_run || self.mode != Mode::None)
        {
            self.push_unique(Intent::Start);
            self.ensure_target_intent();
        }
    }

    fn handle_control_response(&mut self, result: CpResult, now: Instant, outs: &mut Vec<Out>) {
        if result == CpResult::Success {
            self.silent = false;
            self.grant(outs);
            return;
        }
        if result == CpResult::ControlNotPermitted {
            self.warn(now,
                "A trainert most egy másik alkalmazás vezérli; zárd be, vagy bontsd benne a kapcsolatot a trainerrel.",
                outs,
            );
        }
        self.control_failed(now, outs);
    }

    fn control_failed(&mut self, now: Instant, outs: &mut Vec<Out>) {
        if self.control_attempts >= self.cfg.control_retries {
            self.phase = Phase::Refused;
            self.warn(
                now,
                "A trainer nem adta át a vezérlést, ezért csak adatot olvasunk róla.",
                outs,
            );
        } else {
            self.phase = Phase::NeedControl;
            self.retry_at = Some(now + self.cfg.retry_delay);
        }
    }

    fn lose_control(&mut self, now: Instant, hostile: bool, outs: &mut Vec<Out>) {
        self.queue.clear();
        self.inflight = None;
        self.device_started = false;
        self.last_target_bytes = None;
        outs.push(Out::ControlLost);
        if hostile {
            self.reclaims += 1;
        }
        if self.reclaims > self.cfg.reclaim_limit {
            self.phase = Phase::Refused;
            self.warn(now,
                "Többször is elvesztettük a trainer vezérlését (talán egy másik alkalmazás miatt), ezért nem próbálkozunk tovább.",
                outs,
            );
        } else {
            self.phase = Phase::NeedControl;
            self.control_attempts = 0;
            self.retry_at = Some(now + self.cfg.retry_delay);
            if hostile {
                self.warn(
                    now,
                    "Elvesztettük a trainer vezérlését, megpróbáljuk visszaszerezni.",
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
            CpResult::Success => {}
            CpResult::ControlNotPermitted => self.lose_control(now, true, outs),
            CpResult::OpCodeNotSupported => {
                if intent == Intent::Sim {
                    self.caps.supports_sim = Some(false);
                    self.warn(now,
                        "A trainer nem támogatja a terepszimulációt, ezért az emelkedők nem hatnak az ellenállásra.",
                        outs,
                    );
                } else {
                    self.caps.supports_power_target = Some(false);
                    self.warn(
                        now,
                        "A trainer nem támogatja a célteljesítmény (ERG) módot.",
                        outs,
                    );
                }
            }
            CpResult::InvalidParameter => {
                self.warn(
                    now,
                    "A trainer elutasította a megadott paramétereket.",
                    outs,
                );
            }
            CpResult::OperationFailed | CpResult::Other(_) => {
                // some trainers refuse targets until they are started: start once, then retry
                if !self.device_started && !self.retried_start_for_sim {
                    self.retried_start_for_sim = true;
                    self.push_unique(Intent::Start);
                    self.ensure_target_intent();
                } else {
                    self.warn(
                        now,
                        "A trainer nem tudta végrehajtani a vezérlőparancsot.",
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
        // never controlled: nothing to hand back, and no control is requested while closing
        let mut fresh = ControlSession::new(ControlConfig::default());
        fresh.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        fresh.shutdown(t);
        assert!(fresh.poll(t).is_empty());
        assert_eq!(fresh.next_deadline(t), None);
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
        let t = Instant::now();
        let mut s = ControlSession::new(ControlConfig::default());
        s.on_connected(TrainerCaps::UNKNOWN, Limits::FALLBACK, t);
        // hammer all inputs without ever polling
        for i in 0..10_000 {
            s.set_simulation(sim(f64::from(i % 20)), t);
            s.set_target_power(f64::from(i % 400), t);
            s.start(t);
            s.pause(t);
        }
        assert!(s.queue.len() <= 7, "{} queued intents", s.queue.len());
    }
}
