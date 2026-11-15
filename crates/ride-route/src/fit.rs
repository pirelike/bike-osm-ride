//! Reader of the `record` messages of a Garmin FIT activity file: time, position, altitude,
//! heart rate, cadence, distance, speed and power, one per sample.
//!
//! Written from the public FIT protocol description (file header, definition / data messages,
//! compressed time stamp headers, developer fields, either byte order); no SDK code is used. Every
//! other message is read past and ignored. The reader never trusts a length from the file: a field
//! or message running past the end, an unknown local type or a broken header ends the parse with
//! an error, never a panic. The CRC is not required (some exporters write none).

use ride_i18n::t;

use crate::{err, Result};

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31 00:00:00 UTC).
const FIT_EPOCH_OFFSET: f64 = 631_065_600.0;
/// The `record` message.
const MESG_RECORD: u16 = 20;
/// A file with more samples than this is refused (about four days at 1 Hz).
const MAX_RECORDS: usize = 400_000;

/// One `record` message; fields the file does not have (or marks invalid) are `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FitRecord {
    /// Seconds since the Unix epoch.
    pub time: Option<f64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude: Option<f64>,
    pub hr: Option<f64>,
    pub cadence: Option<f64>,
    pub distance: Option<f64>,
    pub speed: Option<f64>,
    pub power: Option<f64>,
}

/// Whether the bytes start like a FIT file.
pub fn is_fit(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && matches!(bytes[0], 12 | 14) && &bytes[8..12] == b".FIT"
}

#[derive(Clone)]
struct Field {
    number: u8,
    size: usize,
    base: u8,
}

#[derive(Clone)]
struct Definition {
    global: u16,
    big_endian: bool,
    fields: Vec<Field>,
    /// Bytes of developer fields after the normal ones.
    developer_size: usize,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
}

/// The unsigned value of a field (the first element of an array), `None` for the base type's
/// invalid value.
fn unsigned(raw: &[u8], base: u8, big_endian: bool) -> Option<u64> {
    let width = match base & 0x1F {
        0x00 | 0x01 | 0x02 | 0x0A | 0x0D => 1,
        0x03 | 0x04 | 0x0B => 2,
        0x05 | 0x06 | 0x0C => 4,
        0x0E..=0x10 => 8,
        _ => return None,
    };
    let raw = raw.get(..width)?;
    let mut value: u64 = 0;
    if big_endian {
        for b in raw {
            value = (value << 8) | u64::from(*b);
        }
    } else {
        for b in raw.iter().rev() {
            value = (value << 8) | u64::from(*b);
        }
    }
    let invalid = match base & 0x1F {
        0x00 | 0x02 | 0x0D => value == 0xFF,
        0x01 => value == 0x7F,
        0x0A | 0x0B | 0x0C | 0x10 => value == 0,
        0x03 => value == 0x7FFF,
        0x04 => value == 0xFFFF,
        0x05 => value == 0x7FFF_FFFF,
        0x06 => value == 0xFFFF_FFFF,
        0x0E => value == 0x7FFF_FFFF_FFFF_FFFF,
        0x0F => value == u64::MAX,
        _ => true,
    };
    (!invalid).then_some(value)
}

/// The signed value of a field of a signed base type.
fn signed(raw: &[u8], base: u8, big_endian: bool) -> Option<i64> {
    let value = unsigned(raw, base, big_endian)?;
    Some(match base & 0x1F {
        0x01 => i64::from(value as u8 as i8),
        0x03 => i64::from(value as u16 as i16),
        0x05 => i64::from(value as u32 as i32),
        _ => value as i64,
    })
}

/// Reads every `record` message of a FIT file, in file order.
pub fn read_records(bytes: &[u8]) -> Result<Vec<FitRecord>> {
    if !is_fit(bytes) {
        return err(t("route.fit.a_fajl_nem_fit_fajl"));
    }
    let header_size = usize::from(bytes[0]);
    let data_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let end = header_size
        .checked_add(data_size)
        .filter(|e| *e <= bytes.len())
        .unwrap_or(bytes.len());
    let mut cursor = Cursor {
        bytes: bytes.get(..end).unwrap_or(bytes),
        at: header_size,
    };
    let mut definitions: [Option<Definition>; 16] = Default::default();
    let mut out: Vec<FitRecord> = Vec::new();
    let mut last_timestamp: Option<u32> = None;
    let broken = || t("route.fit.a_fit_fajl_serult_vagy_csonka");

    while cursor.at < cursor.bytes.len() {
        let Some(header) = cursor.byte() else {
            break;
        };
        if header & 0x80 != 0 {
            // compressed time stamp header: a data message with a 5-bit time offset
            let local = usize::from((header >> 5) & 0x03);
            let offset = u32::from(header & 0x1F);
            let Some(def) = definitions[local].clone() else {
                return err(broken());
            };
            let time = last_timestamp.map(|last| {
                let mut t = (last & !0x1F) | offset;
                if offset < (last & 0x1F) {
                    t = t.wrapping_add(0x20);
                }
                t
            });
            if let Some(t) = time {
                last_timestamp = Some(t);
            }
            let record = read_data(&mut cursor, &def, &mut last_timestamp)
                .ok_or_else(|| crate::RouteError::Message(broken().to_string()))?;
            if let Some(mut r) = record {
                if r.time.is_none() {
                    r.time = time.map(|t| f64::from(t) + FIT_EPOCH_OFFSET);
                }
                out.push(r);
            }
        } else if header & 0x40 != 0 {
            let local = usize::from(header & 0x0F);
            let developer = header & 0x20 != 0;
            let def = read_definition(&mut cursor, developer)
                .ok_or_else(|| crate::RouteError::Message(broken().to_string()))?;
            definitions[local] = Some(def);
        } else {
            let local = usize::from(header & 0x0F);
            let Some(def) = definitions[local].clone() else {
                return err(broken());
            };
            let record = read_data(&mut cursor, &def, &mut last_timestamp)
                .ok_or_else(|| crate::RouteError::Message(broken().to_string()))?;
            if let Some(r) = record {
                out.push(r);
            }
        }
        if out.len() > MAX_RECORDS {
            return err(t("route.fit.a_fit_fajl_tul_hosszu"));
        }
    }
    Ok(out)
}

fn read_definition(cursor: &mut Cursor<'_>, developer: bool) -> Option<Definition> {
    let head = cursor.take(5)?;
    let big_endian = head[1] == 1;
    let global = if big_endian {
        u16::from_be_bytes([head[2], head[3]])
    } else {
        u16::from_le_bytes([head[2], head[3]])
    };
    let count = usize::from(head[4]);
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let f = cursor.take(3)?;
        fields.push(Field {
            number: f[0],
            size: usize::from(f[1]),
            base: f[2],
        });
    }
    let mut developer_size = 0;
    if developer {
        let n = usize::from(cursor.byte()?);
        for _ in 0..n {
            let f = cursor.take(3)?;
            developer_size += usize::from(f[1]);
        }
    }
    Some(Definition {
        global,
        big_endian,
        fields,
        developer_size,
    })
}

/// Reads one data message. `Some(None)`: read, but not a record; `None`: the file ended inside it.
fn read_data(
    cursor: &mut Cursor<'_>,
    def: &Definition,
    last_timestamp: &mut Option<u32>,
) -> Option<Option<FitRecord>> {
    let mut record = FitRecord::default();
    let mut altitude: Option<f64> = None;
    let mut enhanced_altitude: Option<f64> = None;
    let mut speed: Option<f64> = None;
    let mut enhanced_speed: Option<f64> = None;
    for field in &def.fields {
        let raw = cursor.take(field.size)?;
        let be = def.big_endian;
        if field.number == 253 {
            if let Some(v) = unsigned(raw, field.base, be) {
                let v = v as u32;
                *last_timestamp = Some(v);
                if def.global == MESG_RECORD {
                    record.time = Some(f64::from(v) + FIT_EPOCH_OFFSET);
                }
            }
            continue;
        }
        if def.global != MESG_RECORD {
            continue;
        }
        match field.number {
            0 => {
                record.lat =
                    signed(raw, field.base, be).map(|v| v as f64 * (180.0 / 2_147_483_648.0))
            }
            1 => {
                record.lon =
                    signed(raw, field.base, be).map(|v| v as f64 * (180.0 / 2_147_483_648.0))
            }
            2 => altitude = unsigned(raw, field.base, be).map(|v| v as f64 / 5.0 - 500.0),
            3 => record.hr = unsigned(raw, field.base, be).map(|v| v as f64),
            4 => record.cadence = unsigned(raw, field.base, be).map(|v| v as f64),
            5 => record.distance = unsigned(raw, field.base, be).map(|v| v as f64 / 100.0),
            6 => speed = unsigned(raw, field.base, be).map(|v| v as f64 / 1000.0),
            7 => record.power = unsigned(raw, field.base, be).map(|v| v as f64),
            73 => enhanced_speed = unsigned(raw, field.base, be).map(|v| v as f64 / 1000.0),
            78 => enhanced_altitude = unsigned(raw, field.base, be).map(|v| v as f64 / 5.0 - 500.0),
            _ => {}
        }
    }
    cursor.take(def.developer_size)?;
    if def.global != MESG_RECORD {
        return Some(None);
    }
    record.altitude = enhanced_altitude.or(altitude);
    record.speed = enhanced_speed.or(speed);
    Some(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RecordPoint, RideSummary};
    use ride_sim::fit::{to_fit, SUB_SPORT_VIRTUAL};

    fn points(n: u32) -> Vec<RecordPoint> {
        (0..=n)
            .map(|t| RecordPoint {
                t: f64::from(t),
                unix_s: 1_700_000_000.0 + f64::from(t),
                dist: f64::from(t) * 8.0,
                lat: 47.5 + f64::from(t) * 1e-5,
                lon: 19.04,
                ele: 120.0 + f64::from(t) * 0.2,
                speed_ms: 8.0,
                power_w: Some(200.0 + f64::from(t % 7)),
                cadence_rpm: Some(88.0),
                hr_bpm: if t == 3 { None } else { Some(141.0) },
                grade: 1.0,
            })
            .collect()
    }

    #[test]
    fn our_own_export_reads_back() {
        let summary = RideSummary {
            started_unix_s: 1_700_000_000.0,
            elapsed_s: 120.0,
            dist: 960.0,
            ..RideSummary::default()
        };
        let bytes = to_fit(
            &summary,
            &points(120),
            &[(0, 60), (60, 120)],
            SUB_SPORT_VIRTUAL,
        )
        .expect("file");
        let records = read_records(&bytes).expect("records");
        assert_eq!(records.len(), 121);
        let r = records[10];
        assert_eq!(r.time, Some(1_700_000_010.0));
        assert!((r.lat.unwrap() - 47.5001).abs() < 1e-6, "{r:?}");
        assert!((r.lon.unwrap() - 19.04).abs() < 1e-6);
        assert!((r.altitude.unwrap() - 122.0).abs() < 0.21);
        assert_eq!(r.hr, Some(141.0));
        assert_eq!(r.cadence, Some(88.0));
        assert_eq!(r.distance, Some(80.0));
        assert_eq!(r.speed, Some(8.0));
        assert_eq!(r.power, Some(203.0));
        assert_eq!(records[3].hr, None, "the missing heart rate stays missing");
    }

    #[test]
    fn compressed_time_stamps_and_big_endian_messages() {
        // header (12 bytes, no CRC), a big-endian definition of `record` with a time stamp and
        // power, one normal data message, then two compressed-header ones
        let mut data = vec![
            0x40, 0, 1, 0, 20, 2, // definition: local 0, big endian, global 20, 2 fields
            253, 4, 0x86, // timestamp
            7, 2, 0x84, // power
        ];
        data.push(0x00);
        data.extend_from_slice(&1_000_000_000u32.to_be_bytes()); // ...0 0000 (low 5 bits 0)
        data.extend_from_slice(&250u16.to_be_bytes());
        // compressed: local 0, offset 3 -> 1_000_000_003
        data.push(0x80 | 3);
        data.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes()); // invalid time stamp in the body
        data.extend_from_slice(&260u16.to_be_bytes());
        // compressed: offset 1 rolls over -> 1_000_000_033
        data.push(0x80 | 1);
        data.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        data.extend_from_slice(&0xFFFFu16.to_be_bytes()); // invalid power
        let mut bytes = vec![12, 0x10, 0, 0];
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b".FIT");
        bytes.extend_from_slice(&data);
        let records = read_records(&bytes).expect("records");
        assert_eq!(records.len(), 3);
        let base = 1_000_000_000.0 + FIT_EPOCH_OFFSET;
        assert_eq!(records[0].time, Some(base));
        assert_eq!(records[0].power, Some(250.0));
        assert_eq!(records[1].time, Some(base + 3.0));
        assert_eq!(records[2].time, Some(base + 33.0));
        assert_eq!(records[2].power, None);
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        assert!(read_records(b"").is_err());
        assert!(read_records(b"not a fit file at all").is_err());
        let summary = RideSummary::default();
        let bytes = to_fit(&summary, &points(30), &[], SUB_SPORT_VIRTUAL).expect("file");
        // cut anywhere: never a panic
        for cut in 12..bytes.len() {
            let _ = read_records(&bytes[..cut]);
        }
        // a data message whose local type was never defined
        let mut bad = bytes[..14].to_vec();
        bad.push(0x05);
        assert!(read_records(&bad).is_err());
    }
}
