#!/usr/bin/env bash
#
# The app's own launcher icon and launch window, put into the Gradle project `dx bundle` has just
# generated, and the APK repackaged.
#
# Why this exists: `dx` 0.7.10 writes its Dioxus-template icons (`mipmap-*/ic_launcher.webp`, the
# blue diamond) and its own light `AppTheme` on **every** bundle, and it never reads
# `[android] icon`. The pinned manifest
# (`crates/ride-app/android/AndroidManifest.xml`) therefore points at `@style/BikeSplash` and at
# `@mipmap/ic_launcher` + `@mipmap/ic_launcher_round`, and this script drops those resources beside
# the template ones — after deleting them, since two files with the same resource name in one
# density folder fail the build.
#
# Run from the repository root after `dx bundle --platform android ...`:
#   bash packaging/android/resoverlay.sh
#
# The images come from `packaging/android/make-icons.py` (regenerate with Python + Pillow):
#   python packaging/android/make-icons.py
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

# The project dx has just written: the one whose manifest is the newest (a leftover profile from an
# earlier run must not be patched). An explicit path as the first argument wins.
main="${1:-}"
if [ -z "$main" ]; then
  main="$(for candidate in $(find "$root/target/dx" -type d -path '*android/app/app/src/main' 2>/dev/null); do
    manifest="$candidate/AndroidManifest.xml"
    if [ -f "$manifest" ]; then
      printf '%s %s\n' "$(stat -c %Y "$manifest")" "$candidate"
    fi
  done | sort -rn | head -n 1 | cut -d' ' -f2-)"
fi
if [ -z "$main" ] || [ ! -d "$main" ]; then
  echo "no generated Android project under target/dx — run dx bundle first" >&2
  exit 1
fi
res="$main/res"
echo "android res: $res"

# The icon files use the template names (`ic_launcher.webp`, `ic_launcher_foreground.webp`), so the
# copy below overwrites what dx wrote: a reused Gradle project never collects two resources with the
# same name (that is an AAPT error). These are leftovers from an older version of this script, and
# heal a project that ran it.
rm -f "$res"/mipmap-*/ic_launcher.png "$res"/mipmap-*/ic_launcher_round.png \
      "$res"/mipmap-*/ic_launcher_foreground.png
rm -f "$res"/values/themes.xml

cp -R "$here/res/." "$res/"

# btleplug's Android backend (droidplug) is half Rust, half Java: the Rust side looks the Java
# classes up by name (`com.nonpolynomial.btleplug.android.impl.Adapter`, ...). `dx` knows nothing
# about them, so without this step the APK has no such class and the first scan fails with
# "failed to resolve java class ... class not found". The sources are copied from the exact
# crate version the build used (found through `cargo metadata`, so Cargo.lock decides) into the
# Gradle project, and R8 gets keep rules (a release build shrinks classes only JNI reaches).
module="$main/../.."
manifest="$(cd "$root" && cargo metadata --format-version 1 --filter-platform aarch64-linux-android 2>/dev/null \
  | grep -o '"manifest_path":"[^"]*btleplug-[0-9][^"]*Cargo.toml"' | head -n 1 \
  | sed -e 's/^"manifest_path":"//' -e 's/"$//' -e 's/\\\\/\//g')"
if [ -z "$manifest" ]; then
  echo "btleplug not found in cargo metadata" >&2
  exit 1
fi
if command -v cygpath >/dev/null 2>&1; then
  manifest="$(cygpath -u "$manifest")"
fi
javasrc="$(dirname "$manifest")/src/droidplug/java/src/main/java"
if [ ! -d "$javasrc" ]; then
  echo "btleplug Java sources missing: $javasrc" >&2
  exit 1
fi
echo "btleplug java: $javasrc"
mkdir -p "$main/java"
cp -R "$javasrc/." "$main/java/"
cp "$here/btleplug.pro" "$module/btleplug.pro"

# same build type dx chose: `assembleRelease` only when `[bundle.android]` configures signing,
# otherwise `assembleDebug` (the debug-signed APK the release assets have always been)
project="$(cd "$main/../../.." && pwd)"
if [ -f "$module/build/outputs/apk/release/app-release.apk" ]; then
  task=assembleRelease
else
  task=assembleDebug
fi
echo "repackaging with $task in $project"
cd "$project"
./gradlew "$task" --console=plain
