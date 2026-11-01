//! Differential and conformance tests for the BLE protocol codecs.
//!
//! The same wire bytes are decoded three independent ways — the `ftms` crate, the vendored
//! `cycling-ble` FTMS parser (`crate::vendor`) and our production adapters in `parsers.rs` —
//! and compared against hand-built vectors derived from the Bluetooth SIG specification.
//! Control Point encodings, the revolution/time-base arithmetic and the parser property
//! (never panics; every returned value stays in physical bounds) are pinned here as well.
//!
//! When one of these tests fails, the three sources disagree. Check the specification
//! first, then the two libraries, and only then touch production code.

use std::time::{Duration, Instant};

use ftms::measurement::{
    decode_measurement, MeasurementField, MeasurementKind, MeasurementOptions,
    MeasurementResistanceFormat, RawMeasurement,
};
use ftms::normalized::{normalize_control_request, NormalizedControlRequest};
use ftms::{decode_control_request, encode_control_request, ControlOptions, ControlRequest};

use crate::control::{
    encode_request_control, encode_simulation, encode_start, encode_stop_pause,
    encode_target_power, filter, op, quantise_sim, Limits, Verdict, WireSim, MAX_GRADE_PCT,
};
use crate::csc::{CscDecoder, RevTracker};
use crate::hr::HrDecoder;
use crate::merge::Field;
use crate::parsers::{
    parse_cp_response, parse_csc, parse_cycling_power, parse_features, parse_heart_rate,
    parse_inclination_range, parse_indoor_bike, parse_machine_status, parse_power_range,
    parse_resistance_range, IndoorBike, ParseError, ResistanceWidth, RevSample, MAX_CADENCE_RPM,
    MAX_HR_BPM, MAX_POWER_W, MAX_SPEED_MS, MIN_HR_BPM,
};
use crate::power::PowerDecoder;
use crate::sensor::Decoder;
use crate::vendor::cycling_ble as cble;
use crate::vendor::cycling_ble::{
    revolutions_per_minute, CP_WHEEL_EVENT_TIME_HZ, CRANK_EVENT_TIME_HZ, CRANK_REVOLUTIONS_WRAP_AT,
    CSC_WHEEL_EVENT_TIME_HZ, WHEEL_REVOLUTIONS_WRAP_AT,
};

// ───────────────────────────── comparison helpers ─────────────────────────────

#[track_caller]
fn assert_close_f64(actual: Option<f64>, expected: Option<f64>, what: &str) {
    match (actual, expected) {
        (None, None) => {}
        (Some(a), Some(e)) => assert!(
            (a - e).abs() <= 1e-9 * a.abs().max(e.abs()).max(1.0),
            "{what}: {a} != {e}"
        ),
        other => panic!("{what}: {other:?}"),
    }
}

#[track_caller]
fn assert_close_f32(actual: Option<f32>, expected: Option<f32>, what: &str) {
    match (actual, expected) {
        (None, None) => {}
        (Some(a), Some(e)) => assert!(
            (a - e).abs() <= 1e-5 * a.abs().max(e.abs()).max(1.0),
            "{what}: {a} != {e}"
        ),
        other => panic!("{what}: {other:?}"),
    }
}

// ───────────────── Indoor Bike Data (0x2AD2): spec vectors ─────────────────

/// Indoor Bike Data fields in wire units, in the order the SIG layout defines them.
///
/// The resistance level is the one field whose width vendors disagree about: the SIG GSS
/// uses `uint8`, while several trainers (and `cycling-ble`) use a signed 16-bit 0.1 level.
/// A vector therefore carries exactly one of the two variants.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct IbdWire {
    speed_001_kmh: Option<u16>,
    avg_speed_001_kmh: Option<u16>,
    cadence_half_rpm: Option<u16>,
    avg_cadence_half_rpm: Option<u16>,
    distance_m: Option<u32>,
    resistance_u8: Option<u8>,
    resistance_s16_tenths: Option<i16>,
    power_w: Option<i16>,
    avg_power_w: Option<i16>,
    /// Expended energy: total kcal, kcal/h, kcal/min.
    energy: Option<(u16, u16, u8)>,
    hr_bpm: Option<u8>,
    met_tenths: Option<u8>,
    elapsed_s: Option<u16>,
    remaining_s: Option<u16>,
}

impl IbdWire {
    fn width(&self) -> ResistanceWidth {
        match (self.resistance_u8, self.resistance_s16_tenths) {
            (Some(_), Some(_)) => panic!("one vector cannot carry both resistance widths"),
            (Some(_), None) => ResistanceWidth::U8,
            (None, Some(_)) => ResistanceWidth::S16,
            (None, None) => ResistanceWidth::Absent,
        }
    }

    fn options(&self) -> MeasurementOptions {
        MeasurementOptions {
            resistance_format: if self.width() == ResistanceWidth::S16 {
                MeasurementResistanceFormat::Signed16Tenths
            } else {
                MeasurementResistanceFormat::Uint8Whole
            },
            ..MeasurementOptions::default()
        }
    }

    /// The flag word: bit 0 is inverted (0 = instantaneous speed present).
    fn flags(&self) -> u16 {
        let mut f = 0u16;
        if self.speed_001_kmh.is_none() {
            f |= 1 << 0;
        }
        if self.avg_speed_001_kmh.is_some() {
            f |= 1 << 1;
        }
        if self.cadence_half_rpm.is_some() {
            f |= 1 << 2;
        }
        if self.avg_cadence_half_rpm.is_some() {
            f |= 1 << 3;
        }
        if self.distance_m.is_some() {
            f |= 1 << 4;
        }
        if self.resistance_u8.is_some() || self.resistance_s16_tenths.is_some() {
            f |= 1 << 5;
        }
        if self.power_w.is_some() {
            f |= 1 << 6;
        }
        if self.avg_power_w.is_some() {
            f |= 1 << 7;
        }
        if self.energy.is_some() {
            f |= 1 << 8;
        }
        if self.hr_bpm.is_some() {
            f |= 1 << 9;
        }
        if self.met_tenths.is_some() {
            f |= 1 << 10;
        }
        if self.elapsed_s.is_some() {
            f |= 1 << 11;
        }
        if self.remaining_s.is_some() {
            f |= 1 << 12;
        }
        f
    }

    fn encode(&self) -> Vec<u8> {
        let mut v = self.flags().to_le_bytes().to_vec();
        if let Some(x) = self.speed_001_kmh {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.avg_speed_001_kmh {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.cadence_half_rpm {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.avg_cadence_half_rpm {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.distance_m {
            v.extend_from_slice(&x.to_le_bytes()[..3]); // u24
        }
        if let Some(x) = self.resistance_u8 {
            v.push(x);
        }
        if let Some(x) = self.resistance_s16_tenths {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.power_w {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.avg_power_w {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some((total, per_hour, per_minute)) = self.energy {
            v.extend_from_slice(&total.to_le_bytes());
            v.extend_from_slice(&per_hour.to_le_bytes());
            v.push(per_minute);
        }
        if let Some(x) = self.hr_bpm {
            v.push(x);
        }
        if let Some(x) = self.met_tenths {
            v.push(x);
        }
        if let Some(x) = self.elapsed_s {
            v.extend_from_slice(&x.to_le_bytes());
        }
        if let Some(x) = self.remaining_s {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v
    }

    /// What the production adapter (`parsers::IndoorBike`) must return.
    fn expected(&self) -> IndoorBike {
        IndoorBike {
            speed_ms: self.speed_001_kmh.map(|v| f64::from(v) / 100.0 / 3.6),
            cadence_rpm: self.cadence_half_rpm.map(|v| f64::from(v) * 0.5),
            power_w: self.power_w.map(f64::from),
            resistance: match self.width() {
                ResistanceWidth::Absent => None,
                ResistanceWidth::U8 => self.resistance_u8.map(f64::from),
                ResistanceWidth::S16 => self.resistance_s16_tenths.map(|v| f64::from(v) / 10.0),
            },
            // 0xFFFF is the FTMS "not available" sentinel, not 65 535 kcal.
            hr_bpm: self.hr_bpm.map(f64::from),
            energy_kcal: self
                .energy
                .and_then(|(total, _, _)| (total != u16::MAX).then_some(f64::from(total))),
            distance_m: self.distance_m.map(f64::from),
            resistance_width: self.width(),
        }
    }
}

fn raw_value(m: &RawMeasurement, field: MeasurementField) -> Option<f64> {
    (m.present & field.mask() != 0 && m.unavailable & field.mask() == 0)
        .then(|| f64::from(m.values[field.index()]))
}

#[track_caller]
fn check_production(w: &IbdWire) {
    let bytes = w.encode();
    let parsed = parse_indoor_bike(&bytes).unwrap_or_else(|e| {
        panic!("production parser rejected a complete spec vector {bytes:02X?}: {e}")
    });
    assert_eq!(
        parsed,
        w.expected(),
        "production adapter disagrees, bytes {bytes:02X?}"
    );
}

#[track_caller]
fn check_ftms(w: &IbdWire) {
    let bytes = w.encode();
    let m = decode_measurement(MeasurementKind::IndoorBike, &bytes, w.options())
        .expect("the flag word is always present");
    assert_eq!(
        m.flags,
        u32::from(w.flags()),
        "flag word, bytes {bytes:02X?}"
    );
    assert!(
        !m.truncated && !m.trailing_bytes,
        "vector not consumed exactly: {bytes:02X?}"
    );
    assert_eq!(m.more_data, w.speed_001_kmh.is_none(), "bit 0 inversion");

    assert_close_f64(
        raw_value(&m, MeasurementField::Speed).map(|v| v / 100.0 / 3.6),
        w.expected().speed_ms,
        "ftms speed (0.01 km/h)",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::AverageSpeed).map(|v| v / 100.0 / 3.6),
        w.avg_speed_001_kmh.map(|v| f64::from(v) / 100.0 / 3.6),
        "ftms average speed",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Cadence).map(|v| v * 0.5),
        w.cadence_half_rpm.map(|v| f64::from(v) * 0.5),
        "ftms cadence (0.5 rpm)",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::AverageCadence).map(|v| v * 0.5),
        w.avg_cadence_half_rpm.map(|v| f64::from(v) * 0.5),
        "ftms average cadence",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Distance),
        w.distance_m.map(f64::from),
        "ftms total distance (u24)",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Resistance).map(|v| {
            if w.width() == ResistanceWidth::S16 {
                v / 10.0
            } else {
                v
            }
        }),
        w.expected().resistance,
        "ftms resistance",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Power),
        w.power_w.map(f64::from),
        "ftms power",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::AveragePower),
        w.avg_power_w.map(f64::from),
        "ftms average power",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Energy),
        w.expected().energy_kcal,
        "ftms energy",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::HeartRate),
        w.hr_bpm.map(f64::from),
        "ftms heart rate",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Met).map(|v| v / 10.0),
        w.met_tenths.map(|v| f64::from(v) / 10.0),
        "ftms MET",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Elapsed),
        w.elapsed_s.map(f64::from),
        "ftms elapsed",
    );
    assert_close_f64(
        raw_value(&m, MeasurementField::Remaining),
        w.remaining_s.map(f64::from),
        "ftms remaining",
    );
}

#[track_caller]
fn check_cycling_ble(w: &IbdWire) {
    if w.width() == ResistanceWidth::U8 {
        // `cycling-ble` only implements the signed-16 tenths variant. On a SIG uint8
        // packet its resistance read consumes the power field as well, so it is not a
        // second opinion for that layout. `indoor_bike_resistance_width_is_resolved_by_length`
        // pins this difference explicitly.
        return;
    }
    let bytes = w.encode();
    let m = cble::ftms::parse(&bytes).unwrap_or_else(|e| {
        panic!("cycling-ble rejected a complete spec vector {bytes:02X?}: {e}")
    });
    assert_close_f32(
        m.instantaneous_speed,
        w.speed_001_kmh.map(|v| f32::from(v) / 100.0),
        "cycling-ble speed",
    );
    assert_close_f32(
        m.average_speed,
        w.avg_speed_001_kmh.map(|v| f32::from(v) / 100.0),
        "cycling-ble average speed",
    );
    assert_close_f32(
        m.instantaneous_cadence,
        w.cadence_half_rpm.map(|v| f32::from(v) / 2.0),
        "cycling-ble cadence",
    );
    assert_close_f32(
        m.average_cadence,
        w.avg_cadence_half_rpm.map(|v| f32::from(v) / 2.0),
        "cycling-ble average cadence",
    );
    assert_eq!(m.total_distance, w.distance_m, "cycling-ble total distance");
    assert_eq!(
        m.resistance_level, w.resistance_s16_tenths,
        "cycling-ble resistance"
    );
    assert_eq!(m.instantaneous_power, w.power_w, "cycling-ble power");
    assert_eq!(m.average_power, w.avg_power_w, "cycling-ble average power");
    assert_eq!(
        m.total_energy,
        w.energy.map(|e| e.0),
        "cycling-ble total energy"
    );
    assert_eq!(
        m.energy_per_hour,
        w.energy.map(|e| e.1),
        "cycling-ble energy/hour"
    );
    assert_eq!(
        m.energy_per_minute,
        w.energy.map(|e| e.2),
        "cycling-ble energy/min"
    );
    assert_eq!(m.heart_rate_bpm, w.hr_bpm, "cycling-ble heart rate");
    assert_close_f32(
        m.metabolic_equivalent,
        w.met_tenths.map(|v| f32::from(v) / 10.0),
        "cycling-ble MET",
    );
    assert_eq!(m.elapsed_time_secs, w.elapsed_s, "cycling-ble elapsed");
    assert_eq!(
        m.remaining_time_secs, w.remaining_s,
        "cycling-ble remaining"
    );
}

#[track_caller]
fn check_all_three(w: &IbdWire) {
    check_production(w);
    check_ftms(w);
    check_cycling_ble(w);
}

// ───────────────── Indoor Bike Data: flag combinations ─────────────────

#[test]
fn indoor_bike_speed_only() {
    let w = IbdWire {
        speed_001_kmh: Some(2500),
        ..IbdWire::default()
    };
    assert_eq!(
        w.flags() & 1,
        0,
        "bit 0 clear = instantaneous speed present"
    );
    check_all_three(&w);
}

#[test]
fn indoor_bike_speed_and_cadence() {
    let w = IbdWire {
        speed_001_kmh: Some(1234),
        cadence_half_rpm: Some(171),
        ..IbdWire::default()
    };
    check_all_three(&w);
    assert_eq!(w.expected().cadence_rpm, Some(85.5), "0.5 rpm resolution");
}

#[test]
fn indoor_bike_power_only_and_negative_power() {
    let w = IbdWire {
        power_w: Some(250),
        ..IbdWire::default()
    };
    assert_eq!(w.flags() & 1, 1, "bit 0 set = no instantaneous speed");
    check_all_three(&w);
    // Regenerative braking / a freewheeling moment can be negative on the wire.
    check_all_three(&IbdWire {
        power_w: Some(-5),
        ..IbdWire::default()
    });
}

#[test]
fn indoor_bike_every_field() {
    let w = IbdWire {
        speed_001_kmh: Some(2500),
        avg_speed_001_kmh: Some(2400),
        cadence_half_rpm: Some(170),
        avg_cadence_half_rpm: Some(160),
        distance_m: Some(1_000_000),
        resistance_s16_tenths: Some(-35),
        power_w: Some(300),
        avg_power_w: Some(280),
        energy: Some((500, 600, 10)),
        hr_bpm: Some(145),
        met_tenths: Some(85),
        elapsed_s: Some(3600),
        remaining_s: Some(1200),
        ..IbdWire::default()
    };
    assert_eq!(w.flags(), 0x1FFE, "all bits except the inverted bit 0");
    check_all_three(&w);
    let p = parse_indoor_bike(&w.encode()).expect("complete packet");
    assert_eq!(p.speed_ms, Some(25.0 / 3.6));
    assert_eq!(p.cadence_rpm, Some(85.0));
    assert_eq!(p.power_w, Some(300.0));
    assert_eq!(p.resistance, Some(-3.5));
    assert_eq!(p.distance_m, Some(1_000_000.0));
    assert_eq!(p.hr_bpm, Some(145.0));
    assert_eq!(p.energy_kcal, Some(500.0));
    assert_eq!(p.resistance_width, ResistanceWidth::S16);
}

#[test]
fn indoor_bike_averages_only() {
    // No instantaneous field at all: the production adapter exposes only instant values,
    // while the two libraries must agree on every average.
    let w = IbdWire {
        avg_speed_001_kmh: Some(1800),
        avg_cadence_half_rpm: Some(160),
        avg_power_w: Some(220),
        ..IbdWire::default()
    };
    assert_eq!(w.flags() & 1, 1, "speed absent");
    check_all_three(&w);
    let p = parse_indoor_bike(&w.encode()).expect("averages-only packet");
    assert_eq!(p.speed_ms, None);
    assert_eq!(p.cadence_rpm, None);
    assert_eq!(p.power_w, None);
    assert_eq!(p.resistance, None);
}

#[test]
fn indoor_bike_energy_met_and_time_fields() {
    let w = IbdWire {
        energy: Some((500, 600, 10)),
        hr_bpm: Some(145),
        met_tenths: Some(85),
        elapsed_s: Some(3600),
        remaining_s: Some(1200),
        ..IbdWire::default()
    };
    assert_eq!(
        w.flags(),
        0x1F01,
        "bits 8, 9, 10, 11, 12 plus the inverted bit 0"
    );
    check_all_three(&w);
}

#[test]
fn indoor_bike_minimal_payload_is_flags_only() {
    let w = IbdWire::default(); // speed absent, so nothing else is selected
    assert_eq!(w.encode(), vec![0x01, 0x00]);
    check_all_three(&w);
    let p = parse_indoor_bike(&w.encode()).expect("flags-only packet");
    assert_eq!(p, IndoorBike::default());
}

#[test]
fn indoor_bike_bit_zero_is_inverted() {
    // bit 0 clear: the two following bytes are instantaneous speed.
    let speed_present = [0x00, 0x00, 0xC4, 0x09];
    let p = parse_indoor_bike(&speed_present).expect("speed packet");
    assert_eq!(p.speed_ms, Some(25.0 / 3.6));
    assert_eq!(p.power_w, None);
    let m = cble::ftms::parse(&speed_present).expect("vendored speed packet");
    assert_eq!(m.instantaneous_speed, Some(25.0));
    assert_eq!(m.instantaneous_power, None);
    let f = decode_measurement(
        MeasurementKind::IndoorBike,
        &speed_present,
        MeasurementOptions::default(),
    )
    .expect("flag word");
    assert_eq!(raw_value(&f, MeasurementField::Speed), Some(2500.0));
    assert_eq!(raw_value(&f, MeasurementField::Power), None);

    // bit 0 set: instantaneous speed is absent and those bytes would be the next field.
    let speed_absent = [0x01, 0x00];
    assert_eq!(
        parse_indoor_bike(&speed_absent)
            .expect("empty packet")
            .speed_ms,
        None
    );
    assert_eq!(
        cble::ftms::parse(&speed_absent)
            .expect("empty packet")
            .instantaneous_speed,
        None
    );
    let f = decode_measurement(
        MeasurementKind::IndoorBike,
        &speed_absent,
        MeasurementOptions::default(),
    )
    .expect("flag word");
    assert_eq!(raw_value(&f, MeasurementField::Speed), None);
    assert!(f.more_data);

    // The classic "power only" frame is exactly the same payload the inverted bit guards.
    let power_only = [0x41, 0x00, 0xFA, 0x00];
    let p = parse_indoor_bike(&power_only).expect("power packet");
    assert_eq!(p.speed_ms, None);
    assert_eq!(p.power_w, Some(250.0));
    assert_eq!(
        cble::ftms::parse(&power_only)
            .expect("vendored power packet")
            .instantaneous_power,
        Some(250)
    );
}

#[test]
fn indoor_bike_resistance_width_is_resolved_by_length() {
    // SIG GSS layout: resistance is one unitless byte before the power field.
    let u8_layout = IbdWire {
        resistance_u8: Some(25),
        power_w: Some(200),
        ..IbdWire::default()
    };
    let bytes = u8_layout.encode();
    assert_eq!(bytes, vec![0x61, 0x00, 25, 0xC8, 0x00]);
    let p = parse_indoor_bike(&bytes).expect("u8 resistance packet");
    assert_eq!(p.resistance, Some(25.0));
    assert_eq!(p.power_w, Some(200.0));
    assert_eq!(p.resistance_width, ResistanceWidth::U8);
    // The strict `ftms` decoder agrees when asked for the uint8 form.
    let m = decode_measurement(
        MeasurementKind::IndoorBike,
        &bytes,
        MeasurementOptions {
            resistance_format: MeasurementResistanceFormat::Uint8Whole,
            ..MeasurementOptions::default()
        },
    )
    .expect("u8 resistance packet");
    assert_eq!(raw_value(&m, MeasurementField::Resistance), Some(25.0));
    assert_eq!(raw_value(&m, MeasurementField::Power), Some(200.0));
    // `cycling-ble` assumes the signed-16 tenths variant (as several trainers use). On the
    // SIG layout it must fail rather than silently shift the power value.
    assert!(
        cble::ftms::parse(&bytes).is_err(),
        "cycling-ble must not accept the uint8 resistance layout as if it were signed16"
    );

    // Trainer variant: two bytes, signed, 0.1 resolution. All three agree.
    let s16_layout = IbdWire {
        resistance_s16_tenths: Some(250),
        power_w: Some(200),
        ..IbdWire::default()
    };
    let bytes = s16_layout.encode();
    assert_eq!(bytes, vec![0x61, 0x00, 0xFA, 0x00, 0xC8, 0x00]);
    check_all_three(&s16_layout);
    let p = parse_indoor_bike(&bytes).expect("s16 resistance packet");
    assert_eq!(p.resistance, Some(25.0), "250 tenths");
    assert_eq!(p.resistance_width, ResistanceWidth::S16);
    assert_eq!(
        cble::ftms::parse(&bytes)
            .expect("vendored s16 packet")
            .resistance_level,
        Some(250)
    );
}

#[test]
fn indoor_bike_energy_not_available_sentinel() {
    // Expended energy: total 0xFFFF, per-hour 0xFFFF, per-minute 0xFF = "not available".
    let bytes = [0x01, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let p = parse_indoor_bike(&bytes).expect("sentinel packet");
    assert_eq!(p.energy_kcal, None, "0xFFFF is not 65 535 kcal");
    let m = decode_measurement(
        MeasurementKind::IndoorBike,
        &bytes,
        MeasurementOptions::default(),
    )
    .expect("sentinel packet");
    assert_ne!(m.unavailable & MeasurementField::Energy.mask(), 0);
    // The vendored parser reports the raw sentinel; this difference is deliberate.
    assert_eq!(
        cble::ftms::parse(&bytes)
            .expect("vendored sentinel packet")
            .total_energy,
        Some(u16::MAX)
    );
    // A real value in the same layout is kept.
    let p = parse_indoor_bike(&[0x01, 0x01, 0xF4, 0x01, 0x58, 0x02, 0x0A]).expect("energy packet");
    assert_eq!(p.energy_kcal, Some(500.0));
}

#[test]
fn indoor_bike_truncated_payloads_error_but_never_panic() {
    let full = IbdWire {
        speed_001_kmh: Some(2500),
        avg_speed_001_kmh: Some(2400),
        cadence_half_rpm: Some(170),
        avg_cadence_half_rpm: Some(160),
        distance_m: Some(1_000_000),
        resistance_s16_tenths: Some(-35),
        power_w: Some(300),
        avg_power_w: Some(280),
        energy: Some((500, 600, 10)),
        hr_bpm: Some(145),
        met_tenths: Some(85),
        elapsed_s: Some(3600),
        remaining_s: Some(1200),
        ..IbdWire::default()
    }
    .encode();
    for len in 0..full.len() {
        let prefix = &full[..len];
        // A prefix can only be accepted by the documented resistance-width ambiguity: the
        // packet is one byte shorter than the signed-16 layout, which is exactly the length
        // of the uint8 layout, so it is a complete (differently aligned) uint8 packet.
        if let Ok(bike) = parse_indoor_bike(prefix) {
            assert_eq!(
                len,
                full.len() - 1,
                "a {len}-byte prefix must not parse: {prefix:02X?}"
            );
            assert_eq!(bike.resistance_width, ResistanceWidth::U8);
        }
        assert!(
            cble::ftms::parse(prefix).is_err(),
            "the vendored parser accepted a {len}-byte prefix"
        );
        match decode_measurement(
            MeasurementKind::IndoorBike,
            prefix,
            MeasurementOptions::default(),
        ) {
            Err(_) => assert!(
                len < 2,
                "only a missing flag word may be a hard length error"
            ),
            Ok(m) => {
                if len == full.len() - 1 {
                    // The signed-16 layout minus one byte is exactly the complete uint8
                    // layout: the field is valid, only its meaning differs.
                    assert!(!m.truncated && !m.trailing_bytes);
                } else {
                    assert!(m.truncated, "{len}-byte prefix was not marked truncated");
                }
                assert!(m.bytes_read <= len);
            }
        }
    }
    // Degenerate inputs.
    assert_eq!(parse_indoor_bike(&[]).unwrap_err(), ParseError::Truncated);
    assert_eq!(
        parse_indoor_bike(&[0x44]).unwrap_err(),
        ParseError::Truncated
    );
    assert_eq!(
        parse_indoor_bike(&[0x00, 0x00]).unwrap_err(),
        ParseError::Truncated,
        "speed selected"
    );
}

#[test]
fn indoor_bike_spec_matrix_agrees() {
    // One representative vector per flag group. The resistance-carrying cases use the
    // signed-16 layout so all three implementations can be compared.
    let matrix = [
        IbdWire {
            speed_001_kmh: Some(0),
            ..IbdWire::default()
        },
        IbdWire {
            speed_001_kmh: Some(12),
            ..IbdWire::default()
        },
        IbdWire {
            speed_001_kmh: Some(65535),
            ..IbdWire::default()
        },
        IbdWire {
            cadence_half_rpm: Some(0),
            ..IbdWire::default()
        },
        IbdWire {
            cadence_half_rpm: Some(255),
            ..IbdWire::default()
        },
        IbdWire {
            distance_m: Some(0x00FF_FFFF),
            ..IbdWire::default()
        },
        IbdWire {
            resistance_s16_tenths: Some(0),
            ..IbdWire::default()
        },
        IbdWire {
            resistance_s16_tenths: Some(-32768),
            ..IbdWire::default()
        },
        IbdWire {
            resistance_s16_tenths: Some(32767),
            ..IbdWire::default()
        },
        IbdWire {
            power_w: Some(i16::MIN),
            ..IbdWire::default()
        },
        IbdWire {
            power_w: Some(i16::MAX),
            ..IbdWire::default()
        },
        IbdWire {
            avg_power_w: Some(-1),
            ..IbdWire::default()
        },
        IbdWire {
            hr_bpm: Some(0),
            ..IbdWire::default()
        },
        IbdWire {
            hr_bpm: Some(255),
            ..IbdWire::default()
        },
        IbdWire {
            met_tenths: Some(255),
            ..IbdWire::default()
        },
        IbdWire {
            elapsed_s: Some(u16::MAX),
            remaining_s: Some(0),
            ..IbdWire::default()
        },
    ];
    for w in matrix {
        check_all_three(&w);
    }
}

// ───────────────── Control Point (0x2AD9) ─────────────────

fn ftms_encode(req: ControlRequest) -> Vec<u8> {
    let mut buf = [0u8; 11];
    let n = encode_control_request(req, ControlOptions::default(), &mut buf)
        .unwrap_or_else(|e| panic!("the ftms crate rejected a spec-valid request: {e}"));
    buf[..n].to_vec()
}

fn request(opcode: u8, operands: [i32; 5], operand_count: usize) -> ControlRequest {
    ControlRequest {
        opcode,
        operands,
        operand_count,
    }
}

#[test]
fn control_point_simple_encodings_match_the_spec() {
    // Request Control 0x00.
    assert_eq!(encode_request_control(), vec![0x00]);
    assert_eq!(
        ftms_encode(request(op::REQUEST_CONTROL, [0; 5], 0)),
        vec![0x00]
    );
    assert_eq!(
        normalize_control_request(NormalizedControlRequest::RequestControl).map(ftms_encode),
        Ok(vec![0x00])
    );

    // Start/Resume 0x07.
    assert_eq!(encode_start(), vec![0x07]);
    assert_eq!(
        ftms_encode(request(op::START_OR_RESUME, [0; 5], 0)),
        vec![0x07]
    );
    assert_eq!(
        normalize_control_request(NormalizedControlRequest::StartResume).map(ftms_encode),
        Ok(vec![0x07])
    );

    // Stop/Pause 0x08 with the mandatory control-information byte.
    assert_eq!(encode_stop_pause(false), vec![0x08, 0x01]);
    assert_eq!(encode_stop_pause(true), vec![0x08, 0x02]);
    assert_eq!(
        ftms_encode(request(op::STOP_OR_PAUSE, [1, 0, 0, 0, 0], 1)),
        vec![0x08, 0x01]
    );
    assert_eq!(
        ftms_encode(request(op::STOP_OR_PAUSE, [2, 0, 0, 0, 0], 1)),
        vec![0x08, 0x02]
    );
    assert_eq!(
        normalize_control_request(NormalizedControlRequest::StopPause { action: 2 })
            .map(ftms_encode),
        Ok(vec![0x08, 0x02])
    );
    assert_eq!(
        decode_control_request(&[0x08, 0x01], ControlOptions::default()),
        Ok(request(op::STOP_OR_PAUSE, [1, 0, 0, 0, 0], 1))
    );

    // Target Power 0x05 is a sint16 in watts.
    assert_eq!(encode_target_power(200.0), vec![0x05, 0xC8, 0x00]);
    assert_eq!(
        encode_target_power(200.4),
        vec![0x05, 0xC8, 0x00],
        "rounds to whole watts"
    );
    assert_eq!(encode_target_power(-50.0), vec![0x05, 0xCE, 0xFF]);
    assert_eq!(
        ftms_encode(request(op::SET_TARGET_POWER, [200, 0, 0, 0, 0], 1)),
        vec![0x05, 0xC8, 0x00]
    );
    assert_eq!(
        normalize_control_request(NormalizedControlRequest::TargetPowerWatts(200)).map(ftms_encode),
        Ok(vec![0x05, 0xC8, 0x00])
    );
    assert_eq!(
        decode_control_request(&[0x05, 0xCE, 0xFF], ControlOptions::default()),
        Ok(request(op::SET_TARGET_POWER, [-50, 0, 0, 0, 0], 1))
    );
}

#[test]
fn control_point_stop_pause_requires_the_parameter_byte() {
    // The bare op code is not a valid FTMS request: the control-information byte is
    // mandatory. The ftms crate, the production filter and our encoder all agree.
    assert_eq!(
        decode_control_request(&[0x08], ControlOptions::default()),
        Err(ftms::Error::WrongLength {
            expected: 2,
            actual: 1
        })
    );
    assert_eq!(
        decode_control_request(&[0x08, 0x01, 0x00], ControlOptions::default()),
        Err(ftms::Error::WrongLength {
            expected: 2,
            actual: 3
        })
    );
    assert_eq!(
        encode_control_request(
            request(op::STOP_OR_PAUSE, [0; 5], 0),
            ControlOptions::default(),
            &mut [0u8; 11]
        ),
        Err(ftms::Error::InvalidRange),
        "Stop/Pause needs its operand"
    );
    // The safety filter refuses the malformed request; it never reaches a trainer.
    assert_eq!(
        filter(&[0x08], &Limits::FALLBACK),
        Verdict::Reject([0x80, 0x08, 0x03])
    );
    assert_eq!(
        filter(&[0x08, 0x00], &Limits::FALLBACK),
        Verdict::Reject([0x80, 0x08, 0x03])
    );
    assert_eq!(
        filter(&[0x08, 0x03], &Limits::FALLBACK),
        Verdict::Reject([0x80, 0x08, 0x03])
    );
    assert_eq!(
        filter(&[0x08, 0x01], &Limits::FALLBACK),
        Verdict::Forward(vec![0x08, 0x01])
    );
    assert_eq!(
        filter(&[0x08, 0x02], &Limits::FALLBACK),
        Verdict::Forward(vec![0x08, 0x02])
    );
}

fn trainer_sim(grade: f64) -> ride_model::TrainerSim {
    ride_model::TrainerSim {
        grade_pct: grade,
        crr: 0.004,
        cw: 0.39,
        wind_ms: 0.0,
    }
}

/// The exact wire parameters for the on-grid values used below.
fn wire_sim(wind: i16, grade: i16, crr: u8, cw: u8) -> WireSim {
    WireSim {
        wind,
        grade,
        crr,
        cw,
    }
}

#[test]
fn control_point_simulation_boundary_vectors() {
    let window = (-MAX_GRADE_PCT, MAX_GRADE_PCT);

    // 0.00 %, crr 0.004 -> 40, cw 0.39 -> 39.
    let zero = quantise_sim(&trainer_sim(0.0), window);
    assert_eq!(zero, wire_sim(0, 0, 40, 39));
    assert_eq!(
        encode_simulation(zero),
        vec![0x11, 0x00, 0x00, 0x00, 0x00, 0x28, 0x27]
    );

    // +20.00 % = +2000 (0x07D0).
    let up = quantise_sim(&trainer_sim(20.0), window);
    assert_eq!(up.grade, 2000);
    assert_eq!(
        encode_simulation(up),
        vec![0x11, 0x00, 0x00, 0xD0, 0x07, 0x28, 0x27]
    );

    // -20.00 % = -2000 (0xF830).
    let down = quantise_sim(&trainer_sim(-20.0), window);
    assert_eq!(down.grade, -2000);
    assert_eq!(
        encode_simulation(down),
        vec![0x11, 0x00, 0x00, 0x30, 0xF8, 0x28, 0x27]
    );

    // Negative wind: -1.235 m/s is -1235 on the 0.001 m/s grid.
    let wind = wire_sim(-1235, 0, 40, 39);
    assert_eq!(
        encode_simulation(wind),
        vec![0x11, 0x2D, 0xFB, 0x00, 0x00, 0x28, 0x27]
    );

    // The `ftms` crate's human-unit encoder must produce exactly the same bytes from the
    // quantised wire values (the on-grid floats are representable as f32).
    for w in [zero, up, down, wind] {
        let normalised = NormalizedControlRequest::IndoorBikeSimulation {
            wind_speed_mps: f32::from(w.wind) / 1000.0,
            grade_percent: f32::from(w.grade) / 100.0,
            crr: f32::from(w.crr) / 10_000.0,
            cw_kg_per_m: f32::from(w.cw) / 100.0,
        };
        let req =
            normalize_control_request(normalised).expect("quantised values lie on the wire grid");
        assert_eq!(ftms_encode(req), encode_simulation(w), "{w:?}");
    }
}

#[test]
fn control_point_simulation_quantisation_is_mandatory() {
    // The ftms crate rejects values that are not on the wire grid with InvalidRange...
    let off_grid = [
        NormalizedControlRequest::IndoorBikeSimulation {
            wind_speed_mps: 0.0,
            grade_percent: 3.456,
            crr: 0.004,
            cw_kg_per_m: 0.39,
        },
        NormalizedControlRequest::IndoorBikeSimulation {
            wind_speed_mps: 0.0,
            grade_percent: 5.0,
            crr: 0.004_999,
            cw_kg_per_m: 0.39,
        },
        NormalizedControlRequest::IndoorBikeSimulation {
            wind_speed_mps: 0.0,
            grade_percent: 5.0,
            crr: 0.004,
            cw_kg_per_m: 0.196,
        },
        NormalizedControlRequest::IndoorBikeSimulation {
            wind_speed_mps: -1.2345,
            grade_percent: 5.0,
            crr: 0.004,
            cw_kg_per_m: 0.39,
        },
    ];
    for bad in off_grid {
        assert_eq!(
            normalize_control_request(bad),
            Err(ftms::Error::InvalidRange),
            "{bad:?} is not on the wire grid"
        );
    }

    // ...so production quantises first. Edge values round half away from zero.
    let sim = ride_model::TrainerSim {
        grade_pct: 3.456,
        crr: 0.004_999,
        cw: 0.196,
        wind_ms: -1.2345,
    };
    let w = quantise_sim(&sim, (-MAX_GRADE_PCT, MAX_GRADE_PCT));
    assert_eq!(w, wire_sim(-1235, 346, 50, 20));
    let bytes = encode_simulation(w);
    assert_eq!(bytes, vec![0x11, 0x2D, 0xFB, 0x5A, 0x01, 0x32, 0x14]);
    let req = normalize_control_request(NormalizedControlRequest::IndoorBikeSimulation {
        wind_speed_mps: -1.235,
        grade_percent: 3.46,
        crr: 0.005,
        cw_kg_per_m: 0.20,
    })
    .expect("quantised values are on the grid");
    assert_eq!(ftms_encode(req), bytes);

    // Non-finite inputs never reach the wire and fall back to safe values.
    let bad = quantise_sim(
        &ride_model::TrainerSim {
            grade_pct: f64::NAN,
            crr: f64::INFINITY,
            cw: f64::NEG_INFINITY,
            wind_ms: f64::NAN,
        },
        (-MAX_GRADE_PCT, MAX_GRADE_PCT),
    );
    assert_eq!(bad, wire_sim(0, 0, 40, 0));
}

#[test]
fn control_point_grade_is_clamped_to_the_trainer_and_plus_minus_20() {
    let window = (-MAX_GRADE_PCT, MAX_GRADE_PCT);
    // The encoder itself is already clamped by `quantise_sim`...
    assert_eq!(quantise_sim(&trainer_sim(35.0), window).grade, 2000);
    assert_eq!(quantise_sim(&trainer_sim(-35.0), window).grade, -2000);
    // ...and the safety filter clamps again, against the trainer's own inclination range.
    let narrow = Limits::from_ranges(Some((-10.0, 15.0)), Some((0, 500)));
    let steep = encode_simulation(wire_sim(0, 2000, 40, 39));
    // +20 % on a trainer that only accepts +15 % becomes +15 % = 1500 = 0x05DC.
    assert_eq!(
        filter(&steep, &narrow),
        Verdict::Forward(vec![0x11, 0x00, 0x00, 0xDC, 0x05, 0x28, 0x27])
    );
    let down = encode_simulation(wire_sim(0, -2000, 40, 39));
    // -20 % on a trainer that only accepts -10 % becomes -1000 = 0xFC18.
    assert_eq!(
        filter(&down, &narrow),
        Verdict::Forward(vec![0x11, 0x00, 0x00, 0x18, 0xFC, 0x28, 0x27])
    );
    // Target power is clamped to the trainer's advertised range as well.
    assert_eq!(
        filter(&[0x05, 0xF4, 0x01], &narrow),
        Verdict::Forward(vec![0x05, 0xF4, 0x01])
    );
    assert_eq!(
        filter(&[0x05, 0x90, 0x02], &narrow),
        Verdict::Forward(vec![0x05, 0xF4, 0x01])
    );
    // A range wider than +/-20 % never widens the production window.
    assert_eq!(
        Limits::from_ranges(Some((-40.0, 40.0)), None).grade_pct,
        (-20.0, 20.0)
    );
}

#[test]
fn control_point_every_encoder_output_passes_the_filter_unchanged() {
    let limits = Limits::from_ranges(Some((-20.0, 20.0)), Some((0, 2000)));
    for raw in (-2000..=2000).step_by(7) {
        let bytes = encode_simulation(wire_sim(0, raw as i16, 40, 39));
        assert_eq!(
            filter(&bytes, &limits),
            Verdict::Forward(bytes.clone()),
            "grade {raw}"
        );
    }
    for watts in [
        -32000.0,
        -1.0,
        0.0,
        1.0,
        200.0,
        2000.0,
        2001.0,
        99999.0,
        f64::NAN,
    ] {
        let bytes = encode_target_power(watts);
        let Verdict::Forward(out) = filter(&bytes, &limits) else {
            panic!("encoder output {watts} W was rejected")
        };
        assert_eq!(out.len(), 3);
        assert!(i16::from_le_bytes([out[1], out[2]]) <= 2000);
    }
}

// ───────────────── CPS (0x2A63) / CSC (0x2A5B) ─────────────────

fn cp_crank_packet(power: i16, revs: u16, time: u16) -> Vec<u8> {
    let mut v = vec![0x20, 0x00]; // bit 5: crank revolution data
    v.extend_from_slice(&power.to_le_bytes());
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

fn cp_wheel_packet(power: i16, revs: u32, time: u16) -> Vec<u8> {
    let mut v = vec![0x10, 0x00]; // bit 4: wheel revolution data
    v.extend_from_slice(&power.to_le_bytes());
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

fn csc_crank_packet(revs: u16, time: u16) -> Vec<u8> {
    let mut v = vec![0x02]; // bit 1: crank revolution data
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

fn csc_wheel_packet(revs: u32, time: u16) -> Vec<u8> {
    let mut v = vec![0x01]; // bit 0: wheel revolution data
    v.extend_from_slice(&revs.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

#[test]
fn cps_crank_cadence_from_revolution_deltas() {
    // One crank revolution in 1024 ticks (1 s at 1024 Hz) = 60 rpm.
    let prev = parse_cycling_power(&cp_crank_packet(200, 100, 0)).expect("crank packet");
    let curr = parse_cycling_power(&cp_crank_packet(210, 101, 1024)).expect("crank packet");
    assert_eq!(prev.crank, Some(RevSample { revs: 100, time: 0 }));
    assert_eq!(
        curr.crank,
        Some(RevSample {
            revs: 101,
            time: 1024
        })
    );
    let rpm = revolutions_per_minute(
        100,
        0,
        101,
        1024,
        CRANK_REVOLUTIONS_WRAP_AT,
        CRANK_EVENT_TIME_HZ,
    )
    .expect("one revolution");
    assert!((rpm - 60.0).abs() < 1e-4, "{rpm}");

    // The same delta through the production tracker.
    let t = Instant::now();
    let mut tracker = RevTracker::crank();
    assert_eq!(tracker.update(prev.crank.expect("crank"), t), None);
    let rpm = tracker
        .update(curr.crank.expect("crank"), t + Duration::from_secs(1))
        .expect("rate");
    assert!((rpm - 60.0).abs() < 1e-4, "{rpm}");
}

#[test]
fn cps_csc_u16_revolution_wraparound() {
    // Crank counter wraps 65535 -> 1: two revolutions in 1024 ticks = 120 rpm.
    let rpm = revolutions_per_minute(
        65_535,
        65_000,
        1,
        488,
        CRANK_REVOLUTIONS_WRAP_AT,
        CRANK_EVENT_TIME_HZ,
    )
    .expect("wrapped revolution delta");
    assert!((rpm - 120.0).abs() < 1e-4, "{rpm}");

    let t = Instant::now();
    let mut tracker = RevTracker::crank();
    tracker.update(
        RevSample {
            revs: 65_535,
            time: 65_000,
        },
        t,
    );
    let from_parser = tracker.update(RevSample { revs: 1, time: 488 }, t + Duration::from_secs(1));
    assert!((from_parser.expect("rate") - 120.0).abs() < 1e-4);

    // The CSC parser exposes the same u16 counter; the decoder's tracker must wrap too.
    let mut decoder = CscDecoder::new(2.0);
    assert!(decoder
        .feed(&csc_crank_packet(65_535, 65_000), t)
        .expect("csc packet")
        .is_empty());
    let out = decoder
        .feed(&csc_crank_packet(1, 488), t + Duration::from_secs(1))
        .expect("csc packet");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, Field::Cadence);
    assert!((out[0].1 - 120.0).abs() < 1e-4, "{:?}", out);
}

#[test]
fn event_time_u16_wraparound_is_handled() {
    // prev event time 65 435 + 1024 ticks wraps to 923; ten revolutions = 600 rpm.
    let rpm = revolutions_per_minute(
        0,
        65_435,
        10,
        923,
        CRANK_REVOLUTIONS_WRAP_AT,
        CRANK_EVENT_TIME_HZ,
    )
    .expect("wrapped event time");
    assert!((rpm - 600.0).abs() < 1e-4, "{rpm}");
    // Duplicate notifications carry no rate.
    assert_eq!(
        revolutions_per_minute(
            5,
            100,
            5,
            100,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ
        ),
        None
    );
    // Zero elapsed ticks is not an infinite rate.
    assert_eq!(
        revolutions_per_minute(
            5,
            100,
            6,
            100,
            CRANK_REVOLUTIONS_WRAP_AT,
            CRANK_EVENT_TIME_HZ
        ),
        None
    );
}

#[test]
fn wheel_time_bases_differ_1024_vs_2048_hz() {
    // CSC wheel event time is 1/1024 s, Cycling Power wheel event time is 1/2048 s.
    let csc = revolutions_per_minute(
        0,
        0,
        1,
        1024,
        WHEEL_REVOLUTIONS_WRAP_AT,
        CSC_WHEEL_EVENT_TIME_HZ,
    )
    .expect("one wheel revolution");
    assert!((csc - 60.0).abs() < 1e-4, "1024 ticks = 1 s -> {csc} rpm");
    let cps = revolutions_per_minute(
        0,
        0,
        1,
        1024,
        WHEEL_REVOLUTIONS_WRAP_AT,
        CP_WHEEL_EVENT_TIME_HZ,
    )
    .expect("one wheel revolution");
    assert!(
        (cps - 120.0).abs() < 1e-4,
        "1024 ticks = 0.5 s -> {cps} rpm"
    );

    // The parsers preserve the raw event time; only the service-specific resolution differs.
    let prev = parse_cycling_power(&cp_wheel_packet(0, 100, 0)).expect("wheel packet");
    let curr = parse_cycling_power(&cp_wheel_packet(0, 102, 1024)).expect("wheel packet");
    let wheel_prev = prev.wheel.expect("wheel data");
    let wheel_curr = curr.wheel.expect("wheel data");
    let rpm = revolutions_per_minute(
        wheel_prev.revs,
        wheel_prev.time,
        wheel_curr.revs,
        wheel_curr.time,
        WHEEL_REVOLUTIONS_WRAP_AT,
        CP_WHEEL_EVENT_TIME_HZ,
    )
    .expect("rate");
    assert!((rpm - 240.0).abs() < 1e-4, "2 revs in 0.5 s -> {rpm} rpm");

    // CSC wheel: the production decoder turns wheel rpm into m/s with its circumference.
    let t = Instant::now();
    let mut decoder = CscDecoder::new(2.0);
    assert!(decoder
        .feed(&csc_wheel_packet(100, 0), t)
        .expect("csc packet")
        .is_empty());
    let out = decoder
        .feed(&csc_wheel_packet(101, 1024), t + Duration::from_secs(1))
        .expect("csc packet");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, Field::Speed);
    assert!(
        (out[0].1 - 2.0).abs() < 1e-6,
        "60 rpm x 2 m = 2 m/s: {:?}",
        out
    );
}

#[test]
fn wheel_u32_revolution_wraparound() {
    let rpm = revolutions_per_minute(
        u32::MAX - 4,
        0,
        5,
        1024,
        WHEEL_REVOLUTIONS_WRAP_AT,
        CSC_WHEEL_EVENT_TIME_HZ,
    )
    .expect("wrapped wheel delta");
    assert!((rpm - 600.0).abs() < 1e-4, "10 revs in 1 s -> {rpm} rpm");

    let t = Instant::now();
    let mut decoder = CscDecoder::new(2.0);
    assert!(decoder
        .feed(&csc_wheel_packet(u32::MAX - 4, 0), t)
        .expect("csc packet")
        .is_empty());
    let out = decoder
        .feed(&csc_wheel_packet(5, 1024), t + Duration::from_secs(1))
        .expect("csc packet");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, Field::Speed);
    assert!(
        (out[0].1 - 20.0).abs() < 1e-4,
        "600 rpm x 2 m / 60 = 20 m/s: {:?}",
        out
    );
}

#[test]
fn csc_parser_keeps_wheel_and_crank_offsets() {
    // Both fields present: wheel (u32 + u16) comes before crank (u16 + u16).
    let mut bytes = vec![0x03];
    bytes.extend_from_slice(&12345u32.to_le_bytes());
    bytes.extend_from_slice(&2000u16.to_le_bytes());
    bytes.extend_from_slice(&600u16.to_le_bytes());
    bytes.extend_from_slice(&4000u16.to_le_bytes());
    let m = parse_csc(&bytes).expect("csc packet");
    assert_eq!(
        m.wheel,
        Some(RevSample {
            revs: 12_345,
            time: 2000
        })
    );
    assert_eq!(
        m.crank,
        Some(RevSample {
            revs: 600,
            time: 4000
        })
    );
    // Truncated packets are errors, never panics.
    assert_eq!(parse_csc(&[]).unwrap_err(), ParseError::Truncated);
    assert_eq!(parse_csc(&[0x02, 0x58]).unwrap_err(), ParseError::Truncated);
}

// ───────────────── Heart Rate (0x2A37) ─────────────────

#[test]
fn heart_rate_8_and_16_bit_forms() {
    // 8-bit value, flags 0: no RR, no contact feature.
    let hr = parse_heart_rate(&[0x00, 65]).expect("8-bit HR");
    assert_eq!((hr.bpm, hr.contact), (65, None));
    assert_eq!(
        cble::heart_rate::parse(&[0x00, 65])
            .expect("vendored 8-bit HR")
            .bpm,
        65
    );

    // 16-bit value (bit 0 set): 150 bpm.
    let hr = parse_heart_rate(&[0x01, 0x96, 0x00]).expect("16-bit HR");
    assert_eq!((hr.bpm, hr.contact), (150, None));
    let vendored = cble::heart_rate::parse(&[0x01, 0x96, 0x00]).expect("vendored 16-bit HR");
    assert_eq!((vendored.bpm, vendored.rr_intervals_secs.len()), (150, 0));

    // 16-bit value above the plausible window still parses; the decoder drops it.
    let hr = parse_heart_rate(&[0x01, 0x2C, 0x01]).expect("16-bit HR");
    assert_eq!(hr.bpm, 300);
    let mut decoder = HrDecoder;
    assert!(decoder
        .feed(&[0x01, 0x2C, 0x01], Instant::now())
        .expect("HR packet")
        .is_empty());
}

#[test]
fn heart_rate_contact_bits_are_not_interchangeable() {
    // bit 2 = feature supported, bit 1 = contact detected.
    assert_eq!(
        parse_heart_rate(&[0x04, 70]).expect("HR").contact,
        Some(false)
    );
    assert_eq!(
        parse_heart_rate(&[0x06, 70]).expect("HR").contact,
        Some(true)
    );
    // bit 1 without bit 2 does not claim contact support.
    assert_eq!(parse_heart_rate(&[0x02, 70]).expect("HR").contact, None);
}

#[test]
fn heart_rate_rr_intervals_with_and_without_energy() {
    // RR only: 512 -> 0.5 s, 1024 -> 1.0 s.
    let bytes = [0x10, 80, 0x00, 0x02, 0x00, 0x04];
    let m = cble::heart_rate::parse(&bytes).expect("RR packet");
    assert_eq!(m.bpm, 80);
    assert_eq!(m.rr_intervals_secs, vec![0.5, 1.0]);
    // The production adapter only keeps bpm/contact, but must not be disturbed by RR.
    let hr = parse_heart_rate(&bytes).expect("RR packet");
    assert_eq!((hr.bpm, hr.contact), (80, None));

    // 16-bit BPM + energy + RR in one packet: flags 0x19.
    let bytes = [0x19, 0x96, 0x00, 0xF4, 0x01, 0x00, 0x02];
    let m = cble::heart_rate::parse(&bytes).expect("full HR packet");
    assert_eq!(m.bpm, 150);
    assert_eq!(m.energy_expended_kj, Some(500));
    assert_eq!(m.rr_intervals_secs, vec![0.5]);
    assert_eq!(parse_heart_rate(&bytes).expect("full HR packet").bpm, 150);

    // A trailing odd byte cannot hold an RR value and is ignored by the vendored parser.
    let m = cble::heart_rate::parse(&[0x10, 80, 0x00, 0x02, 0xAA]).expect("odd RR packet");
    assert_eq!(m.rr_intervals_secs, vec![0.5]);
}

#[test]
fn heart_rate_decoder_enforces_plausibility_and_contact() {
    let t = Instant::now();
    let mut decoder = HrDecoder;
    let cases: [(&[u8], bool); 7] = [
        (&[0x00, MIN_HR_BPM as u8], true),
        (&[0x00, MAX_HR_BPM as u8], true),
        (&[0x00, (MIN_HR_BPM - 1.0) as u8], false),
        (&[0x01, 0xFB, 0x00], false), // 251 bpm (16-bit form)
        (&[0x00, 0], false),          // "not measured"
        (&[0x06, 80], true),          // contact detected
        (&[0x04, 80], false),         // contact feature supported but not detected
    ];
    for (bytes, ok) in cases {
        let out = decoder.feed(bytes, t).expect("HR packet");
        if ok {
            assert_eq!(
                out,
                vec![(Field::HeartRate, f64::from(bytes[1]))],
                "{bytes:02X?}"
            );
        } else {
            assert!(out.is_empty(), "{bytes:02X?}");
        }
    }
}

// ───────────────── property-style fuzzing ─────────────────

/// Fixed-seed xorshift64: deterministic, tiny and dependency-free.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 32) as u8
    }

    fn payload(&mut self, max_len: usize) -> Vec<u8> {
        let len = (self.next_u64() as usize) % (max_len + 1);
        (0..len).map(|_| self.next_u8()).collect()
    }
}

#[track_caller]
fn assert_bike_in_bounds(b: &IndoorBike) {
    for (name, value) in [
        ("speed", b.speed_ms),
        ("cadence", b.cadence_rpm),
        ("power", b.power_w),
        ("resistance", b.resistance),
        ("hr", b.hr_bpm),
        ("energy", b.energy_kcal),
        ("distance", b.distance_m),
    ] {
        if let Some(v) = value {
            assert!(v.is_finite(), "{name} is not finite: {b:?}");
        }
    }
    if let Some(v) = b.speed_ms {
        assert!(
            (0.0..=f64::from(u16::MAX) / 100.0 / 3.6).contains(&v),
            "speed {v}"
        );
    }
    if let Some(v) = b.cadence_rpm {
        assert!(
            (0.0..=f64::from(u16::MAX) * 0.5).contains(&v),
            "cadence {v}"
        );
    }
    if let Some(v) = b.power_w {
        assert!(
            (f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&v),
            "power {v}"
        );
    }
    match b.resistance_width {
        ResistanceWidth::Absent => assert_eq!(b.resistance, None),
        ResistanceWidth::U8 => {
            assert!(matches!(b.resistance, Some(v) if (0.0..=255.0).contains(&v)))
        }
        ResistanceWidth::S16 => assert!(matches!(
            b.resistance,
            Some(v) if (f64::from(i16::MIN) / 10.0..=f64::from(i16::MAX) / 10.0).contains(&v)
        )),
    }
    if let Some(v) = b.hr_bpm {
        assert!((0.0..=255.0).contains(&v), "hr {v}");
    }
    if let Some(v) = b.distance_m {
        assert!((0.0..=f64::from(0x00FF_FFFF)).contains(&v), "distance {v}");
    }
    // The merger-facing projection applies the plausibility windows.
    for (field, value) in b.fields() {
        assert!(value.is_finite(), "{field:?} is not finite");
        match field {
            Field::Power => assert!((0.0..=MAX_POWER_W).contains(&value), "power {value}"),
            Field::Cadence => assert!((0.0..=MAX_CADENCE_RPM).contains(&value), "cadence {value}"),
            Field::HeartRate => assert!((MIN_HR_BPM..=MAX_HR_BPM).contains(&value), "hr {value}"),
            Field::Speed => assert!((0.0..=MAX_SPEED_MS).contains(&value), "speed {value}"),
            Field::Energy => assert!((0.0..=100_000.0).contains(&value), "energy {value}"),
            Field::Resistance => {}
            Field::PowerBalance => panic!("Indoor Bike Data cannot produce {field:?}"),
        }
    }
}

#[test]
fn random_indoor_bike_payloads_never_panic_and_stay_in_bounds() {
    let mut rng = Rng::new(0x1BAD_B002_D15E_A5E5);
    for _ in 0..4_000 {
        let payload = rng.payload(40);
        if let Ok(bike) = parse_indoor_bike(&payload) {
            assert_bike_in_bounds(&bike);
        }
        // Both raw layouts must always be safe, and the vendored parser must not panic.
        for resistance_format in [
            MeasurementResistanceFormat::Uint8Whole,
            MeasurementResistanceFormat::Signed16Tenths,
        ] {
            if let Ok(m) = decode_measurement(
                MeasurementKind::IndoorBike,
                &payload,
                MeasurementOptions {
                    resistance_format,
                    ..MeasurementOptions::default()
                },
            ) {
                assert!(m.bytes_read <= payload.len(), "bytes_read past the payload");
                assert_eq!(
                    m.trailing_bytes,
                    !m.truncated && m.bytes_read < payload.len()
                );
                assert_eq!(
                    m.unavailable & !m.present,
                    0,
                    "an absent field cannot be marked unavailable"
                );
            }
        }
        let _ = cble::ftms::parse(&payload);
    }
}

#[test]
fn random_cycling_power_payloads_never_panic_and_stay_in_bounds() {
    let mut rng = Rng::new(0xC0DE_1E55_1234_5678);
    let t = Instant::now();
    let mut decoder = PowerDecoder::default();
    for i in 0..4_000u64 {
        let payload = rng.payload(24);
        // The parser returns raw wire values; the decoder applies the physical windows
        // (pedal balance 0..=100 %, power 0..=3000 W) checked on its output below.
        let _ = parse_cycling_power(&payload);
        let now = t + Duration::from_millis(i);
        if let Ok(values) = decoder.feed(&payload, now) {
            for (field, value) in values {
                assert!(value.is_finite());
                match field {
                    Field::Power => assert!((0.0..=MAX_POWER_W).contains(&value)),
                    Field::PowerBalance => assert!((0.0..=100.0).contains(&value)),
                    Field::Cadence => assert!((0.0..=MAX_CADENCE_RPM).contains(&value)),
                    other => panic!("unexpected field from a power meter: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn random_csc_payloads_never_panic_and_stay_in_bounds() {
    let mut rng = Rng::new(0x5EED_C5C0_0BAD_F00D);
    let t = Instant::now();
    let mut decoder = CscDecoder::default();
    for i in 0..4_000u64 {
        let payload = rng.payload(16);
        // The vendored parser and the production adapter must both survive any bytes.
        let _ = parse_csc(&payload);
        let now = t + Duration::from_millis(i);
        if let Ok(values) = decoder.feed(&payload, now) {
            for (field, value) in values {
                assert!(value.is_finite());
                match field {
                    Field::Cadence => assert!((0.0..=MAX_CADENCE_RPM).contains(&value)),
                    Field::Speed => assert!((0.0..=MAX_SPEED_MS).contains(&value)),
                    other => panic!("unexpected field from a CSC sensor: {other:?}"),
                }
            }
        }
        // Timer-driven coasting values obey the same bounds.
        for (field, value) in decoder.idle(now) {
            match field {
                Field::Cadence => assert!((0.0..=MAX_CADENCE_RPM).contains(&value)),
                Field::Speed => assert!((0.0..=MAX_SPEED_MS).contains(&value)),
                other => panic!("unexpected idle field from a CSC sensor: {other:?}"),
            }
        }
    }
}

#[test]
fn random_heart_rate_payloads_never_panic_and_stay_in_bounds() {
    let mut rng = Rng::new(0xA11C_E0F1_E123_4ABC);
    let mut decoder = HrDecoder;
    let t = Instant::now();
    for _ in 0..4_000 {
        let payload = rng.payload(12);
        if let Ok(hr) = parse_heart_rate(&payload) {
            assert!(f64::from(hr.bpm).is_finite(), "bpm is a finite integer");
        }
        if let Ok(values) = decoder.feed(&payload, t) {
            for (field, value) in values {
                assert_eq!(field, Field::HeartRate);
                assert!((MIN_HR_BPM..=MAX_HR_BPM).contains(&value), "hr {value}");
            }
        }
        let _ = cble::heart_rate::parse(&payload);
    }
}

#[test]
fn random_control_and_status_payloads_never_panic() {
    const ALLOWED: [u8; 5] = [
        op::REQUEST_CONTROL,
        op::SET_TARGET_POWER,
        op::START_OR_RESUME,
        op::STOP_OR_PAUSE,
        op::SET_SIMULATION,
    ];
    let mut rng = Rng::new(0x0BAD_CAFE_D00D_1234);
    for _ in 0..2_000 {
        let payload = rng.payload(20);
        // Response/status/capability decoders.
        let _ = parse_cp_response(&payload);
        let _ = parse_machine_status(&payload);
        let _ = parse_features(&payload);
        let _ = parse_inclination_range(&payload);
        let _ = parse_power_range(&payload);
        let _ = parse_resistance_range(&payload);
        let _ = decode_control_request(&payload, ControlOptions::default());
        // The safety filter is the only gate before a trainer.
        match filter(&payload, &Limits::FALLBACK) {
            Verdict::Forward(out) => {
                assert!(
                    ALLOWED.contains(&out[0]),
                    "filter forwarded op {:#04x}",
                    out[0]
                );
                assert_eq!(
                    out.len(),
                    payload.len(),
                    "the filter never pads or truncates"
                );
                assert!(out.len() <= 7, "the longest command is 7 bytes");
            }
            Verdict::Reject([0x80, _, code]) => assert!(matches!(code, 0x02 | 0x03)),
            Verdict::Reject([lead, _, _]) => {
                panic!("a rejection must start with 0x80, got {lead:#04x}")
            }
        }
    }
}
