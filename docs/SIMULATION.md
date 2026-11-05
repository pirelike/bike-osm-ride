# Simulation (`ride-sim` and the ride core)

What turns a watt reading into a position on the route and a grade for the trainer: the physics, the ride
state machine, the numbers shown on screen, workouts in ERG mode, the virtual gearbox, ghost and segments,
and the TCX export. Checked against the code on 2026-11-04.

`ride-sim` is **pure**: no I/O, no async, no clock. The app owns time and calls `RideEngine::step(dt, sample,
unix_now)`. Everything is finite-safe: non-finite sensor values are sanitised, a route without points or a
rider with nonsense settings never panic (and `ride-app` contains panics anyway).

## 1. Physics (`physics.rs`)

The road-cycling power model of Martin et al. (1998, J. Appl. Biomech. 14:276-291; validated against SRM
power on the road, R² = 0.97, SE 2.7 W):

```
P · η = (F_grav + F_roll + F_bearing + F_aero) · v  +  m_eff · v · dv/dt

F_grav    = m g sin θ,           θ = atan(grade / 100)
F_roll    = Crr m g cos θ
F_bearing = (91 + 8.7 v) · 10⁻³ N          only while rolling (v > 0)
F_aero    = ½ ρ (CdA + 0.0044) (v + v_wind) |v + v_wind|     0.0044 m² = spoke drag
m_eff     = m + 1.5 kg                      rotating mass of the wheels
```

- Integrated in **kinetic energy**, in sub-steps of at most 50 ms: that form is stable around `v ≈ 0`, where the
  force form divides by zero. Resistance acts as a force (so a standing rider on a descent starts rolling), the
  pedalled power is injected as energy, speed is clamped to `0..30 m/s` (108 km/h; no rolling backwards).
- Parameters come from `RiderSettings` and are clamped: rider mass 20-250 kg, bike 0-40 kg, CdA 0.05-1.5,
  Crr 0-0.05, drivetrain efficiency 0.5-1, air density 0.5-1.6; grades beyond ±30 % are not meaningful.
- Reference values, asserted in tests: 75 kg rider + 9 kg bike, CdA 0.32, Crr 0.004, η 0.975, ρ 1.225: 250 W on
  the flat settles at about 36.3 km/h, 250 W at 8 % at about 12.3 km/h; cross-checked against the
  open-source `rouleur` implementation of the same paper (its hour-record example, 440.96 W).
- `RiderSettings::use_trainer_speed` replaces the model with the speed the trainer reports (clamped), for
  riders whose trainer already knows its flywheel.

## 2. The ride state machine (`engine.rs`)

`Empty` (no route; the core) → `Ready` → `Riding` ⇄ `Paused` → `Finished`.

- `start` only from `Ready`; `finish` from `Riding`/`Paused`; **the ride finishes by itself** when the
  distance reaches the end of the route; `reset` returns to `Ready` at 0 m with an empty history.
- The timer excludes pauses. `seek` works only when not riding (click on the profile).
- A workout and a ghost set cannot be swapped under a running ride; `set_workout` / `set_ghost` are ignored
  while `Riding` or `Paused`.
- In every state the live sensor values still appear in the snapshot.
- `RideCore` (in `ride-app`) wraps the engine: it turns state transitions and the keep-alive rules into
  `SensorCommand`s and contains engine panics.

### Numbers on the screen

| Value | Definition |
|---|---|
| 3 s power | time-weighted average of the samples within the last 3 s of *engine clock*; empty when fewer than two samples fall inside (which is why `RIDE_TIMESCALE` above ~20 empties it) |
| Normalised power | 4th root of the mean of the 4th powers of the 30 s rolling average (Allen/Coggan) |
| Work, kcal | work in kJ; kcal is taken equal to kJ (about 24 % efficiency) |
| IF, TSS | IF = NP / FTP; TSS = elapsed × IF² / 3600 × 100 |
| Power zones | Coggan, % of FTP: Z1 < 55, Z2 55-75, Z3 76-90, Z4 91-105, Z5 106-120, Z6 121-150, Z7 > 150 (`ride_model::power_zone_of_pct`, used by the engine, the charts and the workout picker alike) |
| HR zones | five zones, % of max HR: < 60, 60-70, 70-80, 80-90, > 90 |
| Ascent / descent done | cumulative along the route, **scaled to the route's own filtered totals**, so "left" never goes negative because of a different smoothing rule |
| ETA | remaining distance ÷ the steady-state speed for the 3 s power on the average grade of the next kilometre; the current speed when there is no power; empty below 0.5 m/s |
| W/kg | power / rider mass (bike excluded) |

## 3. The grade the trainer receives

`target_grade(d) = mean(route grade at d + 5 m and d + 45 m) × trainer_grade_scale`, with the scale clamped to
0 - 1.5 (the "Meredekség erőssége" setting, 0-150 %). Then:

1. **Slew limit:** the simulated grade moves toward the target at most 2 percentage points per second.
2. **Clamp** to ±20 % (the FTMS limit used for consumer trainers; the BLE layer clamps again to the trainer's own
   range).
3. **Quantise** to 0.01 %; Crr to 0.0001; Cw (`½ ρ CdA`, kg/m) to 0.01; wind is always 0.
4. **Virtual gearbox offset** (below) is added in `RideCore` before sending.
5. `RideCore` sends the parameters when the grade moved by at least 0.1 %, when Crr, Cw or wind changed, or after
   5 s of silence (the FTMS keep-alive). Start sends them at once; pause and finish send a neutral flat grade.

While a workout step has a target, **ERG mode** replaces all of this: `SetTargetPower(watts)` goes out when the
target moves by 1 W or every 5 s, and the simulation resumes in free steps and after the last step. The
terrain still moves the rider on the map, but the trainer holds the watts.

## 4. Workouts (`ride-model/src/workout.rs`, `ride-sim/src/workout.rs`, `workout_import.rs`)

- **Library:** `crates/ride-model/assets/workouts.json` is generated by `scripts/gen_workouts.py`, compiled into
  the binary and **validated when loaded** (a broken entry is skipped, never a panic; an unknown tag
  invalidates the entry). 93 entries in 12 categories (recovery 3, endurance 11, tempo 5, sweet spot 10,
  threshold 14, VO2max 17, anaerobic 3, sprint 4, climb 6, race 8, test 9, mixed 3), 23 tags in four facets
  (goal, level, kind, season phase), a one-line goal, a description, the public sources, and **943 timed
  coach messages** (`scripts/coach_messages.json`).
- **Steps** have a kind, a duration, a target (`pct_ftp`, `watts`, `ramp_pct` from/to, `step_watts` for ramp
  tests, or free), an optional cadence hint and note, and timed messages.
- **Runner:** `WorkoutSession` advances with *riding* time (pauses never reach it) and computes targets from the
  **current** FTP, so changing the FTP retargets the rest immediately. It reports `WorkoutProgress`: step, target
  watts and % FTP, time left in the step and in the workout, the next step, and the coach message. A message is
  shown for 12 s of riding time after it is due; a step's note counts as a message at the step's start unless an
  imported file already carries the same text.
- **Import:** `workout_import::from_bytes` reads MyWhoosh exports in either shape, the `<workout_file>` XML
  (the Zwift `.zwo` schema: `SteadyState`, `Warmup`, `Cooldown`, `Ramp`, `IntervalsT`, `FreeRide`, power as a
  fraction of FTP, `textevent` messages with their offsets) or MyWhoosh's own JSON (`WorkoutStepsArray`,
  `StartPower`/`EndPower` marking a ramp). The id is `import-<content hash>` (re-importing replaces the entry);
  imports are rejected only when no step is usable or the total is under a minute. They are stored as
  `<data>/workouts/<id>.json` and merged into the library at start-up, so a selected import is found again.
- **Rules the user set:** a workout cannot be swapped while riding; the virtual gearbox UI shows only when it is
  switched on and **no** workout (ERG) runs.

## 5. The virtual gearbox (`ride-app/src/gearbox.rs`)

A smart trainer has no gears, so the gear is turned into resistance. 2 × 12 drivetrain (50/34 chainrings,
11-34 cassette: 11, 12, 13, 14, 15, 17, 19, 21, 24, 27, 30, 34), duplicates closer than 1.5 % merged, which
leaves **23 gears**. The shift adds `7 · ln(ratio / 2.13)` percent to the grade sent to the trainer: about
-5.3 % in the easiest gear and +5.3 % in the hardest, zero at the reference ratio 2.13 (the default gear). The
ride itself (distance, map speed) still comes from power and the real grade; the gear only changes what the
rider has to produce. The panel also shows the ratio and the speed the cadence would give on a 2.096 m wheel.
Shifts: `←`/`→`, the Váltó panel, the phone gear buttons, or a Zwift Click/Play controller.

## 6. Ride log, ghost and segments

```
<data>/rides/<route id>/<start unix seconds>.json    RideRecord: summary + 1 Hz RecordPoints (compact JSON)
<data>/rides/<route id>/index.json                   Vec<RideMeta>, newest first
```

- A ride shorter than 60 s or 200 m is not stored. `Route::id` is a stable hash of the geometry, so the same
  GPX finds its own history. Deleting a recent route keeps its rides.
- **Segments are the detected climbs.** `segment_key` is the climb's start/end in 10 m steps (`"430-1130"`), so a
  re-processed route with a metre of jitter keeps its history. A segment time is stored only when the ride went
  through the **whole** climb; entry and exit are interpolated from the 1 Hz samples. The PR is the best
  stored time among non-simulated rides.
- **Ghost.** A `Track` is the ride's `(t, dist)` trace, both axes monotone, looked up by binary search and
  linear interpolation. The ghost is aligned so the difference is zero on the start line; `delta = elapsed -
  (t_ghost(dist) - offset)`, positive meaning the rider is behind. Modes (`GhostMode`): `Off`; `Route` (the GPX's
  own `<time>` stamps, available before the first ride); `Last` (most recent, simulated included); `Best` (the
  fastest complete non-simulated ride; with none complete, the furthest partial one, which then stops at its own
  end). The default is `Best`. The settings dialog explains in one line why there is currently no ghost.
- A ride is `simulated` when the virtual rider took part, or when `RIDE_DEMO`/`RIDE_SIM` is set; the flag is sticky
  for the ride. Simulated rides are stored, shown in the summary, but never PRs and never the Best ghost.
- **App wiring** (`ride-app/src/history.rs`): after a route load, a ghost-mode change and a finished ride, a
  `HistoryPlan` (the `GhostSet` plus the PR of every climb) is built on the blocking pool and handed to the
  engine; a running ride keeps the plan it started with. On `Finished` the ride is stored and compared with the PRs
  that existed *before* it (`RideComparison`), then the plan is rebuilt for the next ride.

## 7. TCX export (`tcx.rs`)

Hand-written Garmin TrainingCenterDatabase v2 (no external crates, no locale dependence, valid XML escaping,
ISO-8601 UTC): `Activity Sport="Biking"`, one `Lap` (`TotalTimeSeconds`, `DistanceMeters`, `Calories`, average
and maximum heart rate, cadence, `Intensity` Active, `TriggerMethod` Manual), one `Track` with a trackpoint per
1 Hz record (time, position, altitude, distance, heart rate, cadence) and the `TPX` extension with watts and
speed, a `Notes` text and a `Creator`. The suggested file name is built from the date and the route name
(`suggested_filename`). Strava, Intervals.icu and TrainingPeaks accept it; the upload paths are in
[`STRAVA.md`](STRAVA.md). A ride that was fast-forwarded with `RIDE_TIMESCALE` has a coarser recording.

## 8. Tests worth knowing

Reference-value tests for the physics, state-machine tests for every transition and the "finishes exactly once"
rule, NP/TSS/zone boundary tests, workout step arithmetic and import fixtures for both formats, the ghost and
segment tables, TCX structure and escaping. The app-level `ride_core` tests check the command stream the trainer
would see: start, grade, keep-alive after 5 s, ERG instead of grade, neutral grade on finish, simulation
returning after the last step.
