//! Commands the hub forwards to the trainer part of a connected device.
//!
//! The FTMS state machine itself lives in [`crate::control`] and is driven by
//! [`crate::multi::MultiSession`]; this file only defines the channel message.

use ride_model::TrainerSim;

/// Commands the hub forwards to the trainer task.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TrainerCmd {
    Sim(TrainerSim),
    Power(f64),
    Start,
    Pause,
}
