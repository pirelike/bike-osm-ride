//! Protobuf (proto2) wire format and the Zwift message subset this app uses.
//!
//! Only what the Click / Play controllers and the Zwift Hub need is modelled. The opcode is
//! the first byte of a plaintext packet and is *not* part of the protobuf payload.
//!
//! Decoding never panics: truncated input, varints longer than 64 bits and unknown wire types
//! all become a [`ProtoError`]. Unknown *fields* are skipped, as the protobuf rules require.
//!
//! Field layout follows the reverse-engineered `zwift.proto` of the public-domain
//! `virtual-cog` project (see `NOTICE.md`); Makinolo's notes describe the same messages.

use thiserror::Error;

/// Opcodes (first byte of a plaintext packet).
pub mod opcode {
    /// Zwift Hub info request (`{1: query id}`).
    pub const HUB_INFO: u8 = 0x00;
    /// Zwift Hub riding data (`{1 power, 2 cadence, 3 speed×100, 4 hr, …}`).
    pub const HUB_RIDING_DATA: u8 = 0x03;
    /// Zwift Hub trainer command (power target, simulation, physical parameters).
    pub const HUB_COMMAND: u8 = 0x04;
    /// Zwift Play key pad status.
    pub const PLAY_KEYPAD: u8 = 0x07;
    /// Haptic / vibration command (`PlayCommand`).
    pub const HAPTIC: u8 = 0x12;
    /// Idle keep-alive (ignored).
    pub const IDLE_A: u8 = 0x15;
    /// Idle keep-alive (ignored).
    pub const IDLE_B: u8 = 0x19;
    /// Zwift Click key pad status.
    pub const CLICK_KEYPAD: u8 = 0x37;
}

/// Protobuf wire types we understand. Groups (3, 4) are deprecated and rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
    Varint,
    Fixed64,
    LengthDelimited,
    Fixed32,
}

/// A malformed protobuf payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProtoError {
    #[error("a protobuf üzenet véget ért")]
    Truncated,
    #[error("a protobuf szám túl hosszú")]
    Overflow,
    #[error("ismeretlen protobuf mezőtípus")]
    InvalidWireType,
}

/// Reads protobuf fields from a byte slice without ever indexing out of bounds.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The next `(field number, wire type)`, or `None` at the end of the payload.
    pub fn read_key(&mut self) -> Result<Option<(u32, WireType)>, ProtoError> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let key = self.read_varint()?;
        let field = u32::try_from(key >> 3).map_err(|_| ProtoError::Overflow)?;
        if field == 0 {
            return Err(ProtoError::InvalidWireType);
        }
        let wire = match key & 0x7 {
            0 => WireType::Varint,
            1 => WireType::Fixed64,
            2 => WireType::LengthDelimited,
            5 => WireType::Fixed32,
            _ => return Err(ProtoError::InvalidWireType),
        };
        Ok(Some((field, wire)))
    }

    /// A base-128 varint of at most 10 bytes.
    pub fn read_varint(&mut self) -> Result<u64, ProtoError> {
        let mut value: u64 = 0;
        for shift in (0..64).step_by(7) {
            let byte = *self.data.get(self.pos).ok_or(ProtoError::Truncated)?;
            self.pos += 1;
            let bits = u64::from(byte & 0x7F);
            // the tenth byte may only carry the 64th bit
            if shift == 63 && bits > 1 {
                return Err(ProtoError::Overflow);
            }
            value |= bits << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ProtoError::Overflow)
    }

    /// A varint that must fit in 32 bits.
    pub fn read_u32(&mut self) -> Result<u32, ProtoError> {
        u32::try_from(self.read_varint()?).map_err(|_| ProtoError::Overflow)
    }

    /// A zigzag-encoded `sint32`.
    pub fn read_sint32(&mut self) -> Result<i32, ProtoError> {
        let n = self.read_u32()?;
        Ok(((n >> 1) as i32) ^ -((n & 1) as i32))
    }

    /// The bytes of a length-delimited field.
    pub fn read_bytes(&mut self) -> Result<&'a [u8], ProtoError> {
        let len = usize::try_from(self.read_varint()?).map_err(|_| ProtoError::Truncated)?;
        self.take(len)
    }

    /// Skips the value of a field whose number is unknown to the caller.
    pub fn skip(&mut self, wire: WireType) -> Result<(), ProtoError> {
        match wire {
            WireType::Varint => self.read_varint().map(|_| ()),
            WireType::Fixed64 => self.take(8).map(|_| ()),
            WireType::Fixed32 => self.take(4).map(|_| ()),
            WireType::LengthDelimited => self.read_bytes().map(|_| ()),
        }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ProtoError> {
        let end = self.pos.checked_add(len).ok_or(ProtoError::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(ProtoError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }
}

/// Builds a protobuf payload.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    fn varint(&mut self, mut value: u64) {
        while value > 0x7F {
            self.buf.push((value as u8 & 0x7F) | 0x80);
            value >>= 7;
        }
        self.buf.push(value as u8);
    }

    fn key(&mut self, field: u32, wire: u8) {
        self.varint(u64::from(field) << 3 | u64::from(wire));
    }

    pub fn uint32(&mut self, field: u32, value: u32) {
        self.key(field, 0);
        self.varint(u64::from(value));
    }

    pub fn sint32(&mut self, field: u32, value: i32) {
        self.key(field, 0);
        let zigzag = (value.wrapping_shl(1) ^ (value >> 31)) as u32;
        self.varint(u64::from(zigzag));
    }

    pub fn message(&mut self, field: u32, nested: &[u8]) {
        self.key(field, 2);
        self.varint(nested.len() as u64);
        self.buf.extend_from_slice(nested);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// `ON` is 0 (the button is pressed), `OFF` is 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    On,
    Off,
}

impl ButtonState {
    fn from_wire(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::On),
            1 => Some(Self::Off),
            _ => None,
        }
    }

    /// `true` for a pressed button; a missing field means "not pressed".
    pub fn is_pressed(state: Option<Self>) -> bool {
        state == Some(Self::On)
    }
}

/// Opcode `0x37`: Zwift Click buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClickKeyPadStatus {
    pub plus: Option<ButtonState>,
    pub minus: Option<ButtonState>,
}

impl ClickKeyPadStatus {
    pub fn decode(payload: &[u8]) -> Result<Self, ProtoError> {
        let mut r = Reader::new(payload);
        let mut out = Self::default();
        while let Some((field, wire)) = r.read_key()? {
            match (field, wire) {
                (1, WireType::Varint) => out.plus = ButtonState::from_wire(r.read_varint()?),
                (2, WireType::Varint) => out.minus = ButtonState::from_wire(r.read_varint()?),
                _ => r.skip(wire)?,
            }
        }
        Ok(out)
    }
}

/// Opcode `0x07`: Zwift Play controller buttons and analogue stick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayKeyPadStatus {
    pub right_pad: Option<ButtonState>,
    pub y_up: Option<ButtonState>,
    pub z_left: Option<ButtonState>,
    pub a_right: Option<ButtonState>,
    pub b_down: Option<ButtonState>,
    pub shift: Option<ButtonState>,
    pub on: Option<ButtonState>,
    pub analog_lr: Option<i32>,
    pub analog_ud: Option<i32>,
}

impl PlayKeyPadStatus {
    pub fn decode(payload: &[u8]) -> Result<Self, ProtoError> {
        let mut r = Reader::new(payload);
        let mut out = Self::default();
        while let Some((field, wire)) = r.read_key()? {
            match (field, wire) {
                (1, WireType::Varint) => out.right_pad = ButtonState::from_wire(r.read_varint()?),
                (2, WireType::Varint) => out.y_up = ButtonState::from_wire(r.read_varint()?),
                (3, WireType::Varint) => out.z_left = ButtonState::from_wire(r.read_varint()?),
                (4, WireType::Varint) => out.a_right = ButtonState::from_wire(r.read_varint()?),
                (5, WireType::Varint) => out.b_down = ButtonState::from_wire(r.read_varint()?),
                (6, WireType::Varint) => out.shift = ButtonState::from_wire(r.read_varint()?),
                (7, WireType::Varint) => out.on = ButtonState::from_wire(r.read_varint()?),
                (8, WireType::Varint) => out.analog_lr = Some(r.read_sint32()?),
                (9, WireType::Varint) => out.analog_ud = Some(r.read_sint32()?),
                _ => r.skip(wire)?,
            }
        }
        Ok(out)
    }
}

/// Opcode `0x03`: Zwift Hub riding data (the codec only; not wired to the trainer control).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HubRidingData {
    pub power: Option<u32>,
    pub cadence: Option<u32>,
    /// Speed in units of 1/100 km/h.
    pub speed_x100: Option<u32>,
    pub hr: Option<u32>,
    pub unknown1: Option<u32>,
    pub unknown2: Option<u32>,
}

impl HubRidingData {
    pub fn decode(payload: &[u8]) -> Result<Self, ProtoError> {
        let mut r = Reader::new(payload);
        let mut out = Self::default();
        while let Some((field, wire)) = r.read_key()? {
            match (field, wire) {
                (1, WireType::Varint) => out.power = Some(r.read_u32()?),
                (2, WireType::Varint) => out.cadence = Some(r.read_u32()?),
                (3, WireType::Varint) => out.speed_x100 = Some(r.read_u32()?),
                (4, WireType::Varint) => out.hr = Some(r.read_u32()?),
                (5, WireType::Varint) => out.unknown1 = Some(r.read_u32()?),
                (6, WireType::Varint) => out.unknown2 = Some(r.read_u32()?),
                _ => r.skip(wire)?,
            }
        }
        Ok(out)
    }
}

/// `SimulationParam` inside a [`HubCommand`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimulationParam {
    /// Wind in m/s × 100 (negative is a tailwind).
    pub wind: Option<i32>,
    /// Grade in percent × 100.
    pub incline_x100: Option<i32>,
    /// CdA × 10000 (Zwift default 5100).
    pub cwa: Option<u32>,
    /// Rolling resistance × 100000 (Zwift default 400).
    pub crr: Option<u32>,
}

impl SimulationParam {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        if let Some(v) = self.wind {
            w.sint32(1, v);
        }
        if let Some(v) = self.incline_x100 {
            w.sint32(2, v);
        }
        if let Some(v) = self.cwa {
            w.uint32(3, v);
        }
        if let Some(v) = self.crr {
            w.uint32(4, v);
        }
        w.finish()
    }
}

/// `PhysicalParam` inside a [`HubCommand`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicalParam {
    /// Virtual gear ratio × 10000.
    pub gear_ratio_x10000: Option<u32>,
    /// Bike weight in kg × 100.
    pub bike_weight_x100: Option<u32>,
    /// Rider weight in kg × 100.
    pub rider_weight_x100: Option<u32>,
}

impl PhysicalParam {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        if let Some(v) = self.gear_ratio_x10000 {
            w.uint32(2, v);
        }
        if let Some(v) = self.bike_weight_x100 {
            w.uint32(4, v);
        }
        if let Some(v) = self.rider_weight_x100 {
            w.uint32(5, v);
        }
        w.finish()
    }
}

/// Opcode `0x04`: Zwift Hub trainer command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HubCommand {
    pub power_target: Option<u32>,
    pub simulation: Option<SimulationParam>,
    pub physical: Option<PhysicalParam>,
}

impl HubCommand {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        if let Some(v) = self.power_target {
            w.uint32(3, v);
        }
        if let Some(sim) = &self.simulation {
            w.message(4, &sim.encode());
        }
        if let Some(phys) = &self.physical {
            w.message(5, &phys.encode());
        }
        w.finish()
    }
}

/// Opcode `0x00`: Zwift Hub info request (for example query 520 = gear ratio).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HubInfoRequest {
    pub query_id: Option<u32>,
}

impl HubInfoRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        if let Some(v) = self.query_id {
            w.uint32(1, v);
        }
        w.finish()
    }
}

/// Payload of opcode `0x12`: `PlayCommand{2: {1: {3: HapticPattern}}}`.
pub fn encode_haptic(pattern: u32) -> Vec<u8> {
    let mut params = Writer::new();
    params.uint32(3, pattern);
    let mut contents = Writer::new();
    contents.message(1, &params.finish());
    let mut command = Writer::new();
    command.message(2, &contents.finish());
    command.finish()
}

/// Prepends the opcode to a payload.
pub fn wrap(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(opcode);
    out.extend_from_slice(payload);
    out
}

/// Splits a plaintext packet into its opcode and payload. `None` for an empty packet.
pub fn unwrap(packet: &[u8]) -> Option<(u8, &[u8])> {
    packet.split_first().map(|(op, rest)| (*op, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a hex string (test helper; panics only on malformed test data).
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("test hex"))
            .collect()
    }

    #[test]
    fn varint_round_trip_and_limits() {
        for value in [
            0u64,
            1,
            127,
            128,
            300,
            16_384,
            u64::from(u32::MAX),
            u64::MAX,
        ] {
            let mut w = Writer::new();
            w.varint(value);
            let bytes = w.finish();
            let mut r = Reader::new(&bytes);
            assert_eq!(r.read_varint(), Ok(value));
            assert!(r.read_key().unwrap().is_none());
        }
        // 300 is the classic example: 0xAC 0x02
        assert_eq!(Reader::new(&[0xAC, 0x02]).read_varint(), Ok(300));
        // eleven continuation bytes are an overflow, not a panic
        assert_eq!(
            Reader::new(&[0x80; 11]).read_varint(),
            Err(ProtoError::Overflow)
        );
        // the tenth byte may carry bit 63 only
        let mut too_big = vec![0xFF; 9];
        too_big.push(0x02);
        assert_eq!(
            Reader::new(&too_big).read_varint(),
            Err(ProtoError::Overflow)
        );
        assert_eq!(
            Reader::new(&[0x80]).read_varint(),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn zigzag_matches_the_protobuf_spec() {
        // 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3
        let cases = [(0i32, 0u8), (-1, 1), (1, 2), (-2, 3)];
        for (value, encoded) in cases {
            let mut w = Writer::new();
            w.sint32(1, value);
            assert_eq!(w.finish(), vec![0x08, encoded]);
            let bytes = [0x08, encoded];
            let mut r = Reader::new(&bytes);
            let (field, wire) = r.read_key().unwrap().unwrap();
            assert_eq!((field, wire), (1, WireType::Varint));
            assert_eq!(r.read_sint32(), Ok(value));
        }
        let mut w = Writer::new();
        w.sint32(1, i32::MAX);
        w.sint32(1, i32::MIN);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        r.read_key().unwrap();
        assert_eq!(r.read_sint32(), Ok(i32::MAX));
        r.read_key().unwrap();
        assert_eq!(r.read_sint32(), Ok(i32::MIN));
    }

    #[test]
    fn hub_riding_golden_vector() {
        // Makinolo / virtual-cog golden packet: power 190, cadence 80, speed 829, hr 0
        let packet = hex("0308be01105018bd06200028e2ba01308beb01");
        assert_eq!(packet[0], opcode::HUB_RIDING_DATA);
        let data = HubRidingData::decode(&packet[1..]).unwrap();
        assert_eq!(data.power, Some(190));
        assert_eq!(data.cadence, Some(80));
        assert_eq!(data.speed_x100, Some(829));
        assert_eq!(data.hr, Some(0));
        // fields 5 and 6 are not decoded further, but must still be read as varints
        assert_eq!(data.unknown1, Some(23_906));
        assert_eq!(data.unknown2, Some(30_091));
    }

    #[test]
    fn hub_command_golden_vector() {
        // 04 | 2a 08 | 10 00 | 20 81 05 | 28 c4 3b  = HubCommand{Physical{gear 0, bike 641, rider 7620}}
        let cmd = HubCommand {
            physical: Some(PhysicalParam {
                gear_ratio_x10000: Some(0),
                bike_weight_x100: Some(641),
                rider_weight_x100: Some(7620),
            }),
            ..Default::default()
        };
        let packet = wrap(opcode::HUB_COMMAND, &cmd.encode());
        assert_eq!(packet, hex("042a08100020810528c43b"));
    }

    #[test]
    fn hub_command_simulation_defaults_encode() {
        let cmd = HubCommand {
            power_target: None,
            simulation: Some(SimulationParam {
                wind: Some(0),
                incline_x100: Some(-250),
                cwa: Some(5100),
                crr: Some(400),
            }),
            physical: None,
        };
        let decoded = {
            let payload = cmd.encode();
            let mut r = Reader::new(&payload);
            let (field, wire) = r.read_key().unwrap().unwrap();
            assert_eq!((field, wire), (4, WireType::LengthDelimited));
            let nested = r.read_bytes().unwrap();
            let mut sim = SimulationParam::default();
            let mut inner = Reader::new(nested);
            while let Some((f, w)) = inner.read_key().unwrap() {
                match (f, w) {
                    (1, WireType::Varint) => sim.wind = Some(inner.read_sint32().unwrap()),
                    (2, WireType::Varint) => sim.incline_x100 = Some(inner.read_sint32().unwrap()),
                    (3, WireType::Varint) => sim.cwa = Some(inner.read_u32().unwrap()),
                    (4, WireType::Varint) => sim.crr = Some(inner.read_u32().unwrap()),
                    _ => inner.skip(w).unwrap(),
                }
            }
            sim
        };
        assert_eq!(decoded, cmd.simulation.unwrap());
    }

    #[test]
    fn info_request_gear_probe() {
        let req = HubInfoRequest {
            query_id: Some(520),
        };
        // 00 | 08 88 04 (520 as a varint)
        assert_eq!(wrap(opcode::HUB_INFO, &req.encode()), hex("00088804"));
    }

    #[test]
    fn click_status_decodes_press_and_release() {
        // Button_Plus = ON (0)
        assert_eq!(
            ClickKeyPadStatus::decode(&[0x08, 0x00]).unwrap(),
            ClickKeyPadStatus {
                plus: Some(ButtonState::On),
                minus: None
            }
        );
        // Button_Plus = OFF (1), Button_Minus = ON
        let status = ClickKeyPadStatus::decode(&[0x08, 0x01, 0x10, 0x00]).unwrap();
        assert_eq!(status.plus, Some(ButtonState::Off));
        assert_eq!(status.minus, Some(ButtonState::On));
        assert!(ButtonState::is_pressed(status.minus));
        assert!(!ButtonState::is_pressed(status.plus));
        // an empty payload means nothing is pressed
        assert_eq!(
            ClickKeyPadStatus::decode(&[]).unwrap(),
            ClickKeyPadStatus::default()
        );
        // unknown enum value: no state
        assert_eq!(ClickKeyPadStatus::decode(&[0x08, 0x07]).unwrap().plus, None);
    }

    #[test]
    fn unknown_fields_are_skipped() {
        // field 9 (varint 5), field 10 (length-delimited "ab"), field 11 (fixed32), then plus=ON
        let payload = [
            0x48, 0x05, 0x52, 0x02, b'a', b'b', 0x5D, 1, 2, 3, 4, 0x08, 0x00,
        ];
        let status = ClickKeyPadStatus::decode(&payload).unwrap();
        assert_eq!(status.plus, Some(ButtonState::On));
        // fixed64 unknown field too
        let payload = [0x49, 1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(
            ClickKeyPadStatus::decode(&payload).unwrap(),
            ClickKeyPadStatus::default()
        );
    }

    #[test]
    fn play_status_maps_every_button_and_analogue_stick() {
        // Y_Up ON, Shift ON, Analog_LR = -1 (zigzag 1), Analog_UD = 1 (zigzag 2)
        let mut w = Writer::new();
        w.uint32(2, 0);
        w.uint32(6, 0);
        w.sint32(8, -1);
        w.sint32(9, 1);
        let status = PlayKeyPadStatus::decode(&w.finish()).unwrap();
        assert_eq!(status.y_up, Some(ButtonState::On));
        assert_eq!(status.shift, Some(ButtonState::On));
        assert_eq!(status.b_down, None);
        assert_eq!(status.analog_lr, Some(-1));
        assert_eq!(status.analog_ud, Some(1));
    }

    #[test]
    fn truncated_length_delimited_field_is_an_error() {
        // length says 5 bytes, only 1 follows
        assert_eq!(
            ClickKeyPadStatus::decode(&[0x52, 0x05, 0x01]),
            Err(ProtoError::Truncated)
        );
        // a length that does not fit usize arithmetic
        let huge = [
            0x52, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01,
        ];
        assert!(ClickKeyPadStatus::decode(&huge).is_err());
    }

    #[test]
    fn group_and_zero_field_wire_types_are_rejected() {
        assert_eq!(
            ClickKeyPadStatus::decode(&[0x0B]),
            Err(ProtoError::InvalidWireType),
            "start group"
        );
        assert_eq!(
            ClickKeyPadStatus::decode(&[0x00]),
            Err(ProtoError::InvalidWireType),
            "field 0"
        );
    }

    #[test]
    fn haptic_payload_nests_three_levels() {
        // 12 04 | 0a 02 | 18 02  = PlayCommand{2: {1: {3: 2}}} (the opcode 12 is added by `wrap`)
        let payload = encode_haptic(2);
        assert_eq!(payload, hex("12040a021802"));
        assert_eq!(wrap(opcode::HAPTIC, &payload)[0], 0x12);
    }

    #[test]
    fn packet_wrap_and_unwrap() {
        let packet = wrap(opcode::CLICK_KEYPAD, &[0x08, 0x00]);
        assert_eq!(unwrap(&packet), Some((0x37, &[0x08u8, 0x00][..])));
        assert_eq!(unwrap(&[]), None);
        assert_eq!(unwrap(&[0x15]), Some((0x15, &[][..])));
    }

    #[test]
    fn fuzz_exhaustive_short_inputs_never_panic() {
        // every byte string of length 0..=3 (16.8 M inputs), streamed through every decoder
        for len in 0..=3u32 {
            let count = 256usize.pow(len);
            for n in 0..count {
                let input: Vec<u8> = (0..len).map(|i| (n >> (8 * i)) as u8).collect();
                let _ = ClickKeyPadStatus::decode(&input);
                let _ = PlayKeyPadStatus::decode(&input);
                let _ = HubRidingData::decode(&input);
                let _ = unwrap(&input);
            }
        }
        // length 4 is 4.3 G inputs: the sampled test below covers it
    }

    #[test]
    fn fuzz_random_buffers_never_panic() {
        // deterministic LCG; lengths 0..=64 cover the 3- and 4-byte cases many times over
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        };
        for _ in 0..50_000 {
            let len = usize::from(next()) % 65;
            let buf: Vec<u8> = (0..len).map(|_| next()).collect();
            let _ = ClickKeyPadStatus::decode(&buf);
            let _ = PlayKeyPadStatus::decode(&buf);
            let _ = HubRidingData::decode(&buf);
        }
    }
}
