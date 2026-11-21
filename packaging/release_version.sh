#!/usr/bin/env bash
# Prints the version the release packages are named after.
#
#   packaging/release_version.sh
#
# On a version tag (`v0.7.0`) it is the tag without the `v`, and the tag must match `version` in
# `[workspace.package]` of `Cargo.toml`: the updater compares the tag with the compiled version,
# so a mismatch would ship a build that offers itself as its own update. On a branch (a run that
# only builds the artifacts) it is the `Cargo.toml` version: the branch name is not a version, and
# `dpkg-deb` and the macOS `Info.plist` reject one that does not start with a digit.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_version="$(awk '
    /^\[workspace\.package\]/ { in_pkg = 1; next }
    /^\[/                     { in_pkg = 0 }
    in_pkg && /^version[[:space:]]*=/ {
        gsub(/^version[[:space:]]*=[[:space:]]*"|".*$/, ""); print; exit
    }' "$root/Cargo.toml")"

if [ -z "$cargo_version" ]; then
    echo "no version in [workspace.package] of Cargo.toml" >&2
    exit 1
fi

if [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
    tag_version="${GITHUB_REF_NAME#v}"
    if [ "$tag_version" != "$cargo_version" ]; then
        echo "tag ${GITHUB_REF_NAME} does not match version ${cargo_version} in Cargo.toml" >&2
        exit 1
    fi
fi

echo "$cargo_version"
