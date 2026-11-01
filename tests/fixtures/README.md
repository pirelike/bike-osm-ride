# Test fixtures (ride-route)

GPX files used by `crates/ride-route` (unit tests and `tests/fixtures_pipeline.rs`).
Do not edit them by hand: the tests quote exact point counts and numbers from them.

| File | Origin | Purpose |
|---|---|---|
| `brouter_budapest_short.gpx` | BRouter server response (`brouter.de`, BRouter 1.7.11-beta, profile `trekking`), Budapest, 3.0 km | small real BRouter file with its header comment (`track-length`, `filtered ascend`) |
| `brouter_budapest_dobogoko.gpx` | BRouter server response, fetched once with `curl` on 2026-10-31: `https://brouter.de/brouter?lonlats=18.9805,47.5390\|18.8939,47.7188&profile=trekking&alternativeidx=0&format=gpx` (Hűvösvölgy to Dobogókő, 29.4 km, 933 points, header: `track-length = 29404 filtered ascend = 747 plain-ascend = 432`) | longer hilly route with real climbs; ascent/climb verification |
| `rte_wpt.gpx` | hand-made | `<rte>/<rtept>` route (no `<trk>`), `<wpt>` waypoints (one far away, one with an XML entity in the name), `<metadata><name>` |
| `multi_segment.gpx` | hand-made | one `<trk>` with three `<trkseg>`, Garmin `TrackPointExtension` data (must not leak into `<ele>`), `<time>` elements |
| `no_elevation.gpx` | hand-made | track without any `<ele>` (flat profile, optional Open-Meteo lookup) |
| `broken_truncated.gpx` | hand-made | download cut off in the middle of an attribute (must give a clear error, not a short route) |
| `bom_namespaced.gpx` | hand-made | UTF-8 BOM, CRLF line ends, prefixed namespace (`<gpx:gpx>`, `<gpx:trkpt>`) |

## Licensing

* The hand-made files are part of this project and available under the project licence
  (`MIT OR Apache-2.0`, see `LICENSE-MIT` / `LICENSE-APACHE`). Their coordinates are
  synthetic (near, not on, real roads).
* The two `brouter_*` files are routing results derived from OpenStreetMap data:
  (c) OpenStreetMap contributors, [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/);
  routing by [BRouter](https://brouter.de); elevation values come from BRouter's own
  elevation data. They are included only as test data.
