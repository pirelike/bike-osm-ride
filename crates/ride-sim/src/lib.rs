//! Physics, ride engine, stats, recorder, TCX export. Pure Rust, no I/O, no async:
//! the app owns the clock and feeds [`RideEngine::step`].
//!
//! * [`physics`] — power → speed model (`step_speed`, `steady_state_speed`).
//! * [`analysis`] — summary and normalised power of a recording the engine did not count.
//! * [`estimate`] — watts estimated from speed and grade (the power model run backwards).
//! * [`compare`] — rides of one route on a common distance grid, time difference, section stats.
//! * [`zones`] — Coggan power zones and the five heart-rate zones.
//! * [`engine`] — the state machine, statistics, 1 Hz recorder and trainer syncing.
//! * [`workout`] — the running workout: step timing and ERG targets.
//! * [`ghost`] — replay of an earlier ride on the same route (`Track`, `Ghost`, `GhostSet`).
//! * [`segments`] — the local segments (climbs) with their PR times and the ghost choice.
//! * [`workout_import`] — a workout imported from a MyWhoosh / Zwift file.
//! * [`tcx`] — Garmin TCX export and the suggested file name.
//! * [`power_curve`] — the mean-maximal power curve, the FTP it suggests and the weekly / monthly
//!   training load of the ride log.
//!
//! Everything is finite-safe: bad sensor values are sanitised, a route without points or a
//! rider with nonsense settings never panic; the caller (`ride-app`) additionally contains
//! panics defensively.

pub mod analysis;
pub mod compare;
pub mod engine;
pub mod estimate;
pub mod ghost;
pub mod physics;
pub mod power_curve;
pub mod segments;
pub mod tcx;
pub mod workout;
pub mod workout_import;
pub mod zones;

pub use engine::{RideEngine, MAX_STEP_S};
pub use ghost::{Ghost, GhostSet, Track};
pub use physics::{steady_state_speed, step_speed, PhysicsParams, MAX_SPEED_MS};
pub use workout::WorkoutSession;
