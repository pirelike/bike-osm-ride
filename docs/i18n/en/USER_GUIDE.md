# User guide

> **Translation.** The authoritative version of this document is the Hungarian [user guide](../../USER_GUIDE.md);
> it is also available in [Spanish](../es/USER_GUIDE.md). The interface language can be changed under
> *Settings → Appearance → Language*.

Using bike-osm-ride from the first start to the ride summary. Installation and the short overview are in the
[README](../../../README.md); the developer details are in [`ARCHITECTURE.md`](../../ARCHITECTURE.md) and its companions.
The screenshots were taken of version 0.3.0 and show the interface in its source language, Hungarian; the app also
speaks English and Spanish. The route shown in the document is a real BRouter route that is in the repository as
test data (© OpenStreetMap contributors, ODbL).

**Contents:** [First start](#first-start) · [Loading a route](#loading-a-route) · [Connecting devices](#connecting-devices) ·
[Riding](#riding) · [The screen and the layout](#the-screen-and-the-layout) · [Workouts](#workouts) ·
[Virtual gearbox](#virtual-gearbox) · [Ghost and segments](#ghost-and-segments) ·
[Summary and TCX](#summary-and-tcx) · [Street View](#street-view-optional) ·
[Phone and tablet](#phone-and-tablet) · [Settings](#settings) · [Keyboard shortcuts](#keyboard-shortcuts) ·
[Troubleshooting](#troubleshooting)

## First start

On start, the **Route** window opens above the empty ride screen. There are three ways to begin: a GPX file, a
bikerouter link or the demo route. Without hardware you can also ride with the **Simulated rider** (see
[Connecting devices](#connecting-devices)).

![The Route window](../../screenshots/route-dialog.png)

## Loading a route

- **GPX file.** By browsing (`Open a GPX`, `Ctrl+O`) or by dragging it onto the window. Any GPX works: the program uses
  the `<trk>`/`<trkseg>` points (several segments are joined one after another), and if there are none, the `<rte>`
  points; it also understands a file with accents, a BOM and namespaces; for a broken (truncated) file it gives a
  comprehensible error message, not a shortened route. The size limit is 64 MB.
- **Bikerouter link.** Plan a route on [bikerouter.de](https://bikerouter.de), click the *Share* link, and paste it into
  the field (`Paste`, then `Load a route`). The points, the profile, the alternative and the avoidances all come across
  from the link; the route is calculated by BRouter's server. Raw BRouter addresses and `lonlats=…` parameters also work.
- **Demo route.** A short, hilly Budapest loop to try it out (its coordinates run *near* the roads, not on them).
- **Recent routes.** The last 20, with data (distance, elevation, when you opened it, source); the bin removes it from the
  list (the ride log stays).

If the GPX has no (usable) elevation data, the program asks the Open-Meteo service for it (the coordinates go out); if
that also fails, the route loads with a flat profile and warns. From the file:

- if beside the main track there is a *shorter* track that lies on the route (for example a Strava segment embedded in
  it), or a track marked `<type>segment</type>`, that becomes a **segment**: it shows as a band on the profile, and
  during a ride “Segment: … X km to go” indicates it (Strava's own GPX export contains no segment);
- if *every* point has a timestamp (a Strava or Garmin export), the route is **also its own ghost**: see the
  [Ghost](#ghost-and-segments) chapter.

The program looks for **climbs** in the route: sections at least 500 m long, with an average of at least 3% and gaining
at least 25 m of elevation, with a category from 4th to HC (by length × average gradient).

## Connecting devices

The **Devices** button in the top bar (three dots next to it show how many roles are connected). There are five roles:
**Trainer** (smart trainer, FTMS), **Heart rate**, **Power meter** (CPS), **Cadence sensor** (CSC) and **Shifter**
(Zwift Click/Play).

![The Devices window](../../screenshots/devices.png)

1. `Start a scan`: the program scans without filtering and classifies the devices by the services they advertise (the
   scan stops on its own after 30 seconds).
2. Next to each found device are the buttons for the roles: you connect it in the role you need. One physical device can
   fill **several roles at once**: if your trainer also broadcasts power, cadence or heart rate, a single connection
   fills every card (the cards show a *shared device* mark). Of the sensor data the more accurate source wins (power:
   the power meter before the trainer; cadence: CSC, then the power meter, then the trainer; heart rate: the chest strap
   before the trainer).
3. What has not updated for 3 seconds switches to “—”; on a dropped connection the program retries three times.
4. **Connect the trainer in the Trainer role**: the power-meter role only reads; control (climbs, ERG) comes from the
   Trainer role.
5. **Simulated rider**: the first item in the list, always there, even without Bluetooth. On the left panel you can set
   the watts with a slider (or `↑`/`↓`, ±10 W); quick buttons: Z2 150 W, Z4 250 W, Sprint 700 W, Free 0 W. Cadence and
   heart rate follow it believably.

**How does it control the trainer?** On a climb the trainer gets the gradient of the section ahead of you (about 5–45 m
ahead, smoothed, up to ±20%, scaled to 0–150% with the *Gradient strength* setting). On a pause and at the end the
trainer goes to a flat, neutral resistance. In a workout (ERG) it holds the target watts. The trainer receives the
commands through a *safety filter*: only the necessary operations go out, and the targets are clipped to the trainer's
own range.

> **Important, honestly.** We have tried ERG control with a real trainer (VE200); we have not yet confirmed climb
> simulation with a real trainer, and we have not yet tried the Zwift Click/Play controller, the separate heart-rate
> monitor and the power meter with real hardware. The details: [`TESTING.md`](../../TESTING.md).

## Riding

`Start` (or `Space`) starts the ride, `Pause` stops it (time stops too), `Finish` closes it. At the end of the route the
ride **ends on its own**. Before starting, you can jump to any point of the route by clicking on the profile; during a
ride this does not work. The program keeps the machine awake while you ride (Windows, Linux, macOS; not on Android).

The speed comes from the measured **watts**, with the Martin model validated against road measurements (gravity, rolling,
bearing friction, air resistance with spokes, inertia), from your body weight, the bike's weight, the CdA and the Crr;
with the *Use the trainer's speed* switch you can also use the trainer's own speed. Details:
[`SIMULATION.md`](../../SIMULATION.md).

![Riding, dark theme](../../screenshots/ride-dark.png)

## The screen and the layout

The ride screen consists of **panels**: *Map*, *Power* (the big number, 3 s average, W/kg, zone), *Next climb*, *Stats*,
*Elevation profile*, *Controls*, *Charts*, *Workout*, *Simulated rider*, *Gears*, *Browser*. They work like in a tiling
window manager: **there is no overlap and no gap.**

- **Resizing:** by dragging the divider (the line snaps to the cells of a 96 × 64 grid, respecting the panels' minimum
  size; only the two neighbouring panels change).
- **Moving:** grabbing a panel by its header and dropping it on another panel's *edge* splits that panel in two;
  dropping it on the *middle* swaps their places (a ghost frame shows what will happen).
- **Closing:** the panel's × button; its neighbour fills its place, and the panel moves into the top bar as a *chip*.
  Clicking the chip returns it to where it was; dragging it onto the surface puts it where you drop it.
- **Everything is saved.** *Settings → Layout → Reset the layout* restores the default.
- The program **stores the layout (and the profile's view) separately with and without a workout**: the selected workout
  has its own layout, which follows the free one until you change it there.

![Light theme](../../screenshots/ride-light.png)

**Map.** The map shows the standard OpenStreetMap tiles; the *Dark* and *Light* styles filter them locally, *OSM* is the
original. Zoom with the wheel around the mouse, pan by dragging; the buttons at the bottom left turn on follow mode and
centre it (`M`). The route's colour indicates the gradient (cold: descent, grey: flat, yellow–orange–red–purple:
increasingly steep climb), and the rider is the disc marked with an arrow. At the bottom, the OSM attribution and the
scale.

**Elevation profile.** `Full` (the whole route) or `Look-ahead` (the next ~3 km; its length can be set in the settings,
`P`); the climbs as flags with their category, the km scale at the bottom; click to jump (before a ride), move the mouse
to read data. The look-ahead scale only switches when it has to. If a workout is selected, the **Elevation | Workout**
switch shows the workout's blocks.

**Text size.** *Settings → Appearance → Text size* (80–160%).

**Language.** *Settings → Appearance → Language*: *Automatic* (follows the device's language when its text is
complete), *Magyar*, *English*, *Español*. The switch takes effect immediately; it does not rename routes and rides
you named earlier (those stay as you named them).

## Workouts

The **Workout** button in the top bar (or `W`). The built-in library has **93 workouts** in 12 categories (recovery,
endurance, tempo, sweet spot, threshold, VO2max, anaerobic, sprint, climbing, race, test, mixed), including the 20- and
8-minute FTP tests, ramp and CP tests, and many sessions that follow the protocols of published research (Rønnestad
30/15, Seiler 4 × 8, Helgerud 4 × 4, Billat 30/30, Gibala's sprint intervals and others; the sources are per workout in
the `sources` field of `workouts.json`).

![The workout picker](../../screenshots/workout-picker.png)

- On the left are categories and **tags** (what it is good for, level, character, period; they filter with AND, with the
  number of matches beside them), in the middle the list, on the right the description (what, why, how), the
  **zone-coloured block chart** and the block-by-block list with time, target and coach instruction.
- The targets are calculated from **%FTP**: set your FTP under *Settings → Rider*; if you change it, the remaining part
  is recalculated immediately.
- During a ride the trainer holds the target watts in **ERG mode**; the *Workout* card and the profile panel's *Workout*
  view show the block, the time remaining, the next block and the **coach message** (at the right moment of the workout,
  for 12 seconds, only there, not as a pop-up). Pause/exit also sets the trainer to neutral here.
- **Your own workout:** *Import a workout…* reads a MyWhoosh or Zwift file (`.xml`, `.zwo`) and MyWhoosh's JSON export as
  well; it goes into the `workouts/` folder, and from then on it can be selected like the others.
- During a ride the workout cannot be changed.

![Workout in ERG mode](../../screenshots/workout.png)

## Virtual gearbox

*Settings → Trainer → Virtual gearbox.* 2 × 12 drivetrain (50/34 and 11–34), **23 gears**. You shift with the `←`/`→`
keys, from the Gears panel, with the shift buttons on the phone or with a Zwift Click/Play controller. Since a smart
trainer has no gearbox, the gear modifies the gradient sent to the trainer (the easiest about −5%, the hardest about
+5%); the panel shows the gear's ratio and the speed that follows from the cadence. **During a workout (ERG) the gearbox
is not shown and has no effect** (the target watts decide). With a Zwift Click/Play controller, the first shift switches
the gearbox on; the Click's `+` button shifts towards harder, `−` towards easier; on the Play's right controller the
`Shift`/`Y`, on the left controller the `Shift`/`B` button (this has not yet been tried with a real controller).

## Ghost and segments

Every finished ride **is saved automatically** with its route (per-second recording, summary, segment times), and on the
same route an earlier ride can be replayed as a **ghost**: it waits at the start line before the start, during a ride a
translucent marker on the map and the profile shows where it was when you are here; the lead or deficit is shown in
seconds. *Settings → Map and profile → Ghost*:

| Mode | What it replays |
|---|---|
| Off | nothing |
| Route time | the loaded GPX's own timestamps (with a Strava/Garmin export you can race before the first ride; BRouter routes have no time) |
| Last ride | the route's last ride |
| Best (PR) | the **complete** ride with the smallest time; if no ride has finished yet, the one that got furthest (partial), which stops at its own end |

The line below the selector tells you why there is no ghost right now. The recognised climbs are **local segments**:
before the start the best time (PR) is shown, riding within it the difference measured to the second, and in the summary
the “earlier PR” and the “New PR!” badge. A ride done with the **simulated rider** is saved, but it will not be a PR and
will not be a “Best” ghost.

## Summary and TCX

At the end of the route (or after `Finish`) the **summary** opens: distance, time, elevation, average and normalised
power, IF, TSS, energy, average speed, heart rate, the segment table and the distribution of the power and heart-rate
zones.

![The summary](../../screenshots/summary.png)

At the bottom the `Save as TCX` button (Garmin schema; the suggested file name is made from the date and the route name)
and `New route`.

**Uploading to Strava, Intervals.icu, TrainingPeaks:** there is no built-in upload. On Strava's own site, in the *Upload
file* menu, you can upload the saved TCX by hand; Intervals.icu and TrainingPeaks also accept it. Up to 0.3.1 there was a
background uploader, but in our case the sign-in had to be repeated after every ride, so we removed it (0.3.2).

## Review and comparison of rides

The **`Review`** button (top bar; on the phone, at the bottom of the `Route` window, *Review saved rides*) opens a
full-screen page. `Esc` or `Back` closes it; below it the ride screen stays unchanged.

![The review page](../../screenshots/review.png)

**List.** On the left the route (in the selector every route with a saved ride, and the one currently loaded) and its
rides, newest first. Select at most four: each gets a letter (A–D) and a colour, and the line pattern on the chart
differs too (A is continuous). **A is the reference ride**: the *Time difference* chart and the table's *Difference*
column are relative to it (positive = slower). The bin asks for confirmation before deleting. By default the route's two
most recent rides are selected.

**Chart.** *Power*, *Heart rate*, *Cadence*, *Speed* or *Time difference* as a function of distance. If you move the
mouse (or your finger) over the profile or the chart, the rides' values at the cursor can be read in the legend, and a
dot on the map shows the place. If a ride has no such sensor (for example, no heart-rate monitor), it has no line.

**Zooming into a section.** Drag the mouse (or your finger on a touchscreen) along the profile or the chart, or click the
*climb* buttons; `Whole route` restores it. The map zooms into the section and highlights it, and the chart and the table
narrow to the same section. A drag shorter than 100 m does not count as a section.

**Section table.** Per ride: time, average speed, average and normalised power (NP, from at least 30 seconds of data),
average and maximum heart rate, cadence, climb, VAM and the difference from the reference ride. There is data only for a
section that the ride **covered in full**; a partial ride is empty on the other sections.

### Power: peaks, FTP, load

![The power page](../../screenshots/review-power.png)

At the top of the review, the **Comparison | Power** switch takes you to the other page. Here are the numbers of **all**
your rides, independently of the route:

- **Peak power.** The best average watts for 5 seconds, 1, 5, 20 and 60 minutes, with the ride's date and route. If you
  have selected rides in the Comparison, their values stand beside it in columns (A–D). A ride has no value for a
  duration longer than it was.
- **FTP.** It suggests 95% of your best 20-minute power as FTP, and shows the currently set one beside it. The **Set the
  FTP to N W** button writes it into the Settings; the app never changes it by itself. The TSS of saved rides stays
  calculated with the FTP of their own time. For an exact suggestion you need a real 20-minute best effort: the *FTP
  test 20 minutes* workout (*Workouts*, `W`) gives exactly that.
- **Load.** The last 12 weeks or months (*Weekly* | *Monthly*): its columns show the TSS, the value above it, and the
  tooltip (standing on the column) shows the number and time of the rides.

**Only measured watts count.** The simulated rider and rides with estimated (`~`) watts do not go among the peaks and the
FTP suggestion (in the trend's time they do, in its TSS they do not). An imported ride in which there was real power
counts. The trainer's own watts are not a power meter, so the FTP suggestion is only as accurate as that. The numbers of
older rides are calculated and saved on first opening, which can take a few seconds with many rides.

### Importing a ride from another app

With the **`Import a ride`** button (or by dragging the file onto the page) you load a GPX or TCX file, for example one
exported from Strava, Garmin or Wahoo. Heart rate, cadence and (if present) watts come across. Things to know:

- **You provide the route**: the ride is fitted to the route selected on the page (load or select it first). The points
  of the trace are projected onto the route; what falls more than 30 m from it (a detour, a GPS error) is left out, and
  the page prints the number left out. If the ride covers only a part of the route, it warns.
- The **automatic pause** (a long stop) does not count in the time. A file without timestamps (for example a planned
  route) cannot be imported as a ride.
- The same ride (the same start time) cannot be imported twice.
- The imported ride **will not be a PR and will not be a ghost**: an outdoor ride with stops and freewheeling cannot be
  compared with your trainer times. In the list the `Imported` tag marks it.
- On the phone the import is not available yet. We do not read the FIT format; Strava exports both GPX and TCX.

### Estimated watts

If the file **has no power data**, the program runs the simulation's physics backwards: from the speed and the gradient
it calculates how many watts were needed (the change in kinetic energy and the work against gravity, rolling, bearings
and the air in a 9-second window). On such a ride the numbers get a **`~`** sign and an `Estimated watts` tag is shown.

With the slider icon (*The estimate's settings*) you can set whose data to calculate with:

| Choice | What it uses |
|---|---|
| **My own data** | the Settings' body weight, bike and Crr |
| **Custom values** | the *body weight*, *bike* and *rolling resistance (Crr)* given here, if you are looking at someone else's ride |

CdA, drivetrain efficiency and air density always come from the Settings. Crr on good asphalt is about 0.004; **there is
no separate model for gravel**, there only the given Crr modifies it. `Recalculate` calculates with the new values and
saves the ride; the program never overwrites measured watts with an estimate.

**How accurate is it?** An approximation. It does not know wind or drafting (a headwind looks like fewer watts), the
gradient comes from the route, not the file's noisy GPS elevation, and in sharp bends the projection can give a noisy
speed for a short time. Calculating backwards with the program's own physics returns the lap to within about 3%; we have
not yet compared it with a real power meter, so it is worth counting on a 10–15% difference.

## Street View (optional)

Google only allows this through its API, so **your own Google Maps API key** is needed (*Settings → Street View*; the
steps are described there: Google Cloud project, billing, enabling the *Maps Embed API* and the *Street View Static
API*). Without a key nothing appears. `V` or the map panel's `Street View` button brings a small window into the bottom
right corner of the map; the `X` (or the swap button) swaps the two views. On errors a comprehensible message comes
(invalid key, exhausted quota, no panorama at this place). The **Download** button saves one image every 75–250 m for the
route (at most about 900; valid for 30 days, deletable in the Settings), so next time you can ride the same road without
a key and without a network. The download uses your own Google quota. Details and Google's rules:
[`STREETVIEW.md`](../../STREETVIEW.md).

## Phone and tablet

In a window narrower than 1000 px a tabbed view appears: at the bottom **Ride · Map · Charts · Data**, above them the
large `Start`/`Pause`/`Finish` button row. On the **Ride** tab everything you need during a ride is on one screen: the
workout's block (time remaining, next block, the coach's message), power measured against the target (*on target / low /
high*), the zone bar, cadence, heart rate, speed, gradient, below it the profile (dragging sideways switches between its
views). The dialogs are full-screen on the phone.

<p align="center">
  <img src="../../screenshots/phone-ride.png" width="280" alt="Phone layout, Ride tab with a workout">
  &nbsp;&nbsp;
  <img src="../../screenshots/phone-map.png" width="280" alt="Phone layout, Map tab">
</p>

The Android application is the same program as an APK. You load a route with the system file picker, with a bikerouter
link, from the recent ones or with the demo route; on first start it asks for the Bluetooth permissions; during a ride
the screen stays on (in a pause and at the end of the ride the system's own timeout applies again; on a phone we have not
yet tried it, [`ANDROID.md`](../../ANDROID.md)). If the application gets an error, a red bar shows its first line, and
the program remains usable.

## Settings

Every change takes effect immediately and is saved.

![The Settings window](../../screenshots/settings.png)

| Group | What you set |
|---|---|
| **Rider** | body weight (75 kg), the bike's weight (9 kg), FTP (200 W), max. heart rate (190), CdA (0.32 m²), Crr (0.004) |
| **Trainer** | gradient strength (0–150%), use the trainer's speed, virtual gearbox |
| **Appearance** | language (automatic/Hungarian/English/Spanish), text size (80–160%), theme (dark/light), map style (dark/light/OSM) |
| **Map and profile** | the map follows the rider, profile view (look-ahead/full), look-ahead length, ghost |
| **Layout** | reset the default |
| **Backup** | *Save to a file* and *Restore from a file* (see below) |
| **Street View** | show, the Google key, save panorama ids, download images, delete the stored data |

### Backup

*Save to a file* creates a single **zip file** of your routes, your saved rides, your imported workouts and your settings.
You choose the file (on Android with the system's “Save as” dialog), and nothing goes out to the network. **It does not
contain** the Street View key, the map tiles, the downloaded Street View images or the logs: these are either secrets or
can be downloaded again at any time.

*Restore from a file* **merges**: the backup's routes, rides and workouts are added to the existing ones, **nothing is
deleted and no existing ride is overwritten**. The backup's settings, however, take effect (your Street View key stays
the old one), so the app asks first. Restoring the same file twice does nothing. When moving to a new machine, simply
restore the file. During a ride you cannot make a backup or restore one; if the file is broken or from a newer version,
the app says so and writes nothing. On Android this part has not yet been tried on a phone.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `Space` | start / pause / resume (at the end, bring back the summary) |
| `M` | map follow mode on/off |
| `P` | elevation profile: full ↔ look-ahead |
| `T` | dark/light theme |
| `V` | Street View panel on/off |
| `X` | swap the map and Street View |
| `W` | workout picker |
| `←` / `→` | virtual gearbox: easier / harder |
| `↑` / `↓` | the simulated rider's power ±10 W |
| `F11` | full screen |
| `Esc` | close the window, leave full screen |
| `Ctrl+O` | open a GPX |

While typing in a text field the keys do not trigger a command (except `Esc`, `F11`, `Ctrl+O`); with a dialog open only
`Esc`, `F11` and, in the Route window, `Ctrl+O` work.

## Browser panel (Windows only)

The **Browser** panel embeds the **Firefox** installed on the machine into the window, so you can watch a series or
anything else during a ride. Firefox uses its own profile (your sign-ins stay, your normal Firefox is untouched). On Mac
and Linux the panel does not appear. When a window opens or a panel is dragged, Firefox hides temporarily (the interface
cannot draw over a native window); closing the panel closes Firefox too; it should be at least about 500 px wide.

## Troubleshooting

| Symptom | What to do |
|---|---|
| The trainer does not react to the climbs / the workout | In the **Devices** window it should be connected in the *Trainer* role (the power-meter role only reads). Look at the BLE log (`ble.log`, as described in §11 of [`BLUETOOTH.md`](../../BLUETOOTH.md)); Zwift or a phone app should not be holding the trainer. |
| It does not find the device | Turn on Bluetooth; on Windows the scan is unfiltered, so many devices show up: look at the device's roles. On Android grant the Bluetooth permissions (*Start a scan* asks again); on macOS the system asks for permission on the first scan. |
| The map is empty or grey | There is no network and no cached tile yet: the route and the numbers work anyway. |
| The route does not load | The error message says why (broken file, unsupported link, no network for BRouter). The link must contain the `lonlats=` part. |
| The profile is flat | The file has no elevation data, and Open-Meteo was not available either. Load a GPX with elevation or a bikerouter link. |
| On Linux the window is empty | The program sets the `WEBKIT_DISABLE_DMABUF_RENDERER=1` variable itself; if it is still empty, try `WEBKIT_DISABLE_COMPOSITING_MODE=1`. |
| macOS: “cannot be opened” | Right click → Open → Open (the application is not notarised); or `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`. |
| Windows: if SmartScreen warns | The program is not digitally signed, so Windows may stop it; it can be started through *More info → Run anyway*. |
| Crash / red bar | The first line is in the bar, the full log in `crash.log` (on Android `adb logcat -s bike-osm-ride`). Please report the bug with the log. |

**Where is my data?** In a single folder, together with the settings, the ride log, the tiles and the logs (if you want a
copy, use the *Settings → Backup* button): Windows `%LOCALAPPDATA%\bike-osm-ride`, macOS
`~/Library/Application Support/bike-osm-ride`, Linux `~/.local/share/bike-osm-ride`, Android the application's private
folder. The exact structure is described by [`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); what leaves the machine, by
[`PRIVACY.md`](../../PRIVACY.md).
