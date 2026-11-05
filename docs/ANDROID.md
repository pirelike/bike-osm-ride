# Android

The same Rust shell runs on Android inside the activity that the Dioxus CLI (`dx`) generates, with a handful of
hand-maintained files around it. This document explains why those files exist, how to build and sign the APK, and
what is and is not known to work. Checked against the repository on 2026-11-04 (version 0.3.0).

## 1. Status

| | State |
|---|---|
| Builds in CI, installs, starts, phone layout | yes (the release workflow builds a signed-with-debug-key APK; layout checked at 412 × 860) |
| System file picker for GPX, share of the TCX to the Strava app | implemented through JNI; found and fixed by running the APK, see the 0.3.0 commit message |
| Bluetooth (`btleplug` droidplug backend, runtime permissions) | implemented, **never confirmed with a real sensor or trainer** |
| Keep the screen on while riding | **not implemented**: `wake.rs` does nothing on Android, and nothing in our activity sets `FLAG_KEEP_SCREEN_ON`. Unless the generated base activity does, the system's screen timeout applies. Unverified |
| Hidden Strava upload | impossible: wry on Android allows one WebView only; the share intent is used instead |
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
  It extends `WryActivity` with exactly two things driven from Rust through JNI:
  - `pickGpx()`: `ACTION_OPEN_DOCUMENT` (Storage Access Framework). The picker returns a `content://` URI, which
    the route loader cannot open as a path, so the stream is copied into `cacheDir/gpx/` (sanitised name,
    `.gpx` appended) and the path goes back to Rust through the native method `onPicked` (an empty string means
    cancelled). Rust side: `android_picker.rs`, `Java_dev_dioxus_main_MainActivity_onPicked`; the shell polls the
    answer (`shell::use_android_picker`).
  - `requestBlePermissions()`: `BLUETOOTH_SCAN` + `BLUETOOTH_CONNECT` on Android 12+, fine location before that.
    Asked once in `onCreate`; the Devices dialog's "Keresés indítása" asks again after a refusal.
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

**Signing.** Without a keystore the APK is signed with the debug key (`CN=Android Debug`): installable, but only over
another debug-signed copy; going from debug-signed to a keystore-signed APK needs
`adb uninstall hu.borbelyhuba.bikeosmride` first. The release workflow signs with the repository secrets
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` when present, else with the runner's
debug key (what every release so far has done: no keystore secrets exist). An APK can only be updated in place by
one signed with the same key; the in-app updater just opens the new APK in the browser and the system installer
takes over.

**Installing**: copy the APK to the phone and open it (allow installs from unknown sources), or
`adb install -r <apk>`.

## 5. Things that took time (so they need not again)

- `dx` template icons and theme reappear on every bundle: run `resoverlay.sh` *after* every `dx bundle`.
- Two resources with the same name in one density folder are an AAPT error: the overlay deletes the template files
  before copying ours (and cleans leftovers of an older version of the script).
- `setup-android` in CI needs `packages: platform-tools` only (its default list still names the removed `tools`).
- `jni` versions differ on purpose: `ride-ble` uses 0.22 (btleplug's), `ride-app` 0.21; never mix their environments.
- Tests must not hard-code the Windows tile base `http://tiles.localhost`; Android's is `https://tiles.localhost`.
- The Strava share needs Android 10+ (`MediaStore.Downloads`) and puts the TCX into the shared Download folder; the
  Strava app takes no title or other details from the intent. "Csak mentés TCX-ként" writes into the shared
  Download folder too, or into the app's own `exports` folder when that is not writable (`paths::export_dir`).
