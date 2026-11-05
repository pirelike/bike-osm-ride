//! A recorded ride from another app: GPX with sensor extensions, or TCX.
//!
//! The route reader ([`crate::gpx`]) skips everything inside `<extensions>` on purpose: a planned
//! route has no heart rate. A *recorded* activity (a Strava / Garmin export, or this app's own
//! TCX) is read here instead, with the sensors:
//!
//! * GPX — `<trkpt lat lon>` with `<ele>`, `<time>` and, anywhere below the point (the
//!   extensions, in any namespace), `hr` / `heartrate`, `cad` / `cadence` and `power` / `watts` /
//!   `powerinwatts`. Only track points count; waypoints and route points are not an activity.
//! * TCX — `<Trackpoint>` with `Time`, `Position`, `AltitudeMeters`, `HeartRateBpm/Value`,
//!   `Cadence` and `Extensions/TPX/Watts`. A sport other than `Biking` is refused.
//!
//! A sample needs a valid position and a time stamp; the others are dropped and counted in
//! [`Activity::skipped`]. Out-of-range sensor values (a heart rate of 0 or 400) are `None`.
//! Messages are Hungarian: they reach a toast.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::gpx::{decode_bytes, parse_iso8601, parse_number};
use crate::{err, geo, Result};

/// A recording needs at least this many usable samples.
const MIN_SAMPLES: usize = 10;
/// Plausible sensor ranges; anything outside is a sensor glitch, not data.
const HR_RANGE: std::ops::RangeInclusive<f64> = 30.0..=250.0;
const CADENCE_RANGE: std::ops::RangeInclusive<f64> = 0.0..=250.0;
const POWER_RANGE: std::ops::RangeInclusive<f64> = 0.0..=3000.0;

/// Which file format an activity came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityFormat {
    Gpx,
    Tcx,
}

/// One time-stamped sample of the recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivitySample {
    /// Seconds since the Unix epoch.
    pub time: f64,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    pub hr: Option<f64>,
    pub cadence: Option<f64>,
    pub power: Option<f64>,
}

/// A parsed recording, in file order with strictly increasing time.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub name: String,
    pub format: ActivityFormat,
    pub samples: Vec<ActivitySample>,
    /// Track points left out: no position, no time, or not later than the one before.
    pub skipped: usize,
}

impl Activity {
    pub fn has_power(&self) -> bool {
        self.samples.iter().any(|s| s.power.is_some())
    }

    pub fn has_hr(&self) -> bool {
        self.samples.iter().any(|s| s.hr.is_some())
    }

    pub fn has_cadence(&self) -> bool {
        self.samples.iter().any(|s| s.cadence.is_some())
    }
}

/// Reads a GPX or TCX file (the format is recognised from the content, not the file name).
pub fn from_bytes(bytes: &[u8]) -> Result<Activity> {
    let text = decode_bytes(bytes);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.is_empty() {
        return err("A fájl üres.");
    }
    let head: String = text.chars().take(4096).collect();
    if head.contains("<TrainingCenterDatabase") {
        parse_tcx(text)
    } else if head.to_ascii_lowercase().contains("<gpx") {
        parse_gpx(text)
    } else {
        err("A fájl sem GPX-, sem TCX-fájl. Az edzés importálásához GPX vagy TCX kell.")
    }
}

#[derive(Default)]
struct Pending {
    time: Option<f64>,
    lat: Option<f64>,
    lon: Option<f64>,
    ele: Option<f64>,
    hr: Option<f64>,
    cadence: Option<f64>,
    power: Option<f64>,
}

#[derive(Default)]
struct Collector {
    samples: Vec<ActivitySample>,
    skipped: usize,
}

impl Collector {
    fn push(&mut self, p: Pending) {
        let position = match (p.lat, p.lon) {
            (Some(lat), Some(lon))
                if geo::is_valid_coord(lat, lon) && (lat != 0.0 || lon != 0.0) =>
            {
                Some((lat, lon))
            }
            _ => None,
        };
        let (Some((lat, lon)), Some(time)) = (position, p.time.filter(|t| t.is_finite())) else {
            self.skipped += 1;
            return;
        };
        if self.samples.last().is_some_and(|last| time <= last.time) {
            self.skipped += 1;
            return;
        }
        let within = |v: Option<f64>, range: &std::ops::RangeInclusive<f64>| {
            v.filter(|x| x.is_finite() && range.contains(x))
        };
        self.samples.push(ActivitySample {
            time,
            lat,
            lon,
            ele: p.ele.filter(|e| e.is_finite()),
            hr: within(p.hr, &HR_RANGE),
            cadence: within(p.cadence, &CADENCE_RANGE),
            power: within(p.power, &POWER_RANGE),
        });
    }

    fn finish(self, name: String, format: ActivityFormat) -> Result<Activity> {
        if self.samples.len() < MIN_SAMPLES {
            return if self.skipped > self.samples.len() {
                err("A fájl pontjainak többségéből hiányzik az időbélyeg vagy a pozíció, ezért nem importálható edzésként. Az útvonalak betöltése az Útvonal ablakban lehetséges.")
            } else {
                err("A fájlban túl kevés (időbélyeges) pont van egy edzéshez.")
            };
        }
        Ok(Activity {
            name,
            format,
            samples: self.samples,
            skipped: self.skipped,
        })
    }
}

fn local_name(e: &BytesStart<'_>) -> String {
    e.local_name().as_ref().to_ascii_lowercase()
}

fn attribute(e: &BytesStart<'_>, wanted: &[&str]) -> Option<String> {
    let mut attrs = e.attributes();
    attrs.with_checks(false);
    for a in attrs.flatten() {
        let key = a.key.local_name().as_ref().to_ascii_lowercase();
        if wanted.contains(&key.as_str()) {
            return Some(a.value.to_string());
        }
    }
    None
}

fn xml_error(text: &str, reader: &Reader<&[u8]>, what: &str) -> crate::RouteError {
    let end = usize::try_from(reader.error_position())
        .unwrap_or(usize::MAX)
        .min(text.len());
    let line = text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    crate::RouteError::Message(format!(
        "A {what}-fájl hibás vagy sérült (XML-hiba a(z) {line}. sor környékén)."
    ))
}

/// A Strava / Garmin / Wahoo GPX activity.
pub fn parse_gpx(text: &str) -> Result<Activity> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut pending: Option<Pending> = None;
    let mut collector = Collector::default();
    let mut name = String::new();
    let mut saw_root = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(text, &reader, "GPX"))?;
        let (start, empty) = match &event {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            let tag = local_name(e);
            if stack.is_empty() {
                if tag != "gpx" {
                    return err("A fájl nem GPX-fájl (hiányzik a <gpx> gyökérelem).");
                }
                saw_root = true;
            }
            if tag == "trkpt" {
                pending = Some(Pending {
                    lat: attribute(e, &["lat"]).and_then(|v| parse_number(&v)),
                    lon: attribute(e, &["lon", "lng"]).and_then(|v| parse_number(&v)),
                    ..Pending::default()
                });
            }
            buf.clear();
            if empty {
                if tag == "trkpt" {
                    if let Some(p) = pending.take() {
                        collector.push(p);
                    }
                }
            } else {
                stack.push(tag);
            }
            continue;
        }
        match event {
            Event::Text(t) => buf.push_str(&t.xml10_content()),
            Event::CData(t) => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    buf.push(ch);
                } else {
                    match &*r {
                        "amp" => buf.push('&'),
                        "lt" => buf.push('<'),
                        "gt" => buf.push('>'),
                        "quot" => buf.push('"'),
                        "apos" => buf.push('\''),
                        _ => {}
                    }
                }
            }
            Event::End(_) => {
                let Some(tag) = stack.pop() else { continue };
                let parent = stack.last().map(String::as_str);
                match (tag.as_str(), pending.as_mut()) {
                    ("trkpt", _) => {
                        if let Some(p) = pending.take() {
                            collector.push(p);
                        }
                    }
                    ("time", Some(p)) => p.time = parse_iso8601(&buf),
                    ("ele", Some(p)) => p.ele = parse_number(&buf),
                    ("hr" | "heartrate", Some(p)) => p.hr = parse_number(&buf),
                    ("cad" | "cadence", Some(p)) => p.cadence = parse_number(&buf),
                    ("power" | "watts" | "powerinwatts", Some(p)) => p.power = parse_number(&buf),
                    ("name", None) if parent == Some("trk") && name.is_empty() => {
                        name = clean(&buf);
                    }
                    _ => {}
                }
                buf.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return err("A fájl nem GPX-fájl (nem található <gpx> elem).");
    }
    if !stack.is_empty() {
        return err("A GPX-fájl csonka vagy sérült (hiányzó záró elemek).");
    }
    collector.finish(name, ActivityFormat::Gpx)
}

/// A Garmin TrainingCenterDatabase activity (also what this app exports).
pub fn parse_tcx(text: &str) -> Result<Activity> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut pending: Option<Pending> = None;
    let mut collector = Collector::default();
    let mut notes = String::new();
    let mut saw_root = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(text, &reader, "TCX"))?;
        let (start, empty) = match &event {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            let tag = local_name(e);
            if stack.is_empty() {
                if tag != "trainingcenterdatabase" {
                    return err(
                        "A fájl nem TCX-fájl (hiányzik a <TrainingCenterDatabase> gyökérelem).",
                    );
                }
                saw_root = true;
            }
            if tag == "activity" {
                if let Some(sport) = attribute(e, &["sport"]) {
                    if !sport.eq_ignore_ascii_case("biking") {
                        return err(format!(
                            "A TCX-fájl nem kerékpáros edzés (Sport = {sport}), ezért nem importálható."
                        ));
                    }
                }
            }
            if tag == "trackpoint" {
                pending = Some(Pending::default());
            }
            buf.clear();
            if empty {
                if tag == "trackpoint" {
                    if let Some(p) = pending.take() {
                        collector.push(p);
                    }
                }
            } else {
                stack.push(tag);
            }
            continue;
        }
        match event {
            Event::Text(t) => buf.push_str(&t.xml10_content()),
            Event::CData(t) => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    buf.push(ch);
                } else {
                    match &*r {
                        "amp" => buf.push('&'),
                        "lt" => buf.push('<'),
                        "gt" => buf.push('>'),
                        "quot" => buf.push('"'),
                        "apos" => buf.push('\''),
                        _ => {}
                    }
                }
            }
            Event::End(_) => {
                let Some(tag) = stack.pop() else { continue };
                let parent = stack.last().map(String::as_str);
                match (tag.as_str(), pending.as_mut()) {
                    ("trackpoint", _) => {
                        if let Some(p) = pending.take() {
                            collector.push(p);
                        }
                    }
                    ("time", Some(p)) => p.time = parse_iso8601(&buf),
                    ("latitudedegrees", Some(p)) => p.lat = parse_number(&buf),
                    ("longitudedegrees", Some(p)) => p.lon = parse_number(&buf),
                    ("altitudemeters", Some(p)) => p.ele = parse_number(&buf),
                    ("value", Some(p)) if parent == Some("heartratebpm") => {
                        p.hr = parse_number(&buf)
                    }
                    ("cadence", Some(p)) => p.cadence = parse_number(&buf),
                    ("watts", Some(p)) => p.power = parse_number(&buf),
                    ("notes", None) if notes.is_empty() => notes = clean(&buf),
                    _ => {}
                }
                buf.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return err("A fájl nem TCX-fájl (nem található <TrainingCenterDatabase> elem).");
    }
    if !stack.is_empty() {
        return err("A TCX-fájl csonka vagy sérült (hiányzó záró elemek).");
    }
    collector.finish(notes, ActivityFormat::Tcx)
}

fn clean(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RecordPoint, RideSummary};

    fn strava_gpx(points: usize) -> String {
        let mut s = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx creator="StravaGPX" xmlns="http://www.topografix.com/GPX/1/1"
 xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v1">
 <metadata><time>2026-10-04T07:30:00Z</time></metadata>
 <trk><name>Reggeli &amp; kör</name><type>cycling</type><trkseg>
"#,
        );
        for i in 0..points {
            let lat = 47.5 + i as f64 * 0.0001;
            s.push_str(&format!(
                r#"<trkpt lat="{lat:.6}" lon="19.040000"><ele>{:.1}</ele><time>2026-10-04T07:30:{:02}Z</time>
 <extensions><power>{}</power><gpxtpx:TrackPointExtension><gpxtpx:atemp>14</gpxtpx:atemp><gpxtpx:hr>{}</gpxtpx:hr><gpxtpx:cad>{}</gpxtpx:cad></gpxtpx:TrackPointExtension></extensions>
 </trkpt>
"#,
                100.0 + i as f64 * 0.5,
                i % 60,
                180 + i,
                120 + i,
                80 + i % 5
            ));
        }
        s.push_str("</trkseg></trk></gpx>");
        s
    }

    #[test]
    fn a_strava_style_gpx_gives_the_sensors() {
        let a = from_bytes(strava_gpx(30).as_bytes()).unwrap();
        assert_eq!(a.format, ActivityFormat::Gpx);
        assert_eq!(a.name, "Reggeli & kör");
        assert_eq!(a.samples.len(), 30);
        assert_eq!(a.skipped, 0);
        let s = a.samples[3];
        assert_eq!(s.power, Some(183.0));
        assert_eq!(s.hr, Some(123.0));
        assert_eq!(s.cadence, Some(83.0));
        assert_eq!(s.ele, Some(101.5));
        assert!((s.lat - 47.5003).abs() < 1e-9);
        assert!(a.has_power() && a.has_hr() && a.has_cadence());
        // one-second spacing, from the ISO stamps
        assert!((a.samples[1].time - a.samples[0].time - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_gpx_without_power_is_fine_and_reports_it() {
        let text = strava_gpx(20)
            .replace("<power>", "<x>")
            .replace("</power>", "</x>");
        let a = parse_gpx(&text).unwrap();
        assert!(!a.has_power());
        assert!(a.has_hr());
    }

    #[test]
    fn sensor_glitches_become_missing_values_and_bad_points_are_counted() {
        let mut text = strava_gpx(20);
        text = text.replacen("<gpxtpx:hr>120</gpxtpx:hr>", "<gpxtpx:hr>0</gpxtpx:hr>", 1);
        text = text.replacen("<power>181</power>", "<power>99999</power>", 1);
        // a point without a time, one at 0/0 and one that goes back in time
        text = text.replacen(
            "</trkseg>",
            r#"<trkpt lat="47.51" lon="19.04"><ele>1</ele></trkpt>
               <trkpt lat="0" lon="0"><time>2026-10-04T08:00:00Z</time></trkpt>
               <trkpt lat="47.52" lon="19.04"><time>2026-10-04T07:00:00Z</time></trkpt></trkseg>"#,
            1,
        );
        let a = parse_gpx(&text).unwrap();
        assert_eq!(a.samples.len(), 20);
        assert_eq!(a.skipped, 3);
        assert_eq!(a.samples[0].hr, None, "a heart rate of 0 is a glitch");
        assert_eq!(a.samples[1].power, None, "so is 99 999 W");
        assert_eq!(a.samples[2].power, Some(182.0));
    }

    #[test]
    fn this_apps_own_tcx_reads_back() {
        let points: Vec<RecordPoint> = (0..60)
            .map(|i| RecordPoint {
                t: i as f64,
                unix_s: 1_700_000_000.0 + i as f64,
                dist: i as f64 * 8.0,
                lat: 47.5 + i as f64 * 0.0001,
                lon: 19.0,
                ele: 120.0 + i as f64 * 0.3,
                speed_ms: 8.0,
                power_w: Some(210.0),
                cadence_rpm: Some(88.0),
                hr_bpm: Some(150.0),
                grade: 2.0,
            })
            .collect();
        let summary = RideSummary {
            route_name: "Teszt & kör".into(),
            route_id: "id".into(),
            started_unix_s: 1_700_000_000.0,
            elapsed_s: 59.0,
            dist: 472.0,
            ascent: 18.0,
            avg_speed_ms: 8.0,
            max_speed_ms: 8.0,
            avg_power_w: Some(210.0),
            max_power_w: Some(210.0),
            normalized_power_w: Some(210.0),
            intensity_factor: None,
            tss: None,
            work_kj: 12.0,
            kcal: 12.0,
            avg_hr: Some(150.0),
            max_hr: Some(150.0),
            avg_cadence: Some(88.0),
            power_zone_secs: [0.0; 7],
            hr_zone_secs: [0.0; 5],
        };
        let tcx = ride_sim::tcx::to_tcx(&summary, &points);
        let a = from_bytes(tcx.as_bytes()).unwrap();
        assert_eq!(a.format, ActivityFormat::Tcx);
        assert!(a.name.contains("Teszt & kör"), "{}", a.name);
        assert_eq!(a.samples.len(), 60);
        let s = a.samples[10];
        assert_eq!(
            (s.power, s.hr, s.cadence),
            (Some(210.0), Some(150.0), Some(88.0))
        );
        assert!((s.lat - 47.501).abs() < 1e-6);
        assert!((s.ele.unwrap() - 123.0).abs() < 0.06);
        assert!((s.time - 1_700_000_010.0).abs() < 1e-6);
    }

    #[test]
    fn foreign_and_broken_files_get_hungarian_errors() {
        for (bytes, needle) in [
            (&b""[..], "üres"),
            (&b"ez csak szoveg"[..], "sem GPX-"),
            (
                &b"<gpx><trk><trkseg><trkpt lat=\"47\" lon=\"19\">"[..],
                "csonka",
            ),
            (&b"<gpx><trk><trkseg></trkseg></trk></gpx>"[..], "túl kevés"),
        ] {
            let e = from_bytes(bytes).unwrap_err().to_string();
            assert!(e.contains(needle), "{e}");
        }
        let running = r#"<TrainingCenterDatabase><Activities><Activity Sport="Running"/></Activities></TrainingCenterDatabase>"#;
        let e = from_bytes(running.as_bytes()).unwrap_err().to_string();
        assert!(e.contains("nem kerékpáros"), "{e}");
        // time stamps are mandatory: a planned route is not an activity
        let planned = r#"<gpx><trk><trkseg>
            <trkpt lat="47.5" lon="19.0"/><trkpt lat="47.501" lon="19.0"/><trkpt lat="47.502" lon="19.0"/>
            <trkpt lat="47.503" lon="19.0"/><trkpt lat="47.504" lon="19.0"/><trkpt lat="47.505" lon="19.0"/>
            <trkpt lat="47.506" lon="19.0"/><trkpt lat="47.507" lon="19.0"/><trkpt lat="47.508" lon="19.0"/>
            <trkpt lat="47.509" lon="19.0"/><trkpt lat="47.510" lon="19.0"/></trkseg></trk></gpx>"#;
        let e = from_bytes(planned.as_bytes()).unwrap_err().to_string();
        assert!(e.contains("időbélyeg"), "{e}");
    }

    #[test]
    fn utf16_and_bom_files_are_read() {
        let text = strava_gpx(12);
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(from_bytes(&utf16).unwrap().samples.len(), 12);
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(text.as_bytes());
        assert_eq!(from_bytes(&bom).unwrap().samples.len(), 12);
    }
}
