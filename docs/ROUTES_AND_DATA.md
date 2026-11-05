# Routes and data (`ride-route` and the data folder)

How a GPX file or a bikerouter link becomes a ridable `Route`, and what the app keeps on disk. Checked against
the code on 2026-11-04.

## 1. Where a route comes from

| Source | Entry point | Network |
|---|---|---|
| GPX file (dialog, drag and drop, `Ctrl+O`, `RIDE_GPX`) | `load_route_from_path` / `load_route_from_bytes` | none; Open-Meteo only if the file has no usable elevation |
| bikerouter.de share link, brouter-web link, raw BRouter URL, bare parameters | `load_route_from_bikerouter` | one request to BRouter, then Open-Meteo only if needed |
| Built-in demo route | `demo_route()` | none |

Files over 64 MB are refused before reading. CPU-heavy work runs on tokio's blocking pool. Every error message is
Hungarian and safe to show.

### bikerouter links (`bikerouter.rs`)

Verified against `nrenner/brouter-web` (MIT) and BRouter's parameter parser. Accepted:

1. the **share link** whose state is in the URL fragment:
   `https://bikerouter.de/#map=<zoom>/<lat>/<lon>/<layers>&lonlats=<lon>,<lat>;<lon>,<lat>[;...][&straight=i,j][&nogos=...][&polylines=...][&polygons=...][&profile=<name>][&alternativeidx=<0..3>]`
   (longitude first; `;` or `|` between points; percent-encoding is decoded; `profile` and `alternativeidx`
   default to `trekking` and `0` when omitted, exactly like brouter-web; the `map=` part is ignored);
2. a fragment without `map=`, the legacy `#zoom=...&lat=...&lon=...&layer=...&lonlats=...` form, a query string
   (the fragment wins when both exist);
3. a raw BRouter API URL (`https://brouter.de/brouter?lonlats=...&format=gpx`): a path ending in `/brouter` is
   treated as the endpoint and *its host is used* (a local BRouter server works); `format` is always replaced
   by `gpx`;
4. a bare parameter string (`lonlats=19.04,47.49;19.1,47.5&profile=fastbike`).

Limits: 2 to 200 waypoints, every coordinate in the WGS-84 range, values only digits and `.,|+-eE`, parameter
length 6000. The public endpoint is `https://brouter.de/brouter` with a descriptive User-Agent, one request at a
time, timeouts and a hard size cap (`http.rs`). A route built from a link without a name is called
"Bikerouter útvonal – 29,4 km".

## 2. The pipeline (`gpx.rs`, `build.rs`, `climbs.rs`)

**Parsing** is tolerant and event based (`quick-xml`): GPX 1.0/1.1 and anything that looks like it; only local
element names matter (namespaces and prefixes are ignored, `<extensions>` are skipped); every `<trkseg>` of every
`<trk>` is joined in file order, `<rte>/<rtept>` is the fallback; `<wpt>` become waypoints; `<ele>` is optional per
point and a comma decimal (`254,5`) is tolerated; UTF-8 with or without BOM, UTF-16 with BOM, invalid UTF-8 read as
Latin-1; points without a valid position or at exactly 0°/0° are skipped and counted. **Ill-formed XML is an error**
(a silently shortened ride is worse than a clear message). A further `<trk>` that lies on the main (longest) track
and is much shorter than it, or one marked `<type>segment</type>`, is not joined: it becomes a named
**segment** of the route (a Strava segment exported and added to the file shows up as a band on the profile).
Strava's own GPX export contains no segments. BRouter's header comment (`track-length`, `filtered ascend`) is
exposed so tests can cross-check the numbers.

**Building** (`build_route`), in order:

| Step | Rule |
|---|---|
| clean | drop invalid coordinates; drop points closer than 1 m to the previous kept one (jitter); cumulative haversine distance. There is deliberately no position "outlier" heuristic: a far point is indistinguishable from the tip of an out-and-back |
| elevation check | `has_elevation = false` when fewer than 80 % of the points carry a non-zero elevation or all values are almost identical; exact `0.0` counts as unknown when most are non-zero; isolated impossible spikes (> 3 m and 35 % grade up and straight back) are repaired, gaps interpolated over distance |
| resample | uniform spacing of at most 10 m (the length is split into equal parts, so the last point sits exactly at the total) |
| smooth | 5-sample median (single-point spikes), then a ~50 m moving average whose window shrinks symmetrically at the ends (first and last elevation stay exact) |
| grade | over an ~80 m window, clamped to ±25 % |
| ascent / descent | zig-zag filter with 3 m hysteresis on the smoothed profile |
| climbs | see below |
| bounds, waypoints, id | waypoints farther than 1000 m from the route are dropped (at most 500); `id` = FNV-1a over the cleaned geometry quantised to 1e-5°, so the same track gives the same id from a file or from BRouter |
| timeline | when **every** cleaned point has a `<time>` and the span is 10 s to 24 h, the stamps become `Route::timeline` (elapsed seconds per resampled point; rewinds are clamped); otherwise empty. It makes a Strava/Garmin export its own ghost before the first ride |

Routes shorter than 20 m or longer than 5 000 km are rejected.

**Missing elevation** (`elevation.rs`): the track is sampled about every 100 m, at most 100 coordinates per
request, from `https://api.open-meteo.com/v1/elevation` (CC BY 4.0), and the route is rebuilt. A failed lookup is
not an error: the route loads with `has_elevation = false` (a flat profile).

**Climbs** (`climbs.rs`): a sample whose 80 m grade is at least 2 % is "climbing"; consecutive samples form runs;
runs separated by less than 150 m merge; the ends snap to the real foot and summit within 80 m without crossing a
neighbour. A climb must be at least **500 m** long, average at least **3 %** and gain at least **25 m** (`gain` is
the net difference between foot and summit, so `avg_grade = gain / length`; `max_grade` is the steepest 80 m).
Category by `length [m] × average grade [%]`: ≥ 8 000 cat 4, ≥ 16 000 cat 3, ≥ 32 000 cat 2, ≥ 64 000 cat 1,
≥ 80 000 HC; a smaller climb that passes the three limits has no category.

**The demo route** (`demo.rs`) is deterministic closed-form geometry sent through the same pipeline: about 24 km
north-west of Budapest (Hűvösvölgy towards the Pilis), rolling hills, one 4 km climb of about 7 % from 10.5 km to
14.5 km and a long descent. Its coordinates are *near*, not on, real roads.

## 3. The data folder

One folder holds everything: `%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride` (default `~/.local/share/bike-osm-ride`), or `/data/data/<package>/files` on
Android. `RIDE_DATA_DIR` overrides it everywhere (tests, portable installs, screenshots).

| Path | Content | Notes |
|---|---|---|
| `settings.json` | `Settings` (rider, ui) | pretty JSON; written 500 ms after a change and on exit; holds the Google key |
| `recent.json` | the recent routes, newest first | at most 20; older route caches are deleted |
| `routes/<id>.json` | the processed `Route` | cache so a reopened route is not rebuilt |
| `rides/<route id>/<start>.json`, `index.json` | the ride log | never touched by the recents housekeeping; [`SIMULATION.md`](SIMULATION.md) §6 |
| `workouts/<id>.json` | imported workouts | one file each |
| `tiles/<dark\|light\|osm>/<z>/<x>/<y>.png` | map tile cache | 30 days; no bulk prefetching (OSM tile policy) |
| `streetview/<route id>.json` | panorama ids per 25 m bucket | 30 days; "no panorama" 7 days; ids, our own coordinates, heading and a timestamp, never imagery |
| `streetview/img/<route id>/` | downloaded images and `manifest.json` | only after an explicit download; shown for 30 days |
| `ble.log`, `ble.log.1` | BLE hex log | always on; rotated at start-up beyond 8 MB |
| `crash.log`, `crash.log.1` | caught panics | rotated at 256 KB |
| `strava/`, `strava-webview/` | TCX copies and the debug log; the upload WebView profile (sign-in) | desktop |
| `webview/` | the main WebView's profile | |
| `firefox-profile/` | the browser panel's own Firefox profile | Windows |

**Rules every writer follows** (`store.rs`): writes are atomic (temp file in the same folder, then rename); reads
never panic: a missing, corrupt or foreign file yields defaults or `None`; files over 64 MB are refused; ids that
become file names must be 1 to 64 characters of `[A-Za-z0-9_-]` (a path-traversal test guards this); one global
lock serialises access. When `RIDE_DATA_DIR` and the platform variables are all unusable the folder falls back to
the system temp folder. The `directories` crate is deliberately not used: it would pull in `option-ext`
(MPL-2.0) for a three-line lookup.

**Sanitising settings on load:** non-finite or non-positive rider mass, bike mass, FTP, max HR, CdA, Crr,
efficiency, air density and look-ahead are replaced by their defaults; efficiency is capped at 1; the grade scale
is clamped to 0-1.5; `text_scale()` clamps to 0.8-1.6 and treats NaN as 1.0.

**Compatibility rules.** `Settings` uses `#[serde(default)]` throughout, so a file written by an older version
loads and gains the new fields on the next save. `RideRecord` carries `version` (currently 1) and a record of an
unknown version is ignored. A route cache written before `timeline` existed still loads, and an empty timeline is
not written. New persisted fields need a test that the old shape still parses; look at the tests at the bottom of
`ride-model/src/lib.rs` for the pattern.

## 4. Settings reference (`ride-model`, `Default` impls)

**Rider** (`RiderSettings`): `rider_kg` 75, `bike_kg` 9, `ftp_w` 200, `max_hr` 190, `cda` 0.32 m², `crr` 0.004,
`drivetrain_eff` 0.975, `air_density` 1.225, `trainer_grade_scale` 1.0 (0 to 1.5), `use_trainer_speed` false.

**UI** (`UiSettings`):

| Field | Default | Meaning |
|---|---|---|
| `theme` | `Dark` | `Dark` or `Light` |
| `map_style` | `Dark` | `Dark`, `Light`, `Osm`: all three are the standard OSM raster tiles; dark and light are CSS filters on them |
| `profile_mode`, `profile_mode_workout` | `Lookahead`, none | full route or a window ahead (`lookahead_m`, 3000 m); the workout variant stays unset until the rider changes it with a workout selected |
| `profile_view` | `Workout` | what the profile panel draws while a workout is selected (elevation or workout blocks) |
| `profile_swipe_learned` | false | the phone's swipe hint stops once the rider has swiped |
| `follow_rider` | true | map follow mode |
| `streetview_enabled`, `streetview_large` | false, false | panel on; Street View big and the map small |
| `google_maps_key` | empty | the rider's own key; never in `Debug` output |
| `streetview_cache_panos` | false | remember panorama ids per route |
| `streetview_offline` | true | enables the panel's Download button and the use of downloaded images |
| `virtual_gears`, `gear` | false, 255 | the virtual gearbox; 255 = "not chosen", the default gear |
| `workout_id` | none | the selected workout; none = free ride |
| `ghost` | `Best` | `Off`, `Route`, `Last`, `Best` |
| `tiles`, `tiles_workout` | none | the tiling layout without / with a workout (a binary tree of panel keys); none = the default layout |
| `text_scale` | 1.0 | text size factor, 0.8 to 1.6 |
