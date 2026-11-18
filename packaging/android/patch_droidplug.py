#!/usr/bin/env python3
"""Patches btleplug's droidplug `Peripheral.java` after `resoverlay.sh` copied it into the
Gradle project.

Why this exists (docs/BLUETOOTH.md §7): a link lost while idle (no command in flight) left
the `BluetoothGatt` open, and `disconnect()` was a no-op once the `connected` flag had
dropped. Android then served the next `connect()` through `gatt.connect()` as a
*background auto-connect with no timeout of its own* — which can complete minutes later,
after the app had already given up. The phone ends up GATT-connected to a trainer that
stopped advertising while the app shows the connect as failed: exactly the "the trainer
connected itself to the phone and the app never noticed" report.

The two changes (search for "bike-osm-ride" in the patched file):
  1. `disconnect()` closes a stale `gatt` when the link is already down;
  2. an idle link loss, and a mid-command disconnect no command handled, close the `gatt`.

Upstream: btleplug 0.13.5, MIT OR Apache-2.0 (droidplug is btleplug's own file), pinned by
`Cargo.lock`; origin https://github.com/deviceplug/btleplug. The changes are marked inline
and listed in the app's `NOTICE.md`.

The script is idempotent and fails loudly (non-zero, no write) when an anchor is missing
or ambiguous, so a droidplug version bump that moves the patched lines breaks the build
instead of silently shipping the unfixed source.

Usage: python3 patch_droidplug.py <path-to-Peripheral.java>
"""

import sys
from pathlib import Path

# Pre-patched file marker: the header below is only prepended once.
MARKER = "bike-osm-ride patch of btleplug"

HEADER = """\
// bike-osm-ride patch of btleplug 0.13.5's droidplug Peripheral.java (MIT OR Apache-2.0,
// upstream: https://github.com/deviceplug/btleplug; the changes are listed in the app's
// NOTICE.md, marked "bike-osm-ride" inline, and applied by packaging/android/patch_droidplug.py):
//  1. disconnect() closes a stale gatt when the link is already down,
//  2. an idle link loss (no command in flight), and a mid-command disconnect no command
//     handled, close the gatt. Without these Android serves the next connect() through
//     gatt.connect() as a background auto-connect with no timeout, which can complete
//     after the app gave up and leave the phone connected to a trainer that stopped
//     advertising.
"""

# (anchor, replacement, what it is) — the anchor must occur exactly once.
PATCHES = [
    (
        """\
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        Peripheral.this.wakeCommand(future, null);
                    } else {
""",
        """\
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        // bike-osm-ride: a disconnect on a link the app has already lost must
                        // still drop a stale gatt, or the next connect() takes the
                        // gatt.connect() background auto-connect branch, which has no
                        // timeout and may complete after the app gave up.
                        if (this.gatt != null) {
                            this.gatt.close();
                            this.gatt = null;
                        }
                        Peripheral.this.wakeCommand(future, null);
                    } else {
""",
        "disconnect() closes a stale gatt",
    ),
    (
        """\
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.dispatchToCommandCallback("onConnectionStateChange",
                            () -> Peripheral.this.commandCallback.onConnectionStateChange(gatt, status, newState));
                }
                suppressDisconnectNotification = Peripheral.this.suppressDisconnectNotification;
""",
        """\
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.dispatchToCommandCallback("onConnectionStateChange",
                            () -> Peripheral.this.commandCallback.onConnectionStateChange(gatt, status, newState));
                    // bike-osm-ride: a command in flight that did not close the gatt itself
                    // (connect's callback only handles failures) leaves one behind on a
                    // mid-command disconnect: close it, the link is down either way.
                    if (newState == BluetoothGatt.STATE_DISCONNECTED && Peripheral.this.gatt != null) {
                        Peripheral.this.gatt.close();
                        Peripheral.this.gatt = null;
                    }
                } else if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                    // bike-osm-ride: an idle link loss used to leave the gatt open; Android
                    // then served the next connect() through gatt.connect() as a background
                    // auto-connect with no timeout, which can complete after the app gave up
                    // and leave the phone connected to a trainer that no longer advertises.
                    if (Peripheral.this.gatt != null) {
                        Peripheral.this.gatt.close();
                        Peripheral.this.gatt = null;
                    }
                }
                suppressDisconnectNotification = Peripheral.this.suppressDisconnectNotification;
""",
        "an idle or unhandled disconnect closes the gatt",
    ),
]


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <path-to-Peripheral.java>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.is_file():
        print(f"no such file: {path}", file=sys.stderr)
        return 2
    # Read/write bytes: the file keeps its exact line endings and BOM-free UTF-8.
    text = path.read_bytes().decode("utf-8")
    if MARKER in text:
        print(f"already patched: {path}")
        return 0
    for anchor, replacement, what in PATCHES:
        count = text.count(anchor)
        if count != 1:
            print(
                f"anchor for ({what}) found {count} times, expected 1 — the droidplug "
                "version in Cargo.lock probably changed; review and update "
                "packaging/android/patch_droidplug.py",
                file=sys.stderr,
            )
            return 1
        text = text.replace(anchor, replacement)
    path.write_bytes((HEADER + text).encode("utf-8"))
    print(f"patched: {path} ({len(PATCHES)} changes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
