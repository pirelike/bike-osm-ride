//! Thin adapters from raw GATT payloads to the units the app uses.
//!
//! * FTMS (Indoor Bike Data, Feature, ranges, Control Point responses): the `ftms` crate.
//! * Cycling Power, CSC and Heart Rate Measurement: the vendored `cycling-ble` parsers
//!   (`crate::vendor::cycling_ble`).
//!
//! Nothing here panics on bad input; every function returns an error or `None`. The
//! `diff_tests` module cross-checks both libraries against byte vectors derived from the
//! Bluetooth specification.

use ftms::measurement::{
    decode_measurement, MeasurementField, MeasurementKind, MeasurementOptions,
    MeasurementResistanceFormat, RawMeasurement,
};
use ftms::{decode_control_response, decode_features, decode_range, RangeKind, RangeOptions};

use crate::merge::Field;
use crate::vendor::cycling_ble as cble;

// ───────────────────────────── plausibility limits ─────────────────────────────

/// Values outside these windows are glitches (or the "not measured" sentinel) and are dropped.
pub const MAX_POWER_W: f64 = 3000.0;
pub const MAX_CADENCE_RPM: f64 = 250.0;
pub const MIN_HR_BPM: f64 = 30.0;
pub const MAX_HR_BPM: f64 = 250.0;
pub const MAX_SPEED_MS: f64 = 40.0;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("rövid csomag")]
    Truncated,
    #[error("felesleges bájtok a csomag végén")]
    Trailing,
    #[error("érvénytelen csomag: {0}")]
    Invalid(&'static str),
}

impl From<cble::ParseError> for ParseError {
    fn from(_: cble::ParseError) -> Self {
        ParseError::Truncated
    }
}

fn within(value: f64, lo: f64, hi: f64) -> Option<f64> {
    (value.is_finite() && value >= lo && value <= hi).then_some(value)
}

// ───────────────────────────── FTMS Indoor Bike Data ─────────────────────────────

/// Flags bit 5: the Resistance Level field is present.
const IBD_RESISTANCE_FLAG: u32 = 1 << 5;

/// Width in which the Resistance Level field arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResistanceWidth {
    /// The packet carried no resistance field.
    #[default]
    Absent,
    /// One byte, unitless (Bluetooth SIG GSS `indoor_bike_data`).
    U8,
    /// Two bytes, signed, 0.1 resolution (what several trainers and `cycling-ble` use).
    S16,
}

/// Decoded Indoor Bike Data, in the app's units. Absent fields are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IndoorBike {
    pub speed_ms: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub power_w: Option<f64>,
    pub resistance: Option<f64>,
    pub hr_bpm: Option<f64>,
    pub energy_kcal: Option<f64>,
    pub distance_m: Option<f64>,
    pub resistance_width: ResistanceWidth,
}

impl IndoorBike {
    /// The plausible values as merger input. Negative idle noise in power becomes 0 W;
    /// speed/cadence/heart rate outside their windows are dropped.
    pub fn fields(&self) -> Vec<(Field, f64)> {
        let mut out = Vec::with_capacity(6);
        if let Some(p) = self.power_w {
            if p.is_finite() && p <= MAX_POWER_W {
                out.push((Field::Power, p.max(0.0)));
            }
        }
        if let Some(c) = self
            .cadence_rpm
            .and_then(|c| within(c, 0.0, MAX_CADENCE_RPM))
        {
            out.push((Field::Cadence, c));
        }
        if let Some(h) = self.hr_bpm.and_then(|h| within(h, MIN_HR_BPM, MAX_HR_BPM)) {
            out.push((Field::HeartRate, h));
        }
        if let Some(s) = self.speed_ms.and_then(|s| within(s, 0.0, MAX_SPEED_MS)) {
            out.push((Field::Speed, s));
        }
        if let Some(r) = self.resistance.filter(|r| r.is_finite()) {
            out.push((Field::Resistance, r));
        }
        if let Some(e) = self.energy_kcal.and_then(|e| within(e, 0.0, 100_000.0)) {
            out.push((Field::Energy, e));
        }
        out
    }
}

fn clean(m: &RawMeasurement) -> bool {
    !m.truncated && !m.trailing_bytes
}

/// Parse an Indoor Bike Data (0x2AD2) notification.
///
/// The Resistance Level field is the one place where sources disagree: the Bluetooth SIG
/// GSS defines it as `uint8`, the `ftms` crate defaults to that, but `cycling-ble`, the
/// OsmolControl notes and several trainers use a signed 16-bit value. Because the field
/// sits *before* the power field, guessing wrong would silently shift every later field.
/// The width is therefore decided by the packet length: the layout that consumes the
/// payload exactly wins (the two layouts differ by one byte, so at most one can fit).
///
/// Bits 13–15 are reserved; if a newer trainer sets them, the known fields (which come
/// first) are still returned and the unknown tail is ignored.
///
/// The same holds for a **complete** packet that carries more bytes than the flags define:
/// the layout is fixed by the flags, so every known field is exact and the tail is unknown.
/// Real firmware does this (a captured trainer sends 8 flagged bytes followed by 12 bytes of
/// its own), and rejecting the whole notification would throw away power, cadence and speed.
/// With a resistance field the tail could also be a shifted 16-bit layout, so that ambiguity
/// keeps its own error instead of guessing.
pub fn parse_indoor_bike(bytes: &[u8]) -> Result<IndoorBike, ParseError> {
    let narrow = MeasurementOptions::default(); // Uint8Whole
    let m8 = decode_measurement(MeasurementKind::IndoorBike, bytes, narrow)
        .map_err(|_| ParseError::Truncated)?;
    let has_resistance = m8.flags & IBD_RESISTANCE_FLAG != 0;
    let narrow_width = if has_resistance {
        ResistanceWidth::U8
    } else {
        ResistanceWidth::Absent
    };

    let (m, width) = if clean(&m8) {
        (m8, narrow_width)
    } else {
        let wide_pick = if has_resistance {
            let wide = MeasurementOptions {
                resistance_format: MeasurementResistanceFormat::Signed16Tenths,
                ..narrow
            };
            decode_measurement(MeasurementKind::IndoorBike, bytes, wide)
                .ok()
                .filter(clean)
                .map(|m16| (m16, ResistanceWidth::S16))
        } else {
            None
        };
        match wide_pick {
            Some(pick) => pick,
            None if m8.truncated => return Err(ParseError::Truncated),
            // A complete narrow parse with an unknown tail: keep the known fields.
            None if !has_resistance || m8.reserved_flags => (m8, narrow_width),
            None => return Err(ParseError::Trailing),
        }
    };

    let present = |f: MeasurementField| m.present & f.mask() != 0 && m.unavailable & f.mask() == 0;
    let raw = |f: MeasurementField| f64::from(m.values[f.index()]);
    Ok(IndoorBike {
        speed_ms: present(MeasurementField::Speed)
            .then(|| raw(MeasurementField::Speed) / 100.0 / 3.6),
        cadence_rpm: present(MeasurementField::Cadence)
            .then(|| raw(MeasurementField::Cadence) * 0.5),
        power_w: present(MeasurementField::Power).then(|| raw(MeasurementField::Power)),
        resistance: present(MeasurementField::Resistance).then(|| {
            let r = raw(MeasurementField::Resistance);
            if width == ResistanceWidth::S16 {
                r / 10.0
            } else {
                r
            }
        }),
        hr_bpm: present(MeasurementField::HeartRate).then(|| raw(MeasurementField::HeartRate)),
        energy_kcal: present(MeasurementField::Energy).then(|| raw(MeasurementField::Energy)),
        distance_m: present(MeasurementField::Distance).then(|| raw(MeasurementField::Distance)),
        resistance_width: width,
    })
}

// ───────────────────────── Cycling Power / CSC / Heart Rate ─────────────────────────

/// One (cumulative revolutions, last event time) pair from a power meter or CSC sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevSample {
    pub revs: u32,
    /// Raw 16-bit event time; unit 1/1024 s (CSC wheel/crank, CPS crank) or 1/2048 s (CPS wheel).
    pub time: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CyclingPower {
    pub power_w: i16,
    /// Left pedal share, only when the meter says the percentage refers to the left pedal.
    pub balance_left_pct: Option<f64>,
    pub crank: Option<RevSample>,
    pub wheel: Option<RevSample>,
}

pub fn parse_cycling_power(bytes: &[u8]) -> Result<CyclingPower, ParseError> {
    let m = cble::power::parse(bytes)?;
    Ok(CyclingPower {
        power_w: m.instantaneous_power_watts,
        balance_left_pct: m.pedal_power_balance.and_then(|b| {
            (b.reference == cble::power::PedalPowerBalanceReference::Left)
                .then_some(f64::from(b.percent))
        }),
        crank: m.crank_revolutions.map(|c| RevSample {
            revs: u32::from(c.cumulative_revolutions),
            time: c.last_event_time_raw,
        }),
        wheel: m.wheel_revolutions.map(|w| RevSample {
            revs: w.cumulative_revolutions,
            time: w.last_event_time_raw,
        }),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Csc {
    pub wheel: Option<RevSample>,
    pub crank: Option<RevSample>,
}

pub fn parse_csc(bytes: &[u8]) -> Result<Csc, ParseError> {
    let m = cble::csc::parse(bytes)?;
    Ok(Csc {
        wheel: m.wheel_revolutions.map(|w| RevSample {
            revs: w.cumulative_revolutions,
            time: w.last_event_time_raw,
        }),
        crank: m.crank_revolutions.map(|c| RevSample {
            revs: u32::from(c.cumulative_revolutions),
            time: c.last_event_time_raw,
        }),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartRate {
    pub bpm: u16,
    /// `None` = the strap has no contact detection; `Some(false)` = not touching skin.
    pub contact: Option<bool>,
}

pub fn parse_heart_rate(bytes: &[u8]) -> Result<HeartRate, ParseError> {
    let m = cble::heart_rate::parse(bytes)?;
    Ok(HeartRate {
        bpm: m.bpm,
        contact: m.sensor_contact_detected,
    })
}

// ───────────────────────── FTMS control point / status / capabilities ─────────────────────────

/// Result code of a Fitness Machine Control Point response (FTMS v1.0 table 4.24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpResult {
    Success,
    OpCodeNotSupported,
    InvalidParameter,
    OperationFailed,
    ControlNotPermitted,
    Other(u8),
}

impl CpResult {
    fn from_code(code: u8) -> Self {
        match code {
            0x01 => CpResult::Success,
            0x02 => CpResult::OpCodeNotSupported,
            0x03 => CpResult::InvalidParameter,
            0x04 => CpResult::OperationFailed,
            0x05 => CpResult::ControlNotPermitted,
            other => CpResult::Other(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpResponse {
    pub request_opcode: u8,
    pub result: CpResult,
}

/// Parse a Control Point indication (`80 <request opcode> <result> [params]`).
pub fn parse_cp_response(bytes: &[u8]) -> Option<CpResponse> {
    let r = decode_control_response(bytes).ok()?;
    Some(CpResponse {
        request_opcode: r.request_opcode,
        result: CpResult::from_code(r.result_code),
    })
}

/// What the Fitness Machine Status characteristic told us (only the cases we react to).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineStatus {
    Reset,
    StoppedByUser,
    PausedByUser,
    StoppedBySafetyKey,
    StartedOrResumed,
    ControlPermissionLost,
    /// Target/parameter change echoes and everything else.
    Other(u8),
}

/// Classify a Machine Status notification by its op code (FTMS v1.0 table 4.26). The
/// parameter layouts are deliberately not decoded — we only need the op code — which also
/// avoids the spec's own ambiguity about the resistance parameter width.
pub fn parse_machine_status(bytes: &[u8]) -> Option<MachineStatus> {
    let op = *bytes.first()?;
    Some(match op {
        0x01 => MachineStatus::Reset,
        0x02 => match bytes.get(1) {
            Some(2) => MachineStatus::PausedByUser,
            _ => MachineStatus::StoppedByUser,
        },
        0x03 => MachineStatus::StoppedBySafetyKey,
        0x04 => MachineStatus::StartedOrResumed,
        0xFF => MachineStatus::ControlPermissionLost,
        other => MachineStatus::Other(other),
    })
}

/// Fitness Machine Feature characteristic (0x2ACC): two 32-bit words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Features {
    pub machine: u32,
    pub target: u32,
}

impl Features {
    pub fn supports_inclination_target(&self) -> bool {
        self.target & (1 << 1) != 0
    }
    pub fn supports_power_target(&self) -> bool {
        self.target & (1 << 3) != 0
    }
    pub fn supports_simulation(&self) -> bool {
        self.target & (1 << 13) != 0
    }
}

pub fn parse_features(bytes: &[u8]) -> Option<Features> {
    // The spec says 8 octets; tolerate a longer value, never a shorter one.
    let eight = bytes.get(..8)?;
    let f = decode_features(eight).ok()?;
    Some(Features {
        machine: f.machine,
        target: f.target,
    })
}

/// `(min, max)` as two signed 16-bit values — the common prefix of the power and
/// inclination ranges, also used as a lenient fallback when the strict decoder refuses a
/// range (e.g. increment 0).
fn pair_i16(bytes: &[u8]) -> Option<(i16, i16)> {
    let b = bytes.get(..4)?;
    let min = i16::from_le_bytes([b[0], b[1]]);
    let max = i16::from_le_bytes([b[2], b[3]]);
    (min <= max).then_some((min, max))
}

/// Supported Inclination Range (0x2AD5) in percent: sint16 min/max, 0.1 % resolution.
pub fn parse_inclination_range(bytes: &[u8]) -> Option<(f64, f64)> {
    let raw = match decode_range(RangeKind::Inclination, bytes, RangeOptions::default()) {
        Ok(r) => (r.minimum as i16, r.maximum as i16),
        Err(_) => pair_i16(bytes)?,
    };
    Some((f64::from(raw.0) / 10.0, f64::from(raw.1) / 10.0))
}

/// Supported Power Range (0x2AD8) in watts: sint16 min/max, uint16 increment.
pub fn parse_power_range(bytes: &[u8]) -> Option<(i16, i16)> {
    match decode_range(RangeKind::Power, bytes, RangeOptions::default()) {
        Ok(r) => Some((r.minimum as i16, r.maximum as i16)),
        Err(_) => pair_i16(bytes),
    }
}

/// Supported Resistance Level Range (0x2AD6), decided by length: 3 bytes = three `uint8`
/// (Bluetooth SIG GSS), 6 bytes = sint16/sint16/uint16 with 0.1 resolution (FTMS test suite
/// p5 and real trainers such as the ThinkRider X2 Max).
pub fn parse_resistance_range(bytes: &[u8]) -> Option<(f64, f64)> {
    match bytes.len() {
        3 => {
            let (min, max) = (bytes[0], bytes[1]);
            (min <= max).then_some((f64::from(min), f64::from(max)))
        }
        6 => {
            let (min, max) = pair_i16(bytes)?;
            Some((f64::from(min) / 10.0, f64::from(max) / 10.0))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_x2_max_indoor_bike_frames() {
        // OsmolControl docs/devices/x2-max.md (MIT): real frames, fw V3.2.3
        let b = parse_indoor_bike(&[0x44, 0x00, 0x9b, 0x07, 0x7c, 0x00, 0x52, 0x00]).unwrap();
        assert!((b.speed_ms.unwrap() * 3.6 - 19.47).abs() < 1e-9);
        assert_eq!(b.cadence_rpm, Some(62.0));
        assert_eq!(b.power_w, Some(82.0));
        assert_eq!(b.resistance_width, ResistanceWidth::Absent);
        let idle = parse_indoor_bike(&[0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(idle.power_w, Some(0.0));
        assert_eq!(idle.cadence_rpm, Some(0.0));
        let hand = parse_indoor_bike(&[0x44, 0x00, 0xef, 0x01, 0x00, 0x00, 0x05, 0x00]).unwrap();
        assert!((hand.speed_ms.unwrap() * 3.6 - 4.95).abs() < 1e-9);
        assert_eq!(hand.power_w, Some(5.0));
    }

    #[test]
    fn more_data_bit_is_inverted() {
        // bit 0 = 1: no speed; bit 6: power
        let b = parse_indoor_bike(&[0x41, 0x00, 0xFA, 0x00]).unwrap();
        assert_eq!(b.speed_ms, None);
        assert_eq!(b.power_w, Some(250.0));
        // bit 0 = 0: speed present
        let b = parse_indoor_bike(&[0x00, 0x00, 0xC4, 0x09]).unwrap();
        assert!((b.speed_ms.unwrap() - 25.0 / 3.6).abs() < 1e-9);
    }

    #[test]
    fn resistance_width_is_resolved_by_length() {
        // flags: no speed (bit0), resistance (bit5), power (bit6)
        let u8_layout = [0x61, 0x00, 25, 0xC8, 0x00];
        let s16_layout = [0x61, 0x00, 0xFA, 0x00, 0xC8, 0x00];
        let a = parse_indoor_bike(&u8_layout).unwrap();
        assert_eq!(a.resistance, Some(25.0));
        assert_eq!(a.power_w, Some(200.0));
        assert_eq!(a.resistance_width, ResistanceWidth::U8);
        let b = parse_indoor_bike(&s16_layout).unwrap();
        assert_eq!(b.resistance, Some(25.0), "250 tenths");
        assert_eq!(b.power_w, Some(200.0));
        assert_eq!(b.resistance_width, ResistanceWidth::S16);
    }

    #[test]
    fn truncated_trailing_and_reserved() {
        assert_eq!(parse_indoor_bike(&[]), Err(ParseError::Truncated));
        assert_eq!(parse_indoor_bike(&[0x44]), Err(ParseError::Truncated));
        assert_eq!(
            parse_indoor_bike(&[0x44, 0x00, 0x00]),
            Err(ParseError::Truncated)
        );
        // trailing bytes without a resistance flag: the flagged fields are complete, so the
        // known part is kept and the unknown tail ignored
        let b = parse_indoor_bike(&[0x41, 0x00, 0xFA, 0x00, 0x99]).unwrap();
        assert_eq!(b.power_w, Some(250.0));
        assert_eq!(b.resistance_width, ResistanceWidth::Absent);
        // with a resistance field a tail is ambiguous (it may be the 16-bit layout), so it
        // stays an error rather than shifting the fields after it
        assert_eq!(
            parse_indoor_bike(&[0x61, 0x00, 25, 0xC8, 0x00, 0x99, 0x99]),
            Err(ParseError::Trailing)
        );
        // a newer trainer sets reserved bit 13 and appends an unknown field: keep the known part
        let b = parse_indoor_bike(&[0x41, 0x20, 0xFA, 0x00, 0xAA, 0xBB]).unwrap();
        assert_eq!(b.power_w, Some(250.0));
    }

    #[test]
    fn captured_trainer_frames_with_a_vendor_tail() {
        // Captured from a real FTMS trainer (2026-11-01, ble.log): flags 0x0044 = speed,
        // cadence and power, followed by 12 bytes the specification does not define. Before,
        // every one of these 271 frames was rejected as "trailing bytes" and the ride had no
        // FTMS speed, cadence or power at all.
        let riding = parse_indoor_bike(&hex("44000803aa0026006f06c200070130010e0f7401")).unwrap();
        assert!(
            (riding.speed_ms.unwrap() * 3.6 - 7.76).abs() < 1e-9,
            "7.76 km/h"
        );
        assert_eq!(riding.cadence_rpm, Some(85.0));
        assert_eq!(riding.power_w, Some(38.0));

        let idle = parse_indoor_bike(&hex("44000000000000000000feff00002e0100106801")).unwrap();
        assert_eq!(idle.speed_ms, Some(0.0));
        assert_eq!(idle.cadence_rpm, Some(0.0));
        assert_eq!(idle.power_w, Some(0.0));
        assert_eq!(idle.resistance_width, ResistanceWidth::Absent);
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn energy_not_available_sentinel_is_dropped() {
        // bit0 + bit8 (expended energy): total 0xFFFF, per hour 0xFFFF, per minute 0xFF
        let b = parse_indoor_bike(&[0x01, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
        assert_eq!(b.energy_kcal, None);
        let b = parse_indoor_bike(&[0x01, 0x01, 0xF4, 0x01, 0x58, 0x02, 0x0A]).unwrap();
        assert_eq!(b.energy_kcal, Some(500.0));
    }

    #[test]
    fn fields_apply_plausibility_window() {
        let b = IndoorBike {
            power_w: Some(-3.0),
            cadence_rpm: Some(400.0),
            hr_bpm: Some(0.0),
            speed_ms: Some(9.0),
            ..Default::default()
        };
        let f = b.fields();
        assert!(f.contains(&(Field::Power, 0.0)));
        assert!(f.contains(&(Field::Speed, 9.0)));
        assert!(
            !f.iter().any(|(k, _)| *k == Field::Cadence),
            "400 rpm is a glitch"
        );
        assert!(
            !f.iter().any(|(k, _)| *k == Field::HeartRate),
            "0 bpm = not measured"
        );
        let huge = IndoorBike {
            power_w: Some(9000.0),
            ..Default::default()
        };
        assert!(huge.fields().is_empty());
    }

    #[test]
    fn captured_cycling_power_frame() {
        // OsmolControl x2-max capture: power 83 W, wheel 492 @30291, crank 57 @59812
        let cp = parse_cycling_power(&[
            0x30, 0x00, 0x53, 0x00, 0xec, 0x01, 0x00, 0x00, 0x53, 0x76, 0x39, 0x00, 0xa4, 0xe9,
        ])
        .unwrap();
        assert_eq!(cp.power_w, 83);
        assert_eq!(
            cp.wheel,
            Some(RevSample {
                revs: 492,
                time: 30291
            })
        );
        assert_eq!(
            cp.crank,
            Some(RevSample {
                revs: 57,
                time: 59812
            })
        );
        assert_eq!(cp.balance_left_pct, None);
    }

    #[test]
    fn pedal_balance_only_with_left_reference() {
        let left = parse_cycling_power(&[0x03, 0x00, 0xC8, 0x00, 110]).unwrap();
        assert_eq!(left.balance_left_pct, Some(55.0));
        let unknown = parse_cycling_power(&[0x01, 0x00, 0xC8, 0x00, 110]).unwrap();
        assert_eq!(unknown.balance_left_pct, None);
    }

    #[test]
    fn csc_and_heart_rate() {
        let c = parse_csc(&[0x02, 0x58, 0x02, 0xA0, 0x0F]).unwrap();
        assert_eq!(
            c.crank,
            Some(RevSample {
                revs: 600,
                time: 4000
            })
        );
        assert_eq!(c.wheel, None);
        assert_eq!(parse_csc(&[0x02, 0x58]), Err(ParseError::Truncated));
        assert_eq!(
            parse_heart_rate(&[0x14, 72, 0x00, 0x02]).unwrap(),
            HeartRate {
                bpm: 72,
                contact: Some(false)
            }
        );
        assert_eq!(
            parse_heart_rate(&[0x00, 65]).unwrap(),
            HeartRate {
                bpm: 65,
                contact: None
            }
        );
        assert!(parse_heart_rate(&[]).is_err());
    }

    #[test]
    fn control_point_responses() {
        let r = parse_cp_response(&[0x80, 0x00, 0x01]).unwrap();
        assert_eq!(
            r,
            CpResponse {
                request_opcode: 0,
                result: CpResult::Success
            }
        );
        assert_eq!(
            parse_cp_response(&[0x80, 0x11, 0x05]).unwrap().result,
            CpResult::ControlNotPermitted
        );
        assert_eq!(
            parse_cp_response(&[0x80, 0x07, 0x04]).unwrap().result,
            CpResult::OperationFailed
        );
        assert_eq!(
            parse_cp_response(&[0x80, 0x11, 0x02]).unwrap().result,
            CpResult::OpCodeNotSupported
        );
        assert_eq!(
            parse_cp_response(&[0x80, 0x11, 0x03]).unwrap().result,
            CpResult::InvalidParameter
        );
        assert_eq!(
            parse_cp_response(&[0x80, 0x11, 0x09]).unwrap().result,
            CpResult::Other(9)
        );
        assert_eq!(parse_cp_response(&[0x80, 0x00]), None);
        assert_eq!(parse_cp_response(&[]), None);
        assert_eq!(
            parse_cp_response(&[0x11, 0x00, 0x01]),
            None,
            "not a response code"
        );
    }

    #[test]
    fn machine_status_classification() {
        assert_eq!(
            parse_machine_status(&[0xFF]),
            Some(MachineStatus::ControlPermissionLost)
        );
        assert_eq!(parse_machine_status(&[0x01]), Some(MachineStatus::Reset));
        assert_eq!(
            parse_machine_status(&[0x02, 0x02]),
            Some(MachineStatus::PausedByUser)
        );
        assert_eq!(
            parse_machine_status(&[0x02, 0x01]),
            Some(MachineStatus::StoppedByUser)
        );
        assert_eq!(
            parse_machine_status(&[0x03]),
            Some(MachineStatus::StoppedBySafetyKey)
        );
        assert_eq!(
            parse_machine_status(&[0x04]),
            Some(MachineStatus::StartedOrResumed)
        );
        assert_eq!(
            parse_machine_status(&[0x12, 0, 0, 0, 0, 0, 0]),
            Some(MachineStatus::Other(0x12))
        );
        assert_eq!(parse_machine_status(&[]), None);
    }

    #[test]
    fn features_and_ranges_from_the_x2_max_capture() {
        // docs/devices/x2-max.md: Feature 034000000ca00000, power 0000d0070100, resistance 0000c8000100
        let f = parse_features(&[0x03, 0x40, 0x00, 0x00, 0x0c, 0xa0, 0x00, 0x00]).unwrap();
        assert_eq!(f.machine, 0x0000_4003);
        assert_eq!(f.target, 0x0000_A00C);
        assert!(f.supports_simulation());
        assert!(f.supports_power_target());
        assert!(!f.supports_inclination_target());
        assert_eq!(
            parse_power_range(&[0x00, 0x00, 0xd0, 0x07, 0x01, 0x00]),
            Some((0, 2000))
        );
        assert_eq!(
            parse_resistance_range(&[0x00, 0x00, 0xc8, 0x00, 0x01, 0x00]),
            Some((0.0, 20.0))
        );
        assert_eq!(parse_resistance_range(&[0, 100, 1]), Some((0.0, 100.0)));
        assert_eq!(parse_resistance_range(&[1, 2]), None);
        assert_eq!(parse_features(&[1, 2, 3]), None);
    }

    #[test]
    fn inclination_range_is_in_percent() {
        // -10.0 % .. +20.0 %, increment 0.1
        let bytes = [0x9C, 0xFF, 0xC8, 0x00, 0x01, 0x00];
        assert_eq!(parse_inclination_range(&bytes), Some((-10.0, 20.0)));
        // increment 0 is refused by the strict decoder; the lenient fallback still works
        let zero_inc = [0x9C, 0xFF, 0xC8, 0x00, 0x00, 0x00];
        assert_eq!(parse_inclination_range(&zero_inc), Some((-10.0, 20.0)));
        assert_eq!(parse_inclination_range(&[1, 2]), None);
        // inverted range
        assert_eq!(
            parse_inclination_range(&[0xC8, 0x00, 0x9C, 0xFF, 1, 0]),
            None
        );
    }
}
