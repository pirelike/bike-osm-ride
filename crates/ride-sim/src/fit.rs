//! Export of a ride as a Garmin FIT activity file (binary), the format Strava, Garmin Connect,
//! TrainingPeaks and Intervals.icu read best. Written from the public FIT protocol description
//! (file header, definition and data messages, CRC-16); no SDK code is used.
//!
//! The file holds, in this order: `file_id`, a timer-start `event`, one `record` per 1 Hz sample,
//! a timer-stop `event`, the `lap`s (the workout's steps, or one lap for the whole ride), one
//! `session` and the `activity`. Units follow the profile: distance in centimetres, speed in mm/s,
//! altitude `(m + 500) · 5`, positions in semicircles, times in seconds since 1989-12-31 UTC.
//!
//! Pure: bytes in memory, no I/O.

use ride_model::{RecordPoint, RideSummary};

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31 00:00:00 UTC).
pub const FIT_EPOCH_OFFSET: f64 = 631_065_600.0;

/// FIT profile version written in the header (21.32).
const PROFILE_VERSION: u16 = 2132;

// global message numbers
const MESG_FILE_ID: u16 = 0;
const MESG_SESSION: u16 = 18;
const MESG_LAP: u16 = 19;
const MESG_RECORD: u16 = 20;
const MESG_EVENT: u16 = 21;
const MESG_ACTIVITY: u16 = 34;

// base types
const ENUM: u8 = 0x00;
const UINT8: u8 = 0x02;
const UINT16: u8 = 0x84;
const SINT32: u8 = 0x85;
const UINT32: u8 = 0x86;

const SPORT_CYCLING: u8 = 2;
/// `sub_sport` of a ride on a smart trainer through a virtual course ("virtual ride").
pub const SUB_SPORT_VIRTUAL: u8 = 58;
/// `sub_sport` of a ride of unknown kind (an imported outdoor ride).
pub const SUB_SPORT_GENERIC: u8 = 0;

const CRC_TABLE: [u16; 16] = [
    0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401,
    0x5000, 0x9C01, 0x8801, 0x4400,
];

/// The FIT CRC-16 of `bytes`, continuing from `crc`.
pub fn crc16(mut crc: u16, bytes: &[u8]) -> u16 {
    for &byte in bytes {
        let mut tmp = CRC_TABLE[usize::from(crc & 0xF)];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ CRC_TABLE[usize::from(byte & 0xF)];
        tmp = CRC_TABLE[usize::from(crc & 0xF)];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ CRC_TABLE[usize::from((byte >> 4) & 0xF)];
    }
    crc
}

/// A field value; `None` writes the base type's "invalid" value.
#[derive(Clone, Copy)]
enum Value {
    Enum(Option<u8>),
    U8(Option<u8>),
    U16(Option<u16>),
    I32(Option<i32>),
    U32(Option<u32>),
}

impl Value {
    fn base_type(self) -> (u8, u8) {
        match self {
            Value::Enum(_) => (1, ENUM),
            Value::U8(_) => (1, UINT8),
            Value::U16(_) => (2, UINT16),
            Value::I32(_) => (4, SINT32),
            Value::U32(_) => (4, UINT32),
        }
    }

    fn write(self, out: &mut Vec<u8>) {
        match self {
            Value::Enum(v) | Value::U8(v) => out.push(v.unwrap_or(0xFF)),
            Value::U16(v) => out.extend_from_slice(&v.unwrap_or(0xFFFF).to_le_bytes()),
            Value::I32(v) => out.extend_from_slice(&v.unwrap_or(0x7FFF_FFFF).to_le_bytes()),
            Value::U32(v) => out.extend_from_slice(&v.unwrap_or(0xFFFF_FFFF).to_le_bytes()),
        }
    }
}

/// The data part of the file: definitions are written once per local message type, and again
/// when the same local type is reused with other fields.
struct Writer {
    data: Vec<u8>,
    defined: [Option<(u16, Vec<u8>)>; 16],
}

impl Writer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            defined: Default::default(),
        }
    }

    fn message(&mut self, local: u8, global: u16, fields: &[(u8, Value)]) {
        let local = local & 0x0F;
        let layout: Vec<u8> = fields.iter().map(|(n, _)| *n).collect();
        let slot = &mut self.defined[usize::from(local)];
        if slot.as_ref() != Some(&(global, layout.clone())) {
            self.data.push(0x40 | local);
            self.data.push(0); // reserved
            self.data.push(0); // little endian
            self.data.extend_from_slice(&global.to_le_bytes());
            self.data.push(fields.len() as u8);
            for (number, value) in fields {
                let (size, base) = value.base_type();
                self.data.extend_from_slice(&[*number, size, base]);
            }
            *slot = Some((global, layout));
        }
        self.data.push(local);
        for (_, value) in fields {
            value.write(&mut self.data);
        }
    }

    fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() + 16);
        out.push(14);
        out.push(0x20); // protocol 2.0
        out.extend_from_slice(&PROFILE_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(b".FIT");
        let header_crc = crc16(0, &out);
        out.extend_from_slice(&header_crc.to_le_bytes());
        out.extend_from_slice(&self.data);
        let crc = crc16(0, &out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }
}

/// FIT time stamp of a Unix time.
pub fn fit_time(unix_s: f64) -> Option<u32> {
    let t = (unix_s - FIT_EPOCH_OFFSET).round();
    (unix_s.is_finite() && t >= 0.0 && t < f64::from(u32::MAX)).then_some(t as u32)
}

fn semicircles(deg: f64) -> Option<i32> {
    (deg.is_finite() && deg.abs() <= 180.0).then(|| {
        (deg * (2_147_483_648.0 / 180.0))
            .round()
            .clamp(-2_147_483_647.0, 2_147_483_647.0) as i32
    })
}

fn scaled_u16(value: Option<f64>, scale: f64) -> Option<u16> {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * scale).round())
        .filter(|v| *v < 65_535.0)
        .map(|v| v as u16)
}

fn scaled_u32(value: Option<f64>, scale: f64) -> Option<u32> {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * scale).round())
        .filter(|v| *v < 4_294_967_295.0)
        .map(|v| v as u32)
}

fn byte(value: Option<f64>) -> Option<u8> {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v.round())
        .filter(|v| *v < 255.0)
        .map(|v| v as u8)
}

/// The numbers of the samples in one lap.
struct LapStats {
    start: f64,
    end: f64,
    elapsed: f64,
    dist: f64,
    avg_speed: Option<f64>,
    max_speed: Option<f64>,
    avg_power: Option<f64>,
    max_power: Option<f64>,
    avg_hr: Option<f64>,
    max_hr: Option<f64>,
    avg_cadence: Option<f64>,
    ascent: f64,
}

fn lap_stats(points: &[RecordPoint]) -> Option<LapStats> {
    let first = points.first()?;
    let last = points.last()?;
    let elapsed = (last.t - first.t).max(0.0);
    let dist = (last.dist - first.dist).max(0.0);
    let mean = |f: &dyn Fn(&RecordPoint) -> Option<f64>| {
        let (s, n) = points
            .iter()
            .filter_map(f)
            .filter(|v| v.is_finite())
            .fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
        (n > 0).then(|| s / n as f64)
    };
    let max = |f: &dyn Fn(&RecordPoint) -> Option<f64>| {
        points
            .iter()
            .filter_map(f)
            .filter(|v| v.is_finite())
            .reduce(f64::max)
    };
    let ascent = points
        .windows(2)
        .map(|w| w[1].ele - w[0].ele)
        .filter(|d| d.is_finite() && *d > 0.0)
        .sum();
    Some(LapStats {
        start: first.unix_s,
        end: last.unix_s,
        elapsed,
        dist,
        avg_speed: (elapsed > 0.0).then(|| dist / elapsed),
        max_speed: max(&|p| Some(p.speed_ms)),
        avg_power: mean(&|p| p.power_w),
        max_power: max(&|p| p.power_w),
        avg_hr: mean(&|p| p.hr_bpm),
        max_hr: max(&|p| p.hr_bpm),
        avg_cadence: mean(&|p| p.cadence_rpm),
        ascent,
    })
}

/// The ride as a FIT activity. `laps` are timer-second ranges `[start, end)` (the workout's steps);
/// empty: one lap for the whole ride. `sub_sport` is [`SUB_SPORT_VIRTUAL`] for a trainer ride.
/// `None` when the ride has no usable sample or its time stamps are outside the FIT range.
pub fn to_fit(
    summary: &RideSummary,
    points: &[RecordPoint],
    laps: &[(usize, usize)],
    sub_sport: u8,
) -> Option<Vec<u8>> {
    let points: Vec<RecordPoint> = points
        .iter()
        .copied()
        .filter(|p| p.t.is_finite() && fit_time(p.unix_s).is_some())
        .collect();
    let first = *points.first()?;
    let last = *points.last()?;
    let start = fit_time(first.unix_s)?;
    let end = fit_time(last.unix_s)?;
    let mut w = Writer::new();

    w.message(
        0,
        MESG_FILE_ID,
        &[
            (0, Value::Enum(Some(4))),    // type: activity
            (1, Value::U16(Some(255))),   // manufacturer: development
            (2, Value::U16(Some(0))),     // product
            (4, Value::U32(Some(start))), // time_created
        ],
    );
    let event = |w: &mut Writer, at: u32, event_type: u8| {
        w.message(
            1,
            MESG_EVENT,
            &[
                (253, Value::U32(Some(at))),
                (0, Value::Enum(Some(0))), // event: timer
                (1, Value::Enum(Some(event_type))),
            ],
        );
    };
    event(&mut w, start, 0); // start

    for p in &points {
        let Some(at) = fit_time(p.unix_s) else {
            continue;
        };
        w.message(
            2,
            MESG_RECORD,
            &[
                (253, Value::U32(Some(at))),
                (0, Value::I32(semicircles(p.lat))),
                (1, Value::I32(semicircles(p.lon))),
                (2, Value::U16(scaled_u16(Some(p.ele + 500.0), 5.0))),
                (3, Value::U8(byte(p.hr_bpm))),
                (4, Value::U8(byte(p.cadence_rpm))),
                (5, Value::U32(scaled_u32(Some(p.dist - first.dist), 100.0))),
                (6, Value::U16(scaled_u16(Some(p.speed_ms), 1000.0))),
                (7, Value::U16(scaled_u16(p.power_w, 1.0))),
            ],
        );
    }
    event(&mut w, end, 4); // stop all

    // laps: the given ranges, or the whole ride
    let ranges: Vec<Vec<RecordPoint>> = if laps.is_empty() {
        vec![points.clone()]
    } else {
        laps.iter()
            .map(|&(a, b)| {
                points
                    .iter()
                    .copied()
                    .filter(|p| p.t >= a as f64 && p.t <= b as f64)
                    .collect::<Vec<_>>()
            })
            .filter(|lap| lap.len() >= 2)
            .collect()
    };
    let mut lap_count: u16 = 0;
    for lap in &ranges {
        let Some(s) = lap_stats(lap) else {
            continue;
        };
        let (Some(lap_start), Some(lap_end)) = (fit_time(s.start), fit_time(s.end)) else {
            continue;
        };
        w.message(
            3,
            MESG_LAP,
            &[
                (254, Value::U16(Some(lap_count))),
                (253, Value::U32(Some(lap_end))),
                (0, Value::Enum(Some(9))), // event: lap
                (1, Value::Enum(Some(1))), // stop
                (2, Value::U32(Some(lap_start))),
                (7, Value::U32(scaled_u32(Some(s.elapsed), 1000.0))),
                (8, Value::U32(scaled_u32(Some(s.elapsed), 1000.0))),
                (9, Value::U32(scaled_u32(Some(s.dist), 100.0))),
                (13, Value::U16(scaled_u16(s.avg_speed, 1000.0))),
                (14, Value::U16(scaled_u16(s.max_speed, 1000.0))),
                (15, Value::U8(byte(s.avg_hr))),
                (16, Value::U8(byte(s.max_hr))),
                (17, Value::U8(byte(s.avg_cadence))),
                (19, Value::U16(scaled_u16(s.avg_power, 1.0))),
                (20, Value::U16(scaled_u16(s.max_power, 1.0))),
                (21, Value::U16(scaled_u16(Some(s.ascent), 1.0))),
                (24, Value::Enum(Some(if laps.is_empty() { 7 } else { 1 }))), // session end / time
                (25, Value::Enum(Some(SPORT_CYCLING))),
                (39, Value::Enum(Some(sub_sport))),
            ],
        );
        lap_count += 1;
    }

    let finite = |v: f64| v.is_finite().then_some(v);
    let timer = finite(summary.elapsed_s)
        .unwrap_or(last.t - first.t)
        .max(0.0);
    w.message(
        4,
        MESG_SESSION,
        &[
            (254, Value::U16(Some(0))),
            (253, Value::U32(Some(end))),
            (0, Value::Enum(Some(8))), // event: session
            (1, Value::Enum(Some(1))), // stop
            (2, Value::U32(Some(start))),
            (5, Value::Enum(Some(SPORT_CYCLING))),
            (6, Value::Enum(Some(sub_sport))),
            (7, Value::U32(scaled_u32(Some(timer), 1000.0))),
            (8, Value::U32(scaled_u32(Some(timer), 1000.0))),
            (9, Value::U32(scaled_u32(finite(summary.dist), 100.0))),
            (11, Value::U16(scaled_u16(finite(summary.kcal), 1.0))),
            (
                14,
                Value::U16(scaled_u16(finite(summary.avg_speed_ms), 1000.0)),
            ),
            (
                15,
                Value::U16(scaled_u16(finite(summary.max_speed_ms), 1000.0)),
            ),
            (16, Value::U8(byte(summary.avg_hr))),
            (17, Value::U8(byte(summary.max_hr))),
            (18, Value::U8(byte(summary.avg_cadence))),
            (20, Value::U16(scaled_u16(summary.avg_power_w, 1.0))),
            (21, Value::U16(scaled_u16(summary.max_power_w, 1.0))),
            (22, Value::U16(scaled_u16(finite(summary.ascent), 1.0))),
            (25, Value::U16(Some(0))),
            (26, Value::U16(Some(lap_count))),
            (34, Value::U16(scaled_u16(summary.normalized_power_w, 1.0))),
            (35, Value::U16(scaled_u16(summary.tss, 10.0))),
            (36, Value::U16(scaled_u16(summary.intensity_factor, 1000.0))),
        ],
    );
    w.message(
        5,
        MESG_ACTIVITY,
        &[
            (253, Value::U32(Some(end))),
            (0, Value::U32(scaled_u32(Some(timer), 1000.0))),
            (1, Value::U16(Some(1))),   // one session
            (2, Value::Enum(Some(0))),  // manual
            (3, Value::Enum(Some(26))), // event: activity
            (4, Value::Enum(Some(1))),  // stop
        ],
    );
    Some(w.finish())
}

/// The file name the save dialog suggests: the TCX name with the FIT extension.
pub fn suggested_filename(summary: &RideSummary) -> String {
    let tcx = crate::tcx::suggested_filename(summary);
    match tcx.strip_suffix(".tcx") {
        Some(stem) => format!("{stem}.fit"),
        None => format!("{tcx}.fit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(n: u32) -> Vec<RecordPoint> {
        (0..=n)
            .map(|t| RecordPoint {
                t: f64::from(t),
                unix_s: 1_700_000_000.0 + f64::from(t),
                dist: f64::from(t) * 8.0,
                lat: 47.5 + f64::from(t) * 1e-5,
                lon: 19.0,
                ele: 100.0 + f64::from(t) * 0.1,
                speed_ms: 8.0,
                power_w: Some(200.0 + f64::from(t % 7)),
                cadence_rpm: Some(90.0),
                hr_bpm: if t == 3 { None } else { Some(140.0) },
                grade: 1.0,
            })
            .collect()
    }

    fn summary() -> RideSummary {
        RideSummary {
            route_name: "Kör".into(),
            route_id: "r".into(),
            started_unix_s: 1_700_000_000.0,
            elapsed_s: 600.0,
            dist: 4_800.0,
            avg_power_w: Some(203.0),
            tss: Some(12.3),
            intensity_factor: Some(0.81),
            ..RideSummary::default()
        }
    }

    #[test]
    fn the_crc_matches_the_reference_values() {
        // the FIT CRC is CRC-16/ARC: "123456789" -> 0xBB3D
        assert_eq!(crc16(0, b"123456789"), 0xBB3D);
        assert_eq!(crc16(0, b""), 0);
    }

    #[test]
    fn the_file_is_framed_and_checksummed() {
        let bytes = to_fit(&summary(), &points(600), &[], SUB_SPORT_VIRTUAL).expect("file");
        assert_eq!(bytes[0], 14);
        assert_eq!(&bytes[8..12], b".FIT");
        let data_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(bytes.len(), 14 + data_size + 2);
        assert_eq!(
            u16::from_le_bytes([bytes[12], bytes[13]]),
            crc16(0, &bytes[..12])
        );
        // the CRC over the whole file including its own CRC is zero
        assert_eq!(crc16(0, &bytes), 0);
    }

    #[test]
    fn times_and_units_follow_the_profile() {
        assert_eq!(fit_time(FIT_EPOCH_OFFSET), Some(0));
        assert_eq!(fit_time(0.0), None, "before 1989");
        assert_eq!(fit_time(f64::NAN), None);
        assert_eq!(semicircles(180.0), Some(2_147_483_647));
        assert_eq!(semicircles(-90.0), Some(-1_073_741_824));
        assert_eq!(semicircles(200.0), None);
        assert_eq!(scaled_u16(Some(8.0), 1000.0), Some(8_000));
        assert_eq!(scaled_u16(Some(-1.0), 1.0), None);
        assert_eq!(byte(Some(300.0)), None);
    }

    #[test]
    fn nothing_to_write_gives_no_file() {
        assert!(to_fit(&summary(), &[], &[], SUB_SPORT_VIRTUAL).is_none());
        let mut early = points(10);
        for p in &mut early {
            p.unix_s = 1_000.0;
        }
        assert!(to_fit(&summary(), &early, &[], SUB_SPORT_VIRTUAL).is_none());
    }

    #[test]
    fn the_file_name_follows_the_tcx_one() {
        let name = suggested_filename(&summary());
        assert!(name.ends_with(".fit"), "{name}");
        assert_eq!(
            name.trim_end_matches(".fit"),
            crate::tcx::suggested_filename(&summary()).trim_end_matches(".tcx")
        );
    }
}
