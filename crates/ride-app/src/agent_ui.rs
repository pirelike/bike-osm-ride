//! Review local AI proposals and configure the MCP connection.

use std::io::{self, Write};
use std::path::PathBuf;

use dioxus::prelude::*;
use ride_i18n::t;
use ride_model::{CalendarEntry, StepTarget, Workout, WorkoutLibrary};
use ride_route::store::Store;
use ride_ui_kit::{Dialog, Switch};
use serde::{Deserialize, Serialize};

use crate::agent::{self, AiDraft, DraftKind};
use crate::controller::Controller;

fn marker() -> PathBuf {
    ride_route::store::data_dir().join("ai-enabled")
}

fn set_enabled(enabled: bool) -> io::Result<()> {
    let path = marker();
    if enabled {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, b"")
    } else {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

/// What a client needs to start the local MCP server.
struct Connection {
    command: String,
    data_dir: String,
}

fn connection() -> Connection {
    let executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_default();
    Connection {
        command: executable.to_string_lossy().into_owned(),
        data_dir: ride_route::store::data_dir().to_string_lossy().into_owned(),
    }
}

impl Connection {
    /// The `mcpServers` block of Claude Desktop, Cursor and most other clients.
    fn json(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {"bike-osm-ride": {
                "command": self.command,
                "args": ["--mcp"],
                "env": {"RIDE_DATA_DIR": self.data_dir}
            }}
        }))
        .unwrap_or_default()
    }

    /// One line for the `claude mcp add` command of Claude Code.
    fn claude_code(&self) -> String {
        let quote = |text: &str| {
            if text.contains(char::is_whitespace) {
                format!("\"{text}\"")
            } else {
                text.to_string()
            }
        };
        format!(
            "claude mcp add bike-osm-ride --env {} -- {} --mcp",
            quote(&format!("RIDE_DATA_DIR={}", self.data_dir)),
            quote(&self.command)
        )
    }
}

fn copy(text: &str, mut result: Signal<Option<bool>>) {
    let Ok(value) = serde_json::to_string(text) else {
        result.set(Some(false));
        return;
    };
    let script = format!(
        r#"let success = false;
        try {{
            if (window.isSecureContext && navigator.clipboard) {{
                await navigator.clipboard.writeText({value});
                success = true;
            }} else {{
                const field = document.createElement('textarea');
                field.value = {value};
                field.style.position = 'fixed';
                field.style.opacity = '0';
                document.body.appendChild(field);
                try {{ field.select(); success = document.execCommand('copy'); }}
                finally {{ field.remove(); }}
            }}
        }} catch (_) {{}}
        dioxus.send(success);"#
    );
    let Some(mut eval) = ride_ui_kit::try_eval(&script) else {
        result.set(Some(false));
        return;
    };
    spawn(async move {
        result.set(Some(eval.recv::<bool>().await.unwrap_or(false)));
    });
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn feedback_path(route_id: &str, stem: &str) -> Option<PathBuf> {
    (safe_component(route_id) && safe_component(stem)).then(|| {
        ride_route::store::data_dir()
            .join("ai-feedback")
            .join(route_id)
            .join(format!("{stem}.json"))
    })
}

fn read_feedback(route_id: &str, stem: &str) -> Option<AiDraft> {
    let path = feedback_path(route_id, stem)?;
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > 1024 * 1024 {
        return None;
    }
    let draft: AiDraft = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    match &draft.kind {
        DraftKind::Feedback {
            route_id: route,
            stem: s,
            ..
        } if route == route_id && s == stem => Some(draft),
        _ => None,
    }
}

fn store_feedback(draft: &AiDraft, route_id: &str, stem: &str) -> io::Result<()> {
    let store = Store::default_location();
    if store.load_ride(route_id, stem).is_none() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "ride not found"));
    }
    let path = feedback_path(route_id, stem)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid ride"))?;
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("invalid path"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&serde_json::to_vec_pretty(draft)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, &path)
}

#[derive(Serialize, Deserialize)]
struct PendingPlan {
    draft: AiDraft,
    new_workouts: Vec<ride_model::Workout>,
    #[serde(default)]
    committed: bool,
}

fn pending_dir(store: &Store) -> PathBuf {
    store.dir().join("ai-pending")
}

fn pending_path(store: &Store, id: &str) -> PathBuf {
    pending_dir(store).join(format!("{id}.json"))
}

fn save_pending(store: &Store, pending: &PendingPlan) -> io::Result<()> {
    std::fs::create_dir_all(pending_dir(store))?;
    let path = pending_path(store, &pending.draft.id);
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&serde_json::to_vec(pending)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, path)
}

fn same_calendar_entry(a: &ride_model::CalendarEntry, b: &ride_model::CalendarEntry) -> bool {
    a.id == b.id
        && a.date == b.date
        && a.time == b.time
        && agent::workout_structure_matches(&a.workout, &b.workout)
}

fn finish_or_rollback(store: &Store, pending: &PendingPlan) -> io::Result<()> {
    let DraftKind::Training {
        entries, changes, ..
    } = &pending.draft.kind
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid pending plan",
        ));
    };
    let calendar = store.load_calendar()?;
    // The calendar is written in one atomic step, so it either holds the whole plan or none of it.
    let entries_in = entries.iter().all(|entry| {
        calendar
            .iter()
            .any(|saved| same_calendar_entry(saved, entry))
    });
    let changes_in = changes.iter().all(|change| match &change.entry {
        Some(entry) => calendar
            .iter()
            .any(|saved| same_calendar_entry(saved, entry)),
        None => !calendar.iter().any(|saved| saved.id == change.entry_id),
    });
    let committed = pending.committed
        || ((!entries.is_empty() || !changes.is_empty()) && entries_in && changes_in);
    let current = store.list_workouts();
    for workout in &pending.new_workouts {
        if committed {
            match current.iter().find(|saved| saved.id == workout.id) {
                Some(saved) if saved != workout => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "workout changed during recovery",
                    ))
                }
                Some(_) => {}
                None => store.save_workout(workout)?,
            }
        } else if current.iter().any(|saved| saved == workout) {
            store.delete_workout(&workout.id)?;
        }
    }
    match std::fs::remove_file(pending_path(store, &pending.draft.id)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Complete or roll back an interrupted multi-file plan approval.
pub(crate) fn recover_pending() -> io::Result<()> {
    let root = ride_route::store::data_dir();
    std::fs::create_dir_all(&root)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("ai-approval.lock"))?;
    lock.lock()?;
    recover_pending_locked(&Store::at(root))
}

fn recover_pending_locked(store: &Store) -> io::Result<()> {
    let dir = pending_dir(store);
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        if std::fs::metadata(&path)?.len() > 4_100_000 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "pending plan too large",
            ));
        }
        let pending: PendingPlan = serde_json::from_slice(&std::fs::read(&path)?)?;
        if !safe_component(&pending.draft.id) || path != pending_path(store, &pending.draft.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid pending plan",
            ));
        }
        finish_or_rollback(store, &pending)?;
    }
    Ok(())
}

fn accept(draft: &AiDraft) -> io::Result<()> {
    let root = ride_route::store::data_dir();
    std::fs::create_dir_all(&root)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("ai-approval.lock"))?;
    lock.lock()?;
    let store = Store::default_location();
    recover_pending_locked(&store)?;
    agent::validate_draft(draft)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    match &draft.kind {
        DraftKind::Training {
            workouts,
            entries,
            changes,
            ..
        } => apply_training(&store, draft, workouts, entries, changes)?,
        DraftKind::Feedback { route_id, stem, .. } => store_feedback(draft, route_id, stem)?,
    }
    agent::process_draft(&draft.id)
}

fn apply_training(
    store: &Store,
    draft: &AiDraft,
    workouts: &[ride_model::Workout],
    entries: &[ride_model::CalendarEntry],
    changes: &[agent::EntryChange],
) -> io::Result<()> {
    let existing = store.list_workouts();
    for workout in workouts {
        if existing
            .iter()
            .any(|old| old.id == workout.id && old != workout)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "workout already exists",
            ));
        }
    }
    let library = WorkoutLibrary::with_imported(existing.clone());
    let resolve = |entry: &ride_model::CalendarEntry| -> io::Result<ride_model::CalendarEntry> {
        let resolved = if let Some(workout) = workouts
            .iter()
            .find(|workout| workout.id == entry.workout.id)
        {
            (workout == &entry.workout).then_some(workout)
        } else {
            library
                .get(&entry.workout.id)
                .filter(|workout| agent::workout_structure_matches(workout, &entry.workout))
        };
        let Some(workout) = resolved else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workout snapshot differs",
            ));
        };
        let mut resolved_entry = entry.clone();
        resolved_entry.workout = workout.clone();
        Ok(resolved_entry)
    };
    let resolved_entries = entries
        .iter()
        .map(resolve)
        .collect::<io::Result<Vec<_>>>()?;
    let resolved_changes = changes
        .iter()
        .map(|change| {
            Ok(agent::EntryChange {
                entry: change.entry.as_ref().map(resolve).transpose()?,
                ..change.clone()
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut resolved_draft = draft.clone();
    if let DraftKind::Training {
        entries, changes, ..
    } = &mut resolved_draft.kind
    {
        *entries = resolved_entries.clone();
        *changes = resolved_changes.clone();
    }
    let mut pending = PendingPlan {
        draft: resolved_draft,
        new_workouts: workouts
            .iter()
            .filter(|workout| !existing.iter().any(|old| old.id == workout.id))
            .cloned()
            .collect(),
        committed: false,
    };
    save_pending(store, &pending)?;
    let applied = (|| {
        for workout in &pending.new_workouts {
            store.save_workout(workout)?;
        }
        store.update_calendar(|calendar| {
            for entry in &resolved_entries {
                if let Some(existing) = calendar.iter().find(|item| item.id == entry.id) {
                    if !same_calendar_entry(existing, entry) {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "calendar entry already exists",
                        ));
                    }
                } else {
                    calendar.push(entry.clone());
                }
            }
            for change in &resolved_changes {
                let position = calendar.iter().position(|item| item.id == change.entry_id);
                match (position, &change.entry) {
                    (Some(at), Some(entry)) => calendar[at] = entry.clone(),
                    (Some(at), None) => {
                        calendar.remove(at);
                    }
                    // Already removed (an approval that was interrupted after the write).
                    (None, None) => {}
                    (None, Some(_)) => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "changed calendar entry is gone",
                        ))
                    }
                }
            }
            Ok(())
        })
    })();
    if let Err(error) = applied {
        finish_or_rollback(store, &pending)?;
        return Err(error);
    }
    pending.committed = true;
    if let Err(error) = save_pending(store, &pending) {
        pending.committed = false;
        finish_or_rollback(store, &pending)?;
        return Err(error);
    }
    std::fs::remove_file(pending_path(store, &draft.id))?;
    Ok(())
}

fn target_text(target: &StepTarget) -> String {
    match target {
        StepTarget::PctFtp { value } => format!("{value:.0}% FTP"),
        StepTarget::RampPct { from, to } => format!("{from:.0}–{to:.0}% FTP"),
        StepTarget::Watts { value } => format!("{value:.0} W"),
        StepTarget::RampWatts { from, to } => format!("{from:.0}–{to:.0} W"),
        StepTarget::StepWatts { start, max, .. } => format!("{start:.0}–{max:.0} W"),
        StepTarget::Free => t("app.agent.free").to_string(),
    }
}

fn feedback_rows(good: &str, improve: &str, next: &str) -> Element {
    rsx! {
        dl { class: "shell-agent__kv",
            dt { {t("app.agent.good")} } dd { "{good}" }
            dt { {t("app.agent.improve")} } dd { "{improve}" }
            dt { {t("app.agent.next")} } dd { "{next}" }
        }
    }
}

fn workout_view(workout: &Workout) -> Element {
    rsx! {
        div { class: "shell-agent-workout",
            div { class: "shell-agent-workout__title",
                strong { "{workout.name}" }
                span { "{workout.duration_text()}" }
            }
            ul { class: "shell-agent-steps",
                for step in &workout.steps {
                    li {
                        span { "{step.name}" }
                        span { "{ride_ui_kit::format::fmt_duration(f64::from(step.duration_s))}" }
                        span { "{target_text(&step.target)}" }
                    }
                }
            }
        }
    }
}

fn entry_label(entry: &CalendarEntry) -> String {
    let when = match entry.time.as_deref() {
        Some(time) => format!("{} {time}", entry.date),
        None => entry.date.clone(),
    };
    format!("{when} · {}", entry.workout.name)
}

/// One adjustment of an already scheduled workout: what it was, what it becomes, and why.
fn change_view(change: &agent::EntryChange, calendar: &[CalendarEntry]) -> Element {
    let before = calendar
        .iter()
        .find(|entry| entry.id == change.entry_id)
        .map(entry_label)
        .unwrap_or_else(|| change.entry_id.clone());
    let summary = match &change.entry {
        Some(entry) => format!("{before}  →  {}", entry_label(entry)),
        None => before,
    };
    let label = if change.entry.is_some() {
        t("app.agent.change_replace")
    } else {
        t("app.agent.change_remove")
    };
    rsx! {
        div { class: "shell-agent-change",
            div { class: "shell-agent-change__head",
                span { class: "tag", {label} }
                span { "{summary}" }
            }
            p { class: "shell-agent-change__reason", strong { {t("app.agent.reason")} ": " } "{change.reason}" }
            if let Some(entry) = &change.entry { {workout_view(&entry.workout)} }
        }
    }
}

#[component]
pub fn AgentDialog() -> Element {
    let ctl = use_context::<Controller>();
    let mut enabled = use_signal(|| marker().is_file());
    let mut drafts = use_signal(|| agent::list_drafts().unwrap_or_default());
    let mut error = use_signal(String::new);
    let copy_result = use_signal(|| None::<bool>);
    let key = ctl.st.agent_ride.read().clone();
    let mut feedback = use_signal(|| {
        key.as_ref()
            .and_then(|(route, stem)| read_feedback(route, stem))
    });
    let mut code_format = use_signal(|| true);
    let connection = connection();
    let (json_text, cli_text) = (connection.json(), connection.claude_code());
    let shown_text = if code_format() {
        cli_text.clone()
    } else {
        json_text.clone()
    };
    let copy_text = shown_text.clone();
    let prompt = key
        .as_ref()
        .map(|(route, stem)| format!("{} route_id={route} stem={stem}", t("app.agent.prompt")));
    rsx! {
        Dialog { open: true, wide: true, title: t("app.agent.title").to_string(), on_close: move |_| ctl.close_dialog(),
            div { class: "shell-agent",
                if cfg!(not(target_os = "android")) {
                    section { class: "shell-agent__card",
                        Switch {
                            row: true,
                            checked: enabled(),
                            label: t("app.agent.connection").to_string(),
                            hint: t("app.agent.explanation").to_string(),
                            on_change: move |next: bool| match set_enabled(next) {
                                Ok(()) => { enabled.set(next); error.set(String::new()); },
                                Err(_) => error.set(t("app.agent.error").to_string()),
                            },
                        }
                        if enabled() {
                            div { class: "shell-agent__head",
                                span { {t("app.agent.config")} }
                                div { class: "seg seg--sm", role: "group", aria_label: t("app.agent.config"),
                                    button { class: if code_format() { "seg__item is-active" } else { "seg__item" }, r#type: "button", aria_pressed: "{code_format()}",
                                        onclick: move |_| code_format.set(true), "Claude Code" }
                                    button { class: if code_format() { "seg__item" } else { "seg__item is-active" }, r#type: "button", aria_pressed: "{!code_format()}",
                                        onclick: move |_| code_format.set(false), "JSON" }
                                }
                            }
                            p { class: "shell-agent__hint", {t("app.agent.config_hint")} }
                            dl { class: "shell-agent-facts",
                                dt { {t("app.agent.fact_command")} } dd { "{connection.command}" }
                                dt { {t("app.agent.fact_args")} } dd { "--mcp" }
                                dt { {t("app.agent.fact_data")} } dd { "{connection.data_dir}" }
                            }
                            button { class: "btn btn--primary", r#type: "button", onclick: move |_| copy(&copy_text, copy_result), {t("app.agent.copy")} }
                            details { class: "shell-agent__raw",
                                summary { {t("app.agent.raw")} }
                                pre { class: "shell-agent__code", "{shown_text}" }
                            }
                        }
                    }
                } else {
                    p { class: "shell-agent__empty", {t("app.agent.desktop")} }
                }
                if let Some(prompt) = prompt {
                    section { class: "shell-agent__card",
                        div { class: "shell-agent__head",
                            span { {t("app.agent.prompt_hint")} }
                            button { class: "btn btn--sm", r#type: "button", onclick: { let text = prompt.clone(); move |_| copy(&text, copy_result) }, {t("app.agent.copy")} }
                        }
                        pre { class: "shell-agent__code", "{prompt}" }
                    }
                }
                if let Some(success) = *copy_result.read() {
                    p { role: "status", class: if success { "shell-agent__status badge badge--ok" } else { "shell-agent__status badge badge--crit" },
                        if success { {t("app.agent.copy_ok")} } else { {t("app.agent.copy_failed")} }
                    }
                }
                if let Some(AiDraft { kind: DraftKind::Feedback { good, improve, next, .. }, .. }) = feedback.read().as_ref() {
                    section { class: "shell-agent__card",
                        h3 { class: "shell-agent__title", {t("app.agent.saved")} }
                        {feedback_rows(good, improve, next)}
                    }
                }
                section { class: "shell-agent__section",
                    div { class: "shell-agent__head",
                        h3 { {t("app.agent.proposals")} }
                        span { class: "tag", "{drafts.read().len()}" }
                        button { class: "btn btn--ghost btn--sm shell-agent__refresh", r#type: "button", onclick: move |_| match agent::list_drafts() {
                            Ok(list) => { drafts.set(list); error.set(String::new()); },
                            Err(_) => error.set(t("app.agent.error").to_string()),
                        }, {t("app.agent.refresh")} }
                    }
                    if !error.read().is_empty() { p { role: "alert", class: "shell-agent__status badge badge--crit", "{error}" } }
                    if drafts.read().is_empty() { p { class: "shell-agent__empty", {t("app.agent.empty")} } }
                    for draft in drafts.read().iter().cloned() {
                        {
                            let calendar_now = Store::default_location().load_calendar().unwrap_or_default();
                            let approve = draft.clone();
                            let reject_id = draft.id.clone();
                            rsx! {
                                article { class: "shell-agent-draft", key: "{draft.id}",
                                    match &draft.kind {
                                        DraftKind::Training { title, workouts, entries, changes } => rsx! {
                                            h4 { "{title}" }
                                            for change in changes { {change_view(change, &calendar_now)} }
                                            for workout in workouts { {workout_view(workout)} }
                                            for entry in entries {
                                                if !workouts.iter().any(|w| w.id == entry.workout.id) {
                                                    {workout_view(&entry.workout)}
                                                }
                                                p { class: "shell-agent-draft__when",
                                                    strong { "{entry.date} {entry.time.as_deref().unwrap_or_default()}" }
                                                    " · {entry.workout.name}"
                                                }
                                            }
                                        },
                                        DraftKind::Feedback { route_id, stem, good, improve, next } => rsx! {
                                            h4 { {t("app.agent.review")} " · {route_id} / {stem}" }
                                            {feedback_rows(good, improve, next)}
                                        },
                                    }
                                    div { class: "shell-agent-draft__actions",
                                        button { class: "btn btn--primary", r#type: "button", onclick: move |_| {
                                            match accept(&approve) {
                                                Ok(()) => {
                                                    let mut imported = ctl.st.imported_workouts;
                                                    imported.set(Store::default_location().list_workouts());
                                                    drafts.set(agent::list_drafts().unwrap_or_default());
                                                    if let Some((route, stem)) = ctl.st.agent_ride.peek().as_ref() {
                                                        feedback.set(read_feedback(route, stem));
                                                    }
                                                    error.set(String::new());
                                                }
                                                Err(_) => error.set(t("app.agent.error").to_string()),
                                            }
                                        }, {t("app.agent.accept")} }
                                        button { class: "btn btn--ghost", r#type: "button", onclick: move |_| {
                                            match agent::process_draft(&reject_id) {
                                                Ok(()) => { drafts.set(agent::list_drafts().unwrap_or_default()); error.set(String::new()); }
                                                Err(_) => error.set(t("app.agent.error").to_string()),
                                            }
                                        }, {t("app.agent.reject")} }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::CalendarEntry;

    fn plan() -> AiDraft {
        let mut workout = WorkoutLibrary::embedded().all()[0].clone();
        workout.id = "import-agent-test".into();
        AiDraft {
            id: "proposal-test".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![workout.clone()],
                entries: vec![CalendarEntry {
                    id: "scheduled-test".into(),
                    date: "2028-03-10".into(),
                    time: Some("10:00".into()),
                    workout,
                }],
                changes: Vec::new(),
            },
        }
    }

    fn apply(store: &Store, draft: &AiDraft) -> io::Result<()> {
        let DraftKind::Training {
            workouts, entries, ..
        } = &draft.kind
        else {
            return Err(io::Error::other("expected training"));
        };
        let changes = match &draft.kind {
            DraftKind::Training { changes, .. } => changes.as_slice(),
            DraftKind::Feedback { .. } => &[],
        };
        apply_training(store, draft, workouts, entries, changes)
    }

    #[test]
    fn repeated_approval_keeps_one_copy_of_each_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let draft = plan();
        apply(&store, &draft).expect("first approval");
        apply(&store, &draft).expect("repeated approval");
        assert_eq!(store.list_workouts().len(), 1);
        assert_eq!(store.load_calendar().expect("calendar").len(), 1);
    }

    #[test]
    fn conflicting_calendar_id_rolls_back_new_workouts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let draft = plan();
        let DraftKind::Training { entries, .. } = &draft.kind else {
            unreachable!()
        };
        let mut existing = entries[0].clone();
        existing.date = "2028-03-11".into();
        store
            .update_calendar(|calendar| {
                calendar.push(existing.clone());
                Ok(())
            })
            .expect("save");
        assert!(apply(&store, &draft).is_err());
        assert!(store.list_workouts().is_empty());
        assert_eq!(store.load_calendar().expect("calendar"), vec![existing]);
    }

    #[test]
    fn interrupted_plan_rolls_back_and_committed_standalone_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let draft = plan();
        let DraftKind::Training { workouts, .. } = &draft.kind else {
            unreachable!()
        };
        let mut pending = PendingPlan {
            draft: draft.clone(),
            new_workouts: workouts.clone(),
            committed: false,
        };
        save_pending(&store, &pending).expect("journal");
        store.save_workout(&workouts[0]).expect("workout");
        recover_pending_locked(&store).expect("recovery");
        assert!(store.list_workouts().is_empty());

        pending.draft.kind = DraftKind::Training {
            title: "Standalone".into(),
            workouts: workouts.clone(),
            entries: vec![],
            changes: Vec::new(),
        };
        pending.committed = true;
        save_pending(&store, &pending).expect("committed journal");
        store.save_workout(&workouts[0]).expect("workout");
        recover_pending_locked(&store).expect("recovery");
        assert_eq!(store.list_workouts(), workouts.clone());
    }

    #[test]
    fn recovery_completes_workouts_when_calendar_entries_already_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let draft = plan();
        let DraftKind::Training {
            workouts, entries, ..
        } = &draft.kind
        else {
            unreachable!()
        };
        let pending = PendingPlan {
            draft: draft.clone(),
            new_workouts: workouts.clone(),
            committed: false,
        };
        save_pending(&store, &pending).expect("journal");
        store
            .update_calendar(|calendar| {
                calendar.extend(entries.clone());
                Ok(())
            })
            .expect("calendar");
        recover_pending_locked(&store).expect("recovery");
        assert_eq!(store.list_workouts(), workouts.clone());
        assert!(!pending_path(&store, &draft.id).exists());
    }

    #[test]
    fn existing_workout_snapshot_must_match_what_agent_received() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let workout = agent::sanitized_workout(&WorkoutLibrary::embedded().all()[0]);
        let entry = CalendarEntry {
            id: "existing".into(),
            date: "2028-03-10".into(),
            time: None,
            workout,
        };
        let draft = AiDraft {
            id: "existing-test".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![],
                entries: vec![entry.clone()],
                changes: Vec::new(),
            },
        };
        apply(&store, &draft).expect("matching snapshot");
        let mut altered = entry;
        altered.workout.steps[0].duration_s += 1;
        let bad = AiDraft {
            id: "bad".into(),
            kind: DraftKind::Training {
                title: "Bad".into(),
                workouts: vec![],
                entries: vec![altered],
                changes: Vec::new(),
            },
        };
        assert!(apply(&store, &bad).is_err());
        assert_eq!(store.load_calendar().expect("calendar").len(), 1);
    }

    #[test]
    fn approved_existing_workout_keeps_coaching_and_retries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let mut workout = WorkoutLibrary::embedded().all()[0].clone();
        workout.id = "import-coaching".into();
        workout.goal = "private goal".into();
        workout.steps[0].note = Some("coaching note".into());
        workout.steps[0].messages = vec![ride_model::StepMessage {
            at_s: 10,
            text: "coaching message".into(),
        }];
        store.save_workout(&workout).expect("store workout");
        let entry = CalendarEntry {
            id: "coaching-test".into(),
            date: "2028-03-10".into(),
            time: None,
            workout: agent::sanitized_workout(&workout),
        };
        let draft = AiDraft {
            id: "coaching-plan".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![],
                entries: vec![entry],
                changes: Vec::new(),
            },
        };
        apply(&store, &draft).expect("approve");
        apply(&store, &draft).expect("retry");
        let calendar = store.load_calendar().expect("calendar");
        assert_eq!(calendar.len(), 1);
        assert_eq!(calendar[0].workout, workout);
    }

    #[test]
    fn localized_library_workout_is_resolved_and_can_be_retried() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let english =
            WorkoutLibrary::from_json(include_str!("../../ride-model/assets/workouts.en.json"));
        let hungarian =
            WorkoutLibrary::from_json(include_str!("../../ride-model/assets/workouts.json"));
        let id = "a-recovery-spin-30";
        let local = WorkoutLibrary::embedded()
            .get(id)
            .expect("local workout")
            .clone();
        let translated = [english.get(id), hungarian.get(id)]
            .into_iter()
            .flatten()
            .find(|candidate| candidate.name != local.name)
            .expect("different language");
        let mut proposed = agent::sanitized_workout(translated);
        let entry = CalendarEntry {
            id: "translated".into(),
            date: "2028-03-10".into(),
            time: None,
            workout: proposed.clone(),
        };
        let draft = AiDraft {
            id: "translated-plan".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![],
                entries: vec![entry],
                changes: Vec::new(),
            },
        };
        apply(&store, &draft).expect("approve translated workout");
        apply(&store, &draft).expect("retry translated workout");
        assert_eq!(store.load_calendar().expect("calendar")[0].workout, local);
        proposed.steps[0].duration_s += 1;
        let entry = CalendarEntry {
            id: "altered".into(),
            date: "2028-03-10".into(),
            time: None,
            workout: proposed,
        };
        let altered = AiDraft {
            id: "altered-plan".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![],
                entries: vec![entry],
                changes: Vec::new(),
            },
        };
        assert!(apply(&store, &altered).is_err());
        let DraftKind::Training { entries, .. } = &altered.kind else {
            unreachable!()
        };
        let mut changed_target = entries[0].clone();
        changed_target.workout.steps[0].duration_s -= 1;
        changed_target.workout.steps[0].target = StepTarget::PctFtp { value: 249.0 };
        let target_draft = AiDraft {
            id: "altered-target".into(),
            kind: DraftKind::Training {
                title: "Plan".into(),
                workouts: vec![],
                entries: vec![changed_target],
                changes: Vec::new(),
            },
        };
        assert!(apply(&store, &target_draft).is_err());
    }

    fn scheduled(
        id: &str,
        date: &str,
        time: Option<&str>,
        workout: &ride_model::Workout,
    ) -> CalendarEntry {
        CalendarEntry {
            id: id.into(),
            date: date.into(),
            time: time.map(Into::into),
            workout: workout.clone(),
        }
    }

    fn change_plan(
        id: &str,
        changes: Vec<agent::EntryChange>,
        new_workouts: Vec<ride_model::Workout>,
    ) -> AiDraft {
        AiDraft {
            id: id.into(),
            kind: DraftKind::Training {
                title: "Adjust".into(),
                workouts: new_workouts,
                entries: vec![],
                changes,
            },
        }
    }

    #[test]
    fn approved_changes_move_swap_and_remove_scheduled_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let library = WorkoutLibrary::embedded();
        let (a, b) = (library.all()[0].clone(), library.all()[1].clone());
        store
            .update_calendar(|calendar| {
                calendar.push(scheduled("keep", "2028-03-10", None, &a));
                calendar.push(scheduled("drop", "2028-03-11", None, &a));
                calendar.push(scheduled("move", "2028-03-12", Some("18:00"), &a));
                Ok(())
            })
            .expect("seed calendar");
        let draft = change_plan(
            "adjust-1",
            vec![
                agent::EntryChange {
                    entry_id: "move".into(),
                    entry: Some(scheduled("move", "2028-03-14", Some("07:30"), &b)),
                    reason: "The last threshold ride was too hard".into(),
                },
                agent::EntryChange {
                    entry_id: "drop".into(),
                    entry: None,
                    reason: "Recovery day".into(),
                },
            ],
            vec![],
        );
        apply(&store, &draft).expect("approve changes");
        let calendar = store.load_calendar().expect("calendar");
        let ids: Vec<_> = calendar.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["keep", "move"]);
        let moved = &calendar[1];
        assert_eq!(
            (moved.date.as_str(), moved.time.as_deref()),
            ("2028-03-14", Some("07:30"))
        );
        assert_eq!(moved.workout.id, b.id);
        // approving the same plan again changes nothing and does not fail
        apply(&store, &draft).expect("repeated approval");
        assert_eq!(store.load_calendar().expect("calendar").len(), 2);
    }

    #[test]
    fn a_change_to_a_missing_entry_fails_and_keeps_the_calendar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let a = WorkoutLibrary::embedded().all()[0].clone();
        store
            .update_calendar(|calendar| {
                calendar.push(scheduled("keep", "2028-03-10", None, &a));
                Ok(())
            })
            .expect("seed calendar");
        let draft = change_plan(
            "adjust-2",
            vec![agent::EntryChange {
                entry_id: "ghost".into(),
                entry: Some(scheduled("ghost", "2028-03-15", None, &a)),
                reason: "No such entry".into(),
            }],
            vec![],
        );
        assert!(apply(&store, &draft).is_err());
        assert_eq!(store.load_calendar().expect("calendar").len(), 1);
    }

    #[test]
    fn recovery_keeps_new_workouts_only_when_the_changes_reached_the_calendar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::at(dir.path());
        let a = WorkoutLibrary::embedded().all()[0].clone();
        let mut fresh = a.clone();
        fresh.id = "import-recovery-change".into();
        store.save_workout(&fresh).expect("store workout");
        let draft = change_plan(
            "adjust-3",
            vec![agent::EntryChange {
                entry_id: "drop".into(),
                entry: None,
                reason: "Recovery day".into(),
            }],
            vec![fresh.clone()],
        );
        let pending = PendingPlan {
            draft,
            new_workouts: vec![fresh.clone()],
            committed: false,
        };
        // the write never happened: the entry is still scheduled, so the workout is rolled back
        store
            .update_calendar(|calendar| {
                calendar.push(scheduled("drop", "2028-03-11", None, &a));
                Ok(())
            })
            .expect("seed calendar");
        save_pending(&store, &pending).expect("journal");
        recover_pending_locked(&store).expect("recovery");
        assert!(!store.list_workouts().iter().any(|w| w.id == fresh.id));
        // the write happened: the entry is gone, so the workout is kept
        store.save_workout(&fresh).expect("store workout again");
        store
            .update_calendar(|calendar| {
                calendar.retain(|entry| entry.id != "drop");
                Ok(())
            })
            .expect("remove entry");
        save_pending(&store, &pending).expect("journal");
        recover_pending_locked(&store).expect("recovery");
        assert!(store.list_workouts().iter().any(|w| w.id == fresh.id));
    }
}
