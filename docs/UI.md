# UI architecture

How the screen is built: the shell, the tiling dock, the phone layout, dialogs, and the rules that keep the
CSS honest. The visual language itself (colours, type, primitives) is in [`DESIGN.md`](DESIGN.md). Checked
against the code on 2026-11-04.

## 1. Stack

Dioxus 0.7 drives the system webview (`LaunchBuilder::desktop()`, `mobile()` on Android). Everything is
components, CSS and inline SVG: the map is `<img>` tiles under one transformed layer plus an SVG route, the
charts are SVG paths, there is no canvas and no 3D. Fonts (Barlow and Barlow Condensed, OFL) are embedded as
data URIs, so the app looks the same offline.

`shell::head_html` injects, before the first paint: the theme attribute (`data-theme`), the text scale
(`--ui-scale`, `data-text`), and then the CSS in a fixed order: `ride_ui_kit::CSS` (fonts + tokens +
primitives) first, then each crate's CSS (`map`, `profile`, `hud`, `dialogs`, `streetview`), then the shell's
own. A test checks that no CSS can break out of its `<style>` element (`style_end_tags_inside_css_cannot_break_out`).

## 2. State and data flow

- `AppState` (`ride-app/src/state.rs`) is a bundle of Dioxus `Signal`s created once. The controller is the only
  writer of the ride signals; components get `ReadSignal` props and own no business state.
- The two high-frequency signals, `snapshot` (10 Hz) and `history` (1 Hz, `Arc<Vec<RecordPoint>>`), are read **inside
  the UI crates** that display them, so the shell does not re-render at 10 Hz. Layout-relevant facts that change
  rarely (is a workout running = `erg`, is a simulated rider connected) are kept as their own signals.
- Heavy derived data (route SVG paths per integer zoom, tick lists, the ghost plan) is computed once per
  route/zoom with `use_memo` or on the blocking pool.
- Every number the rider sees comes from `ride_ui_kit::format`; a missing value is "—".

## 3. The tiling dock (`dock.rs`, `shell.rs`)

The desktop ride screen is a **tiling layout like a tiling window manager**: panels never overlap and never
leave a gap, and everything is resizable.

- **Model.** A binary tree (`ride_model::TileNode`: `Empty`, `Leaf { panel }`, `Split { rows, ratio, a, b }`)
  over a grid of `COLS × ROWS = 96 × 64` cells (`FINE = 4` times the original 24 × 16). Dividers snap to
  whole cells; each panel has a minimum size in cells (`PanelId::min_size`), and the browser needs 48 × 32.
  CSS geometry is given in percent of the grid, each rectangle one pixel larger than its share so neighbouring
  borders lie on top of each other instead of doubling.
- **Panels** (`PanelId`, stable `key()` used in the settings file and in `data-panel`): `Map`, `Profile`,
  `Controls`, `Metrics` (power hero and cells), `Climb`, `Stats`, `Charts`, `Workout`, `Rider` (simulated
  rider), `Gears`, `Browser`. Conditional ones stay in the tree but take no space while unavailable:
  `Workout` needs a selected workout, `Rider` a connected simulated rider, `Gears` the virtual gearbox on and
  no ERG, `Browser` the Windows platform.
- **Operations.** Drag a divider: only the two panels that touch it resize (nested same-direction dividers keep
  their place; a panel at its minimum pushes the next). Drag a title bar onto the edge of another panel: it
  splits that panel (a ghost frame shows the result); onto its middle: the two swap. Close with the × button:
  the neighbour takes the space and a chip appears in the top bar; pressing the chip puts the panel back
  where it was, dragging it drops it wherever released. A closed panel with no remembered place splits the
  biggest one.
- **Persistence.** `UiSettings::tiles` is the layout without a workout, `tiles_workout` the layout while a
  workout is selected (it follows the free one until the rider arranges it there); always go through
  `active_tiles()` / `set_active_tiles()` / `active_profile_mode()`. A tree read from the settings file is
  rebuilt through `Node::from_tile` (depth limit 12) and a damaged one is repaired
  (`a_damaged_settings_tree_is_repaired`), so a bad file never breaks the screen. The layout is saved
  synchronously when a drag ends (a debounced task spawned from the drag catcher would be cancelled when the
  catcher unmounts).
- **Mouse handling.** While a drag runs, `DragCatcher` is a full-screen element following the mouse, so
  nothing underneath steals events; a press that travels less than 4 px is a click. Real mouse events are
  needed to test it (the screenshot script uses DevTools `Input.dispatchMouseEvent`).
- The dock logic is pure and heavily tested (`dock::tests`: tiling invariants after every operation, divider
  arithmetic, drop targets, repair of bad trees, settings round trip).

## 4. Compact layout: phone and portrait tablet

Below **1000 px** of window width (`shell::COMPACT_BELOW_PX`) the tiling is replaced by tabs. Fixed tab bar at the
bottom: **Tekerés · Térkép · Grafikonok · Adatok**; above it the transport bar (48 px buttons: Start/Pause,
Befejezés, gear − / +, or just "Fokozat N" with a Zwift shifter).

- **Tekerés** is everything needed mid-effort on one screen (`ride_ui_hud::RideDash`, "head unit"): the workout
  block with time left and the next block and the coach message, the power against the target ("célban" /
  "kevés" / "sok"), the zone bar, four cells (cadence, heart rate, speed, grade), and the profile below.
- **Térkép** is a slim head unit, the map and the elevation pages. **Grafikonok** the live charts, **Adatok** one
  content-height column.
- The profile has swipe pages (`ride_ui_profile::swipe`): workout (ahead / full) and elevation (ahead / full), with
  dots, a one-time "húzd oldalra" hint and a nudge until `profile_swipe_learned`.
- Under 700 px the top bar drops the logo (the app icon plus the name, see `DESIGN.md` § Icons; the route length would not fit), dialogs are full screen, the workout
  picker becomes two pages (Lista | Részletek) with categories as a strip and the tag filters behind a button;
  the footer is a two-column grid. The top bar and the tabs are padded by `env(safe-area-inset-*)`.
- At 1000 px and above, even on a landscape phone or tablet, the tiling layout is used.
- Text scale (Beállítások → Megjelenés, 80-160 %) multiplies every `--t-*` type token through `--ui-scale`;
  from 115 % `data-text="large"` lays the head unit's four cells out two by two.
- Language (Beállítások → Megjelenés → Nyelv) switches the catalog in force; the root component is keyed by the
  language, so the whole tree is rebuilt and every label is looked up again. The picker is "Automatic" plus
  `ride_i18n::Lang::complete()` — a language whose catalog is missing keys is not offered, and the language's own
  name is never translated.

## 5. Dialogs (`ride-ui-dialogs`)

One frame for all: `.dialog` is 560 px wide (`.dialog--wide`: 960 px) and `clamp(720px, 85vh, 960px)` high, with
a 2 px ink rule on top, a title, a × button (every dialog applies its changes immediately, so there is no
second close button; a test enforces it), a scrolling body and an optional `foot`. Esc and the backdrop close;
focus moves in, Tab is trapped, focus returns on close. `DialogKind`: `Route`, `Devices`, `Settings`, `Workouts`,
`About`, `Summary`. The route chooser opens at start-up over the empty ride screen unless a hook loads a route.
The pure helpers (link validation, sorting, date and label formatting, settings patches, zone-bar percentages)
live in `helpers.rs` with the tests; the components only render what they return.

The settings sheet has a *Biztonsági mentés* group (`BackupGroup`): *Mentés fájlba* and *Visszaállítás fájlból*. The
restore asks first (an inline two-step, like the review page's delete, because the settings of the backup replace
the current ones); both are `#[props(default)]` `EventHandler<()>`s the shell wires to `Controller::backup_save` /
`backup_restore` (`controller/backup.rs`). They sit in the group, not in the footer: the footer carries no actions.

## 5a. The review page (`ride-ui-review`, `ReviewLayer` in `shell.rs`)

Not a dialog and not a dock panel: a layer (`.shell-review`, `position: fixed` below the top bar, `z-index: 20`, under the
dialogs and toasts) that the shell mounts while `AppState.review.open` is set. The ride screen stays mounted underneath, so the
map camera and the Street View are not reloaded when it closes. While it is open `Controller::on_key` ignores every ride key
(Space must not start a ride behind it) and `Esc` closes it (a dialog on top closes first); the browser panel's native Firefox
window steps aside as it does for a dialog; a dropped file is imported instead of loaded as a route.

The state is `ReviewSignals` (open, routes, route id and `Route`, ticked stems, loaded records, busy, notes), written only by
`controller/review.rs`. The page owns just view state: metric, section (`range`), cursor, open forms. `chart.rs` is the pure part
(lines on the distance grid, y range, SVG markup, table rows, `hu()` decimal comma); `HitLayer` reports the distance under the
pointer and the section a drag swept (pointer events with `touch-action: pan-y`: a vertical swipe still scrolls the page, which
the plots otherwise blocked on a phone where they fill the screen; a sideways drag selects; a finger's tap sets the cursor and
leaves it there, since a finger has no hover); `Overview` is the whole elevation profile with the section as a band;
`DistanceChart` draws the lines in route space inside one `view_transform`. Without a cursor the legend shows each ride's mean
over the window (`Ø`; the time difference at the window's end), from `chart::window_value`. The distance axes carry `km` on their
first numeral and the chart's top rule names the unit; numerals at a plot edge are pulled inside (`is-start` / `is-end`).

`MapView` got additive props: `overlay: ReadSignal<MapOverlay>` (a highlighted section in the accent colour, ride markers, and a
camera command `fit` + `fit_seq`) with `route_path::range_path` and `range_bounds`; the ride screen passes none.

**Layout.** Desktop: list column (340 px) and a main column; every section is a panel (`.rv-panel`) with the dock's 24 px
title strip (`Edzések`, `Becsült watt`, `Térkép`, `Szakasz`, `Grafikon`, `A szakasz adatai`) but no grip, since these panels do
not move. Phone and portrait tablet: one header row (back icon, the `Összehasonlítás | Teljesítmény` switch, import icon; the top
bar already names the page) and the sections behind a four-tab `Segmented` (`Edzések`, `Térkép`, `Grafikon`, `Szakasz`), without
frames or title strips (the tab names them). The metric buttons show the unit there (`W · bpm · rpm · km/h · ±s`: the five names
do not fit one row in every language) and the chart head names the metric in full; the section table is turned on its side
(`.rv-table--by-metric`: a row per number, a column per ride) so four rides fit without sideways scrolling; the power curve hides
its bar column and puts the route under the date. The review page's `fit_seq` starts at 1, so the map fits the section once it
first has a real size (on a phone it is hidden behind a tab until then). On a phone the top bar's theme switch gives way to
`Áttekintés` (the theme is in Settings → Appearance); the route dialog still carries a link (`on_review`). Hooks are classes
(`shell-tb--theme`, `shell-tb--fullscreen`): an `[aria-label="…"]` selector with an accented value did not match in the webview.

**The power view** (`power.rs`, `PowerView`) is a second page of the same layer, switched by a `Segmented` row under the header
(`Összehasonlítás` | `Teljesítmény`, view state only; the phone's four-tab strip belongs to the comparison and is hidden on the
power page). It is fed by `ReviewSignals.analysis` (an `Arc<ride_sim::power_curve::Analysis>`, computed in `review_refresh` after
the index backfill) plus the loaded records of the comparison (their curves are computed on the spot by `mean_max`), the
current FTP and `on_apply_ftp` (`Controller::review_apply_ftp`: rounds, clamps to 50-600 W, toasts). Three cards: the curve
(a table with an accent bar, no chart geometry), the FTP card and the twelve-period trend (flat CSS columns, `rv-trend-*`).
Everything is `rv-`-prefixed and token-only like the rest of the page; `PowerView` is tested on its own with SSR.

Looking at it: `RIDE_REVIEW=1 RIDE_DATA_DIR=<folder>` after `cargo run -p ride-route --example seed_review -- <folder>`, or
`node scripts/readme_shots.mjs --only review,review-phone`.

## 6. Map and profile specifics

- **Map** (`ride-ui-map`): Web-Mercator and camera maths are pure and tested; tiles are absolutely positioned
  `<img>` in `.map__layer`, whose single `transform: translate3d(...) scale(...)` encodes the camera, so pan and
  zoom touch one attribute. The route is one SVG path per grade bucket in world-pixel coordinates at an integer
  tile zoom (memoised), with a casing; markers are projected into screen space so they never scale. Follow mode
  keeps the rider at about 60 % of the height and opens at street zoom; zoom 3-19. Every style requests the
  standard OSM tiles; dark and light are CSS filters. Attribution "© OpenStreetMap contributors" is always shown.
- **Street View** lives inside the map tile (small bottom right, `X` swaps the two), see
  [`STREETVIEW.md`](STREETVIEW.md).
- **Profile** (`ride-ui-profile`): flat grade-bucket fill, 2 px outline, km ruler, climbs as flags, the ridden part
  covered at 55 %. The look-ahead axis uses `geom::stable_y_range` (hysteresis plus one window ahead) so the scale
  does not jump while riding (an 18 km rolling test route went from 94 rescales to 3). A *Magasság | Edzés*
  switch swaps the elevation for the workout blocks.
- **Charts**: power and heart rate of the last N seconds as one flat area with a 2 px line and a zone strip;
  memoised static geometry keeps 10 Hz smooth.

## 7. Keyboard, files, theme

- **Keys** come from a small page script that forwards them to the controller (`shell::use_key_bridge`): `Space`,
  `M`, `P`, `T`, `V`, `X`, `W`, `←`/`→`, `↑`/`↓`, `F11`, `Esc`, `Ctrl+O`. Typing in a field never triggers a
  command, except `Esc`, `F11` and `Ctrl+O`; with a dialog open only `Esc`, `F11` and (on the route dialog)
  `Ctrl+O` act.
- **Files**: native open dialog (`rfd`), drag and drop onto the window (`drag_active` draws a drop hint), the
  Android system picker.
- **Theme and map style**: toggling the theme moves the map style with it (Dark ↔ Light) unless the rider
  chose the OSM style, which is kept; a `RIDE_THEME` hook is never written to disk.

## 8. The browser panel (Windows only)

`browser.rs` starts the installed Firefox with its own profile (`<data>/firefox-profile`, `-no-remote`, a
`userChrome` that hides the window buttons), finds its window and makes it a **Win32 child window** of the
app window over the panel body, clipped with `SetWindowRgn`; a script reports the panel's rectangle. A webpage
cannot draw over a native window, so the shell hides Firefox while a dialog is open or a panel is dragged.
Closing the panel closes Firefox. Elsewhere the panel does not exist (no tile, no chip, no tab).

## 9. Looking at the UI without running the app

`cargo run -p ride-ui-kit --example kit_preview` (and the `*_preview` examples of the other UI crates) render
components with `dioxus-ssr` to `target/preview/*.html` for both themes; `ride-ui-map/tests/render.rs` renders
the map markup in a test. For the real window use the DevTools tools in [`DEVELOPMENT.md`](DEVELOPMENT.md) §4 and
`scripts/readme_shots.mjs`.

## 10. Known rough edges (2026-11-04)

- At phone width with a workout selected the profile's header can clip its last label ("Következik"): the
  header has no room for four items.
- The workout layout leaves the left column below the workout card empty until the rider docks a panel there.
- **One unexplained capture.** Once in about fifteen screenshots the look-ahead elevation profile showed the last 3 km of
  the route (9.5 to 12 km on the axis) while the rider was at 3.2 km, so the rider marker sat in the lower left corner.
  It did not repeat in two reruns and the cause is unknown; if it shows up again, note what changed in the layout just
  before (a panel closed, a resize) and look at how the look-ahead window start is derived from the snapshot.
- The shell is 2 300 lines and `controller.rs` 2 100: the next change that touches them should consider
  splitting by feature (workout, device, layout).
