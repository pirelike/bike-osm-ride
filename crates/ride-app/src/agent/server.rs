//! MCP stdio transport for local clients.

use ride_model::{RideRecord, Settings, Workout, WorkoutLibrary};
use ride_route::store::Store;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};

use super::{AiDraft, DraftKind};

struct Agent;

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn tools() -> Vec<Tool> {
    let date = json!({"type":"string","description":"Local calendar date YYYY-MM-DD"});
    let id = json!({"type":"string","description":"Stable unique ID using letters, digits, underscore or hyphen, at most 64 characters"});
    let step_target = json!({"oneOf":[
        {"type":"object","properties":{"type":{"const":"pct_ftp"},"value":{"type":"number","minimum":0,"maximum":250}},"required":["type","value"]},
        {"type":"object","properties":{"type":{"const":"ramp_pct"},"from":{"type":"number","minimum":0,"maximum":250},"to":{"type":"number","minimum":0,"maximum":250}},"required":["type","from","to"]},
        {"type":"object","properties":{"type":{"const":"free"}},"required":["type"]}
    ]});
    let step = json!({"type":"object","properties":{"kind":{"enum":["warmup","interval","recovery","cooldown","steady","free"]},"name":{"type":"string"},"duration_s":{"type":"integer","minimum":5},"target":step_target,"cadence":{"type":["integer","null"]}},"required":["kind","name","duration_s","target"]});
    let workout = json!({"type":"object","description":"New workout id must start with import-. Sum of step durations must equal duration_min * 60.","properties":{"id":id,"name":{"type":"string"},"category":{"enum":["recovery","endurance","tempo","sweetspot","threshold","vo2max","anaerobic","sprint","climb","race","test","mixed"]},"goal":{"type":"string"},"description":{"type":"string"},"ftp_pct":{"type":"integer","minimum":0,"maximum":250},"duration_min":{"type":"integer","minimum":1},"steps":{"type":"array","items":step}},"required":["id","name","category","goal","description","ftp_pct","duration_min","steps"]});
    let entry = json!({"type":"object","properties":{"id":id,"date":date,"time":{"type":["string","null"],"description":"Optional local time HH:MM"},"workout":{"type":"object","description":"Full workout returned by get_workout or matching a new workout in this proposal"}},"required":["id","date","workout"]});
    let change = json!({"type":"object","description":"Adjust a workout that is already scheduled and not completed. Give the whole new entry (same id as entry_id, with the new date, time and/or workout), or leave entry out to remove it.","properties":{"entry_id":id,"entry":entry,"reason":{"type":"string","description":"Why, in the rider's language: what went badly or why the load changes"}},"required":["entry_id","reason"]});
    vec![
        Tool::new("get_training_context", "Get FTP, workout library summaries and the scheduled workouts of a local date range, each with its full steps and whether the rider already completed it (and by which ride).", object(json!({"type":"object","properties":{"from_date":date,"to_date":date},"required":["from_date","to_date"]}))),
        Tool::new("get_workout", "Get the structure of a workout from the library to use in a training plan.", object(json!({"type":"object","properties":{"workout_id":id},"required":["workout_id"]}))),
        Tool::new("list_recent_rides", "List recent rides, newest first, with summary metrics, the rider's effort rating (rpe, 1-10), the workout that was planned (calendar_id, workout_name) and how closely the targets were met (compliance_pct).", object(json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}}}))),
        Tool::new("get_ride_analysis_context", "Get workout laps, sensor coverage and training metrics for a saved ride.", object(json!({"type":"object","properties":{"route_id":id,"stem":{"type":"string"}},"required":["route_id","stem"]}))),
        Tool::new("submit_training_draft", "Submit a plan for the rider to review and approve in the app: new workouts, new calendar entries, and changes to scheduled entries that are not completed yet (move, swap the workout, remove). Reuse request_id on retry.", object(json!({"type":"object","properties":{"request_id":id,"title":{"type":"string"},"workouts":{"type":"array","items":workout},"entries":{"type":"array","items":entry},"changes":{"type":"array","items":change}},"required":["request_id","title"]}))),
        Tool::new("submit_ride_feedback", "Submit ride feedback for the rider to review and save in the app. Reuse request_id on retry.", object(json!({"type":"object","properties":{"request_id":id,"route_id":id,"stem":{"type":"string"},"good":{"type":"string"},"improve":{"type":"string"},"next":{"type":"string"}},"required":["request_id","route_id","stem","good","improve","next"]}))),
    ]
}

impl ServerHandler for Agent {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("You are the rider's coach. Everything you submit needs the rider's approval in bike-osm-ride. To personalise training: read list_recent_rides (rpe, compliance_pct, tss, calendar_id) and get_training_context; when a ride went badly (targets missed, effort rating too high for the planned intensity, a planned workout skipped) adjust what is still ahead with submit_training_draft: use changes to move, swap or remove scheduled entries that are not completed, and workouts plus entries to add new ones. Give every change a reason. Completed entries are history and cannot be changed. Call get_workout before scheduling a library workout. Never invent missing sensor data.")
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.unwrap_or_default();
        let result = if !super::root().join("ai-enabled").is_file() {
            Err("Enable the local AI connection in bike-osm-ride settings first.".to_string())
        } else {
            let language = configured_language();
            ride_i18n::with_lang(language, || execute(request.name.as_ref(), &args))
        };
        let result = match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        Ok(result.into())
    }
}

/// Resolve the saved language for each request so an already-running MCP server follows changes
/// in the application. The override is thread-local because clients can call tools concurrently.
fn configured_language() -> ride_i18n::Lang {
    let settings: Settings = std::fs::read(super::root().join("settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let override_lang = std::env::var("RIDE_LANG").ok();
    resolve_language(&settings.ui.language, override_lang.as_deref())
}

fn resolve_language(setting: &str, override_lang: Option<&str>) -> ride_i18n::Lang {
    override_lang
        .and_then(ride_i18n::Lang::from_code)
        .or_else(|| {
            if setting.eq_ignore_ascii_case("auto") {
                Some(ride_i18n::detect())
            } else {
                ride_i18n::Lang::from_code(setting).filter(|language| language.has_full_catalog())
            }
        })
        .unwrap_or(ride_i18n::Lang::HU)
}

fn string<'a>(args: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing {field}"))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    year > 0 && day > 0 && day <= max_day
}

fn safe_workout(workout: &Workout) -> Workout {
    super::sanitized_workout(workout)
}

fn ride_analysis_context(record: &RideRecord, stem: &str) -> Value {
    let dense = ride_sim::ride_detail::dense(record);
    let laps: Vec<_> = ride_sim::ride_detail::workout_laps(record.workout.as_ref(), &dense)
        .into_iter()
        .map(|lap| json!({"start_s":lap.start_s,"end_s":lap.end_s,"target_w":lap.target_w,"avg_power_w":lap.avg_power_w,"avg_hr":lap.avg_hr,"avg_cadence":lap.avg_cadence,"compliance_pct":lap.compliance_pct}))
        .collect();
    let coverage = |v: &[Option<f64>]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().filter(|v| v.is_some()).count() as f64 / v.len() as f64
        }
    };
    let summary = &record.summary;
    let power_coverage = coverage(&dense.power_w);
    let heart_rate_coverage = coverage(&dense.hr_bpm);
    json!({"route_id":record.route_id,"stem":stem,
        "summary":{"started_unix_s":summary.started_unix_s,"elapsed_s":summary.elapsed_s,"distance_m":summary.dist,"ascent_m":summary.ascent,"avg_power_w":summary.avg_power_w,"max_power_w":summary.max_power_w,"normalized_power_w":summary.normalized_power_w,"intensity_factor":summary.intensity_factor,"tss":summary.tss,"work_kj":(power_coverage > 0.0).then_some(summary.work_kj),"avg_hr":summary.avg_hr,"max_hr":summary.max_hr,"avg_cadence":summary.avg_cadence,"power_zone_secs":(power_coverage > 0.0).then_some(summary.power_zone_secs),"hr_zone_secs":(heart_rate_coverage > 0.0).then_some(summary.hr_zone_secs)},
        "ftp_w":record.ftp_w,"rpe":record.rpe,"calendar_id":record.completed_calendar_id,"workout":record.workout.as_ref().map(|w| json!({"id":w.id,"name":w.name})),"simulated":record.simulated,"power_estimated":record.power_estimated,"power_coverage":power_coverage,"heart_rate_coverage":heart_rate_coverage,"cadence_coverage":coverage(&dense.cadence_rpm),"laps":laps})
}

fn execute(name: &str, args: &Map<String, Value>) -> Result<Value, String> {
    let store = Store::default_location();
    match name {
        "get_training_context" => {
            let from = string(args, "from_date")?;
            let to = string(args, "to_date")?;
            if !valid_date(from) || !valid_date(to) || from > to {
                return Err("Invalid date range. Use real dates in YYYY-MM-DD format with from_date no later than to_date.".into());
            }
            let settings: Settings = std::fs::read(store.dir().join("settings.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let library = WorkoutLibrary::with_imported(store.list_workouts());
            let workouts: Vec<_> = library.all().iter().map(|w| json!({"id":w.id,"name":w.name,"duration_s":w.total_seconds(),"category":w.category,"ftp_pct":w.ftp_pct})).collect();
            let mut done = std::collections::HashMap::new();
            for route in store.list_all_rides() {
                for ride in route.rides {
                    if let Some(id) = ride.completed_calendar_id {
                        done.insert(id, json!({"route_id":route.route_id,"stem":ride.stem}));
                    }
                }
            }
            let entries = store.load_calendar().map_err(|e| e.to_string())?.into_iter()
                .filter(|entry| entry.date.as_str() >= from && entry.date.as_str() <= to)
                .map(|entry| {
                    let ride = done.get(&entry.id).cloned();
                    json!({"id":entry.id,"date":entry.date,"time":entry.time,"workout_id":entry.workout.id,"workout_name":entry.workout.name,"completed":ride.is_some(),"ride":ride,"workout":safe_workout(&entry.workout)})
                })
                .collect::<Vec<_>>();
            Ok(json!({"ftp_w": settings.rider.ftp_w, "workouts": workouts, "calendar": entries}))
        }
        "get_workout" => {
            let library = WorkoutLibrary::with_imported(store.list_workouts());
            let workout = library
                .get(string(args, "workout_id")?)
                .ok_or("Workout not found")?;
            Ok(json!({"workout":safe_workout(workout)}))
        }
        "list_recent_rides" => {
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(30)
                .clamp(1, 100) as usize;
            let mut rides = Vec::new();
            let root = store.dir().join("rides");
            if root.is_dir() {
                for route in std::fs::read_dir(root)
                    .map_err(|e| e.to_string())?
                    .flatten()
                {
                    if !route.path().is_dir() {
                        continue;
                    }
                    let route_id = route.file_name().to_string_lossy().to_string();
                    if !super::valid_id(&route_id) {
                        continue;
                    }
                    for file in std::fs::read_dir(route.path())
                        .map_err(|e| e.to_string())?
                        .flatten()
                    {
                        let path = file.path();
                        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                            continue;
                        };
                        if path.extension().is_none_or(|ext| ext != "json")
                            || stem.is_empty()
                            || !stem.bytes().all(|c| c.is_ascii_digit())
                        {
                            continue;
                        }
                        if let Some(ride) = store.load_ride(&route_id, stem) {
                            let laps = ride_sim::ride_detail::workout_laps(
                                ride.workout.as_ref(),
                                &ride_sim::ride_detail::dense(&ride),
                            );
                            let compliance: Vec<f64> =
                                laps.iter().filter_map(|lap| lap.compliance_pct).collect();
                            let compliance_pct = (!compliance.is_empty())
                                .then(|| compliance.iter().sum::<f64>() / compliance.len() as f64);
                            rides.push(json!({"route_id":route_id,"stem":stem,"route_name":ride.route_name,"started_unix_s":ride.summary.started_unix_s,"elapsed_s":ride.summary.elapsed_s,"distance_m":ride.summary.dist,"avg_power_w":ride.summary.avg_power_w,"normalized_power_w":ride.summary.normalized_power_w,"intensity_factor":ride.summary.intensity_factor,"avg_hr":ride.summary.avg_hr,"tss":ride.summary.tss,"rpe":ride.rpe,"calendar_id":ride.completed_calendar_id,"workout_id":ride.workout.as_ref().map(|w| w.id.clone()),"workout_name":ride.workout.as_ref().map(|w| w.name.clone()),"compliance_pct":compliance_pct,"simulated":ride.simulated,"power_estimated":ride.power_estimated}));
                        }
                    }
                }
            }
            rides.sort_by(|a, b| {
                b["started_unix_s"]
                    .as_f64()
                    .unwrap_or_default()
                    .total_cmp(&a["started_unix_s"].as_f64().unwrap_or_default())
            });
            rides.truncate(limit);
            Ok(json!({"rides":rides}))
        }
        "get_ride_analysis_context" => {
            let record = store
                .load_ride(string(args, "route_id")?, string(args, "stem")?)
                .ok_or("Ride not found")?;
            Ok(ride_analysis_context(&record, string(args, "stem")?))
        }
        "submit_training_draft" => {
            let draft = AiDraft {
                id: string(args, "request_id")?.to_string(),
                kind: DraftKind::Training {
                    title: string(args, "title")?.to_string(),
                    workouts: serde_json::from_value(
                        args.get("workouts").cloned().unwrap_or_else(|| json!([])),
                    )
                    .map_err(|e| e.to_string())?,
                    entries: serde_json::from_value(
                        args.get("entries").cloned().unwrap_or_else(|| json!([])),
                    )
                    .map_err(|e| e.to_string())?,
                    changes: serde_json::from_value(
                        args.get("changes").cloned().unwrap_or_else(|| json!([])),
                    )
                    .map_err(|e| e.to_string())?,
                },
            };
            let status = super::submit_draft(&draft).map_err(|e| e.to_string())?;
            Ok(json!({"request_id":draft.id,"status":status}))
        }
        "submit_ride_feedback" => {
            let route_id = string(args, "route_id")?;
            let stem = string(args, "stem")?;
            if store.load_ride(route_id, stem).is_none() {
                return Err("Ride not found".into());
            }
            let draft = AiDraft {
                id: string(args, "request_id")?.to_string(),
                kind: DraftKind::Feedback {
                    route_id: route_id.to_string(),
                    stem: stem.to_string(),
                    good: string(args, "good")?.to_string(),
                    improve: string(args, "improve")?.to_string(),
                    next: string(args, "next")?.to_string(),
                },
            };
            let status = super::submit_draft(&draft).map_err(|e| e.to_string())?;
            Ok(json!({"request_id":draft.id,"status":status}))
        }
        _ => Err("Unknown tool".into()),
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let service = Agent.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{RecordPoint, RideSource, RideSummary, RIDE_RECORD_VERSION};

    #[test]
    fn language_resolution_tracks_the_current_setting_and_environment_override() {
        assert_eq!(resolve_language("hu", None), ride_i18n::Lang::HU);
        assert_eq!(resolve_language("en", None), ride_i18n::Lang::EN);
        assert_eq!(resolve_language("hu", Some("en")), ride_i18n::Lang::EN);
        assert_eq!(resolve_language("unsupported", None), ride_i18n::Lang::HU);
    }

    #[test]
    fn ride_analysis_distinguishes_missing_sensor_data_from_recorded_zeroes() {
        let points = (0..=10)
            .map(|second| RecordPoint {
                t: f64::from(second),
                unix_s: f64::from(second),
                dist: f64::from(second),
                odo: f64::from(second),
                lat: 47.0,
                lon: 19.0,
                ele: 0.0,
                speed_ms: 1.0,
                power_w: None,
                cadence_rpm: None,
                hr_bpm: None,
                grade: 0.0,
            })
            .collect();
        let mut record = RideRecord {
            version: RIDE_RECORD_VERSION,
            route_id: "route".into(),
            route_name: "Private route".into(),
            simulated: false,
            summary: RideSummary::default(),
            points,
            source: RideSource::Recorded,
            power_estimated: false,
            estimate: None,
            measured_power: Vec::new(),
            ftp_w: None,
            rider_kg: None,
            workout: None,
            completed_calendar_id: None,
            rpe: None,
            note: "Private note".into(),
        };
        let missing = ride_analysis_context(&record, "100");
        assert_eq!(missing["power_coverage"], 0.0);
        assert_eq!(missing["heart_rate_coverage"], 0.0);
        assert!(missing["summary"]["work_kj"].is_null());
        assert!(missing["summary"]["power_zone_secs"].is_null());
        assert!(missing["summary"]["hr_zone_secs"].is_null());
        assert!(missing.get("points").is_none());
        assert!(missing.get("note").is_none());
        assert!(missing["summary"].get("route_name").is_none());

        for point in &mut record.points {
            point.power_w = Some(0.0);
            point.hr_bpm = Some(120.0);
        }
        let measured = ride_analysis_context(&record, "100");
        assert_eq!(measured["power_coverage"], 1.0);
        assert_eq!(measured["heart_rate_coverage"], 1.0);
        assert_eq!(measured["summary"]["work_kj"], 0.0);
        assert!(measured["summary"]["power_zone_secs"].is_array());
        assert!(measured["summary"]["hr_zone_secs"].is_array());
    }
}
