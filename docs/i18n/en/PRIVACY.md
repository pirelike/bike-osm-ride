# Privacy and network traffic

> **Translation.** The authoritative version of this document is the Hungarian [privacy page](../../PRIVACY.md);
> it is also available in [Spanish](../es/PRIVACY.md).

bike-osm-ride **has no account, no telemetry, no analytics and no advertising**. Your data (rides, settings, sensor data)
stays on your own machine; the program only calls over the network what you request. This page lists exactly what can go
out and what is on your machine. The statements are based on the source code (2026-11-04, version 0.3.0).

## What leaves your machine, and when

| Address | When | What it receives | Note |
|---|---|---|---|
| `tile.openstreetmap.org` | when the map requests tiles | the tile's coordinates (zoom, x, y) and the program's name in the User-Agent (`bike-osm-ride/<version> (desktop app or Android)`) | Tiles are fresh for 30 days; stale offline copies stay on disk up to 90 days / 512 MiB total, with cleanup at startup and hourly during use. |
| `brouter.de` | if you paste a bikerouter link | the points read from the link (longitude, latitude), the profile and the avoidances | the route is calculated by their server |
| `api.open-meteo.com` | only if the GPX has no usable elevation data | the coordinates of the route's sample points (at most 100 per request) | CC BY 4.0, for non-commercial use |
| `api.github.com` | on start, five seconds later | the latest release query, `User-Agent: bike-osm-ride/<version>` | it goes quiet silently if it fails; **it cannot be switched off in the settings** |
| `github.com` (download) | only if you press the *Update* button | the download of the new package | |
| `maps.googleapis.com`, `www.google.com` | only if you have switched on Street View and given an API key | the rider's position (coordinate) and **your own** Google key | Google's terms apply; see [`STREETVIEW.md`](../../STREETVIEW.md) |
| the pages opened in Firefox | only if you use the Browser panel (Windows) | what you look at in it | a separate Firefox profile, your normal browser is untouched |

There is nothing else. There is no sign-in to the program, no crash-report sending, no usage statistics. Bluetooth traffic
only goes between the trainer, the sensors and the machine.

## What is on your machine

Everything is in a single folder (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride`, on Android the application's private folder). The structure is described by
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); in short:

- **settings** (`settings.json`): body weight, FTP, max. heart rate, appearance, layout, your Google API key;
- **routes and ride log**: the processed version of the opened routes, the complete per-second recording of every
  finished ride (position, watts, heart rate, cadence);
- **tiles**, the downloaded **Street View images** (for at most 30 days) and panorama ids;
- **logs**: `ble.log` (the raw data packets of the trainer and the sensors: watts, heart rate, control commands; they
  contain no device name or address) and `crash.log` (caught errors); both rotate and stay on the machine;
- the Browser panel's Firefox profile;
- the remains of the Strava upload of version **0.3.1 and older**: the `strava` and `strava-webview` folders (the latter
  has the cookies of your Strava sign-in). The program no longer uses them and does not delete them: feel free to
  delete them.

**Backup.** *Settings → Backup* creates a single zip file of your routes, your rides, your imported workouts and your
settings. The file goes where you save it (on Android with the system's “Save as” dialog); the program sends nothing
anywhere and does not upload it either. The Google API key **is not in it**, and on restore it does not accept the key
coming from the file either; the tiles, the Street View images and the logs are not in it either. The file contains the
exact routes of your rides and your health data (heart rate, watts), so handle it like your other personal files.

**Secrets.** The only secret is the Google API key. It sits in `settings.json` *as plain text* (not encrypted), so
protect the folder like your other personal files. The program never logs it, it does not end up in an error message and
does not appear in the debug output, and it only goes out to Google (or to a local test server at `127.0.0.1` during
development).

**Deletion.** Deleting the folder makes everything disappear. The Street View images and ids can also be deleted
separately under *Settings → Street View*; the ride log of a route removed from the recent routes list stays.

### Imported ride files

**Import a ride** reads a GPX or TCX file you choose (or drag in). The program processes the file on your own machine,
and only saves the result (the per-second recording: position on the route, elevation, speed, heart rate, cadence, watts)
into the `rides` folder, just like your own rides. The file's original content (for example the name and timestamps
written by Strava) goes nowhere else, and nothing goes out to the network. The calculation of the estimated watts also
happens locally.

## Things worth knowing

- OpenStreetMap, BRouter, Open-Meteo, GitHub and Google see your own requests (IP address, time); this is how it is with
  any network client.
- There is currently no setting to turn off the update check. If you do not want it, block `api.github.com` with a
  firewall; the program does not break because of it.
- The program is not a Google or Zwift product, and has no connection with them.
