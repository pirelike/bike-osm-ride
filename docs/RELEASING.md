# Releasing

How a version reaches users: the checklist, what the workflow builds, the contract with the in-app updater, and
the failures that have already happened. Checked against `.github/workflows/release.yml`, `packaging/` and
`crates/ride-app/src/updater.rs` on 2026-11-04. The current release is **v0.5.0** (2026-11-07); v0.4.0, v0.3.1 and v0.3.0 had all five assets.

## 1. The assets and who consumes them

`.github/workflows/release.yml` builds one package per platform. The names are a **contract**: the in-app updater
(`Platform::matches` in `updater.rs`) looks for exactly these, so renaming one means changing the updater, the
workflow header and the README table together.

| Asset | Platform | Built by | How the updater treats it |
|---|---|---|---|
| `bike-osm-ride-<ver>-windows-x64.exe` | Windows | MSVC, `-C target-feature=+crt-static`: no DLL, no VC++ redistributable | downloads, checks the `MZ` magic and a minimum size, renames the running exe to `*.old`, puts the new one in place, restarts; `cleanup_previous` removes the `.old` at the next start |
| `bike-osm-ride-<ver>-android.apk` | Android | `dx bundle` + `resoverlay.sh` | opens the URL in the browser; the system installer takes over |
| `bike-osm-ride-<ver>-linux-x86_64.AppImage` | Linux | `dx bundle` or `packaging/linux/fallback.sh` | inside an AppImage (`APPIMAGE` is set) it replaces itself (ELF magic check); otherwise it opens the release page |
| `bike-osm-ride-<ver>-linux-amd64.deb` | Linux | the same | opens the release page (the package manager owns `/usr`) |
| `bike-osm-ride-<ver>-macos-universal.dmg` | macOS | `packaging/macos/bundle.sh` | opens the release page (a quarantined, ad-hoc signed bundle cannot replace itself) |

The updater reads the repository from the `repository` field of `Cargo.toml` at compile time, asks
`https://api.github.com/repos/<owner>/<repo>/releases/latest` five seconds after the start, ignores drafts,
pre-releases, foreign URLs and garbage, and compares versions numerically (`v0.3.0` against the compiled
version). It is silent on any failure. **It verifies nothing beyond TLS, the file magic and a minimum size**: no
checksum, no signature (see §6).

## 2. Checklist

1. `cargo test --workspace` (see [`TESTING.md`](TESTING.md) for the `TEMP` caveat) and `cargo clippy --workspace`.
2. Bump `version` in `[workspace.package]` of `Cargo.toml` (the updater compares the tag with the *compiled*
   version, so a tag without a bump offers itself as an update forever). Let `Cargo.lock` follow.
3. Update `CHANGELOG.md`, the README download table if names changed, `THIRD_PARTY_LICENSES.md`
   (`scripts/third_party_licenses.ps1`) if dependencies changed, `NOTICE.md` for vendored code.
4. Commit as `<version>: <summary>` with a body that says what changed for the rider and why.
5. Tag `vX.Y.Z` on that commit and push the commit and the tag.
6. GitHub → *Actions* → *release* → *Run workflow*, choosing the **tag** as the ref. The workflow is
   `workflow_dispatch` only; it has not been switched to `push: tags` yet (the comment in the file says to do that
   once a run has been checked end to end). A run on a branch builds the artifacts but publishes nothing: the
   `release` job runs only when `github.ref` starts with `refs/tags/v`.
7. When it is green, check the release has all five assets, then download and start the Windows exe once.

**Never move or delete a pushed tag.** A fix after a pushed tag is a new patch version. (`v0.2.0` and `v0.2.2`
exist as tags without a release because their runs failed; removing a tag is the owner's job, with
`git push origin :refs/tags/<tag>`.)

## 3. What the workflow does

| Job | Runner | Steps |
|---|---|---|
| `test` | `windows-latest`, `ubuntu-22.04`, `macos-15` (`fail-fast: false`: one red platform must not hide the others) | on Linux install `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libdbus-1-dev pkg-config`; `cargo test --workspace` |
| `windows` | `windows-latest` | `cargo build --release -p ride-app` with `RUSTFLAGS=-C target-feature=+crt-static`; copy to the asset name |
| `android` | `ubuntu-latest` | Rust target `aarch64-linux-android`; Temurin 21; `setup-android` with `packages: platform-tools`; `sdkmanager "ndk;27.2.12479018" "platforms;android-34" "build-tools;34.0.0"`; Dioxus CLI 0.7.10 (binstall, fallback to `cargo install --locked`); `dx bundle --platform android --package-types apk --release --target aarch64-linux-android`; `resoverlay.sh`; sign with the keystore secrets if set, else keep the debug-signed APK |
| `linux` | `ubuntu-22.04` | build dependencies and `xvfb`; plain `cargo build --release -p ride-app`; **smoke test**: run the binary with `RIDE_DEMO=1` under `xvfb-run -a timeout 20` and require exit code 124 (still running, nothing crashed); `dx bundle --platform linux --package-types appimage --package-types deb` (`continue-on-error`); `fallback.sh` when that failed; name and verify both files exist |
| `macos` | `macos-15` | Rust targets `aarch64-apple-darwin` and `x86_64-apple-darwin`; `bundle.sh`; check that `Info.plist` lints, holds `NSBluetoothAlwaysUsageDescription` (without it macOS kills the app the moment it touches CoreBluetooth) and that `lipo -archs` is `x86_64 arm64` |
| `release` | `ubuntu-latest` | needs all of the above; only on a `v*` tag; `softprops/action-gh-release` with generated release notes |

## 4. Platform notes

- **Windows.** The exe is the installer. WebView2 runtime is needed (Windows 11 has it). A locally built GNU exe
  is *not* a release candidate: it imports `WebView2Loader.dll` (v0.1.0 shipped that way and users hit "A
  WebView2Loader.dll nem található"). Only the CI exe is published.
- **Linux.** Built on Ubuntu 22.04 (glibc 2.35) so both packages start on older distributions. The prebuilt `dx`
  binary needs glibc 2.39, so on 22.04 the `dx bundle` step has so far always failed and `packaging/linux/fallback.sh`
  is what really makes the two files: the `.deb` with `dpkg-deb` (depends on `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `bluez`) and the
  AppImage with `linuxdeploy` and its GTK plugin (an **absolute** output directory, and the icon installed as
  `bike-osm-ride.png` in the hicolor theme folder to match the `.desktop` file's `Icon=`; both were run-breakers).
- **macOS.** One Apple Silicon runner cross-compiles the Intel half and `lipo` merges them (GitHub no longer offers
  Intel runners). The bundle is **ad-hoc signed, not notarised**: the signature is required on Apple Silicon, but
  Gatekeeper still quarantines the download, so users open it with right click → Megnyitás or
  `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`. `iconutil` needs the exact `.iconset` names,
  which `bundle.sh` creates from `packaging/icons/`. The first Bluetooth scan asks for permission.
- **Android.** See [`ANDROID.md`](ANDROID.md). Without keystore secrets the APK is signed with the runner's debug
  key, so each release is a different signature: installing over an older release can need an uninstall first.

## 5. Failures so far (each cost a patch version)

| Release | What failed | Fix |
|---|---|---|
| 0.1.0 | published the GNU exe alone | v0.1.1 ships the CI exe; later also static CRT |
| 0.2.0 | a "line-ending clean-up" had stripped the CR of the PNG signature in `packaging/icons/*.png` (the bundlers could not read them); `fallback.sh` used a relative output folder; `setup-android` failed on the removed `tools` package; two tests hard-coded the Windows `tiles.localhost` base; a Street View test counted all requests although the prefetcher may resolve ids ahead | 0.2.1: icons regenerated, `.gitattributes` marks images and fonts binary, absolute folder, `packages: platform-tools`, tests use `ride_ui_kit::protocol::base` |
| 0.2.2 | `linuxdeploy` (`continuous`) rejects an icon file whose name does not match `Icon=` | 0.2.3: install the icon into the hicolor tree under the right name |
| 0.4.0 (caught on a dry run, no tag lost) | deleting `mod strava;` left its `#[cfg(not(target_os = "android"))]` in front of `mod streetview_images;`, so only the APK build failed (E0433); the host build cannot see it | removed the stray `cfg`; `no_module_of_the_shell_is_hidden_from_android` in `main.rs` |
| 0.5.0 (caught on a dry run, no tag lost) | the string extractor turned two constants in the Linux- and macOS-only code of `wake.rs` into `const … = t("…")`; `t()` is not a `const fn` and was not imported there (E0425), and a Windows build never compiles that code | look the text up at call time; `no_constant_is_initialised_with_a_lookup` in `ride-i18n/tests/catalogs.rs` scans every platform's sources |
| 0.5.0 (the same dry run) | both macOS jobs ended after one second with no step: "The configuration 'macos-14-arm64-us-default' is not supported", GitHub had retired the image | `macos-15` for the `test` matrix and the `macos` job |

Lesson: run the whole workflow on a branch (it builds everything and publishes nothing) before tagging. It was worth
it for 0.4.0: Android code is never compiled by a local test run. One expected red on a branch: the Linux fallback
packaging fails with `'Version' field value 'main'` because the version comes from the ref name; on a tag it is fine.

## 6. Known gaps

- **No CI on push or pull request.** `release.yml` is the only workflow and it is started by hand, so nothing
  stops a red `cargo test` from reaching `main`. A small `ci.yml` (fmt, clippy, test on the three desktop systems)
  would close that.
- **Update integrity.** The updater trusts GitHub and TLS. Publishing SHA-256 checksums with the assets and
  verifying them before the swap (or signing the exe) would be the next step.
- **macOS is not notarised** and **Android is debug-signed** (no update in place across signatures).
- The macOS and Linux apps are built (and, on Linux, smoke-tested) by CI; there is no record of the maintainer running them on a real machine.
- The workflow header and the in-app/README tables must be kept equal by hand.
