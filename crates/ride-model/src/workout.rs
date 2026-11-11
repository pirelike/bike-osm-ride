//! Structured workouts: the library, the block model and the live progress the HUD shows.
//!
//! A workout is a list of timed steps with a power target (a percentage of FTP, absolute
//! watts, a ramp, or a ramp-test step pattern) and an optional cadence hint. The embedded
//! library (`assets/workouts.json`) is compiled into the binary; the file is generated content
//! (`scripts/gen_workouts.py`) and every entry is validated when it is loaded (a broken entry is
//! skipped, never a panic). Tags come from a fixed vocabulary ([`WorkoutTag`]) so the picker can
//! filter on them.
//!
//! The runner itself lives in `ride-sim`; this module owns the data model and the pure
//! target arithmetic, so the UI, the engine and the tests all agree on what a step means.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ride_i18n::{t, tf};
use serde::{Deserialize, Serialize};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/workout_catalogs.rs"));
}

/// What a workout is for (the picker groups by this).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkoutCategory {
    Recovery,
    Endurance,
    Tempo,
    Sweetspot,
    Threshold,
    Vo2max,
    Anaerobic,
    Sprint,
    Climb,
    Race,
    Test,
    Mixed,
}

impl WorkoutCategory {
    pub const ALL: [WorkoutCategory; 12] = [
        WorkoutCategory::Recovery,
        WorkoutCategory::Endurance,
        WorkoutCategory::Tempo,
        WorkoutCategory::Sweetspot,
        WorkoutCategory::Threshold,
        WorkoutCategory::Vo2max,
        WorkoutCategory::Anaerobic,
        WorkoutCategory::Sprint,
        WorkoutCategory::Climb,
        WorkoutCategory::Race,
        WorkoutCategory::Test,
        WorkoutCategory::Mixed,
    ];

    /// Hungarian name for the picker's group headers.
    pub fn label(self) -> &'static str {
        match self {
            WorkoutCategory::Recovery => t("model.workout.regeneracio"),
            WorkoutCategory::Endurance => t("model.workout.allokepesseg"),
            WorkoutCategory::Tempo => t("model.workout.tempo"),
            WorkoutCategory::Sweetspot => t("model.workout.sweet_spot"),
            WorkoutCategory::Threshold => t("model.workout.kuszob"),
            WorkoutCategory::Vo2max => "VO2max",
            WorkoutCategory::Anaerobic => t("model.workout.anaerob"),
            WorkoutCategory::Sprint => t("model.workout.sprint"),
            WorkoutCategory::Climb => t("model.workout.hegymaszas"),
            WorkoutCategory::Race => t("model.workout.verseny"),
            WorkoutCategory::Test => t("model.workout.teszt"),
            WorkoutCategory::Mixed => t("model.workout.vegyes"),
        }
    }

    /// One-line explanation under the group header.
    pub fn hint(self) -> &'static str {
        match self {
            WorkoutCategory::Recovery => t("model.workout.konnyu_regeneralo_tekeres"),
            WorkoutCategory::Endurance => t("model.workout.aerob_bazis_hosszu_egyenletes_terheles"),
            WorkoutCategory::Tempo => t("model.workout.tartos_mersekelt_intenzitas"),
            WorkoutCategory::Sweetspot => t("model.workout.ftp_alatti_hatekony_kuszobmunka"),
            WorkoutCategory::Threshold => t("model.workout.kuszobintervallumok_es_ftp_tesztek"),
            WorkoutCategory::Vo2max => t("model.workout.rovid_nagyon_intenziv_ismetlesek"),
            WorkoutCategory::Anaerobic => t("model.workout.tejsavas_kapacitas_30_60_masodpercek"),
            WorkoutCategory::Sprint => t("model.workout.maximalis_sprint_es_kiloves"),
            WorkoutCategory::Climb => t("model.workout.alacsony_kadencia_hegymenet"),
            WorkoutCategory::Race => t("model.workout.szimulalt_versenyhelyzetek"),
            WorkoutCategory::Test => t("model.workout.lepcsos_es_idofutam_tesztek"),
            WorkoutCategory::Mixed => t("model.workout.tobbfele_inger_egy_edzesben"),
        }
    }

    /// Sort key of the picker's groups.
    pub fn order(self) -> u8 {
        Self::ALL
            .iter()
            .position(|c| *c == self)
            .map_or(u8::MAX, |i| i as u8)
    }
}

/// The facet a tag belongs to (the picker shows one chip group per facet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TagGroup {
    /// What the workout develops.
    Goal,
    /// Who it suits.
    Level,
    /// Its character (key session, evidence based, length).
    Kind,
    /// When in the season it belongs.
    Phase,
}

impl TagGroup {
    pub const ALL: [TagGroup; 4] = [
        TagGroup::Goal,
        TagGroup::Level,
        TagGroup::Kind,
        TagGroup::Phase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TagGroup::Goal => t("model.workout.mire_jo"),
            TagGroup::Level => t("model.workout.szint"),
            TagGroup::Kind => t("model.workout.jelleg"),
            TagGroup::Phase => t("model.workout.idoszak"),
        }
    }
}

/// The controlled tag vocabulary of the library. The JSON stores the snake_case id; an
/// unknown id makes the entry invalid (so a typo never silently hides a workout from a filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkoutTag {
    AerobicBase,
    Threshold,
    Vo2max,
    Anaerobic,
    Sprint,
    Durability,
    Climbing,
    TimeTrial,
    RaceDynamics,
    Technique,
    Strength,
    Recovery,
    Testing,
    Opener,
    Beginner,
    Advanced,
    KeySession,
    EvidenceBased,
    TimeCrunched,
    LongRide,
    Base,
    Build,
    Peak,
}

impl WorkoutTag {
    pub const ALL: [WorkoutTag; 23] = [
        WorkoutTag::AerobicBase,
        WorkoutTag::Threshold,
        WorkoutTag::Vo2max,
        WorkoutTag::Anaerobic,
        WorkoutTag::Sprint,
        WorkoutTag::Durability,
        WorkoutTag::Climbing,
        WorkoutTag::TimeTrial,
        WorkoutTag::RaceDynamics,
        WorkoutTag::Technique,
        WorkoutTag::Strength,
        WorkoutTag::Recovery,
        WorkoutTag::Testing,
        WorkoutTag::Opener,
        WorkoutTag::Beginner,
        WorkoutTag::Advanced,
        WorkoutTag::KeySession,
        WorkoutTag::EvidenceBased,
        WorkoutTag::TimeCrunched,
        WorkoutTag::LongRide,
        WorkoutTag::Base,
        WorkoutTag::Build,
        WorkoutTag::Peak,
    ];

    /// Workouts of at most this length carry [`WorkoutTag::TimeCrunched`].
    pub const SHORT_MAX_MIN: u32 = 45;
    /// Workouts of at least this length carry [`WorkoutTag::LongRide`].
    pub const LONG_MIN_MIN: u32 = 120;

    /// The id used in `workouts.json`.
    pub fn id(self) -> &'static str {
        match self {
            WorkoutTag::AerobicBase => "aerobic_base",
            WorkoutTag::Threshold => "threshold",
            WorkoutTag::Vo2max => "vo2max",
            WorkoutTag::Anaerobic => "anaerobic",
            WorkoutTag::Sprint => "sprint",
            WorkoutTag::Durability => "durability",
            WorkoutTag::Climbing => "climbing",
            WorkoutTag::TimeTrial => "time_trial",
            WorkoutTag::RaceDynamics => "race_dynamics",
            WorkoutTag::Technique => "technique",
            WorkoutTag::Strength => "strength",
            WorkoutTag::Recovery => "recovery",
            WorkoutTag::Testing => "testing",
            WorkoutTag::Opener => "opener",
            WorkoutTag::Beginner => "beginner",
            WorkoutTag::Advanced => "advanced",
            WorkoutTag::KeySession => "key_session",
            WorkoutTag::EvidenceBased => "evidence_based",
            WorkoutTag::TimeCrunched => "time_crunched",
            WorkoutTag::LongRide => "long_ride",
            WorkoutTag::Base => "base",
            WorkoutTag::Build => "build",
            WorkoutTag::Peak => "peak",
        }
    }

    pub fn from_id(id: &str) -> Option<WorkoutTag> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Hungarian chip label.
    pub fn label(self) -> &'static str {
        match self {
            WorkoutTag::AerobicBase => t("model.workout.aerob_alap"),
            WorkoutTag::Threshold => t("model.workout.ftp_kuszob"),
            WorkoutTag::Vo2max => "VO2max",
            WorkoutTag::Anaerobic => t("model.workout.anaerob_kapacitas"),
            WorkoutTag::Sprint => t("model.workout.sprint"),
            WorkoutTag::Durability => t("model.workout.tartossag"),
            WorkoutTag::Climbing => t("model.workout.hegymenet"),
            WorkoutTag::TimeTrial => t("model.workout.idofutam"),
            WorkoutTag::RaceDynamics => t("model.workout.versenydinamika"),
            WorkoutTag::Technique => t("model.workout.pedaltechnika"),
            WorkoutTag::Strength => t("model.workout.ero_nyomatek"),
            WorkoutTag::Recovery => t("model.workout.regeneracio"),
            WorkoutTag::Testing => t("model.workout.felmeres"),
            WorkoutTag::Opener => t("model.workout.verseny_elotti_aktivalas"),
            WorkoutTag::Beginner => t("model.workout.kezdoknek"),
            WorkoutTag::Advanced => t("model.workout.haladoknak"),
            WorkoutTag::KeySession => t("model.workout.kulcsedzes"),
            WorkoutTag::EvidenceBased => t("model.workout.kutatason_alapul"),
            WorkoutTag::TimeCrunched => t("model.workout.keves_idore_45_perc"),
            WorkoutTag::LongRide => t("model.workout.hosszu_2_ora"),
            WorkoutTag::Base => t("model.workout.alapozas"),
            WorkoutTag::Build => t("model.workout.felkeszites"),
            WorkoutTag::Peak => t("model.workout.versenyidoszak"),
        }
    }

    /// Tooltip of the chip.
    pub fn hint(self) -> &'static str {
        match self {
            WorkoutTag::AerobicBase => {
                t("model.workout.aerob_kapacitas_zsiranyagcsere_az_edzesfelepites_ala")
            }
            WorkoutTag::Threshold => t("model.workout.a_kuszobteljesitmeny_ftp_emelese_es_tartasa"),
            WorkoutTag::Vo2max => {
                t("model.workout.a_maximalis_oxigenfelvetel_es_az_aerob_plafon_emeles")
            }
            WorkoutTag::Anaerobic => t("model.workout.30_mp_2_perces_erofeszitesek_tejsavtures"),
            WorkoutTag::Sprint => t("model.workout.csucsteljesitmeny_es_robbanekonysag"),
            WorkoutTag::Durability => {
                t("model.workout.teljesitmeny_megtartasa_hosszu_terheles_utan")
            }
            WorkoutTag::Climbing => t("model.workout.emelkedokre_jellemzo_terheles_es_kadencia"),
            WorkoutTag::TimeTrial => {
                t("model.workout.egyenletes_hosszu_kuszob_kozeli_erofeszites_es_adago")
            }
            WorkoutTag::RaceDynamics => {
                t("model.workout.tamadasok_ritmusvaltasok_mezonyszeru_terheles")
            }
            WorkoutTag::Technique => t("model.workout.kerek_pedalozas_szeles_kadencia_tartomany"),
            WorkoutTag::Strength => t("model.workout.nagy_nyomatek_alacsony_kadencia"),
            WorkoutTag::Recovery => t("model.workout.konnyu_regeneralo_tekeres"),
            WorkoutTag::Testing => t("model.workout.ftp_map_cp_es_profilmeres"),
            WorkoutTag::Opener => t("model.workout.rovid_ebreszto_edzes_a_verseny_elott"),
            WorkoutTag::Beginner => {
                t("model.workout.strukturalt_edzessel_ismerkedoknek_is_biztonsagos")
            }
            WorkoutTag::Advanced => t("model.workout.nagy_terheles_biztos_alapok_kellenek_hozza"),
            WorkoutTag::KeySession => {
                t("model.workout.minosegi_intenziv_edzes_hetente_2_3_ilyen_eleg")
            }
            WorkoutTag::EvidenceBased => {
                t("model.workout.publikalt_edzestudomanyi_vizsgalat_protokolljat_kove")
            }
            WorkoutTag::TimeCrunched => t("model.workout.legfeljebb_45_perc_bemelegitessel_egyutt"),
            WorkoutTag::LongRide => t("model.workout.legalabb_ket_ora"),
            WorkoutTag::Base => t("model.workout.alapozo_idoszak_volumen_es_aerob_alap"),
            WorkoutTag::Build => t("model.workout.felkeszito_idoszak_kuszob_es_vo2max_munka"),
            WorkoutTag::Peak => t("model.workout.versenyidoszak_specifikus_versenyszeru_ingerek"),
        }
    }

    pub fn group(self) -> TagGroup {
        match self {
            WorkoutTag::Beginner | WorkoutTag::Advanced => TagGroup::Level,
            WorkoutTag::KeySession
            | WorkoutTag::EvidenceBased
            | WorkoutTag::TimeCrunched
            | WorkoutTag::LongRide => TagGroup::Kind,
            WorkoutTag::Base | WorkoutTag::Build | WorkoutTag::Peak => TagGroup::Phase,
            _ => TagGroup::Goal,
        }
    }
}

/// The role of a step (drives the labels and the profile colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Warmup,
    Interval,
    Recovery,
    Cooldown,
    Steady,
    Test,
    Sprint,
    Free,
    Climb,
    Work,
    Surge,
    Attack,
    Chase,
    Regroup,
}

impl StepKind {
    pub fn label(self) -> &'static str {
        match self {
            StepKind::Warmup => t("model.workout.bemelegites"),
            StepKind::Interval => t("model.workout.intervallum"),
            StepKind::Recovery => t("model.workout.piheno"),
            StepKind::Cooldown => t("model.workout.levezetes"),
            StepKind::Steady => t("model.workout.egyenletes"),
            StepKind::Test => t("model.workout.teszt"),
            StepKind::Sprint => t("model.workout.sprint"),
            StepKind::Free => t("model.workout.szabad"),
            StepKind::Climb => t("model.workout.hegymenet"),
            StepKind::Work => t("model.workout.munka"),
            StepKind::Surge => t("model.workout.kiloves"),
            StepKind::Attack => t("model.workout.tamadas"),
            StepKind::Chase => t("model.workout.uldozes"),
            StepKind::Regroup => t("model.workout.osszeallas"),
        }
    }
}

/// The power target of one step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepTarget {
    /// A percentage of FTP.
    PctFtp { value: f64 },
    /// Absolute watts.
    Watts { value: f64 },
    /// A linear ramp between two FTP percentages.
    RampPct { from: f64, to: f64 },
    /// A linear ramp between two wattages.
    RampWatts { from: f64, to: f64 },
    /// A ramp test: `start`, then `step` watts more after every `step_s`, capped at `max`.
    StepWatts {
        start: f64,
        step: f64,
        step_s: f64,
        max: f64,
    },
    /// No prescribed power (the trainer stays in simulation mode).
    Free,
}

impl StepTarget {
    /// Target watts at `elapsed_s` into a step that lasts `duration_s`; `None` for a free step.
    pub fn watts_at(&self, elapsed_s: f64, duration_s: f64, ftp_w: f64) -> Option<f64> {
        let frac = if duration_s > 0.0 {
            (elapsed_s / duration_s).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ftp = if ftp_w.is_finite() && ftp_w > 0.0 {
            ftp_w
        } else {
            200.0
        };
        let value = match *self {
            StepTarget::PctFtp { value } => ftp * value / 100.0,
            StepTarget::Watts { value } => value,
            StepTarget::RampPct { from, to } => ftp * (from + (to - from) * frac) / 100.0,
            StepTarget::RampWatts { from, to } => from + (to - from) * frac,
            StepTarget::StepWatts {
                start,
                step,
                step_s,
                max,
            } => {
                let increment = if step_s > 0.0 {
                    (elapsed_s / step_s).floor()
                } else {
                    0.0
                };
                (start + step * increment).min(max)
            }
            StepTarget::Free => return None,
        };
        value.is_finite().then(|| value.clamp(0.0, 2000.0))
    }

    /// The target as a percentage of FTP, for the read-out.
    pub fn pct_at(&self, elapsed_s: f64, duration_s: f64, ftp_w: f64) -> Option<f64> {
        let w = self.watts_at(elapsed_s, duration_s, ftp_w)?;
        (ftp_w.is_finite() && ftp_w > 0.0).then_some(w / ftp_w * 100.0)
    }

    pub fn is_free(&self) -> bool {
        matches!(self, StepTarget::Free)
    }
}

/// A coaching message of a step (a `textevent` of a Zwift / MyWhoosh file), shown `at_s`
/// seconds after the step starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepMessage {
    pub at_s: u32,
    pub text: String,
}

/// The coaching message on screen right now (see [`WorkoutProgress::message`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachMessage {
    /// Position of the message in the whole workout, so a new message can be told from the one
    /// already shown.
    pub id: usize,
    pub text: String,
}

/// One block of the workout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkoutStep {
    pub kind: StepKind,
    pub name: String,
    /// Length in seconds (whole seconds, at least 5).
    pub duration_s: u32,
    pub target: StepTarget,
    #[serde(default)]
    pub cadence: Option<u16>,
    #[serde(default)]
    pub note: Option<String>,
    /// Timed coaching messages of the step (imported files); without them the `note` is shown
    /// at the start of the step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<StepMessage>,
}

/// Id prefix of workouts imported from a file (the rest of the id is a content hash).
pub const IMPORT_PREFIX: &str = "import-";

/// One workout of the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workout {
    pub id: String,
    pub name: String,
    pub category: WorkoutCategory,
    pub goal: String,
    pub description: String,
    /// Representative intensity, percent of FTP (for the picker's badge).
    pub ftp_pct: u8,
    /// Stated total length; must equal the sum of the steps.
    pub duration_min: u32,
    /// Ids of [`WorkoutTag`]s (see [`Workout::tag_list`]).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    pub steps: Vec<WorkoutStep>,
}

impl Workout {
    /// Sum of the step lengths. Saturates: an imported or hand-edited file can carry any
    /// `duration_s`, and an overflow must not panic in the picker.
    pub fn total_seconds(&self) -> u32 {
        self.steps
            .iter()
            .fold(0u32, |sum, s| sum.saturating_add(s.duration_s))
    }

    /// This workout came from an imported file, not from the embedded library.
    pub fn is_imported(&self) -> bool {
        self.id.starts_with(IMPORT_PREFIX)
    }

    /// Structural check used when the library is loaded. A failure is a data bug, not a
    /// reason to crash: the entry is skipped.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.name.trim().is_empty() {
            return Err(t("model.workout.hianyzo_azonosito_vagy_nev").into());
        }
        if self.steps.is_empty() {
            return Err(t("model.workout.nincs_lepes").into());
        }
        if self.steps.iter().any(|s| s.duration_s < 5) {
            return Err(t("model.workout.tul_rovid_lepes").into());
        }
        if self.steps.len() > 80 {
            return Err(t("model.workout.tul_sok_lepes").into());
        }
        if self.total_seconds() != self.duration_min.saturating_mul(60) {
            return Err(tf(
                "model.workout.lepes_osszeg_perc",
                &[
                    ("total", &self.total_seconds().to_string()),
                    ("minutes", &self.duration_min.to_string()),
                ],
            ));
        }
        if let Some(bad) = self.tags.iter().find(|t| WorkoutTag::from_id(t).is_none()) {
            return Err(tf("model.workout.ismeretlen_cimke_bad", &[("bad", bad)]));
        }
        Ok(())
    }

    /// The known tags in vocabulary order (unknown ids are dropped; `validate` rejects them).
    pub fn tag_list(&self) -> Vec<WorkoutTag> {
        WorkoutTag::ALL
            .into_iter()
            .filter(|t| self.has_tag(*t))
            .collect()
    }

    pub fn has_tag(&self, tag: WorkoutTag) -> bool {
        self.tags.iter().any(|t| t == tag.id())
    }

    /// True when the workout carries every tag of `tags`.
    pub fn has_all(&self, tags: &[WorkoutTag]) -> bool {
        tags.iter().all(|t| self.has_tag(*t))
    }

    /// Length as hours and minutes, `1:05` / `0:45`. Always both: a bare `45` next to `1:00` in
    /// the workout list read as a count, not as minutes.
    pub fn duration_text(&self) -> String {
        let s = self.total_seconds();
        let (h, m) = (s / 3600, (s % 3600) / 60);
        format!("{h}:{m:02}")
    }
}

/// Live state of the running workout (part of [`RideSnapshot`](crate::RideSnapshot)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkoutProgress {
    pub workout_id: String,
    pub workout_name: String,
    pub step_index: usize,
    pub step_count: usize,
    pub step_name: String,
    pub step_kind: StepKind,
    /// Prescribed power right now (`None` in a free step).
    pub target_w: Option<f64>,
    /// The same target as a percentage of FTP.
    pub ftp_pct: Option<f64>,
    pub cadence: Option<u16>,
    pub step_elapsed_s: f64,
    pub step_remaining_s: f64,
    pub step_progress: f64,
    pub total_elapsed_s: f64,
    pub total_remaining_s: f64,
    /// 0..1 over the whole workout.
    pub progress: f64,
    /// All steps are behind the rider.
    pub finished: bool,
    pub next_step_name: Option<String>,
    pub next_target_w: Option<f64>,
    /// The coaching message of the moment (for a few seconds after it is due).
    #[serde(default)]
    pub message: Option<CoachMessage>,
}

/// Entries stay raw JSON until each is parsed on its own: one malformed workout must cost that
/// workout, not the whole library.
#[derive(Deserialize)]
struct LibraryFile {
    schema_version: u32,
    workouts: Vec<serde_json::Value>,
}

/// The workout library (embedded at build time and validated once).
#[derive(Default, PartialEq)]
pub struct WorkoutLibrary {
    workouts: Vec<Workout>,
}

impl std::fmt::Debug for WorkoutLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkoutLibrary")
            .field("workouts", &self.workouts.len())
            .finish()
    }
}

impl WorkoutLibrary {
    /// Parses and validates a library document; invalid entries are skipped.
    pub fn from_json(text: &str) -> WorkoutLibrary {
        let Ok(file) = serde_json::from_str::<LibraryFile>(text) else {
            return WorkoutLibrary { workouts: vec![] };
        };
        if file.schema_version != 1 {
            return WorkoutLibrary { workouts: vec![] };
        }
        let mut workouts: Vec<Workout> = file
            .workouts
            .into_iter()
            .filter_map(|value| serde_json::from_value::<Workout>(value).ok())
            .filter(|w| w.validate().is_ok())
            .collect();
        // The picker's "everything" view groups by category in its own order, then by name.
        workouts.sort_by(|a, b| {
            a.category
                .order()
                .cmp(&b.category.order())
                .then_with(|| a.name.cmp(&b.name))
        });
        WorkoutLibrary { workouts }
    }

    /// The library compiled into the binary, in the current language.
    ///
    /// Every language with a finished translation ships as `assets/workouts.<code>.json`; a
    /// language without one (or with an unfinished one, which the build refuses to ship) falls
    /// back to the Hungarian source. The parse result is cached per language, so switching the
    /// language mid-session is cheap.
    pub fn embedded() -> &'static WorkoutLibrary {
        static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static WorkoutLibrary>>> =
            OnceLock::new();
        let wanted = ride_i18n::current().code();
        let code = if generated::WORKOUT_CATALOGS
            .iter()
            .any(|(code, _)| *code == wanted)
        {
            wanted
        } else {
            "hu"
        };
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(lib) = guard.get(code) {
            return lib;
        }
        let parse = |code: &str| {
            let text = generated::WORKOUT_CATALOGS
                .iter()
                .find(|(candidate, _)| *candidate == code)
                .map(|(_, text)| *text)
                .unwrap_or_default();
            WorkoutLibrary::from_json(text)
        };
        let mut parsed = parse(code);
        // A translation that lost its entries (a schema slip in a catalogue edit) must not leave
        // the picker empty: the Hungarian source is the fallback, as for a missing language.
        if code != "hu" {
            let source = parse("hu");
            if parsed.len() < source.len() {
                eprintln!(
                    "workout library {code}: {} entries instead of {}, using the source",
                    parsed.len(),
                    source.len()
                );
                parsed = source;
            }
        }
        let lib: &'static WorkoutLibrary = Box::leak(Box::new(parsed));
        guard.insert(code, lib);
        lib
    }

    /// The embedded library plus workouts imported from files (a MyWhoosh or Zwift export).
    ///
    /// Imported entries are not passed through [`Workout::validate`]: they were not written for
    /// this library and a whole-minute length is not something a downloaded file promises. An
    /// entry with the same id replaces the embedded one, so re-importing a corrected file
    /// updates it instead of duplicating it.
    pub fn with_imported(imported: Vec<Workout>) -> WorkoutLibrary {
        let mut workouts = Self::embedded().workouts.clone();
        for workout in imported {
            if let Some(slot) = workouts.iter_mut().find(|w| w.id == workout.id) {
                *slot = workout;
            } else {
                workouts.push(workout);
            }
        }
        workouts.retain(|w| !w.steps.is_empty());
        workouts.sort_by(|a, b| {
            a.category
                .order()
                .cmp(&b.category.order())
                .then_with(|| a.name.cmp(&b.name))
        });
        WorkoutLibrary { workouts }
    }

    pub fn all(&self) -> &[Workout] {
        &self.workouts
    }

    pub fn len(&self) -> usize {
        self.workouts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.workouts.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Workout> {
        self.workouts.iter().find(|w| w.id == id)
    }

    /// Workouts of one category, alphabetically.
    pub fn by_category(&self, category: WorkoutCategory) -> Vec<&Workout> {
        let mut list: Vec<&Workout> = self
            .workouts
            .iter()
            .filter(|w| w.category == category)
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Categories that actually have workouts, in picker order, with their counts.
    pub fn categories(&self) -> Vec<(WorkoutCategory, usize)> {
        let mut out: Vec<(WorkoutCategory, usize)> = WorkoutCategory::ALL
            .iter()
            .map(|c| {
                (
                    *c,
                    self.workouts.iter().filter(|w| w.category == *c).count(),
                )
            })
            .filter(|(_, n)| *n > 0)
            .collect();
        out.sort_by_key(|(c, _)| c.order());
        out
    }

    /// The picker's result list: one category (or all) narrowed to workouts carrying every
    /// selected tag. The order is the library order (category, then name).
    pub fn filter(&self, category: Option<WorkoutCategory>, tags: &[WorkoutTag]) -> Vec<&Workout> {
        self.workouts
            .iter()
            .filter(|w| category.is_none_or(|c| w.category == c) && w.has_all(tags))
            .collect()
    }

    /// For every tag, how many workouts the list would show with that tag added to (or kept
    /// in) the selection; the picker greys out the chips that would empty the list.
    pub fn tag_counts(
        &self,
        category: Option<WorkoutCategory>,
        selected: &[WorkoutTag],
    ) -> Vec<(WorkoutTag, usize)> {
        let base = self.filter(category, selected);
        WorkoutTag::ALL
            .into_iter()
            .map(|tag| (tag, base.iter().filter(|w| w.has_tag(tag)).count()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_library_is_valid() {
        let lib = WorkoutLibrary::embedded();
        assert!(lib.len() >= 50, "only {} workouts", lib.len());
        let mut ids = std::collections::HashSet::new();
        for w in lib.all() {
            assert!(ids.insert(&w.id), "duplicate id {}", w.id);
            assert!(w.validate().is_ok(), "{}: {:?}", w.id, w.validate());
            assert!(!w.name.trim().is_empty());
            assert!(w.steps.first().is_some_and(|s| s.kind == StepKind::Warmup));
            assert!(w.steps.last().is_some_and(|s| s.kind == StepKind::Cooldown));
        }
        // every category that appears has enough entries to be worth a group
        let cats = lib.categories();
        for (category, n) in &cats {
            assert!(*n >= 2, "{category:?} has only {n}");
            assert_eq!(lib.by_category(*category).len(), *n);
        }
        assert_eq!(cats.iter().map(|(_, n)| n).sum::<usize>(), lib.len());
    }

    #[test]
    fn no_entry_of_the_embedded_file_is_silently_skipped() {
        let raw: serde_json::Value =
            serde_json::from_str(include_str!("../assets/workouts.json")).unwrap();
        let entries = raw["workouts"].as_array().unwrap().len();
        assert_eq!(WorkoutLibrary::embedded().len(), entries);
    }

    /// Every translated library is generated from the Hungarian one through a flat string map,
    /// so the structure must be identical: a translation may never change a duration, a target,
    /// a tag or a message's timing. The catalogues are parsed directly (not through `embedded`,
    /// whose fallback to the source would hide a broken file).
    #[test]
    fn every_translated_library_keeps_the_structure_and_loses_the_hungarian_letters() {
        let hungarian = WorkoutLibrary::embedded();
        let translations: Vec<_> = generated::WORKOUT_CATALOGS
            .iter()
            .filter(|(code, _)| *code != "hu")
            .collect();
        assert!(
            translations.len() >= 6,
            "{} translations",
            translations.len()
        );
        for (code, text) in translations {
            let lib = WorkoutLibrary::from_json(text);
            assert_eq!(lib.len(), hungarian.len(), "{code}: workout count");
            // The library is sorted by category and then by *name*, so the orders differ once the
            // names are translated: pair the entries by id.
            let by_id: std::collections::HashMap<&str, &Workout> =
                lib.all().iter().map(|w| (w.id.as_str(), w)).collect();
            for hu in hungarian.all() {
                let tr = *by_id
                    .get(hu.id.as_str())
                    .unwrap_or_else(|| panic!("{code}: {} is missing", hu.id));
                let id = &hu.id;
                assert_eq!(hu.category, tr.category, "{code} {id}: category");
                assert_eq!(hu.ftp_pct, tr.ftp_pct, "{code} {id}: ftp_pct");
                assert_eq!(hu.duration_min, tr.duration_min, "{code} {id}: length");
                assert_eq!(hu.tags, tr.tags, "{code} {id}: tags");
                assert_eq!(hu.steps.len(), tr.steps.len(), "{code} {id}: step count");
                for (a, b) in hu.steps.iter().zip(tr.steps.iter()) {
                    assert_eq!(a.kind, b.kind, "{code} {id}: kind");
                    assert_eq!(a.duration_s, b.duration_s, "{code} {id}: duration");
                    assert_eq!(a.target, b.target, "{code} {id}: target");
                    assert_eq!(a.cadence, b.cadence, "{code} {id}: cadence");
                    let timing = |s: &WorkoutStep| -> Vec<u32> {
                        s.messages.iter().map(|m| m.at_s).collect()
                    };
                    assert_eq!(timing(a), timing(b), "{code} {id}: messages");
                }
                let mut texts = vec![&tr.name, &tr.goal, &tr.description];
                for step in &tr.steps {
                    texts.push(&step.name);
                    texts.extend(step.messages.iter().map(|m| &m.text));
                }
                for text in texts {
                    assert!(
                        !text.contains(['ő', 'ű', 'Ő', 'Ű']),
                        "{code} {id}: Hungarian letters left in {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_malformed_entry_costs_only_that_workout() {
        let good = |id: &str| {
            format!(
                r#"{{"id":"{id}","name":"x","category":"test","goal":"","description":"",
                "ftp_pct":50,"duration_min":1,"steps":[{{"kind":"steady","name":"x",
                "duration_s":60,"target":{{"type":"free"}}}}]}}"#
            )
        };
        let doc = format!(
            r#"{{"schema_version":1,"workouts":[{},{{"id":"bad","steps":"nope"}},{}]}}"#,
            good("a"),
            good("b")
        );
        let lib = WorkoutLibrary::from_json(&doc);
        assert_eq!(lib.len(), 2, "{lib:?}");
        assert!(lib.get("bad").is_none());
    }

    #[test]
    fn huge_step_lengths_saturate_instead_of_overflowing() {
        let step = |d| WorkoutStep {
            kind: StepKind::Steady,
            name: "x".into(),
            duration_s: d,
            target: StepTarget::Free,
            cadence: None,
            note: None,
            messages: Vec::new(),
        };
        let workout = Workout {
            id: "x".into(),
            name: "x".into(),
            category: WorkoutCategory::Test,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 80,
            duration_min: 1,
            tags: vec![],
            sources: vec![],
            steps: vec![step(u32::MAX), step(u32::MAX)],
        };
        assert_eq!(workout.total_seconds(), u32::MAX);
        assert!(!workout.duration_text().is_empty());
        assert!(workout.validate().is_err());
    }

    #[test]
    fn every_workout_is_described_and_tagged_consistently() {
        for w in WorkoutLibrary::embedded().all() {
            assert!(
                w.description.chars().count() >= 120,
                "{}: short description",
                w.id
            );
            assert!(!w.goal.trim().is_empty(), "{}: no goal", w.id);
            assert!(!w.sources.is_empty(), "{}: no source", w.id);
            let tags = w.tag_list();
            assert_eq!(
                tags.len(),
                w.tags.len(),
                "{}: unknown or duplicate tag",
                w.id
            );
            assert!(
                tags.iter().any(|t| t.group() == TagGroup::Goal),
                "{}: says nothing about what it is for",
                w.id
            );
            let minutes = w.duration_min;
            assert_eq!(
                w.has_tag(WorkoutTag::TimeCrunched),
                minutes <= WorkoutTag::SHORT_MAX_MIN,
                "{}: {minutes} min vs the short tag",
                w.id
            );
            assert_eq!(
                w.has_tag(WorkoutTag::LongRide),
                minutes >= WorkoutTag::LONG_MIN_MIN,
                "{}: {minutes} min vs the long tag",
                w.id
            );
            assert!(
                !(w.has_tag(WorkoutTag::Beginner) && w.has_tag(WorkoutTag::Advanced)),
                "{}: both levels",
                w.id
            );
        }
        // every tag of the vocabulary is in use, so no chip is a dead end
        for tag in WorkoutTag::ALL {
            assert!(
                WorkoutLibrary::embedded()
                    .all()
                    .iter()
                    .any(|w| w.has_tag(tag)),
                "{tag:?} is unused"
            );
            assert_eq!(WorkoutTag::from_id(tag.id()), Some(tag));
        }
    }

    #[test]
    fn unknown_tags_invalidate_the_entry() {
        let json = r#"{"schema_version":1,"workouts":[
            {"id":"typo","name":"Elírás","category":"endurance","goal":"","description":"",
             "ftp_pct":60,"duration_min":1,"tags":["aerobic_bsae"],"sources":[],
             "steps":[{"kind":"warmup","name":"B","duration_s":30,"target":{"type":"pct_ftp","value":50}},
                      {"kind":"cooldown","name":"L","duration_s":30,"target":{"type":"pct_ftp","value":40}}]}
        ]}"#;
        assert!(WorkoutLibrary::from_json(json).is_empty());
    }

    #[test]
    fn filtering_by_category_and_tags() {
        let lib = WorkoutLibrary::embedded();
        assert_eq!(lib.filter(None, &[]).len(), lib.len());
        let vo2 = lib.filter(Some(WorkoutCategory::Vo2max), &[]);
        assert_eq!(vo2.len(), lib.by_category(WorkoutCategory::Vo2max).len());

        let picked = [WorkoutTag::Vo2max, WorkoutTag::EvidenceBased];
        let both = lib.filter(None, &picked);
        assert!(!both.is_empty());
        assert!(both.iter().all(|w| w.has_all(&picked)));
        assert!(both.len() <= lib.filter(None, &[WorkoutTag::Vo2max]).len());

        // the count of a tag equals the list length after selecting it
        let counts = lib.tag_counts(Some(WorkoutCategory::Threshold), &[WorkoutTag::KeySession]);
        for (tag, n) in counts {
            let mut sel = vec![WorkoutTag::KeySession];
            if tag != WorkoutTag::KeySession {
                sel.push(tag);
            }
            assert_eq!(
                lib.filter(Some(WorkoutCategory::Threshold), &sel).len(),
                n,
                "{tag:?}"
            );
        }
    }

    #[test]
    fn invalid_entries_are_skipped_not_fatal() {
        let json = r#"{"schema_version":1,"workouts":[
            {"id":"good","name":"Jó","category":"endurance","goal":"","description":"",
             "ftp_pct":60,"duration_min":1,"tags":[],"sources":[],
             "steps":[{"kind":"warmup","name":"B","duration_s":30,"target":{"type":"pct_ftp","value":50}},
                      {"kind":"cooldown","name":"L","duration_s":30,"target":{"type":"pct_ftp","value":40}}]},
            {"id":"bad","name":"Rossz","category":"endurance","goal":"","description":"",
             "ftp_pct":60,"duration_min":2,"tags":[],"sources":[],
             "steps":[{"kind":"warmup","name":"B","duration_s":30,"target":{"type":"pct_ftp","value":50}}]}
        ]}"#;
        let lib = WorkoutLibrary::from_json(json);
        assert_eq!(lib.len(), 1);
        assert!(lib.get("good").is_some() && lib.get("bad").is_none());
        assert!(WorkoutLibrary::from_json("nope").is_empty());
        assert!(WorkoutLibrary::from_json(r#"{"schema_version":9,"workouts":[]}"#).is_empty());
    }

    #[test]
    fn targets_are_computed_in_watts_and_percent() {
        let target = StepTarget::PctFtp { value: 88.0 };
        assert_eq!(target.watts_at(0.0, 600.0, 250.0), Some(220.0));
        assert_eq!(target.pct_at(0.0, 600.0, 250.0), Some(88.0));
        let ramp = StepTarget::RampWatts {
            from: 100.0,
            to: 200.0,
        };
        assert_eq!(ramp.watts_at(300.0, 600.0, 250.0), Some(150.0));
        let step = StepTarget::StepWatts {
            start: 100.0,
            step: 20.0,
            step_s: 60.0,
            max: 400.0,
        };
        assert_eq!(step.watts_at(0.0, 600.0, 250.0), Some(100.0));
        assert_eq!(step.watts_at(61.0, 600.0, 250.0), Some(120.0));
        assert_eq!(step.watts_at(10_000.0, 600.0, 250.0), Some(400.0));
        assert_eq!(
            StepTarget::Free.watts_at(10.0, 60.0, 250.0),
            None,
            "free steps have no ERG target"
        );
        // a missing FTP falls back to a sane 200 W instead of dividing by zero
        assert_eq!(target.watts_at(0.0, 600.0, 0.0), Some(176.0));
    }

    #[test]
    fn durations_are_formatted() {
        let workout = Workout {
            id: "x".into(),
            name: "x".into(),
            category: WorkoutCategory::Test,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 80,
            duration_min: 65,
            tags: vec![],
            sources: vec![],
            steps: vec![WorkoutStep {
                kind: StepKind::Steady,
                name: "x".into(),
                duration_s: 3900,
                target: StepTarget::Free,
                cadence: None,
                note: None,
                messages: Vec::new(),
            }],
        };
        assert_eq!(workout.duration_text(), "1:05");
        assert_eq!(workout.total_seconds(), 3900);
        let mut short = workout.clone();
        short.steps[0].duration_s = 45 * 60;
        assert_eq!(
            short.duration_text(),
            "0:45",
            "under an hour keeps the hours"
        );
        assert!(workout.validate().is_ok());
    }

    #[test]
    fn the_coachs_messages_of_the_library_sit_inside_their_steps() {
        let lib = WorkoutLibrary::embedded();
        let mut count = 0;
        for w in lib.all() {
            for step in &w.steps {
                let mut last: Option<u32> = None;
                for m in &step.messages {
                    assert!(
                        m.at_s < step.duration_s,
                        "{}: {} s in a {} s step",
                        w.id,
                        m.at_s,
                        step.duration_s
                    );
                    assert!(
                        !m.text.trim().is_empty() && m.text.chars().count() <= 160,
                        "{}",
                        w.id
                    );
                    assert!(
                        step.note.is_none() || m.at_s > 0,
                        "{}: a message at 0 s would hide under the note",
                        w.id
                    );
                    if let Some(prev) = last {
                        assert!(m.at_s >= prev, "{}: messages out of order", w.id);
                    }
                    last = Some(m.at_s);
                    count += 1;
                }
            }
        }
        assert!(count > 0, "the library has coaching messages");
    }
}
