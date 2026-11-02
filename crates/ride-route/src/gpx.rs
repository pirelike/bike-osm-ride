//! Tolerant GPX reader (event based, `quick-xml`).
//!
//! What is accepted:
//! * GPX 1.0 / 1.1 and anything that merely *looks* like GPX: only the local element
//!   names matter, XML namespaces and prefixes (`<gpx:trkpt>`) are ignored, unknown
//!   elements and everything inside `<extensions>` is skipped.
//! * `<trk>/<trkseg>/<trkpt>`: every segment of every track is joined in file order.
//!   If the file has no track points, `<rte>/<rtept>` is used instead.
//! * A further `<trk>` that lies on the main (longest) track and is much shorter than it — a
//!   Strava segment exported as GPX and added to the route file, or a track marked
//!   `<type>segment</type>` — is not joined: it is kept in [`GpxData::segment_tracks`] and
//!   becomes a named segment of the route.
//! * `<wpt>` waypoints (name, position, optional elevation) are collected separately.
//! * `<ele>` is optional per point. Comma decimals (`254,5`) are tolerated.
//! * UTF-8 with/without BOM, UTF-16 with BOM; invalid UTF-8 is read as Latin-1.
//! * The BRouter header comment (`track-length = … filtered ascend = …`) is exposed in
//!   [`GpxData::brouter`] so callers can cross-check their own numbers.
//!
//! Points without a valid `lat`/`lon` (or at exactly 0°/0°, a typical GPS dropout
//! value) are skipped and counted in [`GpxData::skipped_points`]. Ill-formed XML
//! (truncated or mismatched tags) is an error: a silently shortened ride would be
//! worse than a clear message.

use std::borrow::Cow;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::{err, geo, Result};

/// One point as found in the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawPoint {
    pub lat: f64,
    pub lon: f64,
    /// `None` when the file has no (parsable) `<ele>` for this point.
    pub ele: Option<f64>,
    /// `<time>` of the point, seconds since the Unix epoch. `None` when the file has none, or it
    /// could not be parsed (BRouter routes carry no times at all).
    pub time: Option<f64>,
}

/// A `<wpt>` of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWaypoint {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
}

/// A `<trk>` of the file that marks a section of the route instead of extending it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTrack {
    pub name: String,
    pub points: Vec<RawPoint>,
}

/// Where [`GpxData::points`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointOrigin {
    Track,
    Route,
}

/// Numbers from the comment BRouter writes right after the XML declaration, e.g.
/// `track-length = 29404 filtered ascend = 747 plain-ascend = 432 cost=54504 …`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BrouterHeader {
    pub track_length_m: Option<f64>,
    pub filtered_ascend_m: Option<f64>,
    pub plain_ascend_m: Option<f64>,
}

/// Result of parsing a GPX document.
#[derive(Debug, Clone, Default)]
pub struct GpxData {
    /// Track name, else route name, else metadata name.
    pub name: Option<String>,
    /// All track points (segments joined) or, failing that, all route points.
    pub points: Vec<RawPoint>,
    pub origin: Option<PointOrigin>,
    /// Number of `<trkseg>` elements seen.
    pub segments: usize,
    pub waypoints: Vec<RawWaypoint>,
    /// Tracks that lie on the route and name a section of it (see the module docs).
    pub segment_tracks: Vec<RawTrack>,
    /// Points dropped because of a missing/invalid position.
    pub skipped_points: usize,
    pub brouter: Option<BrouterHeader>,
}

/// Decode raw file bytes to text: UTF-8 (BOM stripped), UTF-16 (BOM), else Latin-1.
pub fn decode_bytes(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest);
    }
    let utf16 = |rest: &[u8], le: bool| -> String {
        let units = rest.as_chunks::<2>().0.iter().map(|&c| {
            if le {
                u16::from_le_bytes(c)
            } else {
                u16::from_be_bytes(c)
            }
        });
        char::decode_utf16(units)
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect()
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Cow::Owned(utf16(rest, true));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Cow::Owned(utf16(rest, false));
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| char::from(b)).collect()),
    }
}

/// Parse raw bytes (see [`decode_bytes`]).
pub fn parse_bytes(bytes: &[u8]) -> Result<GpxData> {
    parse(&decode_bytes(bytes))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum El {
    Gpx,
    Metadata,
    Trk,
    Trkseg,
    Trkpt,
    Rte,
    Rtept,
    Wpt,
    Ele,
    Name,
    Time,
    Type,
    Extensions,
    Other,
}

fn classify(local: &str) -> El {
    const TABLE: [(&str, El); 13] = [
        ("gpx", El::Gpx),
        ("metadata", El::Metadata),
        ("trk", El::Trk),
        ("trkseg", El::Trkseg),
        ("trkpt", El::Trkpt),
        ("rte", El::Rte),
        ("rtept", El::Rtept),
        ("wpt", El::Wpt),
        ("ele", El::Ele),
        ("name", El::Name),
        ("time", El::Time),
        ("type", El::Type),
        ("extensions", El::Extensions),
    ];
    TABLE
        .iter()
        .find(|(n, _)| local.eq_ignore_ascii_case(n))
        .map_or(El::Other, |&(_, e)| e)
}

struct PtState {
    kind: El,
    coord: Option<(f64, f64)>,
    ele: Option<f64>,
    time: Option<f64>,
    name: String,
}

#[derive(Default)]
struct TrackSpan {
    start: usize,
    name: String,
    kind: String,
}

#[derive(Default)]
struct Parser {
    path: Vec<El>,
    ext_depth: usize,
    text: String,
    pt: Option<PtState>,
    data: GpxData,
    track_pts: Vec<RawPoint>,
    route_pts: Vec<RawPoint>,
    trk_name: Option<String>,
    /// One entry per `<trk>`: where its points start in `track_pts`, its name and `<type>`.
    tracks: Vec<TrackSpan>,
    rte_name: Option<String>,
    meta_name: Option<String>,
    saw_root: bool,
}

impl Parser {
    fn start(&mut self, el: El, e: &BytesStart<'_>) -> Result<()> {
        if self.path.is_empty() {
            if el != El::Gpx {
                return err("A fájl nem GPX-fájl (hiányzik a <gpx> gyökérelem).");
            }
            self.saw_root = true;
        }
        match el {
            El::Trkpt | El::Rtept | El::Wpt if self.ext_depth == 0 && self.pt.is_none() => {
                self.pt = Some(PtState {
                    kind: el,
                    coord: read_coord(e),
                    ele: None,
                    time: None,
                    name: String::new(),
                });
            }
            El::Trkseg if self.ext_depth == 0 => self.data.segments += 1,
            El::Trk if self.ext_depth == 0 => self.tracks.push(TrackSpan {
                start: self.track_pts.len(),
                ..TrackSpan::default()
            }),
            El::Extensions => self.ext_depth += 1,
            _ => {}
        }
        self.text.clear();
        self.path.push(el);
        Ok(())
    }

    fn end(&mut self) {
        let Some(el) = self.path.pop() else { return };
        let parent = self.path.last().copied();
        match el {
            El::Ele => {
                if let (Some(pt), Some(p)) = (self.pt.as_mut(), parent) {
                    if p == pt.kind {
                        pt.ele = parse_number(&self.text);
                    }
                }
            }
            El::Time => {
                if let (Some(pt), Some(p)) = (self.pt.as_mut(), parent) {
                    if p == pt.kind {
                        pt.time = parse_iso8601(&self.text);
                    }
                }
            }
            El::Name => {
                let name = clean_name(&self.text);
                if parent == Some(El::Trk) && self.ext_depth == 0 {
                    if let Some(span) = self.tracks.last_mut() {
                        span.name = name.clone();
                    }
                }
                match (parent, self.pt.as_mut()) {
                    (Some(p), Some(pt)) if p == pt.kind => pt.name = name,
                    (Some(El::Trk), _) if self.trk_name.is_none() && !name.is_empty() => {
                        self.trk_name = Some(name)
                    }
                    (Some(El::Rte), _) if self.rte_name.is_none() && !name.is_empty() => {
                        self.rte_name = Some(name)
                    }
                    (Some(El::Metadata), _) if self.meta_name.is_none() && !name.is_empty() => {
                        self.meta_name = Some(name)
                    }
                    _ => {}
                }
            }
            El::Trkpt | El::Rtept | El::Wpt => {
                if self.pt.as_ref().is_some_and(|p| p.kind == el) {
                    if let Some(pt) = self.pt.take() {
                        self.finish_point(pt);
                    }
                }
            }
            El::Type if parent == Some(El::Trk) && self.ext_depth == 0 => {
                if let Some(span) = self.tracks.last_mut() {
                    span.kind = self.text.trim().to_string();
                }
            }
            El::Extensions => self.ext_depth = self.ext_depth.saturating_sub(1),
            _ => {}
        }
        self.text.clear();
    }

    fn finish_point(&mut self, pt: PtState) {
        let valid = pt
            .coord
            .filter(|&(lat, lon)| geo::is_valid_coord(lat, lon) && !(lat == 0.0 && lon == 0.0));
        match (pt.kind, valid) {
            (El::Wpt, Some((lat, lon))) => self.data.waypoints.push(RawWaypoint {
                name: pt.name,
                lat,
                lon,
                ele: pt.ele,
            }),
            (El::Wpt, None) => {}
            (El::Trkpt, Some((lat, lon))) => self.track_pts.push(RawPoint {
                lat,
                lon,
                ele: pt.ele,
                time: pt.time,
            }),
            (El::Rtept, Some((lat, lon))) => self.route_pts.push(RawPoint {
                lat,
                lon,
                ele: pt.ele,
                time: pt.time,
            }),
            _ => self.data.skipped_points += 1,
        }
    }

    fn push_text(&mut self, s: &str) {
        if matches!(
            self.path.last(),
            Some(El::Ele | El::Name | El::Time | El::Type)
        ) {
            self.text.push_str(s);
        }
    }

    fn comment(&mut self, c: &str) {
        if self.data.brouter.is_none() && c.contains("track-length") {
            self.data.brouter = Some(BrouterHeader {
                track_length_m: number_after(c, "track-length"),
                filtered_ascend_m: number_after(c, "filtered ascend"),
                plain_ascend_m: number_after(c, "plain-ascend"),
            });
        }
    }
}

fn read_coord(e: &BytesStart<'_>) -> Option<(f64, f64)> {
    let (mut lat, mut lon) = (None, None);
    let mut attrs = e.attributes();
    attrs.with_checks(false);
    for a in attrs.flatten() {
        let key = a.key.local_name();
        let key = key.as_ref();
        if key.eq_ignore_ascii_case("lat") {
            lat = parse_number(&a.value);
        } else if key.eq_ignore_ascii_case("lon") || key.eq_ignore_ascii_case("lng") {
            lon = parse_number(&a.value);
        }
    }
    Some((lat?, lon?))
}

/// Parse a finite decimal number; a decimal comma is accepted when there is no dot.
fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let v = match t.parse::<f64>() {
        Ok(v) => v,
        Err(_) if !t.contains('.') && t.matches(',').count() == 1 => {
            t.replace(',', ".").parse::<f64>().ok()?
        }
        Err(_) => return None,
    };
    v.is_finite().then_some(v)
}

fn clean_name(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(200).collect()
}

/// Seconds since the Unix epoch from an ISO 8601 / RFC 3339 timestamp.
///
/// GPX `<time>` is `2025-07-02T17:14:10Z` (Strava, Garmin); a fractional second and a numeric
/// offset (`+02:00`) are accepted too, because the schema allows both. Anything else is `None`:
/// a wrong timeline is worse than no timeline.
pub fn parse_iso8601(s: &str) -> Option<f64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    if !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> { s.get(range)?.parse().ok() };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut rest = s.get(19..)?;
    let mut fraction = 0.0;
    if let Some(after) = rest.strip_prefix('.') {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        fraction = format!("0.{digits}").parse::<f64>().ok()?;
        rest = &after[digits.len()..];
    }
    let offset_s = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let body = &rest[1..];
            let two = |start: usize| -> Option<i64> { body.get(start..start + 2)?.parse().ok() };
            let (hours, minutes) = match body.len() {
                5 if body.as_bytes()[2] == b':' => (two(0)?, two(3)?),
                4 => (two(0)?, two(2)?),
                2 => (two(0)?, 0),
                _ => return None,
            };
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset_s;
    Some(seconds as f64 + fraction)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// First number that follows `key` (skipping spaces and `=`) in `text`.
fn number_after(text: &str, key: &str) -> Option<f64> {
    let rest = &text[text.find(key)? + key.len()..];
    let rest = rest.trim_start_matches(|c: char| c == '=' || c == ':' || c.is_whitespace());
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.')))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn line_of(text: &str, byte_pos: u64) -> usize {
    let end = usize::try_from(byte_pos)
        .unwrap_or(usize::MAX)
        .min(text.len());
    // `end` may fall inside a multi-byte character; count over the bytes.
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Parse a GPX document.
pub fn parse(text: &str) -> Result<GpxData> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.is_empty() {
        return err("A fájl üres.");
    }
    let mut reader = Reader::from_str(text);
    let mut p = Parser::default();

    loop {
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                let line = line_of(text, reader.error_position());
                // The parser's own message is English and technical; keep the UI text Hungarian.
                let _ = e;
                return err(format!(
                    "A GPX-fájl hibás vagy sérült (XML-hiba a(z) {line}. sor környékén). Próbáld meg újra letölteni vagy exportálni."
                ));
            }
        };
        match ev {
            Event::Start(e) => {
                let el = classify(e.local_name().as_ref());
                p.start(el, &e)?;
            }
            Event::Empty(e) => {
                let el = classify(e.local_name().as_ref());
                p.start(el, &e)?;
                p.end();
            }
            Event::End(_) => p.end(),
            Event::Text(t) => p.push_text(&t.xml10_content()),
            Event::CData(t) => p.push_text(&t.xml10_content()),
            Event::GeneralRef(r) => {
                if matches!(p.path.last(), Some(El::Ele | El::Name)) {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        p.text.push(ch);
                    } else {
                        match &*r {
                            "amp" => p.text.push('&'),
                            "lt" => p.text.push('<'),
                            "gt" => p.text.push('>'),
                            "quot" => p.text.push('"'),
                            "apos" => p.text.push('\''),
                            _ => {}
                        }
                    }
                }
            }
            Event::Comment(c) => p.comment(&c.xml10_content()),
            Event::Eof => break,
            _ => {}
        }
    }

    if !p.saw_root {
        return err("A fájl nem GPX-fájl (nem található <gpx> elem).");
    }
    if !p.path.is_empty() {
        return err("A GPX-fájl csonka vagy sérült (hiányzó záró elemek). Próbáld meg újra letölteni vagy exportálni.");
    }

    let mut data = p.data;
    if !p.track_pts.is_empty() {
        let (points, segment_tracks) = split_segment_tracks(p.track_pts, &p.tracks);
        data.points = points;
        data.segment_tracks = segment_tracks;
        data.origin = Some(PointOrigin::Track);
    } else if !p.route_pts.is_empty() {
        data.points = p.route_pts;
        data.origin = Some(PointOrigin::Route);
    }
    data.name = p.trk_name.or(p.rte_name).or(p.meta_name);
    if data.points.is_empty() {
        return err("A GPX-fájl nem tartalmaz útvonalpontokat (trkpt vagy rtept).");
    }
    Ok(data)
}

/// A segment track must stay this close to the main track along its whole length, metres.
const SEGMENT_ON_ROUTE_M: f64 = 35.0;

/// An unmarked track counts as a segment only below this share of the main track's length (a
/// return leg written as a second track lies on the outbound leg too, but is as long as it).
const SEGMENT_MAX_SHARE: f64 = 0.6;

/// Track points sampled to decide whether a track lies on the main one.
const SEGMENT_SAMPLES: usize = 40;

fn track_length(points: &[RawPoint]) -> f64 {
    points
        .windows(2)
        .map(|w| geo::haversine_m(w[0].lat, w[0].lon, w[1].lat, w[1].lon))
        .sum()
}

/// Distance (m) of `p` from the polyline `line`, in a local flat projection around `p`.
fn dist_to_polyline(p: &RawPoint, line: &[RawPoint]) -> f64 {
    let kx = 111_320.0 * p.lat.to_radians().cos();
    let ky = 110_540.0;
    let xy = |q: &RawPoint| ((q.lon - p.lon) * kx, (q.lat - p.lat) * ky);
    if line.len() == 1 {
        let (x, y) = xy(&line[0]);
        return x.hypot(y);
    }
    let mut best = f64::INFINITY;
    for w in line.windows(2) {
        let (ax, ay) = xy(&w[0]);
        let (bx, by) = xy(&w[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (cx, cy) = (ax + t * dx, ay + t * dy);
        best = best.min(cx * cx + cy * cy);
    }
    best.sqrt()
}

/// Every sampled point of `track` lies within [`SEGMENT_ON_ROUTE_M`] of `main`.
fn lies_on(track: &[RawPoint], main: &[RawPoint]) -> bool {
    if track.len() < 2 || main.len() < 2 {
        return false;
    }
    let step = (track.len() / SEGMENT_SAMPLES).max(1);
    track
        .iter()
        .step_by(step)
        .chain(track.last())
        .all(|p| dist_to_polyline(p, main) <= SEGMENT_ON_ROUTE_M)
}

/// Splits the track points into the route (the tracks joined in file order) and the tracks that
/// only mark a section of it: those that lie on the longest track and are either marked
/// `<type>segment</type>` or much shorter than it.
fn split_segment_tracks(
    points: Vec<RawPoint>,
    spans: &[TrackSpan],
) -> (Vec<RawPoint>, Vec<RawTrack>) {
    if spans.len() < 2 {
        return (points, Vec::new());
    }
    let ranges: Vec<(usize, usize)> = spans
        .iter()
        .enumerate()
        .map(|(i, span)| {
            let end = spans.get(i + 1).map_or(points.len(), |next| next.start);
            (span.start.min(end), end)
        })
        .collect();
    let lengths: Vec<f64> = ranges
        .iter()
        .map(|&(a, b)| track_length(&points[a..b]))
        .collect();
    let Some(main) = (0..spans.len()).max_by(|&a, &b| lengths[a].total_cmp(&lengths[b])) else {
        return (points, Vec::new());
    };
    let main_pts = &points[ranges[main].0..ranges[main].1];
    let is_segment: Vec<bool> = (0..spans.len())
        .map(|i| {
            if i == main {
                return false;
            }
            let marked = spans[i].kind.to_ascii_lowercase().contains("segment");
            let short = lengths[i] < lengths[main] * SEGMENT_MAX_SHARE;
            (marked || short) && lies_on(&points[ranges[i].0..ranges[i].1], main_pts)
        })
        .collect();
    if !is_segment.contains(&true) {
        return (points, Vec::new());
    }
    let mut route = Vec::with_capacity(points.len());
    let mut segments = Vec::new();
    for (i, &(a, b)) in ranges.iter().enumerate() {
        if is_segment[i] {
            let name = if spans[i].name.is_empty() {
                format!("Szegmens {}", segments.len() + 1)
            } else {
                spans[i].name.clone()
            };
            segments.push(RawTrack {
                name,
                points: points[a..b].to_vec(),
            });
        } else {
            route.extend_from_slice(&points[a..b]);
        }
    }
    (route, segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

    fn doc(body: &str) -> String {
        format!(
            r#"{HEAD}<gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1">{body}</gpx>"#
        )
    }

    #[test]
    fn iso8601_timestamps_are_parsed() {
        // 2025-07-02T17:14:10Z
        assert_eq!(parse_iso8601("2025-07-02T17:14:10Z"), Some(1_751_476_450.0));
        // fractional seconds and a numeric offset are part of the schema
        assert_eq!(
            parse_iso8601("2025-07-02T17:14:10.500Z"),
            Some(1_751_476_450.5)
        );
        assert_eq!(
            parse_iso8601("2025-07-02T19:14:10+02:00"),
            Some(1_751_476_450.0)
        );
        assert_eq!(
            parse_iso8601("2025-07-02T15:14:10-02:00"),
            Some(1_751_476_450.0)
        );
        assert_eq!(
            parse_iso8601(" 2020-01-01T00:00:00Z "),
            Some(1_577_836_800.0)
        );
        // garbage is rejected instead of guessed: a wrong timeline is worse than none
        for bad in [
            "",
            "2025-07-02",
            "2025-13-02T17:14:10Z",
            "2025-07-02T25:14:10Z",
            "2025-07-02 17:14",
            "2025-07-02T17:14:10+2:00",
            "yesterday",
        ] {
            assert_eq!(parse_iso8601(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_point_times_are_read_from_the_track() {
        let d = parse(&doc(
            r#"<metadata><time>2020-01-01T00:00:00Z</time></metadata><trk><trkseg>
                <trkpt lat="47.5" lon="19.0"><time>2020-01-01T00:00:00Z</time></trkpt>
                <trkpt lat="47.5" lon="19.001"><time>2020-01-01T00:00:10Z</time></trkpt>
            </trkseg></trk>"#,
        ))
        .unwrap();
        assert_eq!(d.points[0].time, Some(1_577_836_800.0));
        assert_eq!(d.points[1].time, Some(1_577_836_810.0));
        // the metadata time is not a point time
        assert_eq!(d.points.len(), 2);
    }

    #[test]
    fn parses_basic_track() {
        let d = parse(&doc(
            r#"<trk><name>Teszt &amp; kör</name><trkseg>
                <trkpt lat="47.5" lon="19.0"><ele>100.5</ele></trkpt>
                <trkpt lon="19.001" lat="47.5"><ele>101</ele><time>2020-01-01T00:00:00Z</time></trkpt>
                <trkpt lat="47.501" lon="19.002"/>
            </trkseg></trk>"#,
        ))
        .unwrap();
        assert_eq!(d.name.as_deref(), Some("Teszt & kör"));
        assert_eq!(d.points.len(), 3);
        assert_eq!(
            d.points[0],
            RawPoint {
                lat: 47.5,
                lon: 19.0,
                ele: Some(100.5),
                time: None,
            }
        );
        assert_eq!(d.points[1].ele, Some(101.0));
        assert_eq!(d.points[1].time, Some(1_577_836_800.0));
        assert_eq!(d.points[2].ele, None);
        assert_eq!(d.origin, Some(PointOrigin::Track));
        assert_eq!(d.segments, 1);
    }

    #[test]
    fn joins_segments_and_tracks() {
        let d = parse(&doc(
            r#"<trk><trkseg><trkpt lat="47.5" lon="19.0"/><trkpt lat="47.51" lon="19.0"/></trkseg>
               <trkseg><trkpt lat="47.52" lon="19.0"/></trkseg></trk>
               <trk><trkseg><trkpt lat="47.53" lon="19.0"/></trkseg></trk>"#,
        ))
        .unwrap();
        assert_eq!(d.points.len(), 4);
        assert_eq!(d.segments, 3);
        assert!((d.points[3].lat - 47.53).abs() < 1e-12);
    }

    #[test]
    fn a_track_lying_on_the_route_becomes_a_segment() {
        // main track: 2 km north along 19.0; the "segment" is its middle 400 m, slightly offset
        let line = |from: f64, to: f64, lon: f64, n: usize| -> String {
            (0..=n)
                .map(|i| {
                    let lat = from + (to - from) * i as f64 / n as f64;
                    format!(r#"<trkpt lat="{lat:.6}" lon="{lon:.6}"/>"#)
                })
                .collect()
        };
        let body = format!(
            "<trk><name>Kör</name><trkseg>{}</trkseg></trk>\
             <trk><name>Strava: Hármashatár</name><trkseg>{}</trkseg></trk>",
            line(47.5, 47.518, 19.0, 60),
            line(47.507, 47.5106, 19.0001, 12),
        );
        let d = parse(&doc(&body)).unwrap();
        assert_eq!(d.points.len(), 61, "the segment is not joined to the route");
        assert_eq!(d.segment_tracks.len(), 1);
        assert_eq!(d.segment_tracks[0].name, "Strava: Hármashatár");
        assert_eq!(d.name.as_deref(), Some("Kör"));

        // a second track somewhere else (or as long as the route) is still joined
        let body = format!(
            "<trk><trkseg>{}</trkseg></trk><trk><trkseg>{}</trkseg></trk>",
            line(47.5, 47.518, 19.0, 60),
            line(47.518, 47.5, 19.0, 60),
        );
        let d = parse(&doc(&body)).unwrap();
        assert_eq!(
            d.points.len(),
            122,
            "an out-and-back return leg is part of the route"
        );
        assert!(d.segment_tracks.is_empty());

        // a marked track is a segment even when long, but only if it lies on the route
        let body = format!(
            "<trk><trkseg>{}</trkseg></trk><trk><type>segment</type><trkseg>{}</trkseg></trk>",
            line(47.5, 47.518, 19.0, 60),
            line(47.5, 47.518, 19.05, 60),
        );
        let d = parse(&doc(&body)).unwrap();
        assert!(d.segment_tracks.is_empty(), "4 km off the route");
    }

    #[test]
    fn falls_back_to_route_points_and_reads_waypoints() {
        let d = parse(&doc(
            r#"<wpt lat="47.505" lon="19.001"><ele>120</ele><name> Kilátó
              pont </name></wpt>
               <rte><name>Tervezett</name><rtept lat="47.5" lon="19.0"/><rtept lat="47.51" lon="19.01"/></rte>"#,
        ))
        .unwrap();
        assert_eq!(d.origin, Some(PointOrigin::Route));
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.name.as_deref(), Some("Tervezett"));
        assert_eq!(d.waypoints.len(), 1);
        assert_eq!(d.waypoints[0].name, "Kilátó pont");
        assert_eq!(d.waypoints[0].ele, Some(120.0));
    }

    #[test]
    fn tracks_win_over_routes() {
        let d = parse(&doc(
            r#"<rte><rtept lat="1" lon="1"/><rtept lat="2" lon="2"/></rte>
               <trk><trkseg><trkpt lat="47.5" lon="19.0"/><trkpt lat="47.6" lon="19.0"/></trkseg></trk>"#,
        ))
        .unwrap();
        assert_eq!(d.origin, Some(PointOrigin::Track));
        assert_eq!(d.points.len(), 2);
    }

    #[test]
    fn namespaces_prefixes_and_extensions_are_ignored() {
        let xml = r#"<?xml version="1.0"?>
<g:gpx xmlns:g="http://www.topografix.com/GPX/1/1" xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v1">
 <g:metadata><g:name>Meta név</g:name><g:author><g:name>Szerző</g:name></g:author></g:metadata>
 <g:trk><g:extensions><g:name>nem ez</g:name></g:extensions><g:trkseg>
  <g:trkpt lat="47.5" lon="19.0"><g:ele>100</g:ele>
    <g:extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>140</gpxtpx:hr><ele>9999</ele></gpxtpx:TrackPointExtension></g:extensions>
  </g:trkpt>
  <g:trkpt lat="47.501" lon="19.0"><g:ele>101</g:ele></g:trkpt>
 </g:trkseg></g:trk></g:gpx>"#;
        let d = parse(xml).unwrap();
        assert_eq!(d.name.as_deref(), Some("Meta név"));
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.points[0].ele, Some(100.0));
    }

    #[test]
    fn bom_and_leading_whitespace_are_tolerated() {
        let xml = format!(
            "\u{feff}\n  {}",
            doc(
                r#"<trk><trkseg><trkpt lat="47.5" lon="19.0"/><trkpt lat="47.6" lon="19.0"/></trkseg></trk>"#
            )
        );
        assert_eq!(parse(&xml).unwrap().points.len(), 2);
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(xml.trim_start_matches('\u{feff}').as_bytes());
        assert_eq!(parse_bytes(&bytes).unwrap().points.len(), 2);
    }

    #[test]
    fn utf16_and_latin1_files_decode() {
        let xml = doc(
            r#"<trk><name>Pécs</name><trkseg><trkpt lat="47.5" lon="19.0"/><trkpt lat="47.6" lon="19.0"/></trkseg></trk>"#,
        );
        let mut le = vec![0xFF, 0xFE];
        for u in xml.encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(parse_bytes(&le).unwrap().name.as_deref(), Some("Pécs"));
        // Latin-1 'é' is a lone 0xE9 byte, invalid UTF-8.
        let latin1: Vec<u8> = xml.chars().map(|c| c as u32 as u8).collect();
        assert_eq!(parse_bytes(&latin1).unwrap().name.as_deref(), Some("Pécs"));
    }

    #[test]
    fn invalid_points_are_skipped_and_counted() {
        let d = parse(&doc(r#"<trk><trkseg>
              <trkpt lat="47.5" lon="19.0"/>
              <trkpt lat="abc" lon="19.0"/>
              <trkpt lat="95" lon="19.0"/>
              <trkpt lon="19.0"/>
              <trkpt lat="0" lon="0"/>
              <trkpt lat="47.6" lon="19.0"><ele>NaN</ele></trkpt>
            </trkseg></trk>"#))
        .unwrap();
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.skipped_points, 4);
        assert_eq!(d.points[1].ele, None);
    }

    #[test]
    fn elevation_formats() {
        assert_eq!(parse_number(" 254.75 "), Some(254.75));
        assert_eq!(parse_number("254,5"), Some(254.5));
        assert_eq!(parse_number("1.5e2"), Some(150.0));
        assert_eq!(parse_number("1,234.5"), None);
        assert_eq!(parse_number("inf"), None);
        assert_eq!(parse_number(""), None);
    }

    #[test]
    fn broken_and_foreign_documents_are_errors() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
        assert!(parse("ez egyáltalán nem XML").is_err());
        assert!(parse("<html><body/></html>").is_err());
        // truncated download
        let cut = format!("{HEAD}<gpx><trk><trkseg><trkpt lat=\"47.5\" lon=\"19.0\"><ele>1");
        let e = parse(&cut).unwrap_err().to_string();
        assert!(e.contains("GPX"), "{e}");
        // mismatched tags
        assert!(parse(&doc("<trk><trkseg></trk></trkseg>")).is_err());
        // valid XML, no points
        assert!(parse(&doc("<trk><trkseg/></trk>")).is_err());
    }

    #[test]
    fn brouter_header_comment_is_parsed() {
        let xml = format!(
            "{HEAD}\n<!-- track-length = 29404 filtered ascend = 747 plain-ascend = 432 cost=54504 energy=.3kwh time=2h 41m 53s -->\n<gpx><trk><trkseg><trkpt lat=\"47.5\" lon=\"19.0\"/><trkpt lat=\"47.6\" lon=\"19.0\"/></trkseg></trk></gpx>"
        );
        let h = parse(&xml).unwrap().brouter.unwrap();
        assert_eq!(h.track_length_m, Some(29404.0));
        assert_eq!(h.filtered_ascend_m, Some(747.0));
        assert_eq!(h.plain_ascend_m, Some(432.0));
        assert_eq!(
            number_after("plain-ascend = -3 cost", "plain-ascend"),
            Some(-3.0)
        );
    }

    #[test]
    fn real_brouter_fixture_parses() {
        let text = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/brouter_budapest_short.gpx"
        ))
        .unwrap();
        let d = parse_bytes(&text).unwrap();
        assert!(d.points.len() > 50);
        assert!(d.points.iter().all(|p| p.ele.is_some()));
        assert_eq!(d.brouter.unwrap().track_length_m, Some(3006.0));
        // The generic BRouter track name is still reported; naming policy lives in `build`.
        assert_eq!(d.name.as_deref(), Some("brouter_trekking_0"));
    }
}
