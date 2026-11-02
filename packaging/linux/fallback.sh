#!/usr/bin/env bash
# Best-effort replacement for `dx bundle --platform linux`.
#
#   packaging/linux/fallback.sh <version> [output directory]
#
# The workflow tries `dx bundle` first (that is the supported path). If the Dioxus Linux bundler
# cannot produce the packages, this script builds the same two files by hand:
#
#   * `bike-osm-ride-<version>-linux-amd64.deb` with `dpkg-deb` (nothing to download), and
#   * `bike-osm-ride-<version>-linux-x86_64.AppImage` with `linuxdeploy` + its GTK plugin, which
#     copies WebKitGTK, GTK 3 and their friends into the image.
#
# Both are written straight into the output directory under the names the in-app updater looks
# for (see `crates/ride-app/src/updater.rs`). Run it on Ubuntu 22.04 (glibc 2.35) so the AppImage
# still starts on older distributions.

set -euo pipefail

version="${1:?usage: fallback.sh <version> [output directory]}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${2:-$root/dist}"
mkdir -p "$out"
# absolute: the AppImage step runs from inside "$out", where a relative path would not resolve
out="$(cd "$out" && pwd)"
cd "$root"

echo "== fallback packaging: bike-osm-ride $version =="
cargo build --release -p ride-app

# ── the .deb ────────────────────────────────────────────────────────────────
debroot="$out/deb-root"
rm -rf "$debroot"
install -Dm755 target/release/bike-osm-ride "$debroot/usr/bin/bike-osm-ride"
install -Dm644 packaging/icons/256.png \
    "$debroot/usr/share/icons/hicolor/256x256/apps/bike-osm-ride.png"
install -Dm644 packaging/linux/bike-osm-ride.desktop \
    "$debroot/usr/share/applications/bike-osm-ride.desktop"
mkdir -p "$debroot/DEBIAN"
cat > "$debroot/DEBIAN/control" <<EOF
Package: bike-osm-ride
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: bike-osm-ride <noreply@github.com>
Installed-Size: $(du -sk "$debroot/usr" | cut -f1)
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, bluez
Description: Virtual rides on GPX routes with a smart trainer
 Rides a GPX route on a map with an elevation profile while a Bluetooth LE
 smart trainer follows the hills. Hungarian user interface.
EOF
dpkg-deb --build --root-owner-group "$debroot" \
    "$out/bike-osm-ride-$version-linux-amd64.deb"
rm -rf "$debroot"

# ── the AppImage ────────────────────────────────────────────────────────────
tools="$out/tools"
mkdir -p "$tools"
linuxdeploy="$tools/linuxdeploy-x86_64.AppImage"
if [ ! -x "$linuxdeploy" ]; then
    curl -fsSL -o "$linuxdeploy" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x "$linuxdeploy"
fi
curl -fsSL -o "$tools/linuxdeploy-plugin-gtk.sh" \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh"
chmod +x "$tools/linuxdeploy-plugin-gtk.sh"

appdir="$out/bike-osm-ride.AppDir"
rm -rf "$appdir"
install -Dm755 target/release/bike-osm-ride "$appdir/usr/bin/bike-osm-ride"

# linuxdeploy wants the plugin on PATH and the GTK major version in the environment.
export PATH="$tools:$PATH"
export DEPLOY_GTK_VERSION=3
export ARCH=x86_64
cd "$out"
"$linuxdeploy" --appimage-extract-and-run \
    --appdir "$appdir" \
    --desktop-file "$root/packaging/linux/bike-osm-ride.desktop" \
    --icon-file "$root/packaging/icons/256.png" \
    --plugin gtk \
    --output appimage

mv -f "$out/bike-osm-ride-x86_64.AppImage" \
    "$out/bike-osm-ride-$version-linux-x86_64.AppImage"
rm -rf "$appdir" "$tools"

echo "== done: $out/bike-osm-ride-$version-linux-x86_64.AppImage"
echo "== done: $out/bike-osm-ride-$version-linux-amd64.deb"
