//! Local agent bridge and proposal inbox.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ride_model::{CalendarEntry, StepTarget, Workout};
use ride_route::store::Store;
use serde::{Deserialize, Serialize};

#[cfg(not(target_os = "android"))]
mod server;
#[cfg(not(target_os = "android"))]
pub use server::run;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiDraft {
    pub id: String,
    pub kind: DraftKind,
}

/// A change to a workout that is already in the calendar: the coach moves it, swaps its workout
/// or drops it, and says why. Applied only after the rider approves, and never to a completed entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryChange {
    pub entry_id: String,
    /// The entry as it should be afterwards (same id); `None` removes it.
    #[serde(default)]
    pub entry: Option<CalendarEntry>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DraftKind {
    Training {
        title: String,
        workouts: Vec<Workout>,
        entries: Vec<CalendarEntry>,
        /// Adjustments of entries that are already scheduled. Absent in drafts written before.
        #[serde(default)]
        changes: Vec<EntryChange>,
    },
    Feedback {
        route_id: String,
        stem: String,
        good: String,
        improve: String,
        next: String,
    },
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

pub fn validate_draft(draft: &AiDraft) -> Result<(), String> {
    validate_draft_with_store(draft, &Store::default_location())
}

fn validate_draft_with_store(draft: &AiDraft, store: &Store) -> Result<(), String> {
    if !valid_id(&draft.id) {
        return Err("Invalid request id".into());
    }
    match &draft.kind {
        DraftKind::Feedback {
            route_id,
            stem,
            good,
            improve,
            next,
        } => {
            if !valid_id(route_id)
                || stem.is_empty()
                || !stem.bytes().all(|c| c.is_ascii_digit())
                || [good, improve, next]
                    .iter()
                    .any(|text| text.trim().is_empty() || text.len() > 8_000)
            {
                return Err("Invalid feedback".into());
            }
        }
        DraftKind::Training {
            title,
            workouts,
            entries,
            changes,
        } => {
            if title.trim().is_empty()
                || title.len() > 200
                || workouts.len() > 100
                || entries.len() > 200
                || changes.len() > 100
                || (workouts.is_empty() && entries.is_empty() && changes.is_empty())
            {
                return Err("Invalid training plan".into());
            }
            if workouts
                .iter()
                .any(|workout| !workout.is_imported() || workout.validate().is_err())
            {
                return Err("New workouts need an import- id and valid whole-minute blocks".into());
            }
            let mut workout_ids = std::collections::HashSet::new();
            if workouts
                .iter()
                .any(|workout| !workout_ids.insert(&workout.id))
            {
                return Err("Duplicate workout id".into());
            }
            for workout in workouts {
                if !valid_id(&workout.id)
                    || workout.name.trim().is_empty()
                    || workout.name.len() > 200
                    || workout.steps.is_empty()
                    || workout.steps.len() > 80
                    || workout.steps.iter().any(|step| {
                        step.duration_s < 5
                            || step.duration_s > 36_000
                            || !valid_target(step.target)
                    })
                    || workout.total_seconds() < 60
                {
                    return Err("Invalid workout".into());
                }
            }
            let library = ride_model::WorkoutLibrary::with_imported(store.list_workouts());
            if workouts.iter().any(|workout| {
                library
                    .get(&workout.id)
                    .is_some_and(|existing| existing != workout)
            }) {
                return Err("New workout id is already in use".into());
            }
            for entry in entries
                .iter()
                .chain(changes.iter().filter_map(|c| c.entry.as_ref()))
            {
                let matches = if let Some(original) = workouts
                    .iter()
                    .find(|workout| workout.id == entry.workout.id)
                {
                    *original == entry.workout
                } else if let Some(original) = library.get(&entry.workout.id) {
                    workout_structure_matches(original, &entry.workout)
                } else {
                    false
                };
                if !matches {
                    return Err("Workout differs from the library or new workout".into());
                }
            }
            if entries
                .iter()
                .any(|entry| !entry.is_valid() || !valid_id(&entry.id))
            {
                return Err("Invalid calendar date or entry".into());
            }
            let mut ids = std::collections::HashSet::new();
            if entries.iter().any(|entry| !ids.insert(&entry.id)) {
                return Err("Duplicate calendar entry id".into());
            }
            validate_changes(changes, &ids, store)?;
        }
    }
    Ok(())
}

/// The calendar entries that a saved ride completed: those are history and stay as they were.
pub(crate) fn completed_entry_ids(store: &Store) -> std::collections::HashSet<String> {
    store
        .list_all_rides()
        .into_iter()
        .flat_map(|route| route.rides)
        .filter_map(|ride| ride.completed_calendar_id)
        .collect()
}

fn validate_changes(
    changes: &[EntryChange],
    new_ids: &std::collections::HashSet<&String>,
    store: &Store,
) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }
    let calendar = store.load_calendar().map_err(|e| e.to_string())?;
    let completed = completed_entry_ids(store);
    let mut seen = std::collections::HashSet::new();
    for change in changes {
        if !valid_id(&change.entry_id) || !seen.insert(&change.entry_id) {
            return Err("Invalid or duplicate change entry_id".into());
        }
        if new_ids.contains(&change.entry_id) {
            return Err("A change cannot target an entry added by the same plan".into());
        }
        if change.reason.trim().is_empty() || change.reason.len() > 1_000 {
            return Err("Every change needs a short reason".into());
        }
        if change
            .entry
            .as_ref()
            .is_some_and(|e| e.id != change.entry_id)
        {
            return Err("A changed entry must keep its id".into());
        }
        if !calendar.iter().any(|e| e.id == change.entry_id) {
            return Err(format!(
                "Calendar entry {} does not exist; use get_training_context to see the calendar",
                change.entry_id
            ));
        }
        if completed.contains(&change.entry_id) {
            return Err(format!(
                "Calendar entry {} is already completed and cannot be changed",
                change.entry_id
            ));
        }
    }
    Ok(())
}

pub(crate) fn sanitized_workout(workout: &Workout) -> Workout {
    let mut workout = workout.clone();
    workout.goal.clear();
    workout.description.clear();
    workout.sources.clear();
    for step in &mut workout.steps {
        step.note = None;
        step.messages.clear();
    }
    workout
}

/// Translated labels may change, while the prescribed workout must stay identical.
pub(crate) fn workout_structure_matches(original: &Workout, proposed: &Workout) -> bool {
    original.id == proposed.id
        && original.category == proposed.category
        && original.steps.len() == proposed.steps.len()
        && original.steps.iter().zip(&proposed.steps).all(|(a, b)| {
            a.kind == b.kind
                && a.duration_s == b.duration_s
                && a.target == b.target
                && a.cadence == b.cadence
        })
}

fn valid_target(target: StepTarget) -> bool {
    match target {
        StepTarget::PctFtp { value } => value.is_finite() && (0.0..=250.0).contains(&value),
        StepTarget::RampPct { from, to } => [from, to]
            .iter()
            .all(|n| n.is_finite() && (0.0..=250.0).contains(n)),
        StepTarget::Free => true,
        _ => false,
    }
}

fn root() -> PathBuf {
    Store::default_location().dir().to_path_buf()
}

fn with_lock<T>(f: impl FnOnce(&Path) -> io::Result<T>) -> io::Result<T> {
    let root = root();
    fs::create_dir_all(&root)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("ai-drafts.lock"))?;
    lock.lock()?;
    f(&root)
}

fn read_draft_unvalidated(path: &Path) -> io::Result<AiDraft> {
    let mut bytes = Vec::new();
    File::open(path)?.take(2_000_001).read_to_end(&mut bytes)?;
    if bytes.len() > 2_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "proposal too large",
        ));
    }
    let draft: AiDraft = serde_json::from_slice(&bytes)?;
    Ok(draft)
}

pub fn list_drafts() -> io::Result<Vec<AiDraft>> {
    with_lock(list_drafts_at)
}

/// Keep a submitted draft visible even if the workout library changes before review.
/// Approval validates against the current library again.
fn list_drafts_at(root: &Path) -> io::Result<Vec<AiDraft>> {
    let dir = root.join("ai-drafts");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut drafts = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(draft) = read_draft_unvalidated(&path) {
                if valid_id(&draft.id)
                    && path.file_stem().and_then(|s| s.to_str()) == Some(draft.id.as_str())
                {
                    drafts.push(draft);
                }
            }
        }
    }
    drafts.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(drafts)
}

pub fn process_draft(id: &str) -> io::Result<()> {
    if !valid_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid id"));
    }
    with_lock(|root| {
        fs::create_dir_all(root.join("ai-processed"))?;
        let name = format!("{id}.json");
        let source = root.join("ai-drafts").join(&name);
        let target = root.join("ai-processed").join(name);
        if target.exists() {
            return Ok(());
        }
        fs::rename(source, target)
    })
}

fn submit_draft(draft: &AiDraft) -> io::Result<&'static str> {
    if !valid_id(&draft.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid request id",
        ));
    }
    with_lock(|root| {
        let name = format!("{}.json", draft.id);
        for folder in ["ai-drafts", "ai-processed"] {
            let existing = root.join(folder).join(&name);
            if existing.exists() {
                return if read_draft_unvalidated(&existing)? == *draft {
                    Ok("already submitted")
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "request id already used",
                    ))
                };
            }
        }
        validate_draft(draft).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let bytes = serde_json::to_vec(draft)?;
        if bytes.len() > 2_000_000 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "proposal too large",
            ));
        }
        let dir = root.join("ai-drafts");
        fs::create_dir_all(&dir)?;
        let path = dir.join(name);
        let temp = dir.join(format!(".{}.{}.tmp", draft.id, std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let written = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(err) = written.and_then(|()| fs::rename(&temp, &path)) {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        Ok("pending approval")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::WorkoutLibrary;

    #[test]
    fn pending_draft_stays_visible_after_its_workout_is_removed() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        let mut workout = WorkoutLibrary::embedded().all()[0].clone();
        workout.id = "import-pending-test".to_string();
        store.save_workout(&workout).expect("store workout");
        let draft = AiDraft {
            id: "pending-test".to_string(),
            kind: DraftKind::Training {
                title: "Plan".to_string(),
                workouts: Vec::new(),
                entries: vec![CalendarEntry {
                    id: "scheduled-test".to_string(),
                    date: "2028-03-10".to_string(),
                    time: None,
                    workout: sanitized_workout(&workout),
                }],
                changes: Vec::new(),
            },
        };
        assert!(validate_draft_with_store(&draft, &store).is_ok());
        let inbox = dir.path().join("ai-drafts");
        fs::create_dir(&inbox).expect("inbox");
        fs::write(
            inbox.join("pending-test.json"),
            serde_json::to_vec(&draft).expect("serialize"),
        )
        .expect("save draft");
        store.delete_workout(&workout.id).expect("remove workout");
        assert!(validate_draft_with_store(&draft, &store).is_err());
        assert_eq!(list_drafts_at(dir.path()).expect("list inbox"), vec![draft]);
    }

    #[test]
    fn library_proposal_survives_language_changes_but_rejects_modified_targets() {
        let hungarian = ride_i18n::with_lang(ride_i18n::Lang::HU, || {
            WorkoutLibrary::embedded().all().to_vec()
        });
        let english = ride_i18n::with_lang(ride_i18n::Lang::EN, || {
            WorkoutLibrary::embedded().all().to_vec()
        });
        let (original, translated) = hungarian
            .iter()
            .find_map(|original| {
                english
                    .iter()
                    .find(|translated| {
                        translated.id == original.id
                            && translated.name != original.name
                            && workout_structure_matches(original, translated)
                    })
                    .map(|translated| (original, translated))
            })
            .expect("a translated workout with identical structure");
        let mut proposal = sanitized_workout(translated);
        assert!(workout_structure_matches(original, &proposal));
        proposal.steps[0].duration_s += 1;
        assert!(!workout_structure_matches(original, &proposal));
        proposal.steps[0].duration_s -= 1;
        proposal.steps[0].target = match proposal.steps[0].target {
            StepTarget::Free => StepTarget::PctFtp { value: 50.0 },
            _ => StepTarget::Free,
        };
        assert!(!workout_structure_matches(original, &proposal));
    }

    fn entry(id: &str, workout: &Workout) -> CalendarEntry {
        CalendarEntry {
            id: id.to_string(),
            date: "2028-03-10".to_string(),
            time: None,
            workout: workout.clone(),
        }
    }

    fn removal(id: &str) -> AiDraft {
        AiDraft {
            id: "change-test".to_string(),
            kind: DraftKind::Training {
                title: "Adjust".to_string(),
                workouts: Vec::new(),
                entries: Vec::new(),
                changes: vec![EntryChange {
                    entry_id: id.to_string(),
                    entry: None,
                    reason: "Too much load after a hard week".to_string(),
                }],
            },
        }
    }

    #[test]
    fn a_draft_written_before_changes_existed_still_loads() {
        let old =
            r#"{"id":"old","kind":{"type":"training","title":"T","workouts":[],"entries":[]}}"#;
        let draft: AiDraft = serde_json::from_str(old).expect("old draft");
        let DraftKind::Training { changes, .. } = draft.kind else {
            panic!("training expected");
        };
        assert!(changes.is_empty());
    }

    #[test]
    fn changes_must_target_an_existing_entry_that_is_not_completed() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        let workout = WorkoutLibrary::embedded().all()[0].clone();
        let draft = removal("planned-1");
        assert!(
            validate_draft_with_store(&draft, &store).is_err(),
            "the entry does not exist yet"
        );
        store
            .update_calendar(|calendar| {
                calendar.push(entry("planned-1", &workout));
                Ok(())
            })
            .expect("seed calendar");
        assert!(validate_draft_with_store(&draft, &store).is_ok());
        // a ride that completed the entry makes it history
        let rides = dir.path().join("rides").join("route-1");
        fs::create_dir_all(&rides).expect("rides folder");
        let meta = ride_model::RideMeta {
            stem: "1700000000".to_string(),
            completed_calendar_id: Some("planned-1".to_string()),
            route_name: "Test".to_string(),
            started_unix_s: 1_700_000_000.0,
            elapsed_s: 600.0,
            ..Default::default()
        };
        fs::write(rides.join("1700000000.json"), b"{}").expect("record file");
        fs::write(
            rides.join("index.json"),
            serde_json::to_vec(&vec![meta]).expect("index"),
        )
        .expect("write index");
        let error = validate_draft_with_store(&draft, &store).expect_err("completed entry");
        assert!(error.contains("completed"), "{error}");
    }

    #[test]
    fn a_change_needs_a_reason_and_must_keep_the_entry_id() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(dir.path());
        let workout = WorkoutLibrary::embedded().all()[0].clone();
        store
            .update_calendar(|calendar| {
                calendar.push(entry("planned-1", &workout));
                Ok(())
            })
            .expect("seed calendar");
        let mut silent = removal("planned-1");
        if let DraftKind::Training { changes, .. } = &mut silent.kind {
            changes[0].reason = "  ".to_string();
        }
        assert!(validate_draft_with_store(&silent, &store).is_err());
        let mut renamed = removal("planned-1");
        if let DraftKind::Training { changes, .. } = &mut renamed.kind {
            changes[0].entry = Some(entry("other-id", &workout));
        }
        assert!(validate_draft_with_store(&renamed, &store).is_err());
    }
}
