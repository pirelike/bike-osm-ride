//! Importing a workout from a file.
//!
//! Two formats are understood, and both express power as a fraction of FTP:
//!
//! * the `<workout_file>` XML that MyWhoosh exports — the same schema Zwift's `.zwo` files use
//!   (`SteadyState`, `Warmup`, `Cooldown`, `Ramp`, `IntervalsT`, `FreeRide`);
//! * MyWhoosh's own JSON export (`WorkoutStepsArray`, power as a fraction of FTP).
//!
//! The result is a [`Workout`] whose steps are [`StepTarget::PctFtp`] / [`StepTarget::RampPct`],
//! so an imported workout follows the rider's FTP exactly like the embedded library does.
//!
//! Imported workouts are not written for this library, so they are *not* checked by
//! [`Workout::validate`] (a whole-minute length is not something a downloaded file promises);
//! the builder only rejects what cannot be ridden at all: no usable step, or a total under a
//! minute.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use ride_i18n::{t, tf};
use ride_model::{
    StepKind, StepMessage, StepTarget, Workout, WorkoutCategory, WorkoutStep, IMPORT_PREFIX,
};

/// Shortest total length an imported workout may have, seconds.
pub const MIN_TOTAL_S: u32 = 60;
/// Longest single step, seconds (a broken file must not produce a 10-hour block).
const MAX_STEP_S: f64 = 36_000.0;
/// A file longer than this many steps is not a workout.
const MAX_STEPS: usize = 300;
/// Coaching messages kept per block / step.
const MAX_MESSAGES: usize = 40;

/// Parses a workout file. `source` is the file name: it becomes the workout's source and the
/// fallback name when the file carries none.
pub fn from_bytes(bytes: &[u8], source: &str) -> Result<Workout, String> {
    let text = decode(bytes);
    let head = text.trim_start();
    let draft = if head.starts_with('{') {
        parse_json(&text)?
    } else if head.starts_with('<') {
        parse_xml(&text)?
    } else {
        return Err(
            t("sim.workout_import.ismeretlen_fajlformatum_sem_xml_mywhoosh_zwift_sem_j").into(),
        );
    };
    build(draft, source)
}

fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// A step as the file describes it, before the length and target are normalised.
#[derive(Debug, Clone, PartialEq)]
struct DraftStep {
    kind: StepKind,
    name: String,
    duration_s: f64,
    target: StepTarget,
    cadence: Option<u16>,
    note: Option<String>,
    messages: Vec<StepMessage>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Draft {
    name: Option<String>,
    description: String,
    steps: Vec<DraftStep>,
}

// ───────────────────────────── XML (ZWO / MyWhoosh) ─────────────────────────────

/// The element currently being read: a step block and its coaching messages (seconds from the
/// start of the block, text). `nested` counts the open child elements (a paired
/// `<textevent>…</textevent>`), whose end tags must not close the block.
struct Pending {
    element: String,
    attributes: Vec<(String, String)>,
    messages: Vec<(f64, String)>,
    nested: usize,
}

/// The header field whose text is being collected, and the text so far (a name can arrive in
/// several pieces: text, entity references, CDATA).
struct Collecting {
    field: &'static str,
    text: String,
}

/// One `<start>`/`<empty>` element: the workout header fields, a step block or a coaching note.
fn handle_start(
    draft: &mut Draft,
    in_workout: &mut bool,
    pending: &mut Option<Pending>,
    text_for: &mut Option<Collecting>,
    e: &BytesStart<'_>,
    empty: bool,
) {
    let local = e.local_name().as_ref().to_lowercase();
    // A paired child of a block (`<textevent …>…</textevent>`, or anything unknown) is counted,
    // so that its end tag is not taken for the end of the block.
    if !empty {
        if let Some(pending) = pending.as_mut() {
            pending.nested += 1;
        }
    }
    match local.as_str() {
        "workout" => *in_workout = true,
        // `<name/>` has no text: nothing is collected (the file name stays the fallback)
        "name" | "description" if !*in_workout && !empty => {
            *text_for = Some(Collecting {
                field: if local == "name" {
                    "name"
                } else {
                    "description"
                },
                text: String::new(),
            });
        }
        "textevent" => {
            if let Some(pending) = pending.as_mut() {
                if let Some(message) = attribute(e, "message") {
                    let message = message.trim();
                    let offset = attribute(e, "timeoffset")
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .unwrap_or(0.0);
                    if !message.is_empty() && pending.messages.len() < MAX_MESSAGES {
                        pending
                            .messages
                            .push((offset, message.chars().take(300).collect::<String>()));
                    }
                }
            }
        }
        _ if *in_workout && pending.is_none() => {
            let attributes = attributes(e);
            if empty {
                // `<SteadyState …/>`: a complete block without a body
                if let Some(steps) = step_from(&local, &attributes) {
                    draft.steps.extend(steps);
                }
            } else {
                *pending = Some(Pending {
                    element: local,
                    attributes,
                    messages: Vec::new(),
                    nested: 0,
                });
            }
        }
        _ => {}
    }
}

fn parse_xml(text: &str) -> Result<Draft, String> {
    let mut reader = Reader::from_str(text);
    let mut draft = Draft::default();
    let mut in_workout = false;
    let mut pending: Option<Pending> = None;
    // "name" / "description" while their text is being collected
    let mut text_for: Option<Collecting> = None;

    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => {
                return Err(t(
                    "sim.workout_import.az_edzesfajl_hibas_vagy_serult_xml_hiba_probald_meg_",
                )
                .into())
            }
        };
        match event {
            Event::Start(e) => handle_start(
                &mut draft,
                &mut in_workout,
                &mut pending,
                &mut text_for,
                &e,
                false,
            ),
            Event::Empty(e) => handle_start(
                &mut draft,
                &mut in_workout,
                &mut pending,
                &mut text_for,
                &e,
                true,
            ),
            Event::End(e) => {
                let local = e.local_name().as_ref().to_lowercase();
                if let Some(open) = pending.as_mut() {
                    if open.nested > 0 {
                        // the end of a child of the block, not of the block itself
                        open.nested -= 1;
                        continue;
                    }
                }
                match local.as_str() {
                    "workout" => in_workout = false,
                    "name" | "description" => {
                        if let Some(done) = text_for.take() {
                            match done.field {
                                "name" => {
                                    let name = done.text.trim();
                                    if !name.is_empty() {
                                        draft.name = Some(name.to_string());
                                    }
                                }
                                _ => draft.description.push_str(&done.text),
                            }
                        }
                    }
                    _ if in_workout && pending.as_ref().is_some_and(|p| p.element == local) => {
                        if let Some(block) = pending.take() {
                            if let Some(mut steps) = step_from(&block.element, &block.attributes) {
                                attach_messages(&mut steps, block.messages);
                                draft.steps.extend(steps);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(collecting) = text_for.as_mut() {
                    collecting.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(collecting) = text_for.as_mut() {
                    collecting.text.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(collecting) = text_for.as_mut() {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        collecting.text.push(ch);
                    } else {
                        match &*r {
                            "amp" => collecting.text.push('&'),
                            "lt" => collecting.text.push('<'),
                            "gt" => collecting.text.push('>'),
                            "quot" => collecting.text.push('"'),
                            "apos" => collecting.text.push('\''),
                            _ => {}
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if draft.steps.is_empty() {
        return Err(
            t("sim.workout_import.a_fajlban_nincs_felismerheto_edzesblokk_steadystate_").into(),
        );
    }
    Ok(draft)
}

/// Hands the coaching messages of a block (offsets from the block start) to the steps they fall
/// in: an `IntervalsT` block becomes many steps, and its messages are spread over them. A message
/// past the end of the block goes to its last step. The first message of a step is also its note.
fn attach_messages(steps: &mut [DraftStep], mut messages: Vec<(f64, String)>) {
    messages.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut start = 0.0;
    let last = steps.len().saturating_sub(1);
    let mut queue = messages.into_iter().peekable();
    for (i, step) in steps.iter_mut().enumerate() {
        let end = start + step.duration_s.max(0.0);
        while let Some((offset, _)) = queue.peek() {
            if *offset >= end && i < last {
                break;
            }
            let Some((offset, text)) = queue.next() else {
                break;
            };
            let at = (offset - start).clamp(0.0, step.duration_s.max(0.0));
            step.messages.push(StepMessage {
                at_s: at.round() as u32,
                text,
            });
        }
        if step.note.is_none() {
            step.note = step.messages.first().map(|m| m.text.clone());
        }
        start = end;
    }
}

fn attributes(e: &BytesStart<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for attribute in e.attributes().with_checks(false).flatten() {
        let key = attribute.key.local_name().as_ref().to_lowercase();
        // `&amp;` / `&#233;` in a coaching message are resolved; a value that cannot be is kept
        // as written
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|_| attribute.value.trim().to_string());
        out.push((key, value));
    }
    out
}

fn attribute(e: &BytesStart<'_>, name: &str) -> Option<String> {
    attributes(e)
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn number(attributes: &[(String, String)], name: &str) -> Option<f64> {
    attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn cadence(attributes: &[(String, String)]) -> Option<u16> {
    number(attributes, "Cadence")
        .filter(|value| (30.0..=200.0).contains(value))
        .map(|value| value.round() as u16)
}

/// A power attribute as a percentage of FTP. ZWO stores fractions (`0.50`, a sprint `2.2`); a
/// file written with percentages (`50`) is accepted too. Nobody rides at 10 % of FTP, and nobody at
/// ten times FTP, so `10` divides the two readings.
fn pct(value: f64) -> f64 {
    let pct = if value.abs() < 10.0 {
        value * 100.0
    } else {
        value
    };
    // 0.01 % is finer than any trainer, and it keeps 1.1 × 100 from becoming 110.00000000000001
    (pct.clamp(0.0, 250.0) * 100.0).round() / 100.0
}

fn step_from(element: &str, attributes: &[(String, String)]) -> Option<Vec<DraftStep>> {
    let duration = number(attributes, "Duration").unwrap_or(0.0);
    let cadence = cadence(attributes);
    let steady = |value: f64| StepTarget::PctFtp { value };
    match element {
        "steadystate" => {
            let power = pct(number(attributes, "Power")?);
            Some(vec![DraftStep {
                kind: StepKind::Steady,
                name: format!("Blokk · {power:.0}% FTP"),
                duration_s: duration,
                target: steady(power),
                cadence,
                note: None,
                messages: Vec::new(),
            }])
        }
        "warmup" | "cooldown" => {
            let low = pct(number(attributes, "PowerLow")?);
            let high = pct(number(attributes, "PowerHigh")?);
            let (from, to, kind, name) = if element == "cooldown" {
                (
                    high,
                    low,
                    StepKind::Cooldown,
                    t("sim.workout_import.levezetes"),
                )
            } else {
                (
                    low,
                    high,
                    StepKind::Warmup,
                    t("sim.workout_import.bemelegites"),
                )
            };
            Some(vec![DraftStep {
                kind,
                name: name.to_string(),
                duration_s: duration,
                target: StepTarget::RampPct { from, to },
                cadence,
                note: None,
                messages: Vec::new(),
            }])
        }
        "ramp" => {
            let from = pct(number(attributes, "PowerLow")?);
            let to = pct(number(attributes, "PowerHigh")?);
            Some(vec![DraftStep {
                kind: StepKind::Interval,
                name: tf(
                    "sim.workout_import.rampa_from_0_to_0_ftp",
                    &[("from", &format!("{from:.0}")), ("to", &format!("{to:.0}"))],
                ),
                duration_s: duration,
                target: StepTarget::RampPct { from, to },
                cadence,
                note: None,
                messages: Vec::new(),
            }])
        }
        "freeride" | "maxeffort" => Some(vec![DraftStep {
            kind: StepKind::Free,
            name: if element == "freeride" {
                t("sim.workout_import.szabad").to_string()
            } else {
                t("sim.workout_import.maximalis_ero").to_string()
            },
            duration_s: duration,
            target: StepTarget::Free,
            cadence,
            note: None,
            messages: Vec::new(),
        }]),
        "intervalst" => {
            let repeat = number(attributes, "Repeat").unwrap_or(1.0).clamp(1.0, 40.0) as usize;
            let on_s = number(attributes, "OnDuration").unwrap_or(0.0);
            let off_s = number(attributes, "OffDuration").unwrap_or(0.0);
            let on_pct = pct(number(attributes, "OnPower").unwrap_or(1.0));
            let off_pct = pct(number(attributes, "OffPower").unwrap_or(0.5));
            let mut steps = Vec::with_capacity(repeat * 2);
            for _ in 0..repeat {
                steps.push(DraftStep {
                    kind: StepKind::Interval,
                    name: format!("Intervall · {on_pct:.0}% FTP"),
                    duration_s: on_s,
                    target: steady(on_pct),
                    cadence,
                    note: None,
                    messages: Vec::new(),
                });
                steps.push(DraftStep {
                    kind: StepKind::Recovery,
                    name: tf(
                        "sim.workout_import.kipiheno_off_pct_0_ftp",
                        &[("pct", &format!("{off_pct:.0}"))],
                    ),
                    duration_s: off_s,
                    target: steady(off_pct),
                    cadence,
                    note: None,
                    messages: Vec::new(),
                });
            }
            Some(steps)
        }
        _ => None,
    }
}

// ───────────────────────────── JSON (MyWhoosh) ─────────────────────────────

#[derive(serde::Deserialize)]
struct JsonFile {
    #[serde(rename = "Name", default)]
    name: Option<String>,
    #[serde(rename = "Description", default)]
    description: Option<String>,
    #[serde(rename = "WorkoutStepsArray", default)]
    steps: Vec<JsonStep>,
}

#[derive(serde::Deserialize)]
struct JsonStep {
    #[serde(rename = "StepType", default)]
    step_type: String,
    #[serde(rename = "Power", default)]
    power: f64,
    #[serde(rename = "StartPower", default)]
    start_power: f64,
    #[serde(rename = "EndPower", default)]
    end_power: f64,
    #[serde(rename = "Time", default)]
    time: f64,
    #[serde(rename = "Rpm", default)]
    rpm: f64,
    #[serde(rename = "WorkoutMessage", default)]
    messages: Vec<JsonMessage>,
}

#[derive(serde::Deserialize)]
struct JsonMessage {
    #[serde(rename = "Message", default)]
    message: String,
    /// Seconds from the start of the step.
    #[serde(rename = "Time", default)]
    time: f64,
}

fn parse_json(text: &str) -> Result<Draft, String> {
    let file: JsonFile = serde_json::from_str(text).map_err(|_| {
        t("sim.workout_import.az_edzes_json_nem_olvashato_serult_vagy_nem_mywhoosh").to_string()
    })?;
    if file.steps.is_empty() {
        return Err(
            t("sim.workout_import.a_json_nem_tartalmaz_edzesblokkot_workoutstepsarray_").into(),
        );
    }
    let mut draft = Draft {
        name: file.name,
        description: file.description.unwrap_or_default(),
        steps: Vec::new(),
    };
    for step in file.steps {
        let cadence = (30.0..=200.0)
            .contains(&step.rpm)
            .then(|| step.rpm.round() as u16);
        let duration = if step.time.is_finite() {
            step.time.max(0.0)
        } else {
            0.0
        };
        let mut messages: Vec<StepMessage> = step
            .messages
            .iter()
            .filter(|message| !message.message.trim().is_empty())
            .take(MAX_MESSAGES)
            .map(|message| StepMessage {
                at_s: (if message.time.is_finite() {
                    message.time
                } else {
                    0.0
                })
                .clamp(0.0, duration)
                .round() as u32,
                text: message.message.trim().chars().take(300).collect(),
            })
            .collect();
        messages.sort_by_key(|message| message.at_s);
        let note = messages.first().map(|message| message.text.clone());
        let ramp = step.start_power > 0.0
            && step.end_power > 0.0
            && (step.start_power - step.end_power).abs() > 1e-6;
        let (target, name, kind) = if ramp {
            let from = pct(step.start_power);
            let to = pct(step.end_power);
            (
                StepTarget::RampPct { from, to },
                tf(
                    "sim.workout_import.rampa_from_0_to_0_ftp",
                    &[("from", &format!("{from:.0}")), ("to", &format!("{to:.0}"))],
                ),
                StepKind::Interval,
            )
        } else if step.step_type.eq_ignore_ascii_case("E_FreeRide") {
            (
                StepTarget::Free,
                t("sim.workout_import.szabad").to_string(),
                StepKind::Free,
            )
        } else {
            let value = pct(step.power);
            (
                StepTarget::PctFtp { value },
                format!("Blokk · {value:.0}% FTP"),
                StepKind::Steady,
            )
        };
        draft.steps.push(DraftStep {
            kind,
            name,
            duration_s: step.time,
            target,
            cadence,
            note,
            messages,
        });
    }
    Ok(draft)
}

// ───────────────────────────── the workout ─────────────────────────────

fn build(draft: Draft, source: &str) -> Result<Workout, String> {
    let mut steps = Vec::new();
    if draft.steps.len() > MAX_STEPS {
        return Err(tf(
            "sim.workout_import.tul_sok_blokk",
            &[
                ("count", &draft.steps.len().to_string()),
                ("max", &MAX_STEPS.to_string()),
            ],
        ));
    }
    for step in draft.steps {
        if !(step.duration_s.is_finite() && step.duration_s >= 1.0) {
            continue;
        }
        let duration_s = step.duration_s.round().clamp(1.0, MAX_STEP_S) as u32;
        steps.push(WorkoutStep {
            kind: step.kind,
            name: step.name,
            duration_s,
            target: step.target,
            cadence: step.cadence,
            note: step.note,
            messages: step.messages,
        });
    }
    if steps.is_empty() {
        return Err(t("sim.workout_import.az_edzes_nem_tartalmaz_hasznalhato_blokkot").into());
    }
    let total: u32 = steps.iter().map(|step| step.duration_s).sum();
    if total < MIN_TOTAL_S {
        return Err(tf(
            "sim.workout_import.az_edzes_tul_rovid_total_s_legalabb_min_total_s_s_ke",
            &[
                ("total", &total.to_string()),
                ("min", &MIN_TOTAL_S.to_string()),
            ],
        ));
    }
    let average = average_pct(&steps);
    let name = draft
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| name_from(source));
    let description = draft.description.trim().to_string();
    let goal = first_sentence(&description)
        .unwrap_or_else(|| t("sim.workout_import.importalt_edzes").to_string());
    Ok(Workout {
        id: format!("{IMPORT_PREFIX}{:016x}", content_hash(&name, &steps)),
        name,
        category: category_for(average),
        goal,
        description: description.chars().take(4_000).collect(),
        ftp_pct: average.round().clamp(1.0, 250.0) as u8,
        duration_min: total.div_ceil(60),
        tags: Vec::new(),
        sources: vec![source.to_string()],
        steps,
    })
}

/// Time-weighted mean intensity, percent of FTP (free steps do not pull it down).
fn average_pct(steps: &[WorkoutStep]) -> f64 {
    let mut weighted = 0.0;
    let mut seconds = 0.0;
    for step in steps {
        let duration = step.duration_s as f64;
        let middle = step.target.pct_at(duration * 0.5, duration, 100.0);
        let Some(middle) = middle else {
            continue;
        };
        weighted += middle * duration;
        seconds += duration;
    }
    if seconds > 0.0 {
        weighted / seconds
    } else {
        60.0
    }
}

fn category_for(average_pct: f64) -> WorkoutCategory {
    if average_pct < 60.0 {
        WorkoutCategory::Recovery
    } else if average_pct < 72.0 {
        WorkoutCategory::Endurance
    } else if average_pct < 80.0 {
        WorkoutCategory::Tempo
    } else if average_pct < 88.0 {
        WorkoutCategory::Sweetspot
    } else if average_pct < 97.0 {
        WorkoutCategory::Threshold
    } else if average_pct < 110.0 {
        WorkoutCategory::Vo2max
    } else if average_pct < 130.0 {
        WorkoutCategory::Anaerobic
    } else {
        WorkoutCategory::Sprint
    }
}

/// Stable id of the content: re-importing the same file keeps the same id, so it replaces the
/// old entry instead of duplicating it.
fn content_hash(name: &str, steps: &[WorkoutStep]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut write = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    write(name.as_bytes());
    for step in steps {
        write(&step.duration_s.to_le_bytes());
        write(&[step.kind as u8]);
        write(format!("{:?}", step.target).as_bytes());
    }
    hash
}

fn name_from(source: &str) -> String {
    let stem = source
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(source)
        .rsplit_once('.')
        .map_or(source, |(stem, _)| stem);
    let name = stem.trim();
    if name.is_empty() {
        t("sim.workout_import.importalt_edzes").to_string()
    } else {
        name.to_string()
    }
}

fn first_sentence(description: &str) -> Option<String> {
    let text = description.trim();
    if text.is_empty() {
        return None;
    }
    let end = text.find(['.', '!', '?']).map_or(text.len(), |i| i + 1);
    let sentence = text[..end].trim();
    let sentence = if sentence.is_empty() { text } else { sentence };
    Some(sentence.chars().take(160).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZWO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<workout_file>
  <author>mywhoosh</author>
  <name>Pacing #2</name>
  <description>Judge your pacing. Build endurance.</description>
  <sportType>bike</sportType>
  <workout>
    <SteadyState Duration="180" Power="0.50" pace="0">
      <textevent timeoffset="0" message="Today we build endurance."/>
      <textevent timeoffset="90" message="Relax your shoulders."/>
    </SteadyState>
    <SteadyState Duration="60" Power="1.09" pace="0"/>
    <Warmup Duration="300" PowerLow="0.40" PowerHigh="0.70" Cadence="90"/>
    <IntervalsT Repeat="3" OnDuration="60" OffDuration="120" OnPower="1.05" OffPower="0.50">
      <textevent timeoffset="190" message="Second one, stay smooth."/>
    </IntervalsT>
    <FreeRide Duration="300"/>
  </workout>
</workout_file>"#;

    const JSON: &str = r#"{
      "Name": "Pacing #2",
      "Description": "mywhoosh - Pacing #2",
      "WorkoutStepsArray": [
        { "StepType": "E_Normal", "Power": 0.5, "StartPower": 0, "EndPower": 0, "Time": 180, "Rpm": 0,
          "WorkoutMessage": [ { "Id": 2, "Time": 120, "Message": "Last minute." },
                              { "Id": 1, "Time": 0, "Message": "Build endurance." } ] },
        { "StepType": "E_Normal", "Power": 0.9, "StartPower": 0.6, "EndPower": 1.1, "Time": 240, "Rpm": 85 },
        { "StepType": "E_Normal", "Power": 0.75, "StartPower": 0, "EndPower": 0, "Time": 120, "Rpm": 0 }
      ]
    }"#;

    #[test]
    fn a_paired_child_element_does_not_drop_its_block() {
        let xml = r#"<workout_file><name>Paired</name><workout>
            <SteadyState Duration="120" Power="0.6">
              <textevent timeoffset="10" message="Hello"></textevent>
              <unknown><deeper/></unknown>
            </SteadyState>
            <SteadyState Duration="60" Power="0.8"/>
        </workout></workout_file>"#;
        let workout = from_bytes(xml.as_bytes(), "paired.zwo").unwrap();
        assert_eq!(workout.steps.len(), 2, "{:?}", workout.steps);
        assert_eq!(workout.steps[0].duration_s, 120);
        assert_eq!(workout.steps[0].messages.len(), 1);
    }

    #[test]
    fn entities_and_split_text_are_kept_whole() {
        let xml = r#"<workout_file><name>Coach&apos;s &amp; <![CDATA[choice]]> &#233;</name>
            <description>A &lt; B</description><workout>
            <SteadyState Duration="120" Power="0.6">
              <textevent timeoffset="0" message="Don&apos;t stop &amp; smile"/>
            </SteadyState>
        </workout></workout_file>"#;
        let workout = from_bytes(xml.as_bytes(), "e.zwo").unwrap();
        assert_eq!(workout.name, "Coach's & choice é");
        assert_eq!(workout.description, "A < B");
        assert_eq!(workout.steps[0].messages[0].text, "Don't stop & smile");
    }

    #[test]
    fn an_empty_name_element_falls_back_to_the_file_name() {
        let xml = r#"<workout_file><name/><sportType>bike</sportType><workout>
            <SteadyState Duration="120" Power="0.6"/>
        </workout></workout_file>"#;
        let workout = from_bytes(xml.as_bytes(), "fallback.zwo").unwrap();
        assert_ne!(workout.name, "bike");
    }

    #[test]
    fn fractions_above_two_are_still_fractions() {
        assert_eq!(pct(2.2), 220.0);
        assert_eq!(pct(1.2), 120.0);
        assert_eq!(pct(50.0), 50.0);
        assert_eq!(pct(120.0), 120.0);
    }

    #[test]
    fn too_many_blocks_are_refused_not_cut() {
        let blocks = r#"<IntervalsT Repeat="40" OnDuration="30" OffDuration="30" OnPower="1.2" OffPower="0.5"/>"#
            .repeat(4);
        let xml = format!("<workout_file><workout>{blocks}</workout></workout_file>");
        assert!(from_bytes(xml.as_bytes(), "long.zwo").is_err());
    }

    #[test]
    fn a_zwift_style_xml_becomes_a_workout() {
        let workout = from_bytes(ZWO.as_bytes(), "Pacing #2.xml").expect("parsed");
        assert_eq!(workout.name, "Pacing #2");
        assert!(workout.is_imported());
        assert_eq!(workout.goal, "Judge your pacing.");
        // 180 + 60 + 300 + 3 × (60 + 120) + 300
        assert_eq!(workout.total_seconds(), 1_380);
        assert_eq!(workout.steps.len(), 10);
        let first = &workout.steps[0];
        assert_eq!(first.kind, StepKind::Steady);
        assert_eq!(first.duration_s, 180);
        assert_eq!(first.target, StepTarget::PctFtp { value: 50.0 });
        assert_eq!(first.note.as_deref(), Some("Today we build endurance."));
        // every message is kept with its time inside the step
        assert_eq!(first.messages.len(), 2);
        assert_eq!(first.messages[1].at_s, 90);
        assert_eq!(first.messages[1].text, "Relax your shoulders.");
        // a message of an interval block lands in the step it falls in: 190 s = 10 s into the
        // second "on" step (60 on + 120 off before it)
        assert!(workout.steps[3].messages.is_empty());
        assert_eq!(workout.steps[5].messages[0].at_s, 10);
        assert_eq!(
            workout.steps[5].messages[0].text,
            "Second one, stay smooth."
        );
        // a warmup ramps up, a cooldown down
        assert_eq!(
            workout.steps[2].target,
            StepTarget::RampPct {
                from: 40.0,
                to: 70.0
            }
        );
        assert_eq!(workout.steps[2].cadence, Some(90));
        // the interval block expands to on/off pairs
        assert_eq!(workout.steps[3].kind, StepKind::Interval);
        assert_eq!(workout.steps[3].duration_s, 60);
        assert_eq!(workout.steps[4].kind, StepKind::Recovery);
        assert_eq!(workout.steps[4].duration_s, 120);
        assert_eq!(workout.steps[5].target, StepTarget::PctFtp { value: 105.0 });
        assert_eq!(workout.steps[9].target, StepTarget::Free);
        // a low-intensity endurance session
        assert_eq!(workout.category, WorkoutCategory::Endurance);
        assert_eq!(workout.sources, vec!["Pacing #2.xml".to_string()]);
    }

    #[test]
    fn the_mywhoosh_json_export_becomes_the_same_kind_of_workout() {
        let workout = from_bytes(JSON.as_bytes(), "Pacing #2.json").expect("parsed");
        assert_eq!(workout.name, "Pacing #2");
        assert_eq!(workout.total_seconds(), 540);
        assert_eq!(workout.steps.len(), 3);
        assert_eq!(workout.steps[0].target, StepTarget::PctFtp { value: 50.0 });
        assert_eq!(workout.steps[0].note.as_deref(), Some("Build endurance."));
        assert_eq!(workout.steps[0].messages.len(), 2, "messages in time order");
        assert_eq!(workout.steps[0].messages[1].at_s, 120);
        // StartPower/EndPower mark a ramp, and the cadence comes from Rpm
        assert_eq!(
            workout.steps[1].target,
            StepTarget::RampPct {
                from: 60.0,
                to: 110.0
            }
        );
        assert_eq!(workout.steps[1].cadence, Some(85));
        assert_eq!(workout.steps[2].target, StepTarget::PctFtp { value: 75.0 });
        assert!(workout.description.starts_with("mywhoosh"));
    }

    #[test]
    fn the_same_file_imports_to_the_same_id() {
        let first = from_bytes(ZWO.as_bytes(), "a.xml").unwrap();
        let again = from_bytes(ZWO.as_bytes(), "b.xml").unwrap();
        assert_eq!(first.id, again.id, "re-import replaces, not duplicates");
        let changed = ZWO
            .replace(r#"Duration="60""#, r#"Duration="90""#)
            .replace(r#"Power="1.09""#, r#"Power="1.20""#);
        let other = from_bytes(changed.as_bytes(), "a.xml").unwrap();
        assert_ne!(first.id, other.id);
    }

    #[test]
    fn garbage_and_too_short_files_are_rejected() {
        assert!(from_bytes(b"hello", "x.txt").is_err());
        assert!(from_bytes(b"{}", "x.json").is_err());
        assert!(from_bytes(b"<workout_file/>", "x.xml").is_err());
        // 30 s is not a workout
        let short = r#"<workout_file><name>Short</name><workout>
            <SteadyState Duration="30" Power="0.5"/></workout></workout_file>"#;
        let err = from_bytes(short.as_bytes(), "short.xml").unwrap_err();
        assert!(err.contains("túl rövid"), "{err}");
        // a step without a duration is dropped, but the rest survives
        let partial = r#"<workout_file><workout>
            <SteadyState Duration="0" Power="0.5"/>
            <SteadyState Duration="120" Power="0.6"/></workout></workout_file>"#;
        let workout = from_bytes(partial.as_bytes(), "partial.xml").unwrap();
        assert_eq!(workout.steps.len(), 1);
        assert_eq!(workout.name, "partial", "the file name is the fallback");
    }

    #[test]
    fn a_percentage_file_is_accepted_too() {
        let xml = r#"<workout_file><name>Pct</name><workout>
            <SteadyState Duration="120" Power="75"/></workout></workout_file>"#;
        let workout = from_bytes(xml.as_bytes(), "pct.xml").unwrap();
        assert_eq!(workout.steps[0].target, StepTarget::PctFtp { value: 75.0 });
    }
}
