# bike-osm-ride — in English

> **Translation.** The authoritative version of this document is the Hungarian [`README.md`](../../../README.md).
> The screenshots show the original (Hungarian) interface; the app speaks Hungarian, English and Spanish.
> The manual and the privacy policy are also available in English: [user guide](USER_GUIDE.md) ·
> [privacy policy](PRIVACY.md).

**Language:** [Hungarian](../../../README.md) · English · [Spanish](../es/README.md) · [Spanish user guide](../es/USER_GUIDE.md) · [Spanish privacy policy](../es/PRIVACY.md)

<p align="center">
  <img src="../../../packaging/icons/128.png" width="96" alt="bike-osm-ride icon">
</p>

<p align="center">
  <b>Virtual riding on your own GPX routes, with a smart trainer – or without one.</b><br>
  Map, elevation profile, workouts, ghost, TCX export. Fast, modern, 2D. Rust.
</p>

<p align="center">
  <a href="https://github.com/pirelike/bike-osm-ride/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/pirelike/bike-osm-ride?display_name=tag&sort=semver"></a>
  <img alt="Licence: MIT or Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue">
  <img alt="Platforms" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-informational">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white">
  <img alt="Interface: Hungarian, English and Spanish" src="https://img.shields.io/badge/interface-Hungarian%20%7C%20English%20%7C%20Spanish-brightgreen">
</p>

<p align="center">
  <img src="../../screenshots/ride-dark.png" alt="The ride screen in the dark theme: map, power, elevation profile, stats" width="100%">
</p>

**Jump to:** [Download](#download) · [Quick start](#quick-start) · [What it does](#what-it-does) · [Screenshots](#screenshots) ·
[Devices and status](#devices-and-status) · [Documentation](#documentation) · [Development](#development) · [Licence](#licence)

## Why this?

- **Your route.** Not a pre-built world: you can ride any GPX or bikerouter link, with real elevation and climbs,
  and the trainer follows the route's gradient.
- **Every number that matters.** Power (3 s average, W/kg, zone), cadence, heart rate, speed, gradient,
  NP/IF/TSS, elevation gain, ETA: in large, readable numbers, without 3D.
- **Workouts and analysis.** 93 built-in workouts with sources, in ERG mode, with coach messages; ghost and local
  segments with PR; TCX export.
- **It stays yours.** No account, no telemetry, your data is on your machine ([privacy](PRIVACY.md)). Open
  source, under a permissive licence.
- **It works without hardware.** With the simulated rider you can try everything without Bluetooth.

## Download

The latest release: **[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**.
In the file names, `<version>` is the release number (for example `0.3.0`).

| Platform | File | Installation |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<version>-windows-x64.exe` | no installer: run it. It needs the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (Windows 11 has it by default). The program is not digitally signed, so SmartScreen may stop it. |
| **Linux** (x86-64, glibc 2.35+) | `bike-osm-ride-<version>-linux-x86_64.AppImage` | `chmod +x`, then run it; it brings WebKitGTK and GTK with it |
| | `bike-osm-ride-<version>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<version>-linux-amd64.deb` (it asks for the `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `bluez` packages) |
| **macOS 11+** (Intel and Apple Silicon) | `bike-osm-ride-<version>-macos-universal.dmg` | drag it into *Applications*; on the first start, **right click → Open → Open** (it is not notarised); on the first Bluetooth scan, allow Bluetooth |
| **Android 7+** | `bike-osm-ride-<version>-android.apk` | copy it to the phone and open it (you have to allow installation from unknown sources), or `adb install -r` |

The program **tells you** when there is a new release: the *Update* button appears in the top bar (on Windows and
as an AppImage it replaces itself, on the other platforms it opens the release page). Android releases require a stable release signing key; missing credentials fail the release build. Uninstall an older debug-signed build before installing the release APK.

## Quick start

1. **Start it.** The *Route* window opens: drag a GPX onto it, paste a bikerouter link, or choose the
   *Demo route*.
2. **Connect the trainer** (*Devices* → *Start a scan* → in the *Trainer* role), or choose the
   **Simulated rider** if you have no hardware at hand. A heart-rate monitor, a power meter, a cadence sensor and a Zwift Click/Play can
   also be connected.
3. *(Optional)* **Workout**: the top bar's *Workout* button (`W`), choose from 93 workouts, or import your own.
4. **`Start`** (`Space`). The trainer follows the climbs (or holds the workout's target watts), and the numbers come alive.
5. At the end, save a TCX from the **summary** (Strava, Intervals.icu and TrainingPeaks all accept it).

The full description: **[User guide](USER_GUIDE.md)**.

## What it does

**Routes**
- GPX file (browse, drag and drop, `Ctrl+O`), bikerouter/BRouter link (the profile, the alternative and the avoidances
  come across too), recent routes, built-in demo route.
- Tolerant GPX reader (BOM, namespaces, several segments, `<rte>`, a comprehensible message for a broken file); missing
  elevation is filled in by Open-Meteo. Climb detection from 4th category to HC; segment tracks put in the file (for
  example a Strava segment) show up as bands.

**Riding**
- Realistic physics (Martin et al. 1998: gravity, rolling resistance, bearing friction, spoked aerodynamic drag,
  inertia); the trainer gets the gradient of the section ahead of you (up to ±20%, with a strength of 0–150%).
- **FTMS trainer** (climbs and ERG), **power meter** (CPS), **speed/cadence** (CSC), **heart-rate monitor** (HR); one
  physical device can fill *several roles* on a single connection; **Zwift Click/Play** virtual gearbox; **simulated rider**.
- **Virtual gearbox**: 2 × 12 drivetrain, 23 gears, modifies the trainer's resistance.

**Workouts and analysis**
- 93 built-in workouts in 12 categories, with tags, a zone-coloured block chart and 943 timed coach messages; import of
  MyWhoosh and Zwift workouts (`.zwo`, XML, JSON).
- Coggan zones, NP, IF, TSS, kcal; heart-rate zones; per-second recording.
- **Ghost** (last ride, best, or the GPX's own timestamps) and **local segments with PR**.
- **Review and comparison of rides**: the rides made on the same route (at most four) side by side by power, heart
  rate, cadence, speed and time difference; zoom into a section (by dragging or from the climbs), section table.
  **Import of a GPX or TCX ride** from a Strava or Garmin export, with heart rate and cadence; if it has no watts, the
  simulation's physics estimates them backwards (with your own or the given body weight, bike and rolling resistance).
- **Power page**: peak power from 5 s to 60 minutes, an FTP suggestion from your best 20 minutes (settable with one
  button), weekly and monthly load. Only measured watts count.
- **Backup**: the routes, the rides, the imported workouts and the settings in a single zip file (without the Google key),
  with a merging restore. Nothing goes out to the network.
- **TCX export** (Garmin schema): Strava, Intervals.icu and TrainingPeaks accept it when uploaded by hand. There is no
  built-in upload: the Strava sign-in would have had to be repeated after every ride, so we removed it (0.3.2).

**Interface**
- **Tiling layout**: the panels work like in a tiling window manager (no overlap, no gaps); they can be resized, moved
  and closed by dragging; a separate layout with and without a workout.
- Dark and light theme (both fully fledged), text size 80–160%, **Hungarian, English and Spanish interface**
  (*Settings → Appearance → Language*, or automatically following the system language), embedded fonts.
- **Phone and tablet**: a tabbed view; the “Ride” tab has everything you need on a ride.
- Optional **Street View** (with your own Google key, offline download) and a **Browser panel** (Windows, embeds the
  installed Firefox).
- Keeps the machine awake while you ride (Windows, Linux, macOS).

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="../../screenshots/ride-light.png" alt="Light theme"><br><sub><b>Light theme</b>: the paper-white theme is on equal footing with the dark one</sub></td>
    <td width="50%"><img src="../../screenshots/workout.png" alt="Workout in ERG mode"><br><sub><b>Workout in ERG mode</b>: workout card, live charts, block chart</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/workout-picker.png" alt="Workout picker"><br><sub><b>93 workouts</b> with filters and a zone-coloured block chart</sub></td>
    <td><img src="../../screenshots/summary.png" alt="The ride summary"><br><sub><b>Summary</b>: NP, IF, TSS, segments, zones; TCX export</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/devices.png" alt="Devices window"><br><sub><b>Devices</b>: one device in several roles</sub></td>
    <td align="center">
      <img src="../../screenshots/phone-ride.png" width="170" alt="Phone, Ride tab">
      <img src="../../screenshots/phone-map.png" width="170" alt="Phone, Map tab"><br>
      <sub><b>On a phone</b>: Ride and Map tabs</sub>
    </td>
  </tr>
  <tr>
    <td><img src="../../screenshots/review.png" alt="Reviewing rides"><br><sub><b>Review</b>: two rides on one climb, the second with estimated watts</sub></td>
    <td align="center">
      <img src="../../screenshots/review-phone.png" width="170" alt="Phone, review">
      <br><sub><b>On a phone</b>: Rides, Map, Chart, Section tabs</sub>
    </td>
  </tr>
</table>

<sub>The screenshots show a real BRouter route (the last 12 km of the Hűvösvölgy–Dobogókő route that is in the repository
as test data), with the simulated rider. Route and map: © OpenStreetMap contributors, ODbL. The images are produced by
`node scripts/readme_shots.mjs`.</sub>

## Devices and status

**Not every item has been tried with real hardware or a real service.** Honestly:

| Item | Status |
|---|---|
| FTMS smart trainer, **ERG** (workout) | tested, **also with a real trainer** (one model: VE200) |
| FTMS **climb simulation** | thoroughly tested (with a simulated transport, using the specification's byte vectors); **not yet confirmed with a real trainer** |
| Heart-rate monitor, separate power meter, CSC sensor | the decoders are tested; the trainer's CPS/CSC data is real; not yet tried with a separate sensor |
| Zwift Click / Play | implemented (unofficial), not yet tried with a real controller |
| Simulated rider | works without hardware, on every platform |
| Windows | in daily use |
| Linux, macOS | CI builds them and (on Linux) runs a 20-second smoke test; no check on a real machine yet (experience welcome) |
| Android | the interface has run (the 0.3.0 bugs were found while running it); the system file picker is implemented; **Bluetooth has not been tried on a device yet** |
| Street View | implemented and tested with a local test server; not yet tried with the real service |

Details: [`docs/TESTING.md`](../../TESTING.md) and [`docs/BLUETOOTH.md`](../../BLUETOOTH.md). We welcome bug reports,
experiences, and compatible or incompatible devices in Issues (attaching the `ble.log` is worthwhile).

## Documentation

| For | What |
|---|---|
| **Users** | [User guide](USER_GUIDE.md) · [Privacy and network traffic](PRIVACY.md) · [Changelog](../../../CHANGELOG.md) · [Hungarian](../../../README.md) · [Spanish](../es/README.md) |
| **Developers** | [Architecture](../../ARCHITECTURE.md) · [Development guide](../../DEVELOPMENT.md) · [Testing](../../TESTING.md) · [Releasing](../../RELEASING.md) · [Decision log](../../DECISIONS.md) · [Translations (i18n)](../../I18N.md) |
| **Topics** | [Bluetooth](../../BLUETOOTH.md) · [Simulation and physics](../../SIMULATION.md) · [Routes and data formats](../../ROUTES_AND_DATA.md) · [Interface](../../UI.md) · [Design system](../../DESIGN.md) · [Android](../../ANDROID.md) · [Street View](../../STREETVIEW.md) · [Prior art and reused code](../../PRIOR_ART.md) |

The complete index: [`docs/README.md`](../../README.md).

## Development

```powershell
cargo run -p ride-app                      # debug build and start (Windows, Linux, macOS)
cargo test --workspace                     # the full test suite (2026-11-06: 1076 tests)
node scripts/readme_shots.mjs              # regenerate the screenshots
```

Rust workspace with ten library crates and the application (`ride-app`):

```
crates/
  ride-model         common data model (Route, Settings, RideSnapshot, Workout, …) – the “contract”
  ride-route         GPX/bikerouter → Route, climbs, persistence, ride log
  ride-sim           physics, RideEngine, workout runner, ghost, segments, TCX (without I/O and a clock)
  ride-ble           BLE hub (FTMS/CPS/CSC/HR/Zwift) + simulated rider
  ride-ui-kit        design system: tokens, palette, icons, formatting
  ride-ui-map        tiled map · ride-ui-profile  profile and charts
  ride-ui-hud        instruments, cards, controls       · ride-ui-dialogs  dialogs
  ride-ui-streetview Street View panel
  ride-app           the shell: state, controller, tiling, tile proxy, updater, Android
packaging/  scripts/  tests/fixtures/  docs/
```

The code is built on Dioxus 0.7 (webview-based interface) and `btleplug`. On Windows development happens on the GNU
toolchain (the release exe is built by CI with MSVC). Prerequisites, switches (`RIDE_*`), debugging, packaging:
[`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md). The project description for Claude Code: [`CLAUDE.md`](../../../CLAUDE.md).

**Contributing.** We welcome bug reports, measurement logs and patches. The rules in short: only code with a permissive
licence may be ported (see below), the source of the user-facing text is Hungarian (every UI string is a key in
`crates/ride-i18n/locales/hu.json`, with the translations beside it; see [`docs/I18N.md`](../../I18N.md)), we do not
panic on external data, and a new feature comes with a test and documentation. Submitted code comes in under the
project's licence (MIT or Apache-2.0) unless you state otherwise.

## Licence

### The project's own code

The code of `bike-osm-ride` itself is **dual-licensed: [MIT](../../../LICENSE-MIT) OR [Apache-2.0](../../../LICENSE-APACHE)**,
as you prefer (SPDX: `MIT OR Apache-2.0`). If you contribute, the submitted code comes in under these same two licences
unless you state otherwise.

### Open-source code reused

We deliberately use only permissively licensed code. The origin and the licence are preserved in the source files and in
[`NOTICE.md`](../../../NOTICE.md).

| Source | Licence | What we use it for |
|---|---|---|
| [`ftms`](https://github.com/deancochran/ftms) crate | MIT | encoding and decoding Bluetooth FTMS (smart trainer) data and control commands |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | interpreting power-meter, cadence and heart-rate sensor data (built in, marked in the source files) |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | trainer connection and command safety filter (ported parts, marked) |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | heart-rate monitor scanning and reconnecting (pattern taken, no code, marked) |
| [Barlow and Barlow Condensed](https://github.com/jpt/barlow) fonts | SIL OFL 1.1 | the interface's text and numerals; **embedded** in the program (subsetted WOFF2, Latin + Latin Extended, covering Hungarian), so they work without a network; the licence text: [`crates/ride-ui-kit/assets/fonts/OFL.txt`](../../../crates/ride-ui-kit/assets/fonts/OFL.txt), details in [`NOTICE.md`](../../../NOTICE.md) |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense (public domain) | the Zwift Click/Play and Zwift Hub BLE protocol (rewritten in Rust): the `zwift` module of `ride-ble`, details in [`NOTICE.md`](../../../NOTICE.md) |
| [`p256`](https://github.com/RustCrypto/elliptic-curves), [`hkdf`](https://github.com/RustCrypto/KDFs), [`sha2`](https://github.com/RustCrypto/hashes), [`aes`](https://github.com/RustCrypto/block-ciphers), [`ccm`](https://github.com/RustCrypto/AEADs) (RustCrypto) | MIT / Apache-2.0 | the Zwift Click key exchange (ECDH P-256, HKDF-SHA256) and AES-CCM encryption |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0 (the program **does not include it**, it launches the copy installed on the machine) | the Browser panel: the installed Firefox window is embedded, with a separate profile |
| [Dioxus](https://dioxuslabs.com), [btleplug](https://github.com/deviceplug/btleplug), [tokio](https://tokio.rs), [quick-xml](https://github.com/tafia/quick-xml) and the other Rust dependencies | mostly MIT / Apache-2.0 | the complete, automatically generated list (638 packages): [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md) |

It served only as a **reference** (without taking code): one numerical value from a worked example of the open-source
Martin implementation called `rouleur` in the physics test, [`nrenner/brouter-web`](https://github.com/nrenner/brouter-web) (MIT)
for checking the bikerouter link format, and Makinolo's reverse-engineering notes on the Zwift Click/Play protocol as
documentation: <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>. This is unofficial interoperability
work: we have no connection to Zwift, the program is not a Zwift product, and Zwift does not support it.

We have **not** used code from GPL/AGPL-licensed projects (for example OpenCycling, suffice, GoldenCheetah, QDomyos-Zwift),
because that would put the whole program under copyleft; the research details:
[`docs/PRIOR_ART.md`](../../PRIOR_ART.md).

**Licence audit (2026-11-04, `cargo metadata --filter-platform`, normal and build dependencies).** The number of external
(crates.io) crates per platform: Windows (GNU) 396, Linux 458, macOS (Apple Silicon) 400, Android (aarch64) 449. Among them
there is **no GPL, AGPL or LGPL, and no unknown licence**; the permissive licences are MIT, Apache-2.0, BSD, ISC, Zlib,
Unicode, CC0, Unlicense and the like.

**The only copyleft type is MPL-2.0**, which is **file-level**, and we take the crates unmodified from crates.io (no
vendored code); their source is available in the repositories given in [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md):

- on Linux and macOS `option-ext` (through the `dioxus-desktop → tray-icon → dirs` chain);
- on Android `selectors`, `cssparser`, `cssparser-macros` and `dtoa-short` (through the `wry → kuchikiki` chain);
- on Windows none of them.

This does not affect the project's own licence: our own code remains `MIT OR Apache-2.0`. `Cargo.lock` contains every
platform's dependencies; regenerating the list: `.\scripts\third_party_licenses.ps1`, the per-platform audit commands are
in point 9 of [`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md).

### Maps and data sources (attribution)

The application calls external services while it runs; their data may be used under their own terms, and the application
displays the required attributions on the map (the complete list of calls: the [privacy policy](PRIVACY.md)):

- **Map data:** © [OpenStreetMap](https://www.openstreetmap.org/copyright) contributors, [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Tiles:** the standard [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles) tiles (`tile.openstreetmap.org`). The *Dark* and
  *Light* map styles filter the same tiles locally (CSS filtering), so no API key or third-party style server is needed.
  Downloaded tiles are cached locally to spare the servers ([OSM tile usage policy](https://operations.osmfoundation.org/policies/tiles/));
  there is no bulk prefetching.
- **Street View (optional):** [Google Maps Platform](https://mapsplatform.google.com) services: the *metadata* endpoint of the
  Street View Static API (JSON only: status, panorama id, position), the image endpoint of the Static API (only for the offline
  download you request) and the Street View view of the Maps Embed API. The program uses **your own** API key (your Google Cloud
  account, your quota and billing); the key is only on your machine, in `settings.json`, is never logged, and is only sent to
  `maps.googleapis.com` / `www.google.com`. The [Google Maps Platform terms](https://cloud.google.com/maps-platform/terms) and the
  [Street View policies](https://developers.google.com/maps/documentation/streetview/policies) apply to the use. The program
  **does not download images by default**: only when you press the *Download* button in the panel does it save images for the route
  on your machine for at most 30 days; it shows them only without a key or when there is a network error, and otherwise the live,
  full-resolution view is shown. Permanent storage of Street View images is restricted by Google's terms, so this is your decision
  and responsibility; the download uses your own Google account's quota. The policies allow storing *panorama ids* (these may change
  over time, so the coordinate and the timestamp are kept beside them, and the embedded view also receives the coordinate next to
  the id). This program is not a Google product and has no connection with Google.
- **Workout descriptions:** the built-in workout library is our own text, edited from public training protocols (British Cycling,
  TrainingPeaks, CTS, FasCat, TrainerRoad and others) and training-science studies (Rønnestad, Seiler, Helgerud, Billat, Gibala,
  Bossi, Karsten and others); the sources used appear per workout in the `sources` field of
  `crates/ride-model/assets/workouts.json`.
- **Route planning:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de) (public service; we only call it if you
  paste a bikerouter link, and the route is calculated by their server). The `tests/fixtures/brouter_*.gpx` files are BRouter
  responses, from OSM data (ODbL), and the same route appears in the README screenshots.
- **Filling in missing elevation data (optional):** [Open-Meteo](https://open-meteo.com) Elevation API,
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), for non-commercial use.

Our own licence does not cover that data and those services.

### Legal notice

The software is available “as is”, without warranty; the details are in the [LICENSE-MIT](../../../LICENSE-MIT) and the
[LICENSE-APACHE](../../../LICENSE-APACHE) files. Device control (the automatic adjustment of the trainer's resistance) is at
your own risk.
