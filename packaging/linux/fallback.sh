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
# Pinned upstream release asset and GTK source revision. Verify cached tools as well.
linuxdeploy_sha256="df908b19f1ed1cdafa79508d415aa12d6c56ce4d56d4149aa8c09286718706e5"
gtk_commit="7a3fbc31a9e5075073ff8790f26effbac5f84453"
gtk_sha256="b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171"
if [ ! -f "$linuxdeploy" ]; then
    curl -fsSL -H 'Accept: application/octet-stream' -o "$linuxdeploy" \
        "https://api.github.com/repos/linuxdeploy/linuxdeploy/releases/assets/699514588"
fi
printf '%s  %s\n' "$linuxdeploy_sha256" "$linuxdeploy" | sha256sum -c -
chmod +x "$linuxdeploy"
curl -fsSL -o "$tools/linuxdeploy-plugin-gtk.sh" \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/$gtk_commit/linuxdeploy-plugin-gtk.sh"
printf '%s  %s\n' "$gtk_sha256" "$tools/linuxdeploy-plugin-gtk.sh" | sha256sum -c -
chmod +x "$tools/linuxdeploy-plugin-gtk.sh"

appdir="$out/bike-osm-ride.AppDir"
rm -rf "$appdir"
install -Dm755 target/release/bike-osm-ride "$appdir/usr/bin/bike-osm-ride"
# The icon must be called what the .desktop file's `Icon=` says and sit in the icon theme folder
# (linuxdeploy looks the name up there: handed "256.png" it fails with "Could not find suitable
# icon for Icon entry"), and the .desktop file belongs in the AppDir as well.
icon="$appdir/usr/share/icons/hicolor/256x256/apps/bike-osm-ride.png"
install -Dm644 "$root/packaging/icons/256.png" "$icon"
install -Dm644 "$root/packaging/linux/bike-osm-ride.desktop" \
    "$appdir/usr/share/applications/bike-osm-ride.desktop"

# linuxdeploy wants the plugin on PATH and the GTK major version in the environment.
export PATH="$tools:$PATH"
export DEPLOY_GTK_VERSION=3
export ARCH=x86_64
cd "$out"
"$linuxdeploy" --appimage-extract-and-run \
    --appdir "$appdir" \
    --desktop-file "$appdir/usr/share/applications/bike-osm-ride.desktop" \
    --icon-file "$icon" \
    --plugin gtk \
    --output appimage

mv -f "$out/bike-osm-ride-x86_64.AppImage" \
    "$out/bike-osm-ride-$version-linux-x86_64.AppImage"
rm -rf "$appdir" "$tools"

echo "== done: $out/bike-osm-ride-$version-linux-x86_64.AppImage"
echo "== done: $out/bike-osm-ride-$version-linux-amd64.deb"
