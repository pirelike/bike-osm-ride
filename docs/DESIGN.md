# Design system — `ride-ui-kit`

A **bike head unit / printed race sheet**. Structure comes from 1 px hairlines and a strict grid,
not from floating cards. Typography carries the design: very large tabular condensed numerals for
live values, a plain grotesque for text, small labels in sentence case. Warm graphite dark theme and
a warm paper-white light theme (both first-class), **one accent** (vermilion) used sparingly, all
other colour is *data* colour and validated.

What it deliberately is not: navy glass panels, backdrop blur, glows, decorative gradients, soft
shadows, uniformly rounded cards, tracked-out capital eyebrow labels, pill chips everywhere, a ring
gauge.

Everything here is enforced by code:

* `crates/ride-ui-kit/assets/kit.css` — tokens and primitive classes. `ride_ui_kit::CSS` =
  `assets/fonts.css` (embedded fonts, generated) + `kit.css`; the shell injects it first, before every
  `ride-ui-*` CSS block.
* `crates/ride-ui-kit/src/palette.rs` — data and accent colours as Rust constants, mirrored by the
  CSS tokens and kept in sync by a unit test.
* `crates/ride-ui-kit/src/format.rs` — every number the user sees (decimal comma).
* `crates/ride-ui-kit/src/color.rs` — WCAG contrast, OKLab, ΔE, used by the palette tests.
* House-rule lints in `cargo test -p ride-ui-kit`: no `blur(` / `backdrop-filter` / `text-shadow` /
  `drop-shadow`, every `box-shadow` has 0 blur, gradients only as marked hard-stop value fills,
  radius ≤ 4 px (pill only on `.badge` / `.chip--ok|warn|crit`), no `text-transform`, tracking ≤ 0.02 em,
  no literal px in `font` / `font-size`, spacing on the 4 px grid, dialog / toast animation uses
  `--dur` (150 ms).

Token **names** are the contract (the older names all still exist); every `ride-ui-*` crate adds
only its own prefixed classes (`map-`, `profile-`, `hud-`, `dlg-`, `shell-`), uses `var(--…)` only
(no literal colours; data colours from `ride_ui_kit::palette`), no emoji, icons from
`ride_ui_kit::Icon`.

Reference renderings: `cargo run -p ride-ui-kit --example preview` writes four pages × two themes to
`target/preview/ride-ui-kit-{foundations,controls,surfaces,headunit}-{dark,light}.html`. The
`headunit` page (and `profile_svg`, `map_svg`, `power_history_svg` in `examples/preview.rs`) is the
reference the other crates are measured against.

## Surfaces and rules

| Token | Dark | Light | Use |
|---|---|---|---|
| `--bg` | `#12110f` | `#ece7da` | window, map letterbox |
| `--bg-1` | `#1a1916` | `#f8f5ec` | panels, **the chart surface** |
| `--bg-2` | `#22211d` | `#eee9dd` | raised rows, table heads |
| `--bg-3` | `#2c2a26` | `#e2dccd` | pressed / selected, flat chart areas |
| `--rule` | `#3a3832` | `#d3cdbd` | the hairline: cell borders, separators (1.6:1 / 1.5:1 on `--bg-1`) |
| `--rule-strong` | `#6e6a61` | `#8e8777` | control borders (input, switch, segmented): ≥ 3:1 on `--bg-1` |
| `--rule-ink` | `#f0eadf` | `#1d1a15` | the one heavy rule (dialog top edge, page title rule) |
| `--border`, `--border-strong` | 14 % / 30 % ink | 14 % / 34 % ink | translucent equivalents, usable over the map |
| `--fill-1…3`, `--hover` | 4 / 7 / 11 % ink | 3.5 / 6 / 10 % ink | hover / pressed on any surface |
| `--scrim` | 72 % black | 45 % ink | dialog backdrop (no blur) |

`--glass`, `--glass-strong`, `--highlight`, `--accent-glow`, `--shadow-1` are kept as names so older
CSS keeps working, but they are now opaque / transparent / `none`.

**Structure.** `.panel` (flat, flush, 1 px `--rule`), `.cells` + `.cell` (ruled data grid: the
container paints the rule colour, each cell paints the surface, 1 px `gap`, so any number of
columns stays aligned), `.panel__head` (28 px title row), `.rule-t/b/l/r`, `.divider`. Panels sit
flush against the window edges; the only heavy line is a 2 px `--rule-ink` on dialogs and page titles.

## Ink, accent and status

Contrast (WCAG) on `--bg-1`; every combination used also reaches ≥ 4.5:1 on `--bg-2` (unit-tested):

| Pair | Dark | Light |
|---|---|---|
| `--text` | 14.7:1 | 15.8:1 |
| `--text-dim` (`#b4ab9b` / `#4f493f`) | 7.1:1 (on bg-2) | 7.4:1 (on bg-2) |
| `--text-faint` (`#928a7a` / `#665f51`) | 4.7:1 (on bg-2) | 5.2:1 (on bg-2) |
| `--accent` (`#ff6f1f` / `#b93405`) | ≥ 5.6:1 | ≥ 4.8:1 |
| `--accent-ink` on `--accent` | 6.8:1 | 5.9:1 |
| `--ok` `#7fd18b` / `#1b6e38`, `--warn` `#f2c14e` / `#845000`, `--crit` `#ff6f94` / `#a8195a` | ≥ 4.5:1 | ≥ 4.5:1 |

`--text-faint` is for metadata only (axis ticks, hints), never for a value read while riding.

**The accent** (`--accent`, hover `--accent-strong`, tints `--accent-soft`, `--accent-line`) marks
exactly: the primary action, keyboard focus (`outline: 2px solid var(--accent)` on `:focus-visible`
only, 1 px more on inputs), the rider marker, the switch in its on state, text selection. A toggled
or selected *button / segment* is **inverted ink** (`--text` on `--bg-1`), not coloured. The accent
is a warm hue next to data hues (zone 5, climb colours): the rider marker therefore always carries a
paper ring (`--marker-ring`) and `crit` is pushed toward rose so that accent ≠ error (test: ΔE ≥ 10).

## Type

Two embedded families, both **SIL OFL 1.1**, no reserved font name (`NOTICE.md`,
`assets/fonts/OFL.txt`): **Barlow Condensed** (500/600/700) for numerals and headings, **Barlow**
(400/500/600) for text. They are subset to Basic Latin + Latin-1 + Latin Extended-A (U+0100-017F,
which holds the Hungarian ő/ű; checked glyph by glyph with fontTools: Ő ő Ű ű Á É Í Ó Ö Ú Ü are all
present) plus common punctuation and maths signs, WOFF2, embedded as data URIs in `assets/fonts.css`
(≈ 130 KB), so the app works offline. `₂` (VO₂max) is not in the subset: it falls back to the system
font for that one glyph. `tools/gen_fonts_css.mjs` regenerates `fonts.css`; a unit test fails if it
drifts from the `.woff2` files.

Size scale (4 steps of rhythm: label · body · emphasis · title · headline · value · hero):

| Token | px | Use |
|---|---|---|
| `--t-11` | 11 | labels (sentence case, `--text-dim`), units in small cells, hints |
| `--t-13` | 13 | body, buttons, inputs, dense rows |
| `--t-15` | 15 | emphasis, lead text, large buttons, numeric inputs |
| `--t-20` | 20 | titles (dialogs, panels), small numerals |
| `--t-28` | 28 | headlines, medium numerals |
| `--t-56` | 56 | big values (data cells) |
| `--t-96` | 96 | the power hero |

Older names map onto it: `--fs-2xs`, `--fs-xs` = 11; `--fs-sm`, `--fs-md`, `--fs-base` = 13;
`--fs-lg` = 15; `--fs-xl`, `--fs-2xl` = 20; `--fs-3xl` = 28; `--fs-4xl` = 56; `--fs-hero` = 96.
`--font-ui` Barlow, `--font-display` and `--font-num` Barlow Condensed.

**Numerals.** `.num` = condensed 600, `tnum` + `lnum` (Barlow Condensed's default digits are
proportional, so tabular figures are switched on explicitly), `line-height: 1`. Sizes `.num--sm|md|lg|xl|hero`
= 15/20/28/56/96. The unit is a `.unit` span *inside* the `.num` (same baseline, weight 500, `--text-dim`,
11/13/13/15/20 px depending on the numeral size); inside a cell it is `.cell__unit`. `.num--trim` trims
the box to cap height / baseline (`text-box`) so a numeral aligns with a hairline; it is progressive
enhancement. Labels are never uppercase and never tracked out (lint).

## Primitives (kit.css)

| Class | What |
|---|---|
| `.btn` (+ `--primary`, `--ghost`, `--danger`, `--icon`, `--sm` 24, default 32, `--lg` 40, `--block`) | 1 px `--rule-strong` border, 3 px radius, flat; `.is-active` / `[aria-pressed=true]` = inverted ink; `.btn-group` (+ `--v`) = ruled, joined buttons for map controls and transport |
| `.input`, `.field` (+ `--row`, `.is-invalid`), `NumberField`, `TextField` | 32 px, 1 px border, focus = accent border + 1 px ring |
| `.tag` (+ `--solid`, `--data`), `.chip` | square 2 px-radius labels; `--data` carries a **square** swatch (`--c`) and the words stay in ink |
| `.badge` (+ `--ok|warn|crit|accent`), `.chip--ok|warn|crit`, `.dot` | the **only round elements**: states |
| `.cells` (`--1…--4`, `--flush`), `.cell` (`--hero`, `--md`, `--sm`, `--span2`), `.cell__label/__value/__unit/__note` | ruled data cells |
| `.zonebar` + `__seg`, `.is-current`, `__ftp` | 7 equal steps Z1–Z7 in the zone colours; the current one is tall, an FTP tick (`--ftp`, percent of the width) on top; colours never dim |
| `.tabs`/`.tab`, `Tabs` | underline tabs (`role=tablist`, arrows / Home / End) |
| `.seg`/`.seg__item`, `Segmented` | ruled segmented control, active = inverted ink |
| `.switch`, `Switch` | 32 × 16 square track, square thumb, on = accent |
| `.slider`, `Slider` | 2 px rule, 8 × 16 square thumb, fill = ink (hard-stop fill) |
| `.dialog`, `.dialog-backdrop`, `Dialog` | 1 px border, 2 px ink top rule, 4 px radius, hard-offset shadow, 150 ms |
| `.toasts`/`.toast`, `ToastHost` | flat, 4 px status bar, hard-offset shadow |
| `.kbd`, `.tip`, `.progress`, `.spinner`, `.empty` | small stuff |

Interactive primitives are keyboard accessible by construction (`Dialog`: focus moves in, Tab trap, Esc /
backdrop close, focus returns; `Segmented`/`Tabs`: roving tabindex; `Switch`: `role=switch`). Icon-only
buttons always carry `aria-label`.

## Shape, spacing, motion

Radii: `--radius-xs…xl` = 1 / 2 / 3 / 4 / 4 px (buttons and inputs 3, tags 2), `--radius-pill` only for state
badges. Shadows: none, except **hard offsets** on popovers: `--shadow-2` = `6px 6px 0 var(--shadow-ink)` (dialog),
`3–4px` on tooltip and toast. Spacing is a 4 px grid (`--space-1…7` = 4/8/12/16/24/32/48); the lint allows
0–2 px hairline adjustments and otherwise multiples of 4 for padding, margin, gap, width and height
(the native range thumb is the one documented exception).

Motion is functional only: hover / press 90 ms (`--dur-fast`), dialog and toast in 150 ms (`--dur`), the map
camera glide 420 ms (`--dur-slow`, used by the map crate only), the busy dot / spinner say "working".
`prefers-reduced-motion` collapses everything.

## Data colours

**Power zones** Z1–Z7 (Coggan, cool → hot) — `--z1…--z7`, `palette::power_zone_color`; shown as the 7-step
zone bar, never as a ring.

| | z1 | z2 | z3 | z4 | z5 | z6 | z7 |
|---|---|---|---|---|---|---|---|
| dark | `#5b5de3` | `#239ec8` | `#00ae67` | `#be8900` | `#bd4014` | `#c1108c` | `#9e66dd` |
| light | `#594ed2` | `#248cbe` | `#037546` | `#af8719` | `#cf3313` | `#901d54` | `#954fb9` |

**Heart-rate zones** H1–H5, one rose hue (`--h1…--h5`): dark `#7b435f #a04b6e #c65377 #ee5c77 #ff7e79`,
light `#d29db5 #cf7b9b #cd597d #bf2f52 #a7001c`.

**Grade scale** (diverging: cool descent → warm grey 0 % → yellow, orange, red, crimson), anchors at
`-10, -0.5, 0.5, 3, 6, 9.5, 13` percent, OKLab-interpolated, clamped beyond:

| | descent | flat | up 1 | up 2 | up 3 | up 4 |
|---|---|---|---|---|---|---|
| dark | `#4a90e8` | `#9b9488` | `#f6c330` | `#fa8927` | `#f1453f` | `#c01a5e` |
| light | `#2171cc` | `#8a857a` | `#d9a200` | `#de7000` | `#d73431` | `#990247` |

**Drawing rule: flat, not washed.** The map route and the profile fill both use the 15 **grade buckets**
(`palette::grade_bucket` / `grade_bucket_color`), merged into runs of equal bucket, so the fill reads as
a printed scale with hard colour steps instead of a gradient smear (this is what the reference renders do).
The continuous `grade_color` is for legends and tooltips.

**Climb categories** — ordinal violet ramp `--cat-4` (easiest) … `--cat-hc`, `--cat-none` warm grey. On the
profile a climb is a **flag** on a pole with the category numeral (4 3 2 1 HC) in the category colour
(numeral colour = whichever of black / white contrasts more) and a bar of the same colour on the km ruler.

Every colour that carries meaning is paired with a number or a word.

## Validation (the palette gates)

Run with `validate_palette.js` (dataviz skill) on the **new surfaces** — dark `--bg-1` `#1a1916`, light
`--bg-1` `#f8f5ec` — and mirrored by `cargo test -p ride-ui-kit`:

```
zones dark   (7)  PASS lightness L 0.48–0.67 · PASS chroma ≥ 0.10 · PASS CVD adjacent ΔE 8.3 (deutan, #be8900↔#00ae67)
                  PASS normal-vision adjacent ΔE ≥ 16.5 · PASS contrast ≥ 3:1 vs surface
zones light  (7)  PASS lightness L 0.43–0.77 · PASS chroma · WARN CVD adjacent ΔE 7.5 (deutan, #cf3313↔#af8719; 6–8 band, legal
                  with secondary encoding: Z1–Z7 labels sit under every zone bar) · PASS normal ΔE ≥ 16.7 · PASS contrast ≥ 3:1
hr dark / light   (--ordinal)  PASS monotone · PASS ΔL ≥ 0.06 · PASS light end 2.35:1 / 2.09:1 · PASS single hue (7°)
climb dark / light(--ordinal)  PASS monotone · PASS ΔL ≥ 0.06 · PASS light end 2.78:1 / 2.10:1 · PASS single hue (1°)
grade warm arm dark / light (--ordinal)  PASS monotone · PASS ΔL ≥ 0.06 · PASS light end 2.97:1 / 2.11:1 ·
                  the "single hue" check fails by construction (a yellow → crimson heat ramp, 80–84° of hue); it is a
                  diverging scale and is covered by the continuity + warm-arm-monotone tests instead
```

Additional unit tests: adjacent zone ΔE ≥ 15 and contrast ≥ 3:1 for every zone; the grade scale has no ΔE
jump > 2.5 per 0.1 %; light end of every ordinal ramp ≥ 2:1; `--rule-strong` ≥ 3:1 and `--rule` between 1.25
and 2.2:1 on `--bg-1`; every text / status / accent token ≥ 4.5:1 on `--bg-1` and `--bg-2`; accent vs crit
ΔE ≥ 10, vs warn ≥ 10; every route colour has ≥ 1.8:1 against `--route-casing`, the accent ≥ 2:1 against
`--marker-ring`. Changing a colour means changing `palette.rs` **and** `kit.css`; a test fails if they drift.

Chart / map chrome tokens: `--chart-grid`, `--chart-axis`, `--chart-ink` (the 2 px profile outline, km
ruler, FTP rule), `--route-casing` (near-black on dark, white on light), `--marker-ring` (paper ring around
the rider disc).

## Reference compositions

* **Power hero**: label row (`Teljesítmény` · `3 mp átlag`), the numeral at `--t-96` with the `W` unit on
  its baseline at 20 px, the zone bar under it with the current zone tall and an FTP tick, `Z1…Z7` numerals
  below, then plain text `Z4 Küszöb` and `3,27 W/kg`. No ring.
* **Data grid**: `.cells--2` of `.cell--md` (cadence, heart rate, speed, grade): label top-left, secondary
  figure top-right, value `--t-28`, unit 13 px.
* **Elevation profile**: flat bucket fill, 2 px `--chart-ink` outline, elevation hairlines labelled on the
  rule, **km ruler** along the bottom (tick per km, numeral per 5 km), climbs as flags + ruler bars, the
  ridden part covered by the surface colour at 55 %, the rider a solid accent disc with paper ring and a
  vertical hairline.
* **Map**: land / water / park in desaturated greys, thin streets, route as solid bucket colours over
  `--route-casing`, rider = solid accent disc + heading wedge with paper ring, start = hollow ring,
  finish = square, controls = `.btn-group--v` of 32 px square buttons, a printed scale bar.
* **Live chart**: one flat area, 2 px ink line, FTP dashed rule, the zone colours only in an 8 px strip along
  the bottom edge.

## Layout contract (shell)

≥ 1280 px: top bar (40 px) · map full-bleed with a flush `ClimbCard` panel (top-left, ruled to the edges)
and square map controls (top-right) · right rail 380 px · bottom profile ~232 px with transport docked
at its left edge. < 1100 px the rail moves under the map. Window default 1440 × 900, minimum 1024 × 640.
The map keeps the rider at ~60 % of the viewport height in follow mode.

The rail starts with the flush `WorkoutCard` (only while a workout is selected: block name, ERG target,
countdowns, a 4 px progress rule) and then the power hero, so the workout context sits above the live
numbers. The workout picker is a three-column ruled dialog (categories and tag chips | workouts | blocks)
opened from the top bar or `W`; tag chips are grouped by facet, combine with AND, show the count
they would leave and are disabled when that count is zero; the selected row is marked with the accent rule, never with a card or a shadow.

The shell is adjustable: the rail's left rule and the profile's top rule are drag splitters (a 1 px
accent line appears on hover), and the map and Street View can swap roles (`X`, or the panel's swap
button). The focused pane fills the stage; the other one moves into a shared small-pane rectangle
(fractions of the stage, persisted), with a 6-dot move grip on its header/edge and a 10 px corner
bracket for resizing. In the small pane the map is a rider minimap at a fixed street-level zoom;
it never fits a whole route into a few hundred pixels. The small pane is a flush ruled tile, not a
floating card; nothing gets a shadow or a blur.

## Icons

42 icons (`IconName`, unchanged), 24 px grid, 2 px stroke, **square caps and mitred joins**, straight segments
where the object has a straight edge (circles and arcs stay round), transport glyphs solid without outline,
coordinates on whole pixels. Constants `ICON_STROKE_WIDTH`, `ICON_LINECAP`, `ICON_LINEJOIN`. Tests check
well-formed markup, no rounded rects, no per-icon styling, and that every coordinate stays inside the grid.
