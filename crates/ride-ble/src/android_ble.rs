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
use ride_i18n::t;
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
        .map_err(|_| t("ble.android_ble.az_android_kornyezet_mar_nem_elerheto").to_string())?;
    // SAFETY: the JavaVM pointer comes from the NDK context the webview runtime set up and stays
    // valid for the life of the process.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    let activity = context.context();
    // The hub thread is not a Java thread: attach it (permanently) and initialize inside.
    let outcome: Result<(), Box<dyn std::error::Error + Send + Sync>> =
        vm.attach_current_thread(|env| {
            use_app_class_loader(env, activity)?;
            btleplug::platform::init(env)?;
            Ok(())
        });
    outcome.map_err(|err| err.to_string())
}

/// Makes the activity's class loader the context class loader of the current thread.
///
/// btleplug looks its Java classes (`com.nonpolynomial.btleplug.android.impl.Adapter`, ...) up
/// with the default `LoaderContext`: the thread's context class loader, then `FindClass`. On a
/// native thread attached to the JVM both are the *system* class loader, which only knows the
/// framework classes, not the ones in the APK's dex: "failed to resolve java class ... class not
/// found". The activity's loader knows them. Doing this once is enough: `platform::init` looks
/// every class up and the bindings keep them in global caches.
#[cfg(target_os = "android")]
fn use_app_class_loader(
    env: &mut jni::Env,
    activity: *mut std::ffi::c_void,
) -> jni::errors::Result<()> {
    use jni::{jni_sig, jni_str, objects::JObject};

    // SAFETY: the NDK context holds a global reference to the activity for the life of the
    // process; `JObject` has no `Drop`, so wrapping it never deletes that reference.
    let activity = unsafe { JObject::from_raw(env, activity.cast()) };
    let loader = env
        .call_method(
            &activity,
            jni_str!("getClassLoader"),
            jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )?
        .l()?;
    let thread = env
        .call_static_method(
            jni_str!("java/lang/Thread"),
            jni_str!("currentThread"),
            jni_sig!("()Ljava/lang/Thread;"),
            &[],
        )?
        .l()?;
    env.call_method(
        &thread,
        jni_str!("setContextClassLoader"),
        jni_sig!("(Ljava/lang/ClassLoader;)V"),
        &[(&loader).into()],
    )?;
    Ok(())
}

/// The BLE devices the phone is currently GATT-connected to, as the activity reports them:
/// `(address, name)` pairs. Empty when the activity is gone or the permission is missing —
/// a diagnostic lookup must never break the scan. Such a device usually does not advertise,
/// and while another app owns the link it cannot be connected from here; the devices dialog
/// lists it so the rider knows what holds the trainer.
#[cfg(target_os = "android")]
pub fn connected_ble_devices() -> Vec<(String, Option<String>)> {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // `android_context()` panics once the activity released the context (app going away).
    let Ok(context) = catch_unwind(AssertUnwindSafe(ndk_context::android_context)) else {
        return Vec::new();
    };
    // SAFETY: the JavaVM pointer comes from the NDK context the webview runtime set up and
    // stays valid for the life of the process.
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(context.vm().cast()) }) else {
        return Vec::new();
    };
    let activity = context.context();
    // The calling thread is not a Java thread: attach it for this call.
    let outcome: jni::errors::Result<Vec<(String, Option<String>)>> =
        vm.attach_current_thread(|env| connected_via(env, activity));
    outcome.unwrap_or_default()
}

/// One `"AA:BB:CC:DD:EE:FF|Name"` line per connected device, split back into parts.
#[cfg(target_os = "android")]
fn connected_via(
    env: &mut jni::Env,
    activity: *mut std::ffi::c_void,
) -> jni::errors::Result<Vec<(String, Option<String>)>> {
    use jni::{
        jni_sig, jni_str,
        objects::{JObject, JString},
    };

    // SAFETY: as in `use_app_class_loader`; the reference is only read within this call.
    let activity = unsafe { JObject::from_raw(env, activity.cast()) };
    let answer = env.call_method(
        &activity,
        jni_str!("connectedBleDevices"),
        jni_sig!("()Ljava/lang/String;"),
        &[],
    );
    let text = match answer {
        Ok(value) => value.l()?,
        Err(_) => {
            // A refused BLUETOOTH_CONNECT or a disappearing activity: swallow the pending
            // exception and answer with nothing.
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            return Ok(Vec::new());
        }
    };
    let text = env.cast_local::<JString>(text)?;
    let text: String = String::from(text.mutf8_chars(env)?);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    Ok(parse_connected_devices(&text))
}

#[cfg(any(target_os = "android", test))]
fn parse_connected_devices(text: &str) -> Vec<(String, Option<String>)> {
    text.lines()
        .filter_map(|line| {
            let (addr, name) = line.split_once('|')?;
            let name = name.trim();
            Some((
                addr.trim().to_string(),
                (!name.is_empty()).then(|| name.to_string()),
            ))
        })
        .filter(|(addr, _)| !addr.is_empty())
        .collect()
}

#[cfg(test)]
#[test]
fn connected_device_names_survive_the_activity_response() {
    assert_eq!(
        parse_connected_devices("AA:BB:CC:DD:EE:01| VE200 \nAA:BB:CC:DD:EE:02|\n"),
        vec![
            ("AA:BB:CC:DD:EE:01".into(), Some("VE200".into())),
            ("AA:BB:CC:DD:EE:02".into(), None),
        ],
    );
}

/// Nothing to initialize outside Android.
#[cfg(not(target_os = "android"))]
pub fn init_backend() -> Result<(), String> {
    Ok(())
}
