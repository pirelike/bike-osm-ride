//! The Click / Play controller as a pure state machine: no I/O, no clock of its own.
//!
//! The caller writes [`ControllerSession::start`] to the control point, feeds every
//! notification / indication into [`ControllerSession::on_data`] together with the current
//! time, and writes whatever [`ControllerSession::haptic_ack`] returns after a shift.
//!
//! Button mapping (the single place to change it is [`play_shift_buttons`]):
//!
//! | Controller | Shift up (harder) | Shift down (easier) |
//! |------------|-------------------|---------------------|
//! | Zwift Click | `Button_Plus` | `Button_Minus` |
//! | Zwift Play, right | `Shift` or `Y_Up` | — |
//! | Zwift Play, left | — | `Shift` or `B_Down` |
//!
//! A Play packet is only used when the advertised side is known; the side is not inferred
//! from the packet.

use std::time::{Duration, Instant};

use super::crypto::{ClickCrypto, Encryption};
use super::proto::{self, opcode, ButtonState, ClickKeyPadStatus, PlayKeyPadStatus};
use super::ControllerKind;

/// A button press is accepted at most once in this window (per button).
pub const SHIFT_DEBOUNCE: Duration = Duration::from_millis(180);

/// Haptic pattern sent as acknowledgement of a shift.
pub const HAPTIC_PATTERN: u32 = 2;

/// What the controller reports to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerEvent {
    /// One gear harder.
    ShiftUp,
    /// One gear easier.
    ShiftDown,
    /// The device answered the hello; `encrypted` tells whether packets are sealed.
    Ready { encrypted: bool },
    /// The device answered with something that is not a usable key exchange.
    HandshakeFailed,
}

/// Which virtual-shifter buttons a Play controller's report presses, as `(up, down)`.
pub fn play_shift_buttons(side: ControllerKind, status: &PlayKeyPadStatus) -> (bool, bool) {
    let shift = ButtonState::is_pressed(status.shift);
    match side {
        ControllerKind::PlayRight => (shift || ButtonState::is_pressed(status.y_up), false),
        ControllerKind::PlayLeft => (false, shift || ButtonState::is_pressed(status.b_down)),
        // a Click never sends the Play report; nothing to map
        ControllerKind::Click => (false, false),
    }
}

/// One button: its last level and when it last produced a shift.
#[derive(Debug, Clone, Copy, Default)]
struct Edge {
    pressed: bool,
    accepted_at: Option<Instant>,
}

impl Edge {
    /// `true` for a press (released → pressed) that is outside the debounce window.
    /// A press inside the window is swallowed, and holding a button never repeats.
    fn accept(&mut self, pressed: bool, now: Instant, window: Duration) -> bool {
        let rising = pressed && !self.pressed;
        self.pressed = pressed;
        if !rising {
            return false;
        }
        if let Some(last) = self.accepted_at {
            if now.saturating_duration_since(last) < window {
                return false;
            }
        }
        self.accepted_at = Some(now);
        true
    }
}

/// Turns button levels into one shift per press, with a per-button debounce window.
#[derive(Debug, Clone)]
pub struct ShiftDebouncer {
    window: Duration,
    up: Edge,
    down: Edge,
}

impl Default for ShiftDebouncer {
    fn default() -> Self {
        Self::new(SHIFT_DEBOUNCE)
    }
}

impl ShiftDebouncer {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            up: Edge::default(),
            down: Edge::default(),
        }
    }

    /// Feeds the current level of both buttons and returns the shifts it produces.
    pub fn process(
        &mut self,
        up_pressed: bool,
        down_pressed: bool,
        now: Instant,
    ) -> Vec<ControllerEvent> {
        let mut events = Vec::new();
        if self.up.accept(up_pressed, now, self.window) {
            events.push(ControllerEvent::ShiftUp);
        }
        if self.down.accept(down_pressed, now, self.window) {
            events.push(ControllerEvent::ShiftDown);
        }
        events
    }
}

/// One connection to a Click or Play controller.
pub struct ControllerSession {
    side: Option<ControllerKind>,
    crypto: ClickCrypto,
    debounce: ShiftDebouncer,
}

impl ControllerSession {
    /// `side` is the controller type from the advertisement; `None` when it is unknown
    /// (Click packets still work, Play packets are ignored because their side is unknown).
    pub fn new(side: Option<ControllerKind>) -> Self {
        Self {
            side,
            crypto: ClickCrypto::new(),
            debounce: ShiftDebouncer::default(),
        }
    }

    /// The hello to write to the control point (starts a new key exchange).
    pub fn start(&mut self) -> Vec<u8> {
        self.crypto.hello_payload()
    }

    /// `true` once the device answered the hello.
    pub fn is_ready(&self) -> bool {
        self.crypto.is_ready()
    }

    /// Whether the session is encrypted, once known.
    pub fn encryption(&self) -> Option<Encryption> {
        self.crypto.encryption()
    }

    /// Handles one notification or indication. Never panics; bad input yields no events.
    pub fn on_data(&mut self, bytes: &[u8], now: Instant) -> Vec<ControllerEvent> {
        if bytes.starts_with(super::crypto::RIDE_ON) {
            return match self.crypto.on_device_hello(bytes) {
                Ok(mode) => vec![ControllerEvent::Ready {
                    encrypted: mode == Encryption::Encrypted,
                }],
                Err(_) => vec![ControllerEvent::HandshakeFailed],
            };
        }
        let Ok(plain) = self.crypto.decrypt(bytes) else {
            return Vec::new();
        };
        let Some((op, payload)) = proto::unwrap(&plain) else {
            return Vec::new();
        };
        match op {
            opcode::CLICK_KEYPAD => match ClickKeyPadStatus::decode(payload) {
                Ok(status) => self.debounce.process(
                    ButtonState::is_pressed(status.plus),
                    ButtonState::is_pressed(status.minus),
                    now,
                ),
                Err(_) => Vec::new(),
            },
            opcode::PLAY_KEYPAD => match (self.side, PlayKeyPadStatus::decode(payload)) {
                (Some(side), Ok(status)) => {
                    let (up, down) = play_shift_buttons(side, &status);
                    self.debounce.process(up, down, now)
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// The haptic acknowledgement of a shift, sealed when the session is encrypted.
    /// `None` before the handshake has finished.
    pub fn haptic_ack(&mut self) -> Option<Vec<u8>> {
        if !self.crypto.is_ready() {
            return None;
        }
        let packet = proto::wrap(opcode::HAPTIC, &proto::encode_haptic(HAPTIC_PATTERN));
        self.crypto.encrypt(&packet).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::super::crypto::{seal, tests::Device};
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    /// `opcode 0x37` packet with the Click buttons set.
    fn click(plus: Option<bool>, minus: Option<bool>) -> Vec<u8> {
        let mut w = proto::Writer::new();
        if let Some(on) = plus {
            w.uint32(1, if on { 0 } else { 1 });
        }
        if let Some(on) = minus {
            w.uint32(2, if on { 0 } else { 1 });
        }
        proto::wrap(opcode::CLICK_KEYPAD, &w.finish())
    }

    /// `opcode 0x07` packet with the given Play buttons (field numbers from `zwift.proto`).
    fn play(fields: &[(u32, bool)]) -> Vec<u8> {
        let mut w = proto::Writer::new();
        for (field, on) in fields {
            w.uint32(*field, if *on { 0 } else { 1 });
        }
        proto::wrap(opcode::PLAY_KEYPAD, &w.finish())
    }

    fn count(events: &[ControllerEvent], wanted: ControllerEvent) -> usize {
        events.iter().filter(|e| **e == wanted).count()
    }

    /// A plain (unencrypted) Click session after the device answered `RideOn`.
    fn plain_click(now: Instant) -> ControllerSession {
        let mut s = ControllerSession::new(Some(ControllerKind::Click));
        let _ = s.start();
        let events = s.on_data(b"RideOn", now);
        assert_eq!(events, vec![ControllerEvent::Ready { encrypted: false }]);
        s
    }

    #[test]
    fn plain_click_press_and_release_is_exactly_one_shift_up() {
        let now = t0();
        let mut s = plain_click(now);
        let mut all = Vec::new();
        all.extend(s.on_data(&click(Some(true), None), now));
        all.extend(s.on_data(&click(Some(false), None), now + Duration::from_millis(60)));
        all.extend(s.on_data(&[opcode::IDLE_A, 0x00], now + Duration::from_secs(1)));
        assert_eq!(all, vec![ControllerEvent::ShiftUp]);
    }

    #[test]
    fn minus_button_shifts_down() {
        let now = t0();
        let mut s = plain_click(now);
        assert_eq!(
            s.on_data(&click(None, Some(true)), now),
            vec![ControllerEvent::ShiftDown]
        );
    }

    #[test]
    fn holding_a_button_never_repeats() {
        let now = t0();
        let mut s = plain_click(now);
        let mut events = Vec::new();
        for i in 0..40u64 {
            events.extend(s.on_data(
                &click(Some(true), None),
                now + Duration::from_millis(i * 50),
            ));
        }
        assert_eq!(count(&events, ControllerEvent::ShiftUp), 1);
    }

    #[test]
    fn debounce_window_is_180_ms_per_button() {
        let now = t0();
        let mut s = plain_click(now);
        // first press accepted
        assert_eq!(s.on_data(&click(Some(true), None), now).len(), 1);
        // release, then a press 100 ms after the accepted one: swallowed
        s.on_data(&click(Some(false), None), now + Duration::from_millis(20));
        assert!(s
            .on_data(&click(Some(true), None), now + Duration::from_millis(100))
            .is_empty());
        s.on_data(&click(Some(false), None), now + Duration::from_millis(120));
        // a press 200 ms after the accepted one: accepted again
        assert_eq!(
            s.on_data(&click(Some(true), None), now + Duration::from_millis(200)),
            vec![ControllerEvent::ShiftUp]
        );
    }

    #[test]
    fn the_two_buttons_debounce_independently() {
        let now = t0();
        let mut s = plain_click(now);
        assert_eq!(s.on_data(&click(Some(true), None), now).len(), 1);
        // minus pressed 10 ms later is not blocked by plus
        assert_eq!(
            s.on_data(
                &click(Some(true), Some(true)),
                now + Duration::from_millis(10)
            ),
            vec![ControllerEvent::ShiftDown]
        );
    }

    #[test]
    fn packets_before_the_handshake_are_ignored() {
        let now = t0();
        let mut s = ControllerSession::new(Some(ControllerKind::Click));
        assert!(!s.is_ready());
        assert!(s.on_data(&click(Some(true), None), now).is_empty());
        assert!(s.haptic_ack().is_none());
    }

    #[test]
    fn handshake_failure_is_reported() {
        let mut s = ControllerSession::new(None);
        let _ = s.start();
        assert_eq!(
            s.on_data(b"RideOn", t0()),
            vec![ControllerEvent::Ready { encrypted: false }]
        );
        let mut s = ControllerSession::new(None);
        let _ = s.start();
        let mut long = b"RideOn".to_vec();
        long.extend_from_slice(&[0; 66]);
        assert_eq!(
            s.on_data(&long, t0()),
            vec![ControllerEvent::HandshakeFailed]
        );
        assert!(!s.is_ready());
    }

    #[test]
    fn encrypted_session_with_the_in_test_device() {
        let now = t0();
        let device = Device::new();
        let mut s = ControllerSession::new(Some(ControllerKind::Click));
        let hello = s.start();
        assert_eq!(
            s.on_data(&device.hello(), now),
            vec![ControllerEvent::Ready { encrypted: true }]
        );
        assert_eq!(s.encryption(), Some(Encryption::Encrypted));

        let keys = device.keys(&hello);
        // device counts its own packets from 0: plus pressed, then released
        let press = seal(&keys, 0, &click(Some(true), None)).unwrap();
        let release = seal(&keys, 1, &click(Some(false), None)).unwrap();
        assert_eq!(s.on_data(&press, now), vec![ControllerEvent::ShiftUp]);
        assert!(s
            .on_data(&release, now + Duration::from_millis(40))
            .is_empty());

        // the haptic acknowledgement is sealed too and decodes back to the haptic opcode
        let ack = s.haptic_ack().expect("ready session");
        let plain = super::super::crypto::open(&keys, &ack).unwrap();
        assert_eq!(plain[0], opcode::HAPTIC);
        assert_eq!(plain, proto::wrap(opcode::HAPTIC, &proto::encode_haptic(2)));
    }

    #[test]
    fn encrypted_packet_with_a_bad_mic_is_ignored() {
        let now = t0();
        let device = Device::new();
        let mut s = ControllerSession::new(Some(ControllerKind::Click));
        let hello = s.start();
        s.on_data(&device.hello(), now);
        let keys = device.keys(&hello);
        let mut packet = seal(&keys, 0, &click(Some(true), None)).unwrap();
        packet[5] ^= 0x40;
        assert!(s.on_data(&packet, now).is_empty());
        // the good packet with the same counter still works afterwards
        let good = seal(&keys, 1, &click(Some(true), None)).unwrap();
        assert_eq!(s.on_data(&good, now), vec![ControllerEvent::ShiftUp]);
    }

    #[test]
    fn play_right_shift_and_y_up_shift_harder() {
        let now = t0();
        let mut s = ControllerSession::new(Some(ControllerKind::PlayRight));
        let _ = s.start();
        s.on_data(b"RideOn", now);
        // Shift (6) pressed
        assert_eq!(
            s.on_data(&play(&[(6, true)]), now),
            vec![ControllerEvent::ShiftUp]
        );
        // released, then Y_Up (2) pressed after the window
        s.on_data(&play(&[]), now + Duration::from_millis(300));
        assert_eq!(
            s.on_data(&play(&[(2, true)]), now + Duration::from_millis(600)),
            vec![ControllerEvent::ShiftUp]
        );
    }

    #[test]
    fn play_right_ignores_the_easier_buttons() {
        let now = t0();
        let mut s = ControllerSession::new(Some(ControllerKind::PlayRight));
        let _ = s.start();
        s.on_data(b"RideOn", now);
        assert!(s.on_data(&play(&[(5, true)]), now).is_empty(), "B_Down");
    }

    #[test]
    fn play_left_shift_and_b_down_shift_easier() {
        let now = t0();
        let mut s = ControllerSession::new(Some(ControllerKind::PlayLeft));
        let _ = s.start();
        s.on_data(b"RideOn", now);
        assert_eq!(
            s.on_data(&play(&[(5, true)]), now),
            vec![ControllerEvent::ShiftDown]
        );
        s.on_data(&play(&[]), now + Duration::from_millis(300));
        assert_eq!(
            s.on_data(&play(&[(6, true)]), now + Duration::from_millis(600)),
            vec![ControllerEvent::ShiftDown]
        );
        assert!(
            s.on_data(&play(&[(2, true)]), now + Duration::from_secs(2))
                .is_empty(),
            "Y_Up on the left controller"
        );
    }

    #[test]
    fn play_packets_need_a_known_side() {
        let now = t0();
        let mut s = ControllerSession::new(None);
        let _ = s.start();
        s.on_data(b"RideOn", now);
        assert!(s.on_data(&play(&[(6, true)]), now).is_empty());
        // Click packets still work without a known side
        assert_eq!(
            s.on_data(&click(Some(true), None), now),
            vec![ControllerEvent::ShiftUp]
        );
    }

    #[test]
    fn play_mapping_is_one_function() {
        let status = PlayKeyPadStatus {
            shift: Some(ButtonState::On),
            ..Default::default()
        };
        assert_eq!(
            play_shift_buttons(ControllerKind::PlayRight, &status),
            (true, false)
        );
        assert_eq!(
            play_shift_buttons(ControllerKind::PlayLeft, &status),
            (false, true)
        );
        assert_eq!(
            play_shift_buttons(ControllerKind::Click, &status),
            (false, false)
        );
    }

    #[test]
    fn debouncer_reports_edges_only() {
        let t = t0();
        let mut d = ShiftDebouncer::new(Duration::from_millis(180));
        assert_eq!(d.process(true, false, t), vec![ControllerEvent::ShiftUp]);
        assert!(d
            .process(true, false, t + Duration::from_millis(500))
            .is_empty());
        assert!(d
            .process(false, false, t + Duration::from_millis(600))
            .is_empty());
        // both buttons pressed together: one shift each
        assert_eq!(
            d.process(true, true, t + Duration::from_millis(700)),
            vec![ControllerEvent::ShiftUp, ControllerEvent::ShiftDown]
        );
    }

    #[test]
    fn garbage_never_panics() {
        let now = t0();
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        };
        let mut plain = ControllerSession::new(Some(ControllerKind::Click));
        let _ = plain.start();
        plain.on_data(b"RideOn", now);
        // a session whose keys do not match the device: every sealed packet fails to open
        let device = Device::new();
        let mut sealed = ControllerSession::new(None);
        let _ = sealed.start();
        sealed.on_data(&device.hello(), now);
        let mut idle = ControllerSession::new(Some(ControllerKind::PlayLeft));
        for _ in 0..20_000 {
            let len = usize::from(next()) % 40;
            let buf: Vec<u8> = (0..len).map(|_| next()).collect();
            let _ = plain.on_data(&buf, now);
            let _ = sealed.on_data(&buf, now);
            let _ = idle.on_data(&buf, now);
            // a plain Click packet whose first byte is an opcode
            let mut tagged = vec![opcode::CLICK_KEYPAD];
            tagged.extend_from_slice(&buf);
            let _ = plain.on_data(&tagged, now);
        }
        assert!(plain.haptic_ack().is_some());
    }
}
