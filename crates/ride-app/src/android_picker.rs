//! Android: the system file picker, the backup file dialogs, the runtime Bluetooth permissions and
//! the keep-screen-on flag.
//!
//! Android has no native "open a file" dialog a desktop process could call: the app has to ask the
//! **system** through the Storage Access Framework, which means calling into the activity. The
//! activity (`android/MainActivity.kt`, pinned in `Dioxus.toml`) starts the picker, copies the
//! picked document into the app's cache folder and calls
//! [`Java_dev_dioxus_main_MainActivity_onPicked`] back with a path the route loader can open; a
//! cancelled picker answers an empty path.
//!
//! A backup works the same way in both directions: the zip to restore is picked and copied into the
//! cache (`onBackupPicked`), and a zip written by Rust is handed to the system's "save as" dialog
//! (`saveFile`), which answers with `onBackupSaved`. A ride's TCX leaves the app the same way.
//!
//! The answers arrive on Android's schedule, so they are parked here and the shell picks them up
//! (`shell::use_android_picker`), the same way the crash reporter hands its message over.

use std::path::PathBuf;
use std::sync::Mutex;

use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use ride_i18n::t;

/// The file the picker returned, until the shell takes it.
static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);
static PICK_ERROR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static BACKUP_PICK_ERROR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn take_backup_picker_error() -> bool {
    BACKUP_PICK_ERROR.swap(false, std::sync::atomic::Ordering::Relaxed)
}

pub fn take_picker_error() -> bool {
    PICK_ERROR.swap(false, std::sync::atomic::Ordering::Relaxed)
}
/// The backup zip the rider picked (already copied into the cache), until the shell takes it.
static PICKED_BACKUP: Mutex<Option<PathBuf>> = Mutex::new(None);
/// How saving a backup ended: `ok`, `cancel` or `error`, until the shell takes it.
static SAVED_BACKUP: Mutex<Option<String>> = Mutex::new(None);

/// What the "save as" dialog of a backup answered (`MainActivity.onBackupSaved`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupSaved {
    Saved,
    Cancelled,
    Failed,
}

/// An argument of a call into the activity. Strings are made into Java strings on the calling
/// thread, inside the attached environment.
enum Arg<'a> {
    Bool(bool),
    Str(&'a str),
}

/// Opens the system picker for a GPX. The result arrives later, see [`take_picked`].
pub fn pick_gpx() -> Result<(), String> {
    call_activity("pickGpx")
}

/// The picked file, once. `None` while the picker is still open (or was cancelled).
pub fn take_picked() -> Option<PathBuf> {
    PICKED.lock().ok().and_then(|mut slot| slot.take())
}

/// Opens the system picker for a backup zip. The result arrives later, see [`take_picked_backup`].
pub fn pick_backup() -> Result<(), String> {
    call_activity("restoreBackup")
}

/// The picked backup (a copy in the cache folder), once.
pub fn take_picked_backup() -> Option<PathBuf> {
    PICKED_BACKUP.lock().ok().and_then(|mut slot| slot.take())
}

/// Hands the finished file at `path` (a backup zip, a TCX) to the system's "save as" dialog,
/// suggesting `name`, with `mime` as the dialog's type filter. The activity copies it where the
/// rider chose and deletes `path`; the answer arrives later, see [`take_saved_backup`].
pub fn save_file(path: &str, name: &str, mime: &str) -> Result<(), String> {
    call_activity_with(
        "saveFile",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        &[Arg::Str(path), Arg::Str(name), Arg::Str(mime)],
    )
}

/// How the last "save as" ended, once.
pub fn take_saved_backup() -> Option<BackupSaved> {
    let code = SAVED_BACKUP.lock().ok().and_then(|mut slot| slot.take())?;
    Some(match code.as_str() {
        "ok" => BackupSaved::Saved,
        "cancel" => BackupSaved::Cancelled,
        _ => BackupSaved::Failed,
    })
}

/// Asks for the Bluetooth permissions the scan needs; harmless when they are already granted.
pub fn request_ble_permissions() {
    let _ = call_activity("requestBlePermissions");
}

/// Keeps the screen on (`true`) or lets the system time it out again (`false`). Android drops the
/// flag together with the window, so nothing has to be released when the app goes away.
pub fn set_keep_screen_on(on: bool) -> Result<(), String> {
    call_activity_with("setKeepScreenOn", "(Z)Z", &[Arg::Bool(on)])
}

/// The app's private files folder as the activity reports it (`getFilesDir().getAbsolutePath()`),
/// or `None` when the activity is not reachable. Runs before the UI exists, so it never throws.
pub fn files_dir() -> Option<PathBuf> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(files_dir_unguarded))
        .ok()
        .flatten()
}

fn files_dir_unguarded() -> Option<PathBuf> {
    let context = ndk_context::android_context();
    // SAFETY: as in `call_activity_unguarded`.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: as above; `JObject` does not free what it wraps.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = (|| {
        let file = env
            .call_method(&activity, "getFilesDir", "()Ljava/io/File;", &[])
            .ok()?
            .l()
            .ok()?;
        let path = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let path: String = env.get_string(&JString::from(path)).ok()?.into();
        Some(PathBuf::from(path)).filter(|p| p.is_absolute())
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

/// Calls a no-argument method on the live activity, with a JNI environment attached to this thread.
fn call_activity(method: &str) -> Result<(), String> {
    call_activity_with(method, "()V", &[])
}

/// Calls a `void` method on the live activity, with a JNI environment attached to this thread.
fn call_activity_with(method: &str, signature: &str, args: &[Arg]) -> Result<(), String> {
    // `android_context()` panics once the activity has released the context, and a panic that
    // leaves an event handler poisons the global UI mutex: keep the whole call fallible.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        call_activity_unguarded(method, signature, args)
    }))
    .unwrap_or_else(|_| {
        Err(t("app.android_picker.az_android_kornyezet_mar_nem_erheto_el").to_string())
    })
}

fn call_activity_unguarded(method: &str, signature: &str, args: &[Arg]) -> Result<(), String> {
    let context = ndk_context::android_context();
    // SAFETY: the pointers come from `ndk_context`, which the Android glue filled with the live
    // JavaVM and the Activity before `main` ran.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|err| err.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|err| err.to_string())?;
    // SAFETY: as above; `JObject` does not free what it wraps.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };

    // The Java strings have to outlive the call, so they are made first; the values then borrow them.
    let mut strings: Vec<JString> = Vec::new();
    for arg in args {
        if let Arg::Str(text) = arg {
            strings.push(env.new_string(text).map_err(|err| err.to_string())?);
        }
    }
    let mut next_string = strings.iter();
    let mut values: Vec<JValue> = Vec::with_capacity(args.len());
    for arg in args {
        match arg {
            Arg::Bool(on) => values.push(JValue::Bool(u8::from(*on))),
            Arg::Str(_) => {
                let string = next_string.next().ok_or_else(|| {
                    t("app.android_picker.belso_hiba_hianyzo_szoveges_argumentum").to_string()
                })?;
                values.push(JValue::Object(string));
            }
        }
    }

    let result = env
        .call_method(&activity, method, signature, &values)
        .map_err(|err| err.to_string())
        .and_then(|value| {
            if signature.ends_with(")Z") {
                match value.z() {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(t("app.wake.screen_flag_failed").into()),
                    Err(e) => Err(e.to_string()),
                }
            } else {
                Ok(())
            }
        });
    // A Java exception (no picker app, a refused permission dialog) must not travel on.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

/// The text of a `jstring` the JVM handed to a native method.
///
/// # Safety
///
/// `env` and `text` come straight from the JVM: a valid environment for this thread and a live
/// local reference, both only read within the native call.
unsafe fn native_text(env: *mut jni::sys::JNIEnv, text: jni::sys::jstring) -> Option<String> {
    if env.is_null() || text.is_null() {
        return None;
    }
    // SAFETY: the caller's contract.
    let mut env = unsafe { JNIEnv::from_raw(env) }.ok()?;
    // SAFETY: the caller's contract.
    let text = unsafe { JString::from_raw(text) };
    let text: String = env.get_string(&text).ok()?.into();
    Some(text)
}

/// Called from `MainActivity.onActivityResult` through JNI. The path is empty when the rider
/// cancelled; anything else is a file in the app's cache folder.
///
/// # Safety
///
/// The JVM passes a valid environment for this thread, a live activity reference and a live
/// string; both are only read, within this call.
#[no_mangle]
pub unsafe extern "system" fn Java_dev_dioxus_main_MainActivity_onPicked(
    env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    path: jni::sys::jstring,
) {
    // SAFETY: the JVM hands us a valid `JNIEnv` for this thread and a live `jstring`.
    let Some(text) = (unsafe { native_text(env, path) }) else {
        return;
    };
    if text == "error" {
        PICK_ERROR.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    if text.is_empty() {
        return;
    }
    if let Ok(mut slot) = PICKED.lock() {
        *slot = Some(PathBuf::from(text));
    }
}

/// Called by the activity when the backup zip the rider picked has been copied into the cache
/// folder. Empty: cancelled, or the copy failed.
///
/// # Safety
///
/// As for [`Java_dev_dioxus_main_MainActivity_onPicked`].
#[no_mangle]
pub unsafe extern "system" fn Java_dev_dioxus_main_MainActivity_onBackupPicked(
    env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    path: jni::sys::jstring,
) {
    // SAFETY: the JVM hands us a valid `JNIEnv` for this thread and a live `jstring`.
    let Some(text) = (unsafe { native_text(env, path) }) else {
        return;
    };
    if text == "error" {
        BACKUP_PICK_ERROR.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    if text.is_empty() {
        return;
    }
    if let Ok(mut slot) = PICKED_BACKUP.lock() {
        *slot = Some(PathBuf::from(text));
    }
}

/// Called by the activity when the "save as" dialog of a backup is done: `ok`, `cancel` or `error`.
///
/// # Safety
///
/// As for [`Java_dev_dioxus_main_MainActivity_onPicked`].
#[no_mangle]
pub unsafe extern "system" fn Java_dev_dioxus_main_MainActivity_onBackupSaved(
    env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    result: jni::sys::jstring,
) {
    // SAFETY: the JVM hands us a valid `JNIEnv` for this thread and a live `jstring`.
    let code = unsafe { native_text(env, result) }.unwrap_or_else(|| "error".to_string());
    if let Ok(mut slot) = SAVED_BACKUP.lock() {
        *slot = Some(code);
    }
}

#[cfg(test)]
mod tests {
    /// The Kotlin side and this module have to agree on the callback names: they are the JNI symbols.
    #[test]
    fn the_activity_declares_the_callbacks_this_module_implements() {
        const KOTLIN: &str = include_str!("../android/MainActivity.kt");
        const RUST: &str = include_str!("android_picker.rs");
        for (declared, symbol) in [
            (
                "private external fun onPicked(path: String)",
                "Java_dev_dioxus_main_MainActivity_onPicked",
            ),
            (
                "private external fun onBackupPicked(path: String)",
                "Java_dev_dioxus_main_MainActivity_onBackupPicked",
            ),
            (
                "private external fun onBackupSaved(result: String)",
                "Java_dev_dioxus_main_MainActivity_onBackupSaved",
            ),
        ] {
            assert!(
                KOTLIN.contains(declared),
                "MainActivity.kt must declare `{declared}`"
            );
            assert!(
                RUST.split("#[cfg(test)]")
                    .next()
                    .unwrap_or_default()
                    .contains(&format!("pub unsafe extern \"system\" fn {symbol}(")),
                "the JNI symbol {symbol} has to match the Kotlin method"
            );
        }
        for called in [
            "\"pickGpx\"",
            "\"requestBlePermissions\"",
            "\"setKeepScreenOn\"",
            "\"restoreBackup\"",
            "\"saveFile\"",
        ] {
            assert!(RUST.contains(called), "android_picker calls {called}");
        }
        for declared in [
            "fun pickGpx()",
            "fun requestBlePermissions()",
            "fun setKeepScreenOn(on: Boolean)",
            "fun restoreBackup()",
            "fun saveFile(path: String, name: String, mime: String)",
        ] {
            assert!(
                KOTLIN.contains(declared),
                "MainActivity.kt must declare {declared}"
            );
        }
    }
}
