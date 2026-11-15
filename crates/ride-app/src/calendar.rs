//! The workout calendar dialog.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use ride_i18n::{t, tf};
use ride_model::{CalendarEntry, RideState, RouteRides, Workout, WorkoutLibrary};
use ride_route::store::Store;
use ride_ui_kit::Dialog;

use crate::controller::Controller;
use crate::state::DialogKind;

fn next_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{nanos:x}-{:x}-{:x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn month_days(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

/// Monday-first position of the first day of a month.
fn first_weekday(year: u32, month: u32) -> u32 {
    let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let adjusted = year - u32::from(month < 3);
    let sunday_first = (adjusted + adjusted / 4 - adjusted / 100
        + adjusted / 400
        + offsets[(month - 1) as usize]
        + 1)
        % 7;
    (sunday_first + 6) % 7
}

fn adjacent_month(year: u32, month: u32, offset: i32) -> (u32, u32) {
    let index = (year as i32 * 12 + month as i32 - 1 + offset).clamp(12, 9999 * 12 + 11);
    ((index / 12) as u32, (index % 12 + 1) as u32)
}

#[derive(Clone, Debug, PartialEq)]
struct CalendarRide {
    route_id: String,
    stem: String,
    started_unix_s: f64,
    title: String,
    calendar_id: Option<String>,
    local_date: String,
}

fn calendar_rides(routes: Vec<RouteRides>) -> Vec<CalendarRide> {
    let mut rides: Vec<_> = routes
        .into_iter()
        .flat_map(|route| {
            let route_id = route.route_id;
            let route_name = route.route_name;
            route.rides.into_iter().filter_map(move |ride| {
                if !ride.started_unix_s.is_finite() {
                    return None;
                }
                let title = ride
                    .workout_name
                    .filter(|name| !name.trim().is_empty())
                    .or_else(|| (!ride.route_name.trim().is_empty()).then_some(ride.route_name))
                    .unwrap_or_else(|| route_name.clone());
                Some(CalendarRide {
                    route_id: route_id.clone(),
                    stem: ride.stem,
                    started_unix_s: ride.started_unix_s,
                    title,
                    calendar_id: ride.completed_calendar_id,
                    local_date: String::new(),
                })
            })
        })
        .collect();
    rides.sort_by(|a, b| {
        b.started_unix_s
            .total_cmp(&a.started_unix_s)
            .then_with(|| a.route_id.cmp(&b.route_id))
            .then_with(|| a.stem.cmp(&b.stem))
    });
    rides
}

/// A linked ride shares its scheduled entry's row when they are on the same day.
fn show_separate_ride(ride: &CalendarRide, entries: &[CalendarEntry]) -> bool {
    !entries.iter().any(|entry| {
        ride.calendar_id.as_deref() == Some(entry.id.as_str()) && entry.date == ride.local_date
    })
}

/// The ride index rounds timestamps. At a local midnight the original fraction can put a ride
/// on the preceding day, so read that timestamp from the record before assigning the date.
fn restore_midnight_times(
    rides: &mut [CalendarRide],
    dates: &[(String, bool)],
    mut load: impl FnMut(&str, &str) -> Option<f64>,
) -> (bool, bool) {
    let mut changed = false;
    let mut missing = false;
    for (ride, (_, midnight)) in rides.iter_mut().zip(dates) {
        if !midnight {
            continue;
        }
        match load(&ride.route_id, &ride.stem).filter(|time| time.is_finite()) {
            Some(time) => {
                changed |= time != ride.started_unix_s;
                ride.started_unix_s = time;
            }
            None => missing = true,
        }
    }
    (changed, missing)
}

fn local_date_script(times: &[f64]) -> Option<String> {
    let times = serde_json::to_string(times).ok()?;
    Some(format!(
        r#"const times = {times};
        dioxus.send(times.map(s => {{
            const d = new Date(s * 1000);
            if (!Number.isFinite(d.getTime())) return ['', false];
            const year = String(d.getFullYear()).padStart(4, '0');
            const date = [year, String(d.getMonth()+1).padStart(2,'0'), String(d.getDate()).padStart(2,'0')].join('-');
            return [date, d.getHours() === 0 && d.getMinutes() === 0 && d.getSeconds() === 0];
        }}));"#
    ))
}

#[component]
fn RideActions(route_id: String, stem: String) -> Element {
    let ctl = use_context::<Controller>();
    let review_route = route_id.clone();
    let review_stem = stem.clone();
    rsx! {
        button { class: "btn btn--ghost btn--sm", r#type: "button",
            onclick: move |_| {
                ctl.close_dialog();
                ctl.review_open_ride_from_calendar(review_route.clone(), review_stem.clone());
            },
            {t("app.calendar.analysis")}
        }
        button { class: "btn btn--ghost btn--sm", r#type: "button",
            onclick: move |_| {
                let mut agent_ride = ctl.st.agent_ride;
                agent_ride.set(Some((route_id.clone(), stem.clone())));
                ctl.open_dialog(DialogKind::Agent);
            },
            {t("app.calendar.ai_analysis")}
        }
    }
}

#[component]
pub fn CalendarDialog(imported: ReadSignal<Vec<Workout>>) -> Element {
    let ctl = use_context::<Controller>();
    let initial = use_hook(|| Store::default_location().load_calendar().map_err(|_| ()));
    let initial_rides = use_hook(|| calendar_rides(Store::default_location().list_all_rides()));
    let completed_ids: HashSet<String> = initial_rides
        .iter()
        .filter_map(|ride| ride.calendar_id.clone())
        .collect();
    let mut rides = use_signal(Vec::<CalendarRide>::new);
    let mut entries = use_signal(|| initial.as_ref().cloned().unwrap_or_default());
    let mut date = use_signal(String::new);
    let mut selected_day = ctl.st.calendar_day;
    let mut time = use_signal(String::new);
    let mut selected = use_signal(String::new);
    let mut editing = use_signal(|| None::<String>);
    let mut error = use_signal(|| {
        if initial.is_err() {
            t("app.calendar.error").to_string()
        } else {
            String::new()
        }
    });
    use_future(move || {
        let mut initial_rides = initial_rides.clone();
        async move {
            if initial_rides.is_empty() {
                return;
            }
            let values: Vec<_> = initial_rides
                .iter()
                .map(|ride| ride.started_unix_s)
                .collect();
            let Some(script) = local_date_script(&values) else {
                error.set(t("app.calendar.local_date_error").to_string());
                return;
            };
            let Some(mut eval) = ride_ui_kit::try_eval(&script) else {
                error.set(t("app.calendar.local_date_error").to_string());
                return;
            };
            match eval.recv::<Vec<(String, bool)>>().await {
                Ok(mut dates) if dates.len() == initial_rides.len() => {
                    if dates.iter().any(|(_, midnight)| *midnight) {
                        let check_dates = dates.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let store = Store::default_location();
                            let (changed, missing) = restore_midnight_times(
                                &mut initial_rides,
                                &check_dates,
                                |route, stem| {
                                    store
                                        .load_ride(route, stem)
                                        .map(|record| record.summary.started_unix_s)
                                },
                            );
                            (initial_rides, changed, missing)
                        })
                        .await;
                        let Ok((restored, changed, missing)) = result else {
                            error.set(t("app.calendar.local_date_error").to_string());
                            return;
                        };
                        initial_rides = restored;
                        if missing {
                            error.set(t("app.calendar.local_date_error").to_string());
                        }
                        if changed {
                            let values: Vec<_> = initial_rides
                                .iter()
                                .map(|ride| ride.started_unix_s)
                                .collect();
                            let Some(script) = local_date_script(&values) else {
                                error.set(t("app.calendar.local_date_error").to_string());
                                return;
                            };
                            let Some(mut eval) = ride_ui_kit::try_eval(&script) else {
                                error.set(t("app.calendar.local_date_error").to_string());
                                return;
                            };
                            match eval.recv::<Vec<(String, bool)>>().await {
                                Ok(updated) if updated.len() == initial_rides.len() => {
                                    dates = updated
                                }
                                _ => {
                                    error.set(t("app.calendar.local_date_error").to_string());
                                    return;
                                }
                            }
                        }
                    }
                    let expected = initial_rides.len();
                    let localized: Vec<_> = initial_rides
                        .into_iter()
                        .zip(dates)
                        .filter_map(|(mut ride, (date, _))| {
                            if date.len() != 10 || !date.is_ascii() {
                                return None;
                            }
                            ride.local_date = date;
                            Some(ride)
                        })
                        .collect();
                    if localized.len() != expected {
                        error.set(t("app.calendar.local_date_error").to_string());
                    }
                    rides.set(localized);
                }
                _ => error.set(t("app.calendar.local_date_error").to_string()),
            }
        }
    });
    let fallback_today = ride_sim::tcx::iso_date(crate::controller::unix_now());
    let mut today = use_signal(|| fallback_today.clone());
    let mut month = ctl.st.calendar_month;
    use_future(move || async move {
        let Some(mut eval) = ride_ui_kit::try_eval(
            "const d = new Date(); dioxus.send([d.getFullYear(), String(d.getMonth()+1).padStart(2,'0'), String(d.getDate()).padStart(2,'0')].join('-'));",
        ) else { return; };
        if let Ok(local) = eval.recv::<String>().await {
            if local.len() == 10 && local.is_ascii() {
                if month.peek().is_none() {
                    month.set(Some(local[..7].to_string()));
                }
                today.set(local);
            }
        }
    });
    let library = WorkoutLibrary::with_imported(imported.read().clone());
    let workouts = library.all();
    let month_value = month
        .read()
        .clone()
        .unwrap_or_else(|| fallback_today.get(..7).unwrap_or("2026-01").to_string());
    let year = month_value
        .get(..4)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2026);
    let month_number = month_value
        .get(5..7)
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|m| (1..=12).contains(m))
        .unwrap_or(1);
    let first = first_weekday(year, month_number);
    let count = month_days(year, month_number);
    let current_today = today.read().clone();
    let current_entries = entries.read().clone();
    let selected_day_value = selected_day.read().clone();
    let selected_entry = editing
        .read()
        .as_ref()
        .and_then(|id| current_entries.iter().find(|entry| &entry.id == id))
        .cloned();
    let selected_missing = selected_entry
        .as_ref()
        .filter(|entry| library.get(&entry.workout.id).is_none())
        .cloned();
    let next_entries: Vec<_> = current_entries
        .iter()
        .filter(|e| !completed_ids.contains(&e.id) && e.date >= current_today)
        .take(5)
        .cloned()
        .collect();
    let shown_entries: Vec<_> = current_entries
        .iter()
        .filter(|e| {
            e.date.starts_with(&month_value)
                && selected_day_value.as_ref().is_none_or(|day| &e.date == day)
        })
        .cloned()
        .collect();
    let current_rides = rides.read().clone();
    let separate_rides: Vec<_> = current_rides
        .iter()
        .filter(|ride| show_separate_ride(ride, &current_entries))
        .cloned()
        .collect();
    let shown_rides: Vec<_> = separate_rides
        .iter()
        .filter(|ride| {
            ride.local_date.starts_with(&month_value)
                && selected_day_value
                    .as_ref()
                    .is_none_or(|day| &ride.local_date == day)
        })
        .cloned()
        .collect();

    rsx! {
        Dialog {
            open: true,
            title: t("app.calendar.title").to_string(),
            wide: true,
            on_close: move |_| ctl.close_dialog(),
            div { class: "shell-calendar-nav",
                button { class: "btn btn--ghost btn--sm", r#type: "button", title: t("app.calendar.previous_month"), aria_label: t("app.calendar.previous_month"),
                    onclick: move |_| { let (y, m) = adjacent_month(year, month_number, -1); month.set(Some(format!("{y:04}-{m:02}"))); selected_day.set(None); }, "‹" }
                strong { "{month_value}" }
                button { class: "btn btn--ghost btn--sm", r#type: "button", title: t("app.calendar.next_month"), aria_label: t("app.calendar.next_month"),
                    onclick: move |_| { let (y, m) = adjacent_month(year, month_number, 1); month.set(Some(format!("{y:04}-{m:02}"))); selected_day.set(None); }, "›" }
            }
            div { class: "shell-calendar-grid",
                for weekday in ride_ui_review::journal::day_names() { strong { "{weekday}" } }
                for _ in 0..first { span { } }
                for day in 1..=count {
                    {
                        let day_date = format!("{year:04}-{month_number:02}-{day:02}");
                        let planned = current_entries.iter().filter(|entry| entry.date == day_date);
                        let events: usize = planned.map(|entry| {
                            current_rides.iter().filter(|ride| ride.local_date == day_date
                                && ride.calendar_id.as_deref() == Some(entry.id.as_str())).count().max(1)
                        }).sum::<usize>() + separate_rides.iter().filter(|ride| ride.local_date == day_date).count();
                        let is_today = day_date == current_today;
                        let is_selected = selected_day_value.as_ref() == Some(&day_date);
                        let day_label = if events == 0 {
                            day_date.clone()
                        } else {
                            tf("app.calendar.day_with_events", &[("date", &day_date), ("count", &events.to_string())])
                        };
                        rsx! {
                            button { class: "shell-calendar-day", r#type: "button", "data-today": "{is_today}", "data-selected": "{is_selected}", aria_pressed: "{is_selected}",
                                aria_label: "{day_label}",
                                onclick: move |_| {
                                    date.set(day_date.clone());
                                    let next = if selected_day.peek().as_ref() == Some(&day_date) { None } else { Some(day_date.clone()) };
                                    selected_day.set(next);
                                },
                                span { "{day}" }
                                if events > 0 { small { "{events}" } }
                            }
                        }
                    }
                }
            }
            if !next_entries.is_empty() {
                section { class: "shell-calendar-upcoming",
                    h3 { {t("app.calendar.upcoming")} }
                    for entry in next_entries {
                        button { class: "btn btn--ghost btn--sm", r#type: "button",
                            onclick: move |_| { if let Some(m) = entry.date.get(..7) { month.set(Some(m.to_string())); selected_day.set(Some(entry.date.clone())); } },
                            "{entry.date} · {entry.workout.name}"
                        }
                    }
                }
            }
            div { class: "shell-calendar-form",
                label { {t("app.calendar.workout")}
                    select { value: "{selected}", onchange: move |event| selected.set(event.value()),
                        option { value: "", {t("app.calendar.choose")} }
                        if let Some(entry) = selected_missing {
                            option { value: "{entry.workout.id}", "{entry.workout.name}" }
                        }
                        for workout in workouts {
                            option { value: "{workout.id}", "{workout.name}" }
                        }
                    }
                }
                label { {t("app.calendar.date")}
                    input { r#type: "date", value: "{date}", onchange: move |event| date.set(event.value()) }
                }
                label { {t("app.calendar.time")}
                    input { r#type: "time", value: "{time}", onchange: move |event| time.set(event.value()) }
                }
                button {
                    class: "btn btn--primary",
                    r#type: "button",
                    onclick: move |_| {
                        let library = WorkoutLibrary::with_imported(imported.peek().clone());
                        let selected_id = selected();
                        let snapshot = editing().and_then(|id| entries.peek().iter()
                            .find(|entry| entry.id == id && entry.workout.id == selected_id)
                            .map(|entry| entry.workout.clone()));
                        let Some(workout) = snapshot.or_else(|| library.get(&selected_id).cloned()) else {
                            error.set(t("app.calendar.invalid").to_string());
                            return;
                        };
                        let selected_time = time();
                        let entry = CalendarEntry {
                            id: editing().unwrap_or_else(next_id),
                            date: date(),
                            time: (!selected_time.is_empty()).then_some(selected_time),
                            workout,
                        };
                        if !entry.is_valid() {
                            error.set(t("app.calendar.invalid").to_string());
                            return;
                        }
                        let saved_date = entry.date.clone();
                        let is_editing = editing.peek().is_some();
                        let store = Store::default_location();
                        match store.update_calendar(|all| {
                            if let Some(index) = all.iter().position(|item| item.id == entry.id) {
                                all[index] = entry;
                            } else if is_editing {
                                return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
                            } else {
                                all.push(entry);
                            }
                            Ok(())
                        }) {
                            Ok(()) => match store.load_calendar() {
                                Ok(all) => {
                                    entries.set(all);
                                    month.set(Some(saved_date[..7].to_string()));
                                    selected_day.set(Some(saved_date));
                                    editing.set(None);
                                    error.set(String::new());
                                }
                                Err(_) => error.set(t("app.calendar.error").to_string()),
                            },
                            Err(_) => error.set(t("app.calendar.error").to_string()),
                        }
                    },
                    if editing.read().is_some() { {t("app.calendar.save")} } else { {t("app.calendar.add")} }
                }
            }
            if !error.read().is_empty() { p { role: "alert", "{error}" } }
            if shown_entries.is_empty() && shown_rides.is_empty() { p { if selected_day_value.is_some() { {t("app.calendar.empty_day")} } else { {t("app.calendar.empty_month")} } } }
            div { class: "shell-calendar-list",
                for entry in shown_entries {
                    {
                        let id = entry.id.clone();
                        let edit_id = entry.id.clone();
                        let edit_workout = entry.workout.id.clone();
                        let edit_date = entry.date.clone();
                        let edit_time = entry.time.clone().unwrap_or_default();
                        let prepare_entry = entry.clone();
                        let complete = completed_ids.contains(&entry.id);
                        let linked_rides: Vec<_> = current_rides.iter()
                            .filter(|ride| ride.calendar_id.as_deref() == Some(entry.id.as_str()))
                            .cloned().collect();
                        let status = if complete { t("app.calendar.completed") } else if entry.date < current_today { t("app.calendar.overdue") } else { t("app.calendar.planned") };
                        rsx! {
                            article { key: "{id}", class: "shell-calendar-entry",
                                div {
                                    strong { "{entry.date}" }
                                    if let Some(time) = &entry.time { span { " {time}" } }
                                    div { "{entry.workout.name}" }
                                    small { "{entry.workout.duration_text()}" }
                                    div { class: "shell-calendar-status", "{status}" }
                                    for ride in linked_rides {
                                        div { class: "shell-calendar-linked",
                                            span { "{ride.local_date} · {ride.title}" }
                                            RideActions { route_id: ride.route_id, stem: ride.stem }
                                        }
                                    }
                                }
                                button {
                                    class: "btn btn--ghost btn--sm", r#type: "button", disabled: complete || matches!(ctl.st.snapshot.read().state, RideState::Riding | RideState::Paused),
                                    onclick: move |_| {
                                        editing.set(Some(edit_id.clone()));
                                        selected.set(edit_workout.clone());
                                        date.set(edit_date.clone());
                                        time.set(edit_time.clone());
                                    },
                                    {t("app.calendar.edit")}
                                }
                                button {
                                    class: "btn btn--ghost btn--sm", r#type: "button",
                                    disabled: complete || matches!(ctl.st.snapshot.read().state, RideState::Riding | RideState::Paused),
                                    onclick: move |_| {
                                        ctl.prepare_scheduled_workout(prepare_entry.clone());
                                        if ctl.st.route.peek().is_none() {
                                            ctl.open_dialog(DialogKind::Route);
                                        } else {
                                            ctl.close_dialog();
                                        }
                                    },
                                    {t("app.calendar.prepare")}
                                }
                                button {
                                    class: "btn btn--ghost btn--sm", r#type: "button",
                                    onclick: move |_| {
                                        let store = Store::default_location();
                                        let deleted_id = id.clone();
                                        match store.update_calendar(|all| { all.retain(|e| e.id != deleted_id); Ok(()) }) {
                                            Ok(()) => match store.load_calendar() {
                                                Ok(all) => {
                                                    entries.set(all);
                                                    if editing.peek().as_ref() == Some(&id) {
                                                        editing.set(None);
                                                        selected.set(String::new());
                                                        date.set(String::new());
                                                        time.set(String::new());
                                                    }
                                                    error.set(String::new());
                                                }
                                                Err(_) => error.set(t("app.calendar.error").to_string()),
                                            },
                                            Err(_) => error.set(t("app.calendar.error").to_string()),
                                        }
                                    },
                                    {t("app.calendar.delete")}
                                }
                            }
                        }
                    }
                }
            }
            if !shown_rides.is_empty() {
                section { class: "shell-calendar-list",
                    h3 { {t("app.calendar.recorded")} }
                    for ride in shown_rides {
                        article { key: "{ride.route_id}-{ride.stem}", class: "shell-calendar-entry",
                            div {
                                strong { "{ride.local_date}" }
                                div { "{ride.title}" }
                            }
                            RideActions { route_id: ride.route_id, stem: ride.stem }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        adjacent_month, calendar_rides, first_weekday, month_days, restore_midnight_times,
        show_separate_ride,
    };
    use ride_model::{CalendarEntry, RideMeta, RouteRides, WorkoutLibrary};

    #[test]
    fn calendar_months_handle_leap_days_and_year_boundaries() {
        assert_eq!(month_days(2028, 2), 29);
        assert_eq!(month_days(2100, 2), 28);
        assert_eq!(first_weekday(2026, 11), 6);
        assert_eq!(first_weekday(2026, 12), 1);
        assert_eq!(adjacent_month(2026, 12, 1), (2027, 1));
        assert_eq!(adjacent_month(2026, 1, -1), (2025, 12));
    }

    #[test]
    fn midnight_rounded_index_uses_the_records_unrounded_time() {
        let mut rides = calendar_rides(vec![RouteRides {
            route_id: "route-a".into(),
            route_name: "Route".into(),
            rides: vec![RideMeta {
                stem: "86400".into(),
                started_unix_s: 86_400.0,
                ..RideMeta::default()
            }],
        }]);
        let dates = vec![("1970-01-02".to_string(), true)];
        let (changed, missing) = restore_midnight_times(&mut rides, &dates, |route, stem| {
            assert_eq!((route, stem), ("route-a", "86400"));
            Some(86_399.9)
        });
        assert!(changed);
        assert!(!missing);
        assert_eq!(rides[0].started_unix_s, 86_399.9);
    }

    #[test]
    fn rides_remain_individually_accessible_with_and_without_a_schedule() {
        let make_meta = |stem: &str, completed_calendar_id: Option<&str>, started| RideMeta {
            stem: stem.into(),
            started_unix_s: started,
            completed_calendar_id: completed_calendar_id.map(str::to_string),
            ..RideMeta::default()
        };
        let mut rides = calendar_rides(vec![RouteRides {
            route_id: "route-a".into(),
            route_name: "Route".into(),
            rides: vec![
                make_meta("one", Some("planned"), 1.0),
                make_meta("two", Some("planned"), 2.0),
                make_meta("old", None, 3.0),
            ],
        }]);
        assert_eq!(rides.len(), 3);
        let workout = WorkoutLibrary::embedded().all()[0].clone();
        let entries = vec![CalendarEntry {
            id: "planned".into(),
            date: "2026-11-12".into(),
            time: None,
            workout,
        }];
        for ride in &mut rides {
            ride.local_date = "2026-11-12".into();
        }
        assert!(show_separate_ride(
            rides.iter().find(|r| r.stem == "old").unwrap(),
            &entries
        ));
        assert!(!show_separate_ride(
            rides.iter().find(|r| r.stem == "one").unwrap(),
            &entries
        ));
        assert!(!show_separate_ride(
            rides.iter().find(|r| r.stem == "two").unwrap(),
            &entries
        ));
        rides
            .iter_mut()
            .find(|r| r.stem == "one")
            .unwrap()
            .local_date = "2026-11-13".into();
        assert!(show_separate_ride(
            rides.iter().find(|r| r.stem == "one").unwrap(),
            &entries
        ));
        assert_eq!(
            rides
                .iter()
                .filter(|r| r.calendar_id.as_deref() == Some("planned"))
                .count(),
            2
        );
    }
}
