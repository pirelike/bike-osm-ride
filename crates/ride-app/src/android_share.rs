//! Android: hands a finished ride's TCX to the Strava app through the system share sheet.
//!
//! The file goes into the shared Download folder through `MediaStore` (the only way a content
//! URI can be made without a `FileProvider` entry in the manifest), then an `ACTION_SEND`
//! intent aimed at `com.strava` carries that URI; if the Strava app is not installed the
//! system chooser opens instead. Title, sport and the other details are typed in the Strava app,
//! it has no way to take them from the intent.

use jni::objects::{JObject, JValue};
use jni::{JNIEnv, JavaVM};

const MIME: &str = "application/vnd.garmin.tcx+xml";
const FLAG_GRANT_READ_URI_PERMISSION: i32 = 1;
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
/// `MediaStore.Downloads` exists from Android 10.
const MIN_SDK: i32 = 29;

/// Saves `bytes` as `name` in the Download folder and opens the Strava app with it.
pub fn share_tcx(name: &str, bytes: &[u8]) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    // SAFETY: the pointers come from `ndk_context`, which the Android glue filled with the live
    // JavaVM and the Activity before `main` ran.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    // SAFETY: as above; `JObject` does not free what it wraps.
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let result = share(&mut env, &activity, name, bytes);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

fn put(env: &mut JNIEnv, values: &JObject, key: &str, value: &str) -> Result<(), String> {
    let key = env.new_string(key).map_err(|e| e.to_string())?;
    let value = env.new_string(value).map_err(|e| e.to_string())?;
    env.call_method(
        values,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&key), JValue::Object(&value)],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn share(env: &mut JNIEnv, activity: &JObject, name: &str, bytes: &[u8]) -> Result<(), String> {
    let err = |e: jni::errors::Error| e.to_string();

    let sdk = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
        .and_then(|v| v.i())
        .map_err(err)?;
    if sdk < MIN_SDK {
        return Err("a megosztáshoz Android 10 vagy újabb kell".to_string());
    }

    // 1. The file, in Download/bike-osm-ride.
    let values = env
        .new_object("android/content/ContentValues", "()V", &[])
        .map_err(err)?;
    put(env, &values, "_display_name", name)?;
    put(env, &values, "mime_type", MIME)?;
    put(env, &values, "relative_path", "Download/bike-osm-ride")?;
    let table = env
        .get_static_field(
            "android/provider/MediaStore$Downloads",
            "EXTERNAL_CONTENT_URI",
            "Landroid/net/Uri;",
        )
        .and_then(|v| v.l())
        .map_err(err)?;
    let resolver = env
        .call_method(
            activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(err)?;
    let uri = env
        .call_method(
            &resolver,
            "insert",
            "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
            &[JValue::Object(&table), JValue::Object(&values)],
        )
        .and_then(|v| v.l())
        .map_err(err)?;
    if uri.is_null() {
        return Err("a fájl nem hozható létre a Letöltések mappában".to_string());
    }
    let out = env
        .call_method(
            &resolver,
            "openOutputStream",
            "(Landroid/net/Uri;)Ljava/io/OutputStream;",
            &[JValue::Object(&uri)],
        )
        .and_then(|v| v.l())
        .map_err(err)?;
    if out.is_null() {
        return Err("a fájl nem írható".to_string());
    }
    let array = env.byte_array_from_slice(bytes).map_err(err)?;
    let array = JObject::from(array);
    env.call_method(&out, "write", "([B)V", &[JValue::Object(&array)])
        .map_err(err)?;
    env.call_method(&out, "close", "()V", &[]).map_err(err)?;

    // 2. The intent: aimed at the Strava app, the system chooser if it is not there.
    let action = env.new_string("android.intent.action.SEND").map_err(err)?;
    let intent = env
        .new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )
        .map_err(err)?;
    let mime = env.new_string(MIME).map_err(err)?;
    env.call_method(
        &intent,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&mime)],
    )
    .map_err(err)?;
    let extra = env.new_string("android.intent.extra.STREAM").map_err(err)?;
    env.call_method(
        &intent,
        "putExtra",
        "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;",
        &[JValue::Object(&extra), JValue::Object(&uri)],
    )
    .map_err(err)?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(
            FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
        )],
    )
    .map_err(err)?;
    let package = env.new_string("com.strava").map_err(err)?;
    env.call_method(
        &intent,
        "setPackage",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&package)],
    )
    .map_err(err)?;

    let started = env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&intent)],
    );
    if started.is_ok() {
        return Ok(());
    }
    // The Strava app is not installed (ActivityNotFoundException): let the rider choose.
    let _ = env.exception_clear();
    env.call_method(
        &intent,
        "setPackage",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&JObject::null())],
    )
    .map_err(err)?;
    let title = env.new_string("TCX megosztása").map_err(err)?;
    let chooser = env
        .call_static_method(
            "android/content/Intent",
            "createChooser",
            "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
            &[JValue::Object(&intent), JValue::Object(&title)],
        )
        .and_then(|v| v.l())
        .map_err(err)?;
    env.call_method(
        &chooser,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
    )
    .map_err(err)?;
    env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&chooser)],
    )
    .map(|_| ())
    .map_err(err)
}
