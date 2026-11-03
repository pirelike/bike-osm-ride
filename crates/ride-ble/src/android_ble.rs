//! Android: btleplug's Java backend has to be initialized once, from a thread attached to the JVM.
//!
//! `btleplug` 0.13 replaced its Android implementation with `droidplug`, which keeps the adapter in
//! a global and **panics** — `Droidplug has not been initialized. Please initialize it with
//! btleplug::platform::init()` — when the platform was never initialized. Initializing it seeds
//! the JavaVM singleton and loads the Java helper classes; after that `Manager::new()` works from
//! any thread (the scan itself still needs the runtime Bluetooth permission, which the activity
//! asks for at startup).
//!
//! Note the `jni` version: `btleplug` 0.13 uses `jni` 0.22, this crate's other Android code
//! (`ride-app`) uses 0.21, so the `Env` here is a 0.22 one and the two must not be mixed.

#[cfg(target_os = "android")]
use std::sync::OnceLock;

/// The result of the one-time initialization; a second attempt answers the same.
#[cfg(target_os = "android")]
static BACKEND: OnceLock<Result<(), String>> = OnceLock::new();

/// Initializes the Android Bluetooth backend. `Err` carries a Hungarian, toast-sized reason.
#[cfg(target_os = "android")]
pub fn init_backend() -> Result<(), String> {
    BACKEND.get_or_init(initialize).clone()
}

/// `Some(reason)` when the Android backend could not be initialized: the adapter then reports the
/// reason instead of calling into btleplug (which would panic).
#[cfg(target_os = "android")]
pub fn backend_error() -> Option<String> {
    BACKEND.get_or_init(initialize).as_ref().err().cloned()
}

/// The initialization itself, run once: the result is kept so a second attempt cannot panic.
#[cfg(target_os = "android")]
fn initialize() -> Result<(), String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // `android_context()` panics once the activity released the context (app going away).
    let context = catch_unwind(AssertUnwindSafe(ndk_context::android_context))
        .map_err(|_| "az Android-környezet már nem elérhető".to_string())?;
    // SAFETY: the JavaVM pointer comes from the NDK context the webview runtime set up and stays
    // valid for the life of the process.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    // The hub thread is not a Java thread: attach it (permanently) and initialize inside.
    let outcome: Result<(), Box<dyn std::error::Error + Send + Sync>> =
        vm.attach_current_thread(|env| {
            btleplug::platform::init(env)?;
            Ok(())
        });
    outcome.map_err(|err| err.to_string())
}

/// Nothing to initialize outside Android.
#[cfg(not(target_os = "android"))]
pub fn init_backend() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The point of this module: without the init call the hub panics instead of reporting.
    #[test]
    fn the_backend_is_initialized_before_the_hub_starts() {
        const HUB: &str = include_str!("lib.rs");
        assert!(
            HUB.contains("init_backend()"),
            "spawn_sensor_hub must initialize the Android backend"
        );
        assert!(HUB.contains("android_ble"), "the hub uses this module");
    }
}
