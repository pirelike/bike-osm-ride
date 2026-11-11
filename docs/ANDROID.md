# Android

The same Rust shell runs on Android inside the activity that the Dioxus CLI (`dx`) generates, with a handful of
hand-maintained files around it. This document explains why those files exist, how to build and sign the APK, and
what is and is not known to work. Checked against the repository on 2026-11-04 (version 0.3.0).

## 1. Status

| | State |
|---|---|
| Builds in CI, installs, starts, phone layout | yes (the release workflow requires the stable release keystore; layout checked at 412 × 860) |
| System file picker for GPX | implemented through JNI; found and fixed by running the APK, see the 0.3.0 commit message |
| Bluetooth (`btleplug` droidplug backend, runtime permissions) | a rider reported (2026-11-08) finding a trainer and connecting it at the first try; **reconnecting after a disconnect was unreliable** (fixes for the app's own races are in, **not confirmed on a phone**; the droidplug stale-gatt problem is open, see [`BLUETOOTH.md`](BLUETOOTH.md) §7) |
| Keep the screen on while riding | implemented (`wake.rs` → `MainActivity.setKeepScreenOn` → `FLAG_KEEP_SCREEN_ON`, on while `Riding`, off when paused or finished), **not yet confirmed on a device** |
| Browser panel (Firefox) | not available |

The app **cannot be built in the sandboxed shell used for this repository** (the NDK's clang fails to create
temporary files there); builds were done in a normal shell and in CI.

## 2. How it is put together

- `dx bundle` cross-compiles `ride-app` into `libmain.so`, generates a Gradle project (`WryActivity`, resources,
  manifest) and runs Gradle. The app is a **`NativeActivity`** (tao event loop + wry webview).
- **A pinned manifest** (`crates/ride-app/android/AndroidManifest.xml`, selected by `[application] android_manifest`
  in `Dioxus.toml`). Why: `dx`'s generated `android:configChanges` lists only
  `orientation|screenLayout|screenSize|keyboardHidden`. Any other change (system dark mode = `uiMode`, font size,
  language, density) recreates the activity, which runs `ANativeActivity_onCreate` again in the *same process*,
  and tao cannot set its event loop up twice: the app dies. The pinned copy lists every change the app survives
  (`orientation|screenLayout|screenSize|smallestScreenSize|keyboardHidden|keyboard|navigation|density|uiMode|fontScale|locale|layoutDirection|touchscreen`),
  also sets `windowSoftInputMode="adjustResize"` (the soft keyboard no longer covers a dialog) and
  `networkSecurityConfig` (the webview talks to loopback over cleartext). `packaging_tests` in `main.rs` keep it
  in step with `Dioxus.toml`: permissions, the identifier, the library name, the config-change list.
- **A pinned activity** (`android/MainActivity.kt`, `android_main_activity` in `Dioxus.toml`; package
  `dev.dioxus.main`, a `typealias BuildConfig` to the application id because the generated `Logger.kt` needs it).
  It extends `WryActivity` with a few things driven from Rust through JNI:
  - `pickGpx()`: `ACTION_OPEN_DOCUMENT` (Storage Access Framework). The picker returns a `content://` URI, which
    the route loader cannot open as a path, so the stream is copied into `cacheDir/gpx/` (sanitised name,
    `.gpx` appended) and the path goes back to Rust through the native method `onPicked` (an empty string means
    cancelled). Rust side: `android_picker.rs`, `Java_dev_dioxus_main_MainActivity_onPicked`; the shell polls the
    answer (`shell::use_android_picker`).
  - `requestBlePermissions()`: `BLUETOOTH_SCAN` + `BLUETOOTH_CONNECT` on Android 12+, fine location before that.
    Asked once in `onCreate`; the Devices dialog's "Keresés indítása" asks again after a refusal.
  - `setKeepScreenOn(on)`: adds or clears `FLAG_KEEP_SCREEN_ON` on the window inside `runOnUiThread` (Rust calls
    from its own thread, a window flag may only change on the UI thread). No permission needed. The controller
    already calls `wake::set` on every ride-state transition, so the Android branch of `wake.rs` is the only Rust
    change. The JNI call is wrapped in `catch_unwind` because `ndk_context::android_context()` panics once the
    activity has let go of the context, and a panic out of an event handler poisons the global UI mutex.
  - `restoreBackup()` / `saveFile(path, name, mime)`: the backup zip, both ways (`ACTION_OPEN_DOCUMENT` and
    `ACTION_CREATE_DOCUMENT`, `application/zip`). A picked zip is copied into `cacheDir/backup/` on a worker thread
    and answered through `onBackupPicked` (empty = cancelled); for a save Rust writes the zip into
    `<data>/backup-tmp/` (a ride's TCX into `<data>/export-tmp/`, type `application/octet-stream` so the dialog keeps
    the `.tcx` name), `saveFile` remembers the path, the dialog result is copied to the chosen URI (`"wt"`, so
    an existing file is replaced) and the temp file deleted, then `onBackupSaved` says `ok`, `cancel` or `error`. Two
    separate Rust slots (`PICKED_BACKUP`, `SAVED_BACKUP`), because `PICKED` always goes to the GPX loader; the shell
    polls all three (`use_android_picker`). **Never run on a device.**
  The two sides are tied by name, so a rename on either side needs the other (and a test checks the contract).
- **Permissions** (`[permissions]` in `Dioxus.toml`): `INTERNET` (default), Bluetooth, and location because
  Android 11 and older need it to scan.
- **SDK levels** (`[android]`): `min_sdk` 24, `compile_sdk` 34 and **`target_sdk` 34, pinned on purpose**. From
  target 35 Android 15 draws every app edge to edge: the page would run under the status bar, the camera cutout and
  the gesture bar. With 34 the system keeps the status bar as its own strip above the app. The CSS already pads
  the top bar and the tabs with `env(safe-area-inset-*)`, so raising the target (the Play Store will ask for 35)
  needs a check on a phone, not a redesign. (The notes kept while pinning it say a WebView reports the insets
  reliably only from about version 140 with `viewport-fit=cover`.)
- **Launcher icon and launch screen.** `dx` 0.7.10 writes its own template icons (a blue diamond) and its own
  light `AppTheme` into the Gradle project on **every** bundle and ignores `[android] icon`. So after
  `dx bundle`, `packaging/android/resoverlay.sh` overwrites `mipmap-*/ic_launcher*.webp` and `values/styles.xml`
  with ours (`packaging/android/res/`: the dark rounded tile with the teal ridge as adaptive icon, a splash
  drawable and a dark `AppTheme` whose window background is the splash, so a cold start does not flash white)
  and repackages with Gradle (`assembleRelease` if the module has a release output, else `assembleDebug`). The images
  come from `packaging/android/make-icons.py` (Python + Pillow) out of `packaging/icons/1024.png`.
  *(The header comment of `resoverlay.sh` still mentions `@style/BikeSplash` and `ic_launcher_round`; the pinned
  manifest and the script's behaviour use `AppTheme` and `ic_launcher`.)*
- **Data folder.** `paths::init_android_data_dir` asks the activity for `getFilesDir()` (`android_picker::files_dir`)
  and falls back to `/data/user/<uid/100000>/<package>/files`, then `/data/data/<package>/files`; each candidate must
  pass a write probe. The old fixed `/data/data/...` guess gave "Permission denied" on phones with a second user, work
  profile or dual apps. Not run on a device.
- **btleplug's Java half.** The droidplug backend looks Java classes up by name from Rust
  (`com.nonpolynomial.btleplug.android.impl.Adapter`, plus `io.github.gedgygedgy.rust.*`). `dx` does not know them,
  so the first APKs failed the first scan with *failed to resolve java class ... class not found*. `resoverlay.sh`
  now copies the sources of the exact `btleplug` version (found with `cargo metadata`, so `Cargo.lock` decides) from
  the cargo registry into `app/src/main/java` and drops `packaging/android/btleplug.pro` next to the module: the
  release build runs R8, which would remove classes that only JNI reaches. The script therefore must run for every
  APK, not only for the icon. The classes alone did not fix it on a phone: droidplug resolves them through the thread's
  context class loader, which on the natively attached hub thread is the system loader; `ride-ble`'s
  `android_ble.rs` now sets the activity's class loader on that thread before `btleplug::platform::init`.

## 3. Surviving panics

`dioxus-desktop`'s Android backend runs every UI event, render and asset request under **one global mutex**
(`android_sync_lock`), locked with `.lock().unwrap()`. A panic inside an event handler poisons it, and from then on
*every* interaction panics too: one real bug turns into "the app crashes on everything", and the first, real panic
is lost. The defences (all in `ride-app`):

- `crash::install()` is the first call in `main`: every panic is written to `<data>/crash.log`, to logcat under the
  tag `bike-osm-ride`, to stderr, and kept for the **banner** the shell shows (a red strip with the first line;
  "Rendben" hides it; the app stays usable).
- `guard::guarded` and `Controller::with` contain panics from calls into other crates and from handlers;
  `document::eval` is wrapped (it `expect`s a live window, which Android takes away much sooner than a desktop).
- Never let a panic leave an event handler. This is a rule for all new code ([`DEVELOPMENT.md`](DEVELOPMENT.md)).

```bash
adb logcat -s bike-osm-ride                                        # live, also while crashing
adb shell run-as hu.borbelyhuba.bikeosmride cat files/crash.log    # debug builds only
```

**The TLS crash** (found in 0.3.0): `reqwest`'s `rustls` feature verifies certificates with
`rustls-platform-verifier`, which on Android needs a JNI initialisation `dx` does not generate. The first network
call, the update check five seconds after the start, panicked inside the event loop, poisoned the mutex and every
later tap, dialog and tab panicked too. On Android the HTTP client now validates against the bundled Mozilla
roots (`rustls` + `webpki-roots`, configured in `ride-route/src/http.rs`; the Street View client uses it too).

## 4. Building

Prerequisites: Android SDK and **NDK 27.2.12479018**, platform 34 and build-tools 34.0.0, JDK 17+ (CI uses
Temurin 21), `rustup target add aarch64-linux-android` (and `x86_64-linux-android` for an emulator), and the
Dioxus CLI **0.7.10** (`cargo install dioxus-cli@0.7.10 --locked`). Accept the SDK licences once
(`sdkmanager --licenses`).

```powershell
$env:ANDROID_HOME     = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:ANDROID_HOME\ndk\27.2.12479018"
$env:GRADLE_OPTS      = "-Dorg.gradle.daemon=false"   # a Gradle daemon keeps background shells hanging
cd crates\ride-app
dx bundle --platform android --package-types apk --release --target aarch64-linux-android
cd ..\..
bash packaging/android/resoverlay.sh                  # icon, launch screen, repackage (needs Gradle from the generated project)
# the APK: target\dx\bike-osm-ride\release\android\app\app\build\outputs\apk\...
```

An APK holds **one ABI**: `aarch64-linux-android` for phones, `x86_64-linux-android` for an emulator.
What `dx bundle` does: cross-compiles with the NDK's clang as linker; links with `-Wl,--export-dynamic` so the JNI
symbols (`Java_dev_dioxus_main_*`) are visible (without it: `UnsatisfiedLinkError`); copies the binary to
`app/src/main/jniLibs/<abi>/libmain.so` (what the generated `WryActivity` loads with
`System.loadLibrary("main")`); writes the Gradle project from the manifest and activity above; runs
`gradlew assemble*` (aapt2, d8/R8, apksigner). To iterate fast, rebuild only the Rust library
(`cargo rustc -p ride-app --target x86_64-linux-android -- -C link-arg=-landroid -C link-arg=-llog -C link-arg=-lOpenSLES -C link-arg=-lc++abi -C link-arg=-Wl,--export-dynamic`,
about 10 s), copy it over `libmain.so` and run `gradlew assembleDebug` in the generated project (about 5 s).

**Signing.** Local development builds may use the debug key (`CN=Android Debug`). The release workflow
requires `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS`; it fails when
credentials are missing and never publishes a debug-signed fallback. Keep the same release key across
versions to support updates in place. Moving from a debug-signed installation to a release-signed APK
requires uninstalling the debug build first. The in-app updater opens the APK in the browser for the system installer.

**Installing**: copy the APK to the phone and open it (allow installs from unknown sources), or
`adb install -r <apk>`.

## 5. Things that took time (so they need not again)

- `dx` template icons and theme reappear on every bundle: run `resoverlay.sh` *after* every `dx bundle`.
- Two resources with the same name in one density folder are an AAPT error: the overlay deletes the template files
  before copying ours (and cleans leftovers of an older version of the script).
- `setup-android` in CI needs `packages: platform-tools` only (its default list still names the removed `tools`).
- `jni` versions differ on purpose: `ride-ble` uses 0.22 (btleplug's), `ride-app` 0.21; never mix their environments.
- Tests must not hard-code the Windows tile base `http://tiles.localhost`; Android's is `https://tiles.localhost`.
- "Mentés TCX-ként" goes through the system's "save as" dialog on Android (`MainActivity.saveFile`), like the backup
  zip. Until 0.4.0 it tried a raw write into `/storage/emulated/0/Download` and fell back to the app's private
  `exports` folder; scoped storage refuses the raw write, so the file ended up where no file manager can see it
  and the toast still said "Mentve". There is no upload or share intent any more.
