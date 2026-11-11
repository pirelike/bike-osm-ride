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

/// Nothing to initialize outside Android.
#[cfg(not(target_os = "android"))]
pub fn init_backend() -> Result<(), String> {
    Ok(())
}
