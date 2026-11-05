# Decision log

The decisions that shaped bike-osm-ride, with the reason and what they cost. Reconstructed on 2026-11-04 from the
code comments, the commit messages and the notes kept during development; dates are when the decision first shows
up in those sources. The project was built between 2026-10-31 and 2026-11-03 at a fast pace, and these reasons were
not written down at the time. Add a new entry whenever a choice is made that someone could reasonably undo.

Format: **what** · *why* · consequence / where it lives.

## Product

**P1. Rust everywhere, no web stack** (2026-10-31). The user's first requirement ("Rustot használj"). The UI is
rendered by a webview, but written in Rust (Dioxus); there is no TypeScript. · The toolchain is one language;
the cost is Dioxus' youth (0.7) and a Rust-only UI ecosystem.

**P2. 2D only: map, elevation profile, everything the trainer sends** (2026-10-31). Fast, modern, no 3D world.
· No renderer to maintain; the elevation profile and the numbers carry the experience.

**P3. All user-visible text is Hungarian** (2026-10-31). The rider is Hungarian. · Errors that can reach a toast
are written in Hungarian even in low-level crates (`ride-ble`, `ride-route`); code and docs stay English.

**P4. Reuse open source, translate it to Rust, keep the licence visible** (2026-10-31). The user wants existing
trainer code reused ("átfordítjuk mindenképpen rustra") and the licence always in the README. · Only permissive
code is copied (MIT, Apache-2.0, BSD, ISC, Zlib, CC0, Unlicense); GPL/AGPL projects (OpenCycling, suffice,
GoldenCheetah, QDomyos-Zwift, FTMS.NET) are read for ideas only. Vendored code carries a header and a `NOTICE.md`
entry. See [`PRIOR_ART.md`](PRIOR_ART.md), `NOTICE.md`, the README `Licenc` section.

**P5. The project is `MIT OR Apache-2.0`** (2026-10-31). The Rust ecosystem's convention, and compatible with all
reused code. · The copyright line is "bike-osm-ride contributors" because no name was given.

**P6. A review page that compares the rides of one route** (2026-11-05). The user wanted to review rides, compare
them on the same route, zoom into sections and see cadence and heart rate within a ride. · A full-screen layer over the ride
screen, not a dock panel or a dialog: it needs the whole window and must not reload the map. The comparison axis is the
distance along the route, which every ride on that route already has.

**P7. Another app's activity is *projected onto a stored route*, not matched by geometry hash** (2026-11-05). Two GPS
recordings of the same road never share a hash. The user picks the route (the one shown on the page) and the track's points
are projected onto it (30 m tolerance, monotone search), with a warning below 90 % coverage. · All rides of a route then share
one distance axis, one elevation profile and one grade. Formats: GPX (with sensor extensions) and TCX; FIT is binary and needs
a reader whose licence would have to be audited, so it was left out. This is a file the user hands over, not an API: it does
not reopen the Strava question of V2.

**P8. Imported and simulated rides are never a PR or a ghost** (2026-11-05). An outdoor ride has stops, coasting and
traffic; a trainer ride does not, and the Strava file's time is not the trainer's time. `best_times` and `choose_ghost` skip
both. · A person who wants an outdoor ride as the ghost can still pick the `Route` mode with a GPX that carries time stamps.

**P9. Estimated watts are marked, and measured watts are never replaced** (2026-11-05). A file without power gets watts
from the power model run backwards, because the comparison is useless without them; but the number is an estimate (no wind, no
drafting, the weight and Crr assumed), so it carries `~`, a tag and the parameters it was made with, and can be redone with
other values. A ride with measured watts refuses the estimate. · The user chooses whose data to use (own settings or custom
weight, bike weight, Crr); the rest (CdA, efficiency, air density) stays the rider's. The surface is only the Crr: there is no
gravel model, as the user said ("még a gravel nincsen supportálva úgyse").

## Platform and build

**B1. Dioxus 0.7 desktop over egui** (2026-10-31). Both compiled with btleplug on the development machine; the
webview (HTML/CSS/SVG) allowed the "super modern" look the user asked for, with real typography and flexible
layout. · A webview per platform (WebView2, WKWebView, WebKitGTK, Android WebView) with their quirks; no immediate
mode GUI.

**B2. The GNU toolchain on the development PC, MSVC in CI** (2026-10-31). The machine has Rust on
`stable-x86_64-pc-windows-gnu` (MinGW/WinLibs) and no MSVC, and the user did not want it installed. · The local exe
imports `WebView2Loader.dll` (`build.rs` copies it; never ship the GNU exe alone: v0.1.0 did). The release exe is
built in CI with MSVC and `+crt-static`, so it needs neither DLL nor redistributable.

**B3. One code base, `cfg`-gated differences, four platforms** (2026-11-01 to 11-03). Windows first, then Android,
Linux and macOS. · No forks; the per-platform table is in [`ARCHITECTURE.md`](ARCHITECTURE.md). Only Windows is
used daily by the maintainer.

**B4. A single data folder, `std`-only lookup** (2026-11-01). Settings, rides, tiles, logs and webview profiles
live under one folder; `RIDE_DATA_DIR` moves it. The lookup is written with `std` because the `directories` crate
brings `option-ext` (MPL-2.0). · Irony noted: `dioxus-desktop → tray-icon → dirs → option-ext` brings that crate
in on Linux and macOS anyway (unmodified, file-level copyleft, acceptable; see the README audit).

**B5. Android: `target_sdk` 34, a pinned manifest and activity, an icon overlay** (2026-11-03). Reasons and
mechanics are in [`ANDROID.md`](ANDROID.md): the NativeActivity must survive every configuration change, `dx` rewrites
its icon and theme on each bundle, and target 35 forces edge-to-edge drawing that has not been checked on a phone.

**B6. Linux on Ubuntu 22.04 (glibc 2.35), AppImage and `.deb`; macOS universal and ad-hoc signed** (2026-11-03). The
oldest baseline that still builds the WebKitGTK 4.1 stack; one universal bundle because GitHub no longer offers
Intel macOS runners and the updater wants one asset. · No notarisation: first start needs right click → Megnyitás.

**B7. Releases are cut by hand from a tag; assets have fixed names** (2026-11-01). The updater looks for exact names
([`RELEASING.md`](RELEASING.md)). · A fix after a pushed tag is a new patch version; tags are never moved.

## Bluetooth

**BT1. Unfiltered scans, classification after the fact** (2026-10-31). On WinRT, service-UUID scan filters are
unreliable (learned from the GPL project opencycling). · The hub lists everything and classifies by services, then
by name; the real services are verified after connecting.

**BT2. One connection serves every role of a device** (2026-10-31). A trainer exposes FTMS, power and cadence at
once, and a second connection to the same device usually fails. · `multi.rs`; confirmed by the capture of the
user's trainer (three streams on one link).

**BT3. Everything written to a trainer passes an allow-list filter** (2026-10-31, ported from OsmolControl).
Reset is never sent; targets are clamped. · [`BLUETOOTH.md`](BLUETOOTH.md) §4.

**BT4. Assume control when a trainer stays silent, treat repeated statuses as steady state** (2026-11-01, 0.1.1 and
after). According to the 0.1.1 commit message the user's trainer sometimes left Request Control unanswered and refused
writes with ATT `0x11` now and then; it also reports "Reset" four times a second, and read literally that meant ERG
and hills did nothing. (The capture kept from 2026-11-01 13:22 shows every Request Control answered; the silent-mode
fallback stays as a safety net for trainers that do not.) · The control session's silent mode, retries and "status
storm" rules. The hex log is always on because it is the only evidence when a trainer misbehaves at home.

**BT5. Zwift Click/Play support from public-domain `virtual-cog`** (2026-11-02). The protocol was re-implemented in
Rust from an Unlicense project and Makinolo's interoperability notes. · Unofficial, no record of a run against a real controller
(see `NOTICE.md`).

## Simulation

**S1. The full Martin et al. 1998 model** (2026-11-02), validated against SRM on the road and cross-checked with the
open-source `rouleur` implementation. · Bearing friction and spoke drag included; integrated in kinetic energy so it is
stable at standstill.

**S2. The trainer is told the grade 5-45 m ahead, slew-limited, ±20 %, scaled 0-150 %** (2026-10-31). Smooth enough to
spare the trainer, anticipating enough to feel the wall of a climb. · [`SIMULATION.md`](SIMULATION.md) §3.

**S3. A virtual gearbox is a grade offset** (2026-11-01). A smart trainer has no gears, so each gear shifts the grade
sent to it by up to about ±5 %; the ride's speed still comes from power and the real grade. · Hidden and ignored
while a workout holds ERG (user rule, 2026-11-02).

**S4. Workouts: a curated, validated library with coach messages shown inline** (2026-11-02). 93 sourced sessions,
943 timed messages; the user asked for no toast so a message is never shown twice. · Import of MyWhoosh and Zwift
files so riders can bring their own.

**S5. Segments are detected climbs; ghosts replay earlier rides or the GPX's own timeline** (2026-11-02). No online
segment database needed. · Simulated rides never become PRs.

## UI

**U1. The "head unit / printed race sheet" design** (2026-10-31, second round). Flat panels with hairlines, huge
tabular condensed numerals, one vermilion accent, dark and light equally first-class, fonts embedded (SIL OFL).
The first glass-and-blur design was dropped. · The rules are enforced by tests ([`DESIGN.md`](DESIGN.md)).

**U2. A real tiling layout, sizes adjustable, separate with and without a workout** (2026-11-01, "teljes tiling").
"Like a tiling window manager, but resizable." · A binary tree over a 96 × 64 grid ([`UI.md`](UI.md)); a panel is never
lost, only parked as a chip.

**U3. Street View inside the map tile; big/small swap** (2026-11-01). One tile, two views, `X` exchanges them.

**U4. A tabbed phone layout below 1000 px, with a head unit tab for riding** (2026-11-02). Everything needed
mid-effort on one screen; swipe pages for the profile.

**U5. Standard OSM tiles; dark and light are CSS filters** (2026-11-01). The CARTO basemaps began returning an
"API KEY REQUIRED" watermark tile. · One attribution, one usage policy, no key; a dark map is a filter, not a style.
No bulk prefetching (OSM policy). *(Some code comments still say CARTO.)*

**U6. A Firefox panel for watching a series while riding, Windows only** (2026-11-01). Embedded as a Win32 child
window of the installed Firefox with its own profile. · Cannot be drawn over; hidden during dialogs and drags.

## Services

**V1. Street View with the user's own key; ids cached, images only on request** (2026-11-01). Google forbids storing
or prefetching imagery but allows panorama ids. The requested "download images" switch became an explicit,
per-route, 30-day download. · [`STREETVIEW.md`](STREETVIEW.md).

**V2. Strava upload: tried without the API, then removed** (added 2026-11-02, removed 2026-11-05). Creating an API app
needs a paid subscription since June 2026 and a shared secret cannot ship in open source, so the upload went through a
hidden WebView on strava.com (desktop) and the share intent (Android). In use the rider had to sign in again after every
ride, which defeated the point (the suspected cause, Strava guarding against automation, was not verified). Code, the
details form of the summary and `docs/STRAVA.md` were deleted; the TCX export stays and is uploaded by hand.

**V5. The FTP suggestion and the power curve use only measured watts** (2026-11-05). The review page can show the rider's
best 5 s to 60 min and suggest 95 % of the best 20 minutes as the FTP. A simulated rider's and an estimated ride's watts are the
physics model run forwards or backwards, so an FTP from them would only confirm the model's own assumptions; they are left
out (the trend still counts their time). The suggestion is a button, never an automatic change, and it says that the
trainer's own wattage is not a power meter. Ramp tests are not recognised (no workout id in the record): the best 20 minutes
counts, test or not. · [`SIMULATION.md`](SIMULATION.md) §7b.

**V4. Backup is one local zip, a whitelist, and restore only merges** (2026-11-05). The data lives in one folder per
machine and a broken disk or a new PC lost everything. A cloud sync would need an account, a server and conflict
handling, which this app deliberately does not have, so the rider gets a file they keep themselves. The zip holds a
**whitelist** (routes, rides, imported workouts, settings), not "the folder minus exclusions", so a cache or a profile
added later never leaks in by accident. The Google key is blanked on export and ignored on import (the file is mailed
around). Restoring **never overwrites or deletes**: that makes a second restore a no-op, needs no safety snapshot, and
gives the same result on a fresh machine; the cost is that a backup cannot "roll back" newer rides. Names inside the
archive are mapped onto the whitelist and never used as paths (zip-slip), sizes are counted on read (zip bombs). ·
[`ROUTES_AND_DATA.md`](ROUTES_AND_DATA.md) §3a, [`PRIVACY.md`](PRIVACY.md).

**V3. Network only for what the rider asks** (2026-10-31). OSM tiles, BRouter links, Open-Meteo for missing elevation,
the update check, optional Google. No account, no telemetry. · [`PRIVACY.md`](PRIVACY.md).

## Resilience

**R1. A panic must never leave an event handler** (2026-11-03). On Android one escaped panic poisons dioxus-desktop's
global mutex and every later tap dies. · `guard`, `Controller::with`, `crash.rs` with a banner and `crash.log`.

**R2. Atomic writes, tolerant reads, versioned records** (2026-10-31 on). A corrupt or foreign file yields defaults;
an unknown record version is ignored. · [`ROUTES_AND_DATA.md`](ROUTES_AND_DATA.md) §3.

**R3. The app starts with no adapter and no network** (2026-10-31). The simulated rider is always there. · Every
platform must keep that property.

## Not done, on purpose

| Alternative | Why not |
|---|---|
| egui | the look the user asked for needs HTML/CSS |
| Tauri/TypeScript UI | the app must be Rust |
| copying GPL/AGPL trainer code | it would make the whole app copyleft |
| the Strava API | paid subscription, shared secret |
| the Strava upload through a hidden WebView | the sign-in had to be repeated after every ride; removed again (V2) |
| a bundled Street View image set | against Google's terms |
| CARTO map styles | require a key now |
| installing MSVC on the dev PC | the user does not want it |
| `target_sdk` 35 now | edge-to-edge needs checking on a phone |
