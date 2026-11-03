//! Android: the system file picker and the runtime Bluetooth permissions.
//!
//! Android has no native "open a file" dialog a desktop process could call: the app has to ask the
//! **system** through the Storage Access Framework, which means calling into the activity. The
//! activity (`android/MainActivity.kt`, pinned in `Dioxus.toml`) starts the picker, copies the
//! picked document into the app's cache folder and calls
//! [`Java_dev_dioxus_main_MainActivity_onPicked`] back with a path the route loader can open; a
//! cancelled picker answers an empty path.
//!
//! The answer arrives on Android's schedule, so it is parked here and the shell picks it up
//! (`shell::use_android_picker`), the same way the crash reporter hands its message over.

use std::path::PathBuf;
use std::sync::Mutex;

use jni::objects::{JObject, JString};
use jni::{JNIEnv, JavaVM};

/// The file the picker returned, until the shell takes it.
static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Opens the system picker for a GPX. The result arrives later, see [`take_picked`].
pub fn pick_gpx() -> Result<(), String> {
    call_activity("pickGpx")
}

/// The picked file, once. `None` while the picker is still open (or was cancelled).
pub fn take_picked() -> Option<PathBuf> {
    PICKED.lock().ok().and_then(|mut slot| slot.take())
}

/// Asks for the Bluetooth permissions the scan needs; harmless when they are already granted.
pub fn request_ble_permissions() {
    let _ = call_activity("requestBlePermissions");
}

/// Calls a no-argument method on the live activity, with a JNI environment attached to this thread.
fn call_activity(method: &str) -> Result<(), String> {
    let context = ndk_context::android_context();
    // SAFETY: the pointers come from `ndk_context`, which the Android glue filled with the live
    // JavaVM and the Activity before `main` ran.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|err| err.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|err| err.to_string())?;
    // SAFETY: as above; `JObject` does not free what it wraps.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = env
        .call_method(&activity, method, "()V", &[])
        .map(|_| ())
        .map_err(|err| err.to_string());
    // A Java exception (no picker app, a refused permission dialog) must not travel on.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

/// Called from `MainActivity.onActivityResult` through JNI. The path is empty when the rider
/// cancelled; anything else is a file in the app's cache folder.
///
/// # Safety
///
/// The JVM passes a valid environment for this thread, a live activity reference and a live
/// string; both are only read, within this call.
#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_MainActivity_onPicked(
    env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    path: jni::sys::jstring,
) {
    if env.is_null() || path.is_null() {
        return;
    }
    // SAFETY: the JVM hands us a valid `JNIEnv` for this thread and a live `jstring`.
    let Ok(mut env) = (unsafe { JNIEnv::from_raw(env) }) else {
        return;
    };
    // SAFETY: `path` is a live local reference for the duration of this call.
    let path = unsafe { JString::from_raw(path) };
    let Ok(text) = env.get_string(&path) else {
        return;
    };
    let text: String = text.into();
    if text.is_empty() {
        return;
    }
    if let Ok(mut slot) = PICKED.lock() {
        *slot = Some(PathBuf::from(text));
    }
}

#[cfg(test)]
mod tests {
    /// The Kotlin side and this module have to agree on the callback name: it is the JNI symbol.
    #[test]
    fn the_activity_declares_the_callback_this_module_implements() {
        const KOTLIN: &str = include_str!("../android/MainActivity.kt");
        assert!(
            KOTLIN.contains("private external fun onPicked(path: String)"),
            "MainActivity.kt must declare `onPicked(path: String)`"
        );
        const RUST: &str = include_str!("android_picker.rs");
        assert!(
            RUST.contains("Java_dev_dioxus_main_MainActivity_onPicked"),
            "the JNI symbol has to match `dev.dioxus.main.MainActivity.onPicked`"
        );
        for called in ["\"pickGpx\"", "\"requestBlePermissions\""] {
            assert!(RUST.contains(called), "android_picker calls {called}");
        }
        for declared in ["fun pickGpx()", "fun requestBlePermissions()"] {
            assert!(
                KOTLIN.contains(declared),
                "MainActivity.kt must declare {declared}"
            );
        }
    }
}
