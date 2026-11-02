#!/usr/bin/env bash
# Builds the unsigned, universal macOS application and packs it into a .dmg.
#
#   packaging/macos/bundle.sh <version> [output directory]
#
# <version> is the release version without the leading `v` (`0.2.0`); it becomes
# CFBundleShortVersionString. The output directory defaults to `<repo>/dist` and receives
# `bike-osm-ride.app` and `bike-osm-ride-<version>-macos-universal.dmg`.
#
# One universal binary serves both architectures: macOS 11 runs arm64 and x86_64, and GitHub no
# longer offers Intel runners, so an Apple Silicon machine cross-compiles the x86_64 half and
# `lipo` merges the two. The updater therefore only ever looks for a single `.dmg`.
#
# The bundle is ad-hoc signed (`codesign --sign -`), not notarised. Gatekeeper keeps it in
# quarantine after the download, so the README tells the rider to open it with a right click →
# "Megnyitás" (or to drop the quarantine attribute). The ad-hoc signature is still needed:
# without it Apple Silicon refuses to start the binary at all.

set -euo pipefail

version="${1:?usage: bundle.sh <version> [output directory]}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${2:-$root/dist}"
app="$out/bike-osm-ride.app"
dmg="$out/bike-osm-ride-$version-macos-universal.dmg"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

echo "== bike-osm-ride $version: universal macOS bundle =="

for target in "${targets[@]}"; do
    if ! rustup target list --installed | grep -qx "$target"; then
        echo "missing Rust target $target: rustup target add $target" >&2
        exit 1
    fi
done

# 1. Both halves of the binary.
for target in "${targets[@]}"; do
    echo "-- cargo build --release --target $target"
    cargo build --release --manifest-path "$root/Cargo.toml" -p ride-app --target "$target"
done

# 2. The .app skeleton.
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

lipo -create \
    "$root/target/aarch64-apple-darwin/release/bike-osm-ride" \
    "$root/target/x86_64-apple-darwin/release/bike-osm-ride" \
    -output "$app/Contents/MacOS/bike-osm-ride"
chmod 755 "$app/Contents/MacOS/bike-osm-ride"
lipo -info "$app/Contents/MacOS/bike-osm-ride"

# 3. Icon: `iconutil` only accepts the exact names/sizes of an .iconset, so the generated PNGs
#    are copied into that layout (32 → 32x32, 128 → 128x128, 256 → 256x256, 512 → 512x512,
#    1024 → 512x512@2x).
iconset="$out/AppIcon.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
cp "$root/packaging/icons/32.png" "$iconset/icon_32x32.png"
cp "$root/packaging/icons/128.png" "$iconset/icon_128x128.png"
cp "$root/packaging/icons/256.png" "$iconset/icon_256x256.png"
cp "$root/packaging/icons/512.png" "$iconset/icon_512x512.png"
cp "$root/packaging/icons/1024.png" "$iconset/icon_512x512@2x.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$iconset"

# 4. Info.plist (version from the tag) and the classic type/creator record.
sed "s/__VERSION__/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

# 5. Ad-hoc signature: mandatory on Apple Silicon, tolerated everywhere else.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

# 6. The disk image (compressed, read-only, no extra files beside the app).
rm -f "$dmg"
hdiutil create \
    -volname "bike-osm-ride $version" \
    -srcfolder "$app" \
    -fs HFS+ \
    -format UDZO \
    -ov \
    "$dmg"

echo "== done: $app"
echo "== done: $dmg"
