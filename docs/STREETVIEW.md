# Street View (`ride-ui-streetview`)

An optional panel inside the map tile that shows Google Street View at the rider's position, looking along the
road. It works **only with the rider's own Google Maps API key**, and it is built around Google's terms, which
forbid storing or prefetching imagery but allow storing panorama ids. Checked against the crate on 2026-11-04.

> **Status: verified only against a local mock** (`scripts/streetview_mock.mjs`) and unit/integration tests; never
> against real Google endpoints. Two metadata parameters (`radius`, `source`) are not confirmed in Google's
> documentation.

## 1. What the rider sees

- The panel sits **inside the map tile**: small at the bottom right of the map by default, `V` or the header
  button toggles it, **`X`** (or the swap button) exchanges the two so Street View is big and the map small
  (the small view takes a fixed 38 % × 44 % corner of the tile). In the small slot the map is a rider minimap at
  street zoom.
- Without a key it explains what to do and opens *Beállítások → Street View*, where the steps for creating a key
  are written (Google Cloud project, billing, enable **Maps Embed API** and **Street View Static API**, restrict the
  key to those two).
- Errors are Hungarian and specific: invalid or denied key, quota exceeded, no panorama here.
- With the offline option (default on, see below) the panel header has a **Letöltés** button.

## 2. How it works

| Piece | File | Does |
|---|---|---|
| metadata client | `client.rs` | `GET https://maps.googleapis.com/maps/api/streetview/metadata?location=<lat,lng>&radius=50&source=outdoor&key=...`: JSON only (status, pano id, position, date), body capped at 64 KB, at most 6 requests in flight; free of charge |
| live view | `url.rs`, `panel.rs` | the panorama in Google's own **Maps Embed API** iframe `https://www.google.com/maps/embed/v1/streetview?key=&pano=&heading=&pitch=&fov=` (plus the coordinate, so a vanished id falls back to the nearest panorama); two iframes crossfade so there is no flicker |
| planner | `planner.rs` | decides *when* a new panorama is needed and where the camera looks (heading along the route), and keeps the visible state from flickering; pure, time passed in |
| id cache | `cache.rs`, `prefetch.rs` | per route, panorama ids per **25 m bucket** (`BUCKET_M`), resolved about 1.5 km ahead of the rider in the background; `<data>/streetview/<route id>.json`; found ids live 30 days, "no panorama" 7 days; the file stores ids, our own route coordinates, heading and a timestamp, never imagery |
| offline images | `imagestore.rs`, `download.rs`, `image.rs` | the opt-in download described below |
| mock | `mock.rs`, `scripts/streetview_mock.mjs` | a local imitation of the three endpoints for tests and screenshots (`RIDE_STREETVIEW_BASE`) |

### The offline download

Because Google's policy forbids bulk prefetching and persistent storage of imagery, **nothing is downloaded unless
the rider presses Letöltés** in the panel header (the Settings switch only enables the button and the use of
downloaded images; its default is on). A download:

- takes one image per `step_for(route length)` = length / 900 clamped to 75-250 m and rounded up to 5 m (so a route
  up to 67.5 km gets an image every 75 m, a longer one at most about 900 images), via the Static API image endpoint (640 × 640 px, FOV 90°, pitch 2°, at most 2 MB each), at most
  4 in flight and about 8 starts per second, cancellable, with a progress line under the header;
- stores them as `<data>/streetview/img/<route id>/<bucket>.jpg` plus a `manifest.json`; they are served to the
  webview by the **`svimg` protocol** (path-validated, `GET` only, `no-store`) and expire after **30 days**;
- uses the rider's own Google quota and billing; the Settings page shows the storage and has a "Törlés" button.

A downloaded image wins over the iframe only when there is **no key**, or the live embed fails (network, quota).
With a key and a network the sharp, full-resolution embed is shown, because the downloaded image is at most
640 px and looks soft on a big panel. A route with a full download rides with **no key and no network at all**.

## 3. The key

- It lives in `settings.json` on that machine only (`UiSettings::google_maps_key`).
- It is a newtype (`ApiKey`) whose `Debug` is redacted, and `UiSettings`' `Debug` prints `<elrejtve>`; no error
  value carries text from a URL or the network. A test (`tests/key_safety.rs`) and the unit tests guard that the key
  never appears in `Debug`, errors or logs.
- It is only ever sent to `maps.googleapis.com` and `www.google.com`, or to a **loopback** mock chosen with
  `RIDE_STREETVIEW_BASE` (anything not loopback is ignored, so the key cannot be redirected).
- Key syntax is checked locally (length up to 256) before anything is sent.

## 4. Policy summary (why it is shaped like this)

Google offers Street View only through its APIs. The terms (Maps Platform terms and the Street View policies) allow
**panorama ids** to be cached (ids change over time, so the cache also stores a coordinate and a timestamp, and the
embed gets the coordinate beside the id), and restrict storing or prefetching **imagery**. The user asked for a
"download images" switch; it was implemented as the explicit, per-route, expiring download above, with the
decision and the responsibility stated in the settings text. The app is not a Google product.

## 5. Testing without Google

```powershell
node scripts/streetview_mock.mjs 9300 [--delay=250] [--gap]       # then start the app with:
$env:RIDE_STREETVIEW_BASE = "http://127.0.0.1:9300"
```

The mock serves the three endpoints with the same paths and parameter names, shows the pano id, heading, pitch
and FOV in the embed page so the plumbing can be seen, and answers special keys: `BADKEY` (denied), `QUOTAKEY`
(over the limit), `HTTP500KEY`, `GARBAGEKEY`. `--gap` removes the panorama of every third 0.01° of longitude.
`/__stats` counts the requests, which proves that a repeat ride with cached ids and downloaded images makes none.
`cargo run -p ride-ui-streetview --example offline_demo -- 48` downloads the first 48 images of the demo route
through the mock. Tests: `tests/offline_flow.rs`, `tests/panel_flow.rs`, `tests/key_safety.rs` and the unit tests in
every module (83 in the library).

## 6. Known nits

At 1024 × 640 the stats pane is tight and a large Street View panel overlaps the map's scale bar. The metadata
call's `radius` and `source` parameters are unconfirmed. The User-Agent strings of the HTTP clients still say
`bike-osm-ride/0.1`.
