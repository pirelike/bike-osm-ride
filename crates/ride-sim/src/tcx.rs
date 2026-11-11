//! Garmin TCX export (Strava / Intervals.icu / TrainingPeaks compatible) and the
//! suggested file name. Written by hand: no external crates, no locale dependence.

use std::fmt::Write as _;

use ride_model::{RecordPoint, RideSummary};

/// Escapes text for XML content and attribute values. Characters XML 1.0 does not allow at all
/// (C0 controls other than tab and line breaks, U+FFFE, U+FFFF) are dropped: a route name from
/// a hand-edited file must not make the whole TCX unreadable.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Days since 1970-01-01 → (year, month, day) with Howard Hinnant's civil algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Splits a Unix timestamp into UTC calendar fields (clamped at the epoch for negatives).
fn unix_parts(unix_s: f64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = if unix_s.is_finite() {
        unix_s.floor().max(0.0) as i64
    } else {
        0
    };
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (
        y,
        m,
        d,
        (sod / 3600) as u32,
        ((sod % 3600) / 60) as u32,
        (sod % 60) as u32,
    )
}

/// `2026-10-31T17:02:07Z`
fn iso_utc(unix_s: f64) -> String {
    let (y, m, d, h, mi, s) = unix_parts(unix_s);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// `2026-10-31`
pub fn iso_date(unix_s: f64) -> String {
    let (y, m, d, _, _, _) = unix_parts(unix_s);
    format!("{y:04}-{m:02}-{d:02}")
}

fn num1(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "0.0".into()
    }
}

fn num0(v: f64) -> String {
    if v.is_finite() {
        format!("{}", v.round() as i64)
    } else {
        "0".into()
    }
}

fn clamp_latlon(v: f64, limit: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-limit, limit)
    } else {
        0.0
    }
}

/// Garmin TCX (Strava/Intervals/TrainingPeaks compatible) incl. power, HR, cadence, distance, altitude, position.
pub fn to_tcx(summary: &RideSummary, points: &[RecordPoint]) -> String {
    let start = iso_utc(summary.started_unix_s);
    let mut xml = String::with_capacity(512 + points.len() * 420);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(concat!(
        "<TrainingCenterDatabase xmlns=\"http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2\"",
        " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
        " xsi:schemaLocation=\"http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2",
        " http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2/TrainingCenterDatabasev2.xsd\">\n"
    ));
    xml.push_str("  <Activities>\n");
    xml.push_str("    <Activity Sport=\"Biking\">\n");
    let _ = writeln!(xml, "      <Id>{start}</Id>");
    let _ = writeln!(xml, "      <Lap StartTime=\"{start}\">");
    let _ = writeln!(
        xml,
        "        <TotalTimeSeconds>{}</TotalTimeSeconds>",
        num1(summary.elapsed_s.max(0.0))
    );
    let _ = writeln!(
        xml,
        "        <DistanceMeters>{}</DistanceMeters>",
        num1(summary.dist.max(0.0))
    );
    let _ = writeln!(
        xml,
        "        <MaximumSpeed>{}</MaximumSpeed>",
        num1(summary.max_speed_ms.max(0.0))
    );
    let _ = writeln!(
        xml,
        "        <Calories>{}</Calories>",
        num0(summary.kcal.max(0.0))
    );
    if let Some(hr) = summary.avg_hr.filter(|v| v.is_finite()) {
        let _ = writeln!(
            xml,
            "        <AverageHeartRateBpm><Value>{}</Value></AverageHeartRateBpm>",
            num0(hr.max(0.0))
        );
    }
    if let Some(hr) = summary.max_hr.filter(|v| v.is_finite()) {
        let _ = writeln!(
            xml,
            "        <MaximumHeartRateBpm><Value>{}</Value></MaximumHeartRateBpm>",
            num0(hr.max(0.0))
        );
    }
    xml.push_str("        <Intensity>Active</Intensity>\n");
    if let Some(c) = summary.avg_cadence.filter(|v| v.is_finite()) {
        let _ = writeln!(
            xml,
            "        <Cadence>{}</Cadence>",
            num0(c.clamp(0.0, 255.0))
        );
    }
    xml.push_str("        <TriggerMethod>Manual</TriggerMethod>\n");

    if !points.is_empty() {
        let start_dist = points
            .first()
            .map(|p| p.dist)
            .filter(|d| d.is_finite())
            .unwrap_or(0.0);
        xml.push_str("        <Track>\n");
        for p in points {
            xml.push_str("          <Trackpoint>\n");
            let _ = writeln!(xml, "            <Time>{}</Time>", iso_utc(p.unix_s));
            let _ = write!(
                xml,
                concat!(
                    "            <Position><LatitudeDegrees>{:.7}</LatitudeDegrees>",
                    "<LongitudeDegrees>{:.7}</LongitudeDegrees></Position>\n"
                ),
                clamp_latlon(p.lat, 90.0),
                clamp_latlon(p.lon, 180.0)
            );
            let _ = writeln!(
                xml,
                "            <AltitudeMeters>{}</AltitudeMeters>",
                num1(p.ele)
            );
            // From the start of the activity, not the route position: a ride may start with a
            // seek, and the lap total is the distance ridden.
            let _ = writeln!(
                xml,
                "            <DistanceMeters>{}</DistanceMeters>",
                num1((p.dist - start_dist).max(0.0))
            );
            if let Some(hr) = p.hr_bpm.filter(|v| v.is_finite()) {
                let _ = writeln!(
                    xml,
                    "            <HeartRateBpm><Value>{}</Value></HeartRateBpm>",
                    num0(hr.max(0.0))
                );
            }
            if let Some(c) = p.cadence_rpm.filter(|v| v.is_finite()) {
                let _ = writeln!(
                    xml,
                    "            <Cadence>{}</Cadence>",
                    num0(c.clamp(0.0, 255.0))
                );
            }
            // ActivityExtension v2 is an `xsd:sequence`: Speed before Watts. The speed is written
            // with or without a power value (a ride without a power meter still has one).
            let speed = Some(p.speed_ms).filter(|v| v.is_finite());
            let watts = p.power_w.filter(|v| v.is_finite());
            if speed.is_some() || watts.is_some() {
                xml.push_str(
                    "            <Extensions><TPX xmlns=\"http://www.garmin.com/xmlschemas/ActivityExtension/v2\">",
                );
                if let Some(v) = speed {
                    let _ = write!(xml, "<Speed>{}</Speed>", num1(v.max(0.0)));
                }
                if let Some(w) = watts {
                    let _ = write!(xml, "<Watts>{}</Watts>", num0(w.max(0.0)));
                }
                xml.push_str("</TPX></Extensions>\n");
            }
            xml.push_str("          </Trackpoint>\n");
        }
        xml.push_str("        </Track>\n");
    }

    xml.push_str("      </Lap>\n");
    let km = summary.dist.max(0.0) / 1000.0;
    let gain = num0(summary.ascent.max(0.0));
    let notes = format!(
        "bike-osm-ride – {} ({km:.1} km, +{gain} m)",
        summary.route_name
    );
    let _ = writeln!(xml, "      <Notes>{}</Notes>", esc(&notes));
    xml.push_str(concat!(
        "      <Creator xsi:type=\"Device_t\">\n",
        "        <Name>bike-osm-ride</Name>\n",
        "        <UnitId>0</UnitId>\n",
        "        <ProductID>0</ProductID>\n",
        "        <Version><VersionMajor>0</VersionMajor><VersionMinor>1</VersionMinor></Version>\n",
        "      </Creator>\n"
    ));
    xml.push_str("    </Activity>\n");
    xml.push_str("  </Activities>\n");
    xml.push_str("</TrainingCenterDatabase>\n");
    xml
}

/// Hungarian accents folded to ASCII, everything else that is not `[A-Za-z0-9]` dropped.
fn ascii_slug(name: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for c in name.chars() {
        let folded = match c {
            'á' | 'ä' | 'à' | 'â' => 'a',
            'Á' | 'Ä' | 'À' | 'Â' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ö' | 'ő' | 'ò' | 'ô' | 'õ' => 'o',
            'Ó' | 'Ö' | 'Ő' | 'Ò' | 'Ô' | 'Õ' => 'O',
            'ú' | 'ü' | 'ű' | 'ù' | 'û' => 'u',
            'Ú' | 'Ü' | 'Ű' | 'Ù' | 'Û' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ß' => 's',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if folded == '-' {
            if !last_dash && out.len() < max_len {
                out.push('-');
                last_dash = true;
            }
        } else if out.len() < max_len {
            out.push(folded);
            last_dash = false;
        }
    }
    out.trim_matches('-').to_string()
}

/// e.g. `2026-10-31_Budapest-korut.tcx` (ASCII, filesystem safe).
pub fn suggested_filename(summary: &RideSummary) -> String {
    let date = iso_date(summary.started_unix_s);
    let mut slug = ascii_slug(&summary.route_name, 60);
    if slug.is_empty() {
        slug = "utvonal".into();
    }
    let mut name = format!("{date}_{slug}");
    if name.len() > 120 {
        name.truncate(120);
    }
    format!("{name}.tcx")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> RideSummary {
        RideSummary {
            route_name: "Budapest-körút <&>".into(),
            route_id: "abcdef0123456789".into(),
            started_unix_s: 1_790_000_000.0, // 2026-09-21T14:13:20Z
            elapsed_s: 3725.4,
            dist: 24_120.0,
            ascent: 412.0,
            avg_speed_ms: 6.47,
            max_speed_ms: 14.2,
            avg_power_w: Some(198.4),
            max_power_w: Some(612.0),
            normalized_power_w: Some(223.1),
            intensity_factor: Some(1.12),
            tss: Some(115.5),
            work_kj: 740.0,
            kcal: 740.0,
            avg_hr: Some(142.0),
            max_hr: Some(178.0),
            avg_cadence: Some(88.0),
            power_zone_secs: [600.0, 900.0, 1200.0, 700.0, 300.0, 25.0, 0.4],
            hr_zone_secs: [100.0, 900.0, 1800.0, 800.0, 125.4],
        }
    }

    fn point(t: f64) -> RecordPoint {
        RecordPoint {
            t,
            unix_s: 1_790_000_000.0 + t,
            dist: t * 6.5,
            lat: 47.4979,
            lon: 19.0402,
            ele: 120.0 + t * 0.1,
            speed_ms: 6.5,
            power_w: Some(200.0),
            cadence_rpm: Some(90.0),
            hr_bpm: Some(140.0),
            grade: 1.5,
        }
    }

    #[test]
    fn iso_time_formatting() {
        assert_eq!(iso_utc(0.0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_790_000_000.0), "2026-09-21T14:13:20Z");
        assert_eq!(iso_date(1_790_000_000.0), "2026-09-21");
        assert_eq!(iso_utc(f64::NAN), "1970-01-01T00:00:00Z");
        // leap day
        assert_eq!(iso_date(1_709_164_800.0), "2024-02-29");
    }

    #[test]
    fn tcx_is_well_formed_and_escaped() {
        let pts: Vec<RecordPoint> = (0..10).map(|i| point(i as f64)).collect();
        let xml = to_tcx(&summary(), &pts);
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<TrainingCenterDatabase"));
        assert!(xml.contains("<Activity Sport=\"Biking\">"));
        assert!(xml.contains("<TotalTimeSeconds>3725.4</TotalTimeSeconds>"));
        assert!(xml.contains("<DistanceMeters>24120.0</DistanceMeters>"));
        assert!(xml.contains("<Calories>740</Calories>"));
        assert!(xml.contains("<Watts>200</Watts>"));
        assert!(xml.contains("<Speed>6.5</Speed>"));
        assert_eq!(xml.matches("<Trackpoint>").count(), 10);
        assert!(!xml.contains("<&>"), "route name must be escaped");
        assert!(xml.contains("&lt;&amp;&gt;"));
        assert!(xml.trim_end().ends_with("</TrainingCenterDatabase>"));
        assert_well_formed(&xml);
    }

    /// Parses the document with a real XML reader: every tag closes, nothing illegal inside.
    fn assert_well_formed(xml: &str) {
        let mut reader = quick_xml::Reader::from_str(xml);
        reader.config_mut().check_end_names = true;
        let mut depth = 0i32;
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(_)) => depth += 1,
                Ok(quick_xml::events::Event::End(_)) => depth -= 1,
                Ok(quick_xml::events::Event::Text(t)) => {
                    let text = t.xml10_content().to_string();
                    assert!(
                        !text
                            .chars()
                            .any(|c| (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r')),
                        "control character in {text:?}"
                    );
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("not well formed: {e}"),
            }
        }
        assert_eq!(depth, 0, "unbalanced elements");
    }

    #[test]
    fn a_point_without_power_keeps_its_speed_and_the_order_is_speed_then_watts() {
        let mut pts: Vec<RecordPoint> = (0..3).map(|i| point(i as f64)).collect();
        pts[1].power_w = None;
        let xml = to_tcx(&summary(), &pts);
        assert_eq!(xml.matches("<Speed>6.5</Speed>").count(), 3);
        assert_eq!(xml.matches("<Watts>").count(), 2);
        assert!(xml.contains("<Speed>6.5</Speed><Watts>200</Watts>"));
        assert_well_formed(&xml);
    }

    #[test]
    fn control_characters_are_dropped_from_names() {
        let mut s = summary();
        s.route_name = "Route\u{1}\u{FFFE}One\tTwo".into();
        let xml = to_tcx(&s, &[point(0.0)]);
        assert!(xml.contains("RouteOne\tTwo"));
        assert_well_formed(&xml);
    }

    #[test]
    fn trackpoint_distances_count_from_the_start_of_the_ride() {
        let mut pts: Vec<RecordPoint> = (0..3).map(|i| point(i as f64)).collect();
        for p in &mut pts {
            p.dist += 4_000.0;
        }
        let xml = to_tcx(&summary(), &pts);
        assert!(xml.contains("<DistanceMeters>0.0</DistanceMeters>"));
        assert!(xml.contains("<DistanceMeters>13.0</DistanceMeters>"));
        assert!(!xml.contains("<DistanceMeters>4000.0</DistanceMeters>"));
    }

    #[test]
    fn tcx_without_points_or_hr_stays_valid() {
        let mut s = summary();
        s.avg_hr = None;
        s.max_hr = None;
        s.avg_cadence = None;
        let xml = to_tcx(&s, &[]);
        assert!(!xml.contains("<Track>"));
        assert!(!xml.contains("<HeartRateBpm>"));
        assert!(xml.contains("</Activity>"));
    }

    #[test]
    fn suggested_filename_is_ascii_and_safe() {
        let name = suggested_filename(&summary());
        assert_eq!(name, "2026-09-21_Budapest-korut.tcx");
        assert!(name.is_ascii());
        assert!(!name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']));
    }

    #[test]
    fn filename_falls_back_when_the_name_is_empty() {
        let mut s = summary();
        s.route_name = "  ".into();
        assert_eq!(suggested_filename(&s), "2026-09-21_utvonal.tcx");
        s.route_name = "///".into();
        assert_eq!(suggested_filename(&s), "2026-09-21_utvonal.tcx");
    }

    #[test]
    fn ascii_slug_folds_accents_and_collapses_dashes() {
        assert_eq!(ascii_slug("Budapest-körút", 60), "Budapest-korut");
        assert_eq!(ascii_slug("Tihany  --  Balaton", 60), "Tihany-Balaton");
        assert_eq!(
            ascii_slug("Árvíztűrő tükörfúrógép", 60),
            "Arvizturo-tukorfurogep"
        );
        assert!(ascii_slug("x", 60).len() <= 60);
        assert!(ascii_slug(&"a".repeat(200), 60).len() <= 60);
    }
}
