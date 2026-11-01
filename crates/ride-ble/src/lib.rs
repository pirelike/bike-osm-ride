//! Sensor backend: real BLE devices (btleplug) and a simulated rider, behind one
//! channel-based interface so the UI never touches BLE types.
//!
//! * [`spawn_sensor_hub`] starts the backend on its own thread + tokio runtime and returns a
//!   [`SensorLink`] (commands in, events out).
//! * The simulated rider (`sim:rider`, "Szimulált tekerő") is always offered, even when no
//!   Bluetooth adapter exists or its initialisation fails.
//! * Protocol codecs are not hand-written: FTMS goes through the `ftms` crate, Cycling
//!   Power / CSC / Heart Rate through the vendored `cycling-ble` parsers (see `NOTICE.md`);
//!   `diff_tests` cross-checks them against byte vectors from the Bluetooth specification.
//! * Everything above the btleplug adapter (`btle.rs`) is written against the small
//!   [`transport`] traits and is tested with an in-memory fake.
//!
//! Module map: `parsers` (adapters), `merge` (priorities + staleness), `control`
//! (safety filter + control state machine), `trainer` / `hr` / `power` / `csc` (device
//! roles), `scan`, `sim`, `hub`, `hexlog`, `zwift` (Click / Play controllers and the Hub codec).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use ride_model::{SensorCommand, SensorEvent};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub mod btle;
pub mod control;
pub mod csc;
pub mod error;
pub mod hexlog;
pub mod hr;
pub mod hub;
pub mod link;
pub mod merge;
pub mod multi;
pub mod parsers;
pub mod power;
pub mod scan;
pub mod sensor;
pub mod sim;
pub mod trainer;
pub mod transport;
pub mod zwift;

/// Parsers and tests copied from other projects; see `NOTICE.md` and the file headers.
#[allow(dead_code, unused_imports)]
pub(crate) mod vendor {
    pub(crate) mod cycling_ble;
}

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod fake;
#[cfg(test)]
mod hub_tests;

pub use hexlog::default_hex_log_path;
pub use hub::{SIM_ID, SIM_NAME};

/// Handle to the running backend. Drop `tx` (or send nothing more) to let it wind down.
pub struct SensorLink {
    pub tx: UnboundedSender<SensorCommand>,
    pub rx: UnboundedReceiver<SensorEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct HubOptions {
    /// Append every raw notification / control-point exchange as hex to this file.
    pub hex_log: Option<PathBuf>,
}

/// Start the backend on its own thread + tokio runtime (the UI runtime is separate).
/// The simulated rider is always available as a device named "Szimulált tekerő"
/// (id `sim:rider`) in `SensorEvent::Discovered`, even when no Bluetooth adapter exists.
///
/// Dropping `SensorLink::tx` shuts the backend down. A deliberate `Disconnect(Trainer)`
/// before exiting the app lets the trainer be handed back in a neutral state.
pub fn spawn_sensor_hub(opts: HubOptions) -> SensorLink {
    let (cmd_tx, cmd_rx) = unbounded_channel::<SensorCommand>();
    let (evt_tx, evt_rx) = unbounded_channel::<SensorEvent>();
    let evt_for_errors = evt_tx.clone();

    let spawned = std::thread::Builder::new()
        .name("ride-ble-hub".into())
        .spawn(move || {
            let log = match &opts.hex_log {
                Some(path) => hexlog::HexLog::open(path).unwrap_or_else(|e| {
                    let _ = evt_tx.send(SensorEvent::Notice(format!(
                        "A BLE-napló nem nyitható meg: {e}"
                    )));
                    hexlog::HexLog::disabled()
                }),
                None => hexlog::HexLog::disabled(),
            };
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("ride-ble-rt")
                .enable_all()
                .build()
                .or_else(|_| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                });
            let runtime = match runtime {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = evt_tx.send(SensorEvent::Notice(format!(
                        "A Bluetooth-háttér nem indítható: {e}"
                    )));
                    return;
                }
            };
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x5EED);
            let panic_tx = evt_tx.clone();
            let init: hub::InitFn<btle::BtleAdapter> =
                Box::new(|| Box::pin(btle::BtleAdapter::init()));
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                runtime.block_on(hub::run_hub(
                    init,
                    link::HubConfig::default(),
                    log,
                    seed,
                    cmd_rx,
                    evt_tx,
                ));
            }));
            if outcome.is_err() {
                let _ = panic_tx.send(SensorEvent::Notice(
                    "A Bluetooth-háttér váratlanul leállt; indítsd újra az alkalmazást."
                        .to_string(),
                ));
            }
        });
    if let Err(e) = spawned {
        let _ = evt_for_errors.send(SensorEvent::Notice(format!(
            "A Bluetooth-háttér nem indítható: {e}"
        )));
    }
    SensorLink {
        tx: cmd_tx,
        rx: evt_rx,
    }
}
