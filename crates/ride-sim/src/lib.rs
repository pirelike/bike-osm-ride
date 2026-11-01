//! Physics, ride engine, stats, recorder, TCX export. Pure Rust, no I/O, no async:
//! the app owns the clock and feeds [`RideEngine::step`].
//!
//! * [`physics`] — power → speed model (`step_speed`, `steady_state_speed`).
//! * [`zones`] — Coggan power zones and the five heart-rate zones.
//! * [`engine`] — the state machine, statistics, 1 Hz recorder and trainer syncing.
//! * [`workout`] — the running workout: step timing and ERG targets.
//! * [`tcx`] — Garmin TCX export and the suggested file name.
//!
//! Everything is finite-safe: bad sensor values are sanitised, a route without points or a
//! rider with nonsense settings never panic; the caller (`ride-app`) additionally contains
//! panics defensively.

pub mod engine;
pub mod physics;
pub mod tcx;
pub mod workout;
pub mod zones;

pub use engine::{RideEngine, MAX_STEP_S};
pub use physics::{steady_state_speed, step_speed, PhysicsParams, MAX_SPEED_MS};
pub use workout::WorkoutSession;
