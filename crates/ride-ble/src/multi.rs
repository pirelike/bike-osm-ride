//! One GATT connection, every role the device can serve.
//!
//! A modern smart trainer often exposes several services at once: FTMS (trainer data and
//! control), Cycling Power (a second, independent power/cadence source), CSC (speed/cadence)
//! and sometimes even a bridged Heart Rate service. Opening one connection per role is both
//! unreliable (the second connect usually fails) and pointless, so this session discovers
//! everything in one go:
//!
//! * it subscribes to every supported measurement and routes each stream to the merger
//!   source of the role the device claimed ([`Source::Trainer`], [`Source::PowerMeter`],
//!   [`Source::Csc`], [`Source::HrStrap`]);
//! * the FTMS Control Point is driven by the same [`ControlSession`] on the same link;
//! * the roles the hub asked for but the device does not offer are reported back with a
//!   Hungarian reason (the other roles stay connected).
//!
//! The session follows the old `trainer.rs` policy for the Control Point (subscribe first,
//! one outstanding procedure, keep-alive, neutral hand-back) and for gap-filling: when the
//! device does **not** own the power-meter slot (another meter is connected), its Cycling
//! Power data only fills gaps left by Indoor Bike Data instead of fighting it.

use std::time::Instant;

use futures::stream::{BoxStream, StreamExt};
use ride_model::DeviceKind;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::control::{ControlConfig, ControlSession, Limits, Out, TrainerCaps};
use crate::csc::CscDecoder;
use crate::error::{bounded, BleError};
use crate::hexlog::Dir;
use crate::hr::HrDecoder;
use crate::link::{stopped, Ctx, DeviceSession, SessionEnd};
use crate::merge::{Field, Source};
use crate::parsers::{
    parse_features, parse_inclination_range, parse_indoor_bike, parse_machine_status,
    parse_power_range, MAX_POWER_W,
};
use crate::power::PowerDecoder;
use crate::sensor::Decoder;
use crate::trainer::TrainerCmd;
use crate::transport::{short_id, GattChar, GattPeripheral, Notification};
use crate::zwift::{self, ControllerEvent, ControllerKind, ControllerSession};
use ride_i18n::{t, tf};

const FTMS: u16 = 0x1826;
const CPS: u16 = 0x1818;
const CSC: u16 = 0x1816;
const HR: u16 = 0x180D;
const IBD: u16 = 0x2AD2;
const CONTROL_POINT: u16 = 0x2AD9;
const MACHINE_STATUS: u16 = 0x2ADA;
const FEATURE: u16 = 0x2ACC;
const INCLINATION_RANGE: u16 = 0x2AD5;
const POWER_RANGE: u16 = 0x2AD8;
const CPS_MEASUREMENT: u16 = 0x2A63;
const CSC_MEASUREMENT: u16 = 0x2A5B;
const HR_MEASUREMENT: u16 = 0x2A37;

/// Why a wanted role cannot be served by this device (shown in the slot card).
fn missing_reason(kind: DeviceKind) -> String {
    let service = match kind {
        DeviceKind::Trainer => "FTMS (0x1826) / Indoor Bike Data",
        DeviceKind::PowerMeter => "Cycling Power (0x1818)",
        DeviceKind::Cadence => "Cycling Speed and Cadence (0x1816)",
        DeviceKind::HeartRate => "Heart Rate (0x180D)",
        DeviceKind::Shifter => "Zwift (00000001-19CA-4651-86E5-FA29DCDD09D1)",
    };
    tf(
        "ble.multi.nincs_service_szolgaltatas",
        &[("service", service)],
    )
}

/// Decoders and timing state of one connection.
struct RxState {
    ibd_power_at: Option<Instant>,
    ibd_cadence_at: Option<Instant>,
    cps: PowerDecoder,
    csc: CscDecoder,
    hr: HrDecoder,
    /// The Zwift controller session, when the device serves the Shifter role.
    controller: Option<ControllerSession>,
    /// A shift happened since the last haptic acknowledgement was written.
    haptic_due: bool,
}

impl RxState {
    fn new() -> Self {
        Self {
            ibd_power_at: None,
            ibd_cadence_at: None,
            cps: PowerDecoder::default(),
            csc: CscDecoder::default(),
            hr: HrDecoder,
            controller: None,
            haptic_due: false,
        }
    }

    fn reset(&mut self) {
        self.ibd_power_at = None;
        self.ibd_cadence_at = None;
        self.cps.reset();
        self.csc.reset();
        self.hr.reset();
    }
}

/// The combined session of one physical device.
pub(crate) struct MultiSession {
    /// `Some` only when the device serves the Trainer role.
    ctl_rx: Option<mpsc::UnboundedReceiver<TrainerCmd>>,
    control: ControlSession,
    /// Left or right Play controller / Click, from the advertisement (`None` = unknown).
    controller_side: Option<ControllerKind>,
}

impl MultiSession {
    pub fn new(
        ctl_rx: Option<mpsc::UnboundedReceiver<TrainerCmd>>,
        cfg: ControlConfig,
        controller_side: Option<ControllerKind>,
    ) -> Self {
        Self {
            ctl_rx,
            control: ControlSession::new(cfg),
            controller_side,
        }
    }

    fn apply(&mut self, cmd: TrainerCmd, now: Instant) {
        match cmd {
            TrainerCmd::Sim(s) => self.control.set_simulation(s, now),
            TrainerCmd::Power(w) => self.control.set_target_power(w, now),
            TrainerCmd::Start => self.control.start(now),
            TrainerCmd::Pause => self.control.pause(now),
        }
    }

    fn forward(&self, ctx: &Ctx, outs: Vec<Out>) {
        for out in outs {
            match out {
                Out::Notice(text) => ctx.notice(text),
                Out::ControlGranted => ctx.log.comment(t("ble.multi.ftms_vezerles_megszerezve")),
                Out::ControlLost => ctx.log.comment(t("ble.multi.ftms_vezerles_elveszett")),
                Out::StatusIgnored(what) => ctx.log.comment(what),
                Out::Write(_) => {}
            }
        }
    }

    /// Runs the control state machine once and performs the (at most one) write it asks for.
    async fn pump<P: GattPeripheral>(
        &mut self,
        p: &P,
        ctx: &Ctx,
        cp: Option<&GattChar>,
    ) -> Result<(), BleError> {
        let Some(cp) = cp else { return Ok(()) };
        let outs = self.control.poll(Instant::now());
        for out in outs {
            match out {
                Out::Write(bytes) => {
                    ctx.log.record(Dir::Write, &cp.uuid, &bytes);
                    if let Err(e) = bounded(
                        t("ble.multi.vezerloparancs"),
                        ctx.cfg.io_timeout,
                        p.write(cp, &bytes, true),
                    )
                    .await
                    {
                        self.control.on_write_failed(Instant::now());
                        ctx.log.comment(&tf(
                            "ble.multi.vezerloparancs_sikertelen_e",
                            &[("e", &e.to_string())],
                        ));
                        if e.is_link_lost() {
                            return Err(e);
                        }
                    }
                }
                other => self.forward(ctx, vec![other]),
            }
        }
        Ok(())
    }

    fn on_notification(&mut self, ctx: &Ctx, n: &Notification, rx: &mut RxState) {
        let now = Instant::now();
        match short_id(&n.uuid) {
            Some(IBD) => {
                ctx.log.record(Dir::Notify, &n.uuid, &n.value);
                match parse_indoor_bike(&n.value) {
                    Ok(bike) => {
                        let fields = bike.fields();
                        if fields.iter().any(|(f, _)| *f == Field::Power) {
                            rx.ibd_power_at = Some(now);
                        }
                        if fields.iter().any(|(f, _)| *f == Field::Cadence) {
                            rx.ibd_cadence_at = Some(now);
                        }
                        ctx.data(Source::Trainer, fields);
                    }
                    Err(e) => ctx.log.comment(&tf(
                        "ble.multi.indoor_bike_data_ertelmezhetetlen_e",
                        &[("e", &e.to_string())],
                    )),
                }
            }
            Some(CONTROL_POINT) => {
                ctx.log.record(Dir::Indicate, &n.uuid, &n.value);
                let outs = self.control.on_indication(&n.value, now);
                self.forward(ctx, outs);
            }
            Some(MACHINE_STATUS) => {
                ctx.log.record(Dir::Notify, &n.uuid, &n.value);
                if let Some(status) = parse_machine_status(&n.value) {
                    let outs = self.control.on_machine_status(status, now);
                    self.forward(ctx, outs);
                }
            }
            Some(CPS_MEASUREMENT) => {
                ctx.log.record(Dir::Notify, &n.uuid, &n.value);
                if let Ok(fields) = rx.cps.feed(&n.value, now) {
                    let owns_meter = ctx.serves(DeviceKind::PowerMeter);
                    let fresh = |t: Option<Instant>| {
                        t.is_some_and(|t| now.saturating_duration_since(t) < ctx.cfg.stale_after)
                    };
                    let mut out = Vec::with_capacity(fields.len());
                    for (field, value) in fields {
                        if owns_meter {
                            out.push((field, value));
                            continue;
                        }
                        // Another power meter is connected: this stream is only a fallback.
                        match field {
                            Field::Power if !fresh(rx.ibd_power_at) && value <= MAX_POWER_W => {
                                out.push((field, value));
                            }
                            Field::Cadence if !fresh(rx.ibd_cadence_at) => out.push((field, value)),
                            _ => {}
                        }
                    }
                    let source = if owns_meter {
                        Source::PowerMeter
                    } else {
                        Source::Trainer
                    };
                    ctx.data(source, out);
                }
            }
            Some(CSC_MEASUREMENT) => {
                ctx.log.record(Dir::Notify, &n.uuid, &n.value);
                if ctx.serves(DeviceKind::Cadence) {
                    if let Ok(fields) = rx.csc.feed(&n.value, now) {
                        ctx.data(Source::Csc, fields);
                    }
                }
            }
            Some(HR_MEASUREMENT) => {
                ctx.log.record(Dir::Notify, &n.uuid, &n.value);
                if ctx.serves(DeviceKind::HeartRate) {
                    if let Ok(fields) = rx.hr.feed(&n.value, now) {
                        ctx.data(Source::HrStrap, fields);
                    }
                }
            }
            _ if n.uuid == zwift::MEASUREMENT_UUID || n.uuid == zwift::RESPONSE_UUID => {
                Self::on_controller(ctx, n, rx, now);
            }
            _ => {}
        }
    }

    /// Feeds a Zwift controller packet to its session and turns shifts into hub events.
    fn on_controller(ctx: &Ctx, n: &Notification, rx: &mut RxState, now: Instant) {
        let dir = if n.uuid == zwift::RESPONSE_UUID {
            Dir::Indicate
        } else {
            Dir::Notify
        };
        ctx.log.record(dir, &n.uuid, &n.value);
        let Some(controller) = rx.controller.as_mut() else {
            return;
        };
        for event in controller.on_data(&n.value, now) {
            match event {
                ControllerEvent::ShiftUp => {
                    ctx.shift(1);
                    rx.haptic_due = true;
                }
                ControllerEvent::ShiftDown => {
                    ctx.shift(-1);
                    rx.haptic_due = true;
                }
                ControllerEvent::Ready { encrypted } => ctx.log.comment(if encrypted {
                    t("ble.multi.zwift_kontroller_titkositott_kapcsolat")
                } else {
                    t("ble.multi.zwift_kontroller_titkositatlan_kapcsolat")
                }),
                ControllerEvent::HandshakeFailed => ctx
                    .log
                    .comment(t("ble.multi.zwift_kontroller_a_parositas_nem_sikerult")),
            }
        }
    }

    /// Writes the haptic acknowledgement of the shifts seen since the last call (one per
    /// batch, not per press).
    async fn flush_haptic<P: GattPeripheral>(
        p: &P,
        ctx: &Ctx,
        ctl: Option<&GattChar>,
        rx: &mut RxState,
    ) -> Result<(), BleError> {
        if !std::mem::take(&mut rx.haptic_due) {
            return Ok(());
        }
        let (Some(ctl), Some(controller)) = (ctl, rx.controller.as_mut()) else {
            return Ok(());
        };
        let Some(packet) = controller.haptic_ack() else {
            return Ok(());
        };
        ctx.log.record(Dir::Write, &ctl.uuid, &packet);
        if let Err(e) = bounded(
            t("ble.multi.visszajelzes"),
            ctx.cfg.io_timeout,
            p.write(ctl, &packet, true),
        )
        .await
        {
            ctx.log.comment(&tf(
                "ble.multi.zwift_kontroller_a_rezgeses_visszajelzes_nem_sikerul",
                &[("e", &e.to_string())],
            ));
            if e.is_link_lost() {
                return Err(e);
            }
        }
        Ok(())
    }

    async fn read_optional<P: GattPeripheral>(
        p: &P,
        ctx: &Ctx,
        ch: Option<&GattChar>,
    ) -> Option<Vec<u8>> {
        let ch = ch.filter(|c| c.props.read)?;
        let value = bounded(t("ble.multi.olvasas"), ctx.cfg.read_timeout, p.read(ch))
            .await
            .ok()?;
        ctx.log.record(Dir::Read, &ch.uuid, &value);
        Some(value)
    }

    /// Hands the trainer back (neutral grade, stop) while still draining notifications so the
    /// Control Point responses can arrive; bounded by `shutdown_flush`.
    async fn hand_back<P: GattPeripheral>(
        &mut self,
        p: &P,
        ctx: &Ctx,
        cp: Option<&GattChar>,
        stream: &mut BoxStream<'static, Notification>,
        rx: &mut RxState,
    ) {
        self.control.shutdown(Instant::now());
        let deadline = tokio::time::Instant::now() + ctx.cfg.shutdown_flush;
        while !self.control.is_idle() && tokio::time::Instant::now() < deadline {
            if self.pump(p, ctx, cp).await.is_err() {
                break;
            }
            let wait = self
                .control
                .next_deadline(Instant::now())
                .map(|d| tokio::time::Instant::from_std(d).min(deadline))
                .unwrap_or(deadline);
            tokio::select! {
                n = stream.next() => match n {
                    Some(n) => self.on_notification(ctx, &n, rx),
                    None => break,
                },
                _ = tokio::time::sleep_until(wait) => {}
            }
        }
    }

    async fn run_connected<P: GattPeripheral>(
        &mut self,
        p: &P,
        ctx: &Ctx,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<SessionEnd, BleError> {
        let cfg = ctx.cfg.clone();
        bounded(t("ble.multi.kapcsolodas"), cfg.connect_timeout, p.connect()).await?;
        bounded(
            t("ble.multi.szolgaltatasok_felderitese"),
            cfg.io_timeout,
            p.discover_services(),
        )
        .await?;

        let chars = p.characteristics();
        let find = |service: u16, short: u16| {
            chars
                .iter()
                .find(|c| c.service_short() == Some(service) && c.short() == Some(short))
                .cloned()
        };
        let ibd = find(FTMS, IBD).filter(|c| c.pushes());
        let cp = find(FTMS, CONTROL_POINT)
            .filter(|c| c.props.write && (c.props.indicate || c.props.notify));
        let status = find(FTMS, MACHINE_STATUS).filter(|c| c.pushes());
        let cps = find(CPS, CPS_MEASUREMENT).filter(|c| c.pushes());
        let csc = find(CSC, CSC_MEASUREMENT).filter(|c| c.pushes());
        let hr = find(HR, HR_MEASUREMENT).filter(|c| c.pushes());
        // Zwift controller: notify + write are required; a Zwift Hub with FTMS is a trainer
        // only, so the Shifter role needs the absence of FTMS Indoor Bike Data.
        let find_zwift = |uuid: Uuid| {
            chars
                .iter()
                .find(|c| c.service == zwift::SERVICE_UUID && c.uuid == uuid)
                .cloned()
        };
        let zwift_notify = find_zwift(zwift::MEASUREMENT_UUID).filter(|c| c.pushes());
        let zwift_ctl = find_zwift(zwift::CONTROL_UUID).filter(|c| c.props.write);
        let zwift_response = find_zwift(zwift::RESPONSE_UUID).filter(|c| c.pushes());
        let shifter = ibd.is_none() && zwift_notify.is_some() && zwift_ctl.is_some();
        let zwift_notify = zwift_notify.filter(|_| shifter);
        let zwift_ctl = zwift_ctl.filter(|_| shifter);
        let zwift_response = zwift_response.filter(|_| shifter);

        if ibd.is_none() && cps.is_none() && csc.is_none() && hr.is_none() && !shifter {
            return Err(BleError::MissingService(
                "Fitness Machine (FTMS), Cycling Power, CSC, Heart Rate vagy Zwift kontroller"
                    .into(),
            ));
        }

        // What this device can really serve, and what was asked for but is missing.
        let mut found = Vec::new();
        if shifter {
            found.push(DeviceKind::Shifter);
        }
        if ibd.is_some() {
            found.push(DeviceKind::Trainer);
        }
        if cps.is_some() {
            found.push(DeviceKind::PowerMeter);
        }
        if csc.is_some() {
            found.push(DeviceKind::Cadence);
        }
        if hr.is_some() {
            found.push(DeviceKind::HeartRate);
        }
        let missing = ctx
            .roles()
            .into_iter()
            .filter(|k| !found.contains(k))
            .map(|k| (k, missing_reason(k)))
            .collect();
        ctx.set_roles(&found, missing);

        // Capabilities (all optional: a failed read means "assume the common case").
        let features = match ibd.as_ref() {
            Some(_) => Self::read_optional(p, ctx, find(FTMS, FEATURE).as_ref())
                .await
                .and_then(|b| parse_features(&b)),
            None => None,
        };
        let inclination = match ibd.as_ref() {
            Some(_) => Self::read_optional(p, ctx, find(FTMS, INCLINATION_RANGE).as_ref())
                .await
                .and_then(|b| parse_inclination_range(&b)),
            None => None,
        };
        let power = match ibd.as_ref() {
            Some(_) => Self::read_optional(p, ctx, find(FTMS, POWER_RANGE).as_ref())
                .await
                .and_then(|b| parse_power_range(&b)),
            None => None,
        };

        // Stream first, then subscribe; the Control Point indication must be enabled before
        // the first write or the trainer answers with "CCCD improperly configured".
        let mut stream = bounded(
            t("ble.multi.ertesitesek"),
            cfg.io_timeout,
            p.notifications(),
        )
        .await?;
        let mut cp = cp;
        if let Some(c) = &cp {
            if let Err(e) =
                bounded(t("ble.multi.feliratkozas"), cfg.io_timeout, p.subscribe(c)).await
            {
                if e.is_link_lost() {
                    return Err(e);
                }
                ctx.log.comment(&tf(
                    "ble.multi.control_point_feliratkozas_sikertelen_e",
                    &[("e", &e.to_string())],
                ));
                cp = None;
            }
        }
        if let Some(s) = &status {
            if let Err(e) =
                bounded(t("ble.multi.feliratkozas"), cfg.io_timeout, p.subscribe(s)).await
            {
                if e.is_link_lost() {
                    return Err(e);
                }
            }
        }
        if let Some(c) = &ibd {
            bounded(t("ble.multi.feliratkozas"), cfg.io_timeout, p.subscribe(c)).await?;
        }
        for c in [
            cps.as_ref(),
            csc.as_ref(),
            hr.as_ref(),
            zwift_notify.as_ref(),
            zwift_response.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Err(e) =
                bounded(t("ble.multi.feliratkozas"), cfg.io_timeout, p.subscribe(c)).await
            {
                if e.is_link_lost() {
                    return Err(e);
                }
                ctx.notice(tf(
                    "ble.multi.egy_szolgaltatasra_nem_sikerult_feliratkozni_e",
                    &[("label", &ctx.name), ("e", &e.to_string())],
                ));
            }
        }
        ctx.connected();

        let caps = TrainerCaps {
            has_control_point: cp.is_some(),
            supports_sim: features.map(|f| f.supports_simulation()),
            supports_power_target: features.map(|f| f.supports_power_target()),
        };
        let limits = Limits::from_ranges(inclination, power);
        let outs = self.control.on_connected(caps, limits, Instant::now());
        self.forward(ctx, outs);

        let mut rx = RxState::new();
        rx.reset();
        if let Some(ctl) = zwift_ctl.as_ref() {
            let mut controller = ControllerSession::new(self.controller_side);
            let hello = controller.start();
            rx.controller = Some(controller);
            ctx.log.record(Dir::Write, &ctl.uuid, &hello);
            if let Err(e) = bounded(
                t("ble.multi.kezfogas"),
                cfg.io_timeout,
                p.write(ctl, &hello, true),
            )
            .await
            {
                if e.is_link_lost() {
                    return Err(e);
                }
                ctx.log.comment(&tf(
                    "ble.multi.zwift_kontroller_a_parositas_nem_sikerult_e",
                    &[("e", &e.to_string())],
                ));
            }
        }
        let mut ctl_rx = self.ctl_rx.take();
        let mut health = tokio::time::interval_at(
            tokio::time::Instant::now() + cfg.health_interval,
            cfg.health_interval,
        );
        let mut idle =
            tokio::time::interval_at(tokio::time::Instant::now() + cfg.idle_tick, cfg.idle_tick);
        let mut last_rx = Instant::now();
        let trainer_like = ibd.is_some();
        // A controller that only sends key-pad packets may stay silent for long; its link is
        // still checked by `is_connected`, so it gets no data watchdog.
        let controller_only = !found.is_empty() && found.iter().all(|k| *k == DeviceKind::Shifter);
        loop {
            self.pump(p, ctx, cp.as_ref()).await?;
            Self::flush_haptic(p, ctx, zwift_ctl.as_ref(), &mut rx).await?;
            let idle_wait = tokio::time::Instant::now() + cfg.health_interval;
            let wake = self
                .control
                .next_deadline(Instant::now())
                .map(tokio::time::Instant::from_std)
                .map_or(idle_wait, |d| d.min(idle_wait));
            tokio::select! {
                biased;
                _ = stopped(stop) => {
                    self.hand_back(p, ctx, cp.as_ref(), &mut stream, &mut rx).await;
                    for c in [&ibd, &cp, &status, &cps, &csc, &hr, &zwift_notify, &zwift_response]
                        .into_iter()
                        .flatten()
                    {
                        let _ = bounded(t("ble.multi.leiratkozas"), cfg.io_timeout, p.unsubscribe(c)).await;
                    }
                    self.ctl_rx = ctl_rx;
                    return Ok(SessionEnd::Stopped);
                }
                cmd = recv_cmd(&mut ctl_rx) => match cmd {
                    Some(c) => self.apply(c, Instant::now()),
                    None => {
                        // the hub went away: leave the trainer in a neutral state
                        self.hand_back(p, ctx, cp.as_ref(), &mut stream, &mut rx).await;
                        return Ok(SessionEnd::Stopped);
                    }
                },
                n = stream.next() => match n {
                    Some(n) => {
                        last_rx = Instant::now();
                        self.on_notification(ctx, &n, &mut rx);
                    }
                    None => return Ok(SessionEnd::LinkLost(t("ble.multi.az_ertesitesek_folyama_megszunt").into())),
                },
                _ = idle.tick() => {
                    if ctx.serves(DeviceKind::Cadence) {
                        let values = rx.csc.idle(Instant::now());
                        ctx.data(Source::Csc, values);
                    }
                    if ctx.serves(DeviceKind::PowerMeter) {
                        let values = rx.cps.idle(Instant::now());
                        ctx.data(Source::PowerMeter, values);
                    }
                }
                _ = tokio::time::sleep_until(wake) => {}
                _ = health.tick() => {
                    match bounded(t("ble.multi.kapcsolat_ellenorzes"), cfg.io_timeout, p.is_connected()).await {
                        Ok(true) => {}
                        Ok(false) | Err(_) => return Ok(SessionEnd::LinkLost("a kapcsolat megszakadt".into())),
                    }
                    if !controller_only {
                        let window = if trainer_like { cfg.trainer_watchdog } else { cfg.sensor_watchdog };
                        if last_rx.elapsed() > window {
                            return Ok(SessionEnd::LinkLost(t("ble.multi.nem_erkezik_adat_az_eszkozrol").into()));
                        }
                    }
                }
            }
        }
    }
}

/// Awaits a trainer command, or forever when the device has no trainer role.
async fn recv_cmd(rx: &mut Option<mpsc::UnboundedReceiver<TrainerCmd>>) -> Option<TrainerCmd> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

impl<P: GattPeripheral> DeviceSession<P> for MultiSession {
    async fn run(
        &mut self,
        p: &P,
        ctx: &Ctx,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<SessionEnd, BleError> {
        let result = self.run_connected(p, ctx, stop).await;
        self.control.on_disconnected();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::kind_label;

    #[test]
    fn missing_roles_get_a_hungarian_reason() {
        for kind in [
            DeviceKind::Trainer,
            DeviceKind::PowerMeter,
            DeviceKind::Cadence,
            DeviceKind::HeartRate,
        ] {
            let reason = missing_reason(kind);
            assert!(reason.starts_with("nincs "), "{reason}");
            assert!(reason.contains("0x18"), "{reason}");
            assert!(reason.contains("szolgáltatás"), "{reason}");
        }
        assert!(missing_reason(DeviceKind::HeartRate).contains("0x180D"));
        let shifter = missing_reason(DeviceKind::Shifter);
        assert!(shifter.starts_with("nincs Zwift"), "{shifter}");
        assert!(shifter.contains("00000001-19CA"), "{shifter}");
        let _ = kind_label(DeviceKind::Trainer);
    }
}
