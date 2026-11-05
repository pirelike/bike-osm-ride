# Bluetooth layer (`ride-ble`)

How the app talks to trainers and sensors, what it refuses to send, which firmware quirks it works around,
and what has and has not been proven on real hardware. Checked against the code and against a real capture
on 2026-11-04.

## 1. What it supports

| Role (`DeviceKind`) | Service | Characteristics used | Decoder |
|---|---|---|---|
| Trainer | FTMS `0x1826` | Indoor Bike Data `0x2AD2` (notify), Control Point `0x2AD9` (write + indicate), Machine Status `0x2ADA`, Feature `0x2ACC`, Supported Inclination Range `0x2AD5`, Supported Power Range `0x2AD8` | the [`ftms`](https://github.com/deancochran/ftms) crate (MIT) behind `parsers.rs` |
| Power meter | Cycling Power `0x1818` | Measurement `0x2A63` | vendored `cycling-ble` power parser: instantaneous power, left/right balance, cadence from crank revolutions |
| Cadence / speed | CSC `0x1816` | Measurement `0x2A5B` | vendored `cycling-ble` CSC parser, revolution counters with `u16` wrap-around |
| Heart rate | Heart Rate `0x180D` | Measurement `0x2A37` | vendored `cycling-ble` HR parser |
| Shifter | Zwift service `00000001-19ca-4651-86e5-fa29dcdd09d1` | `0002` measurement, `0003` control, `0004` response | `zwift/` (Click and Play; the Hub trainer messages are a codec only) |
| Simulated rider | none (`sim:rider`) | none | `sim.rs`: power with lag and wobble, cadence from power and grade, lagged heart rate, speed from the same physics |

The simulated rider is always offered, even without a Bluetooth adapter, and takes any slot.

## 2. Structure

```
 SensorCommand ──► Hub (one task) ──► one DeviceSession task per physical device (any mix of roles)
                    │  merger, emit gate, simulated rider      │
                    │  scan task (unfiltered)                   └─ MultiSession: discovery, subscriptions,
 SensorEvent ◄──────┘  Internal messages ◄──────────────────────   decoders, ControlSession, Zwift session
```

- **`transport.rs`** is the only thing the rest of the crate is written against: `BleAdapter` and
  `GattPeripheral` traits (scan, connect, discover, read, write, subscribe, notifications). `btle.rs` is the
  btleplug implementation and the only module that names btleplug types (plus the one-time Android init in
  `android_ble.rs`). `fake.rs` is an in-memory implementation with failure injection, so connect, reconnect,
  the FTMS handshake, keep-alive and the whole hub run through their real code paths in tests.
- **`hub.rs`** owns the command loop, `slots` (keyed by device id), `role_owner` (role → device), generations
  (an older state message of a replaced connection is stale), the last trainer *intent* (replayed to a trainer
  that connects later) and the merger. It runs on its own thread with a two-worker tokio runtime
  (`spawn_sensor_hub`); dropping the command sender winds it down, and a graceful stop hands the trainer back
  in a neutral state.
- **`multi.rs`: one connection, every role.** A trainer such as the captured one exposes FTMS, Cycling Power and
  CSC at once. Opening one connection per role is unreliable (the second connect usually fails), so the device
  is connected once, services are discovered once, and every supported measurement is subscribed and routed to
  the merger source of the role the device claimed. A role the hub asked for but the device lacks is reported
  with a Hungarian reason ("nincs Cycling Power (0x1818) szolgáltatás") while the other roles stay up. A
  dedicated device connecting later takes over only that role; "Bontás" on any role of a multi-role device
  disconnects the whole device.
- **`merge.rs`.** Every value is remembered per (field, source) with its arrival time; a value not refreshed for
  3 s counts as absent. Priorities: power: power meter, then trainer; cadence: CSC sensor, then power-meter
  cranks, then trainer; heart rate: strap, then trainer; speed: trainer, then CSC wheel; resistance, energy:
  trainer; balance: power meter. When the device does not own the power-meter slot, its Cycling Power data
  only fills gaps left by Indoor Bike Data. `Sample` events leave the hub at most every 90 ms (about 10 Hz).

## 3. Scanning

Scans are **unfiltered**: on Windows (WinRT) service-UUID scan filters are unreliable (a lesson taken from the
GPL project opencycling, idea only). Devices are classified from the services they advertise and, only when
the advertisement is silent, from the name (`scan.rs` lists the prefixes: Kickr, Tacx, Zwift Hub, Elite
Direto/Suito/Zumo, Saris, Polar H/OH, Tickr, Assioma, Favero, Stages, ...). A named device with no recognised
service still shows up when it is closer than -65 dBm, so someone sitting on a trainer with a silent
advertisement can pick it. After connecting, the real services are checked; a wrong guess becomes a clear
error. A scan that saw no event for 30 s is restarted (WinRT goes quiet), a device unseen for 60 s drops off
the list, and `Discovered` is sent at most every 500 ms. A running scan stops on its own after 30 s
(`SCAN_TIMEOUT` in the controller) and always before connecting.

## 4. FTMS control (`control.rs`)

The Control Point is the only way the app can hurt a trainer, so everything passes **one filter**:

- Allowed: Request Control `0x00`, Start/Resume `0x07`, Stop/Pause `0x08` (parameter 1 or 2), Set Target
  Power `0x05`, Set Indoor Bike Simulation `0x11`. Anything else, **Reset `0x01` included** (it drops the control
  permission), never reaches the device; the filter answers with a synthetic "not supported" / "invalid
  parameter" response instead.
- Targets are clamped: ERG watts to the trainer's Supported Power Range (fallback 0 to 1500 W), the simulated
  grade to its Supported Inclination Range intersected with **±20 %**.
- Values are quantised to the wire grid first (grade 0.01 %, crr 0.0001, cw 0.01 kg/m, wind 0.001 m/s):
  the `ftms` crate rejects off-grid numbers.

The session is a pure state machine (time is passed in), driven by `MultiSession`:

1. Subscribe to Control Point indications, Machine Status and Indoor Bike Data **first**, then write Request
   Control and wait for the `80 00 01` indication; then Start; then the simulation or target.
2. **One procedure in flight**; at most 4 writes per second (`min_write_gap` 250 ms); a procedure without an
   indication for 3 s is abandoned.
3. **Keep-alive.** The active target is re-sent at least every 5 s. The queue holds *intents* whose bytes are
   computed at send time, so a slow trainer always receives the latest value.
4. **Pause** queues a neutral simulation (0 % grade) and then Stop/Pause; **shutdown** does the same with Stop.
   The simulation parameters are replayed after a reconnect.
5. Request Control is tried 3 times, 1 s apart. After three unanswered requests control is **assumed** and
   writes are no longer waited for ("silent" mode), because some trainers accept commands but never
   confirm; an explicit refusal still refuses. A write that fails at the transport level (ATT `0x11`
   "insufficient resources" from a busy trainer) is retried after the write gap, up to 40 times in a row.
6. If the trainer takes the control permission away, the app asks again, but only 3 times
   (`reclaim_limit`) so it never fights another app for the trainer; the counter resets when a Start or a
   target is accepted.

### The status storm

Fitness Machine Status `0x01` (Reset) means "your control permission is gone". The captured trainer sends it
about 4 times a second, also about 200 ms after every grant. Read literally, the app spent the whole ride in a
Request Control loop (55 requests) and never wrote a target: ERG and hills did nothing. The session therefore
treats a status as an event only when it is fresh: a status that repeats within 5 s (`status_storm_window`) or
lands within 2 s of a grant (`grant_grace`) is logged once to the hex log and otherwise ignored, the permission
is kept, and, while the ride runs and the trainer reports itself stopped, Start is sent again at most every 3 s
(`restart_interval`). A lone Reset still loses the permission. A repeated "stopped by user" is handled the same
way; an isolated one latches the targets off with a message until the rider restarts the ride.

### Indoor Bike Data with a tail

The same trainer sends 20-byte frames where the flags describe 8. A frame with more bytes than its flags
define is **accepted** (the layout is fixed by the flags, so every known field is exact; the tail is ignored).
With a resistance field the tail is still an error, because it could be the 16-bit resistance layout shifted by
one byte.

## 5. Timing and reconnects (`link.rs`, `HubConfig::default`)

| Setting | Value | Setting | Value |
|---|---|---|---|
| hub tick | 100 ms | connect timeout | 12 s |
| stale value | 3 s | I/O timeout (discover, subscribe, write) | 6 s |
| `Sample` rate limit | 90 ms | optional read timeout (feature, ranges) | 3 s |
| health poll (`is_connected`) | 2 s | trainer watchdog (no packet at all) | 10 s |
| sensor watchdog (strap, meter) | 30 s | reconnect backoff | 1.5 s, 4 s, 10 s |
| budget reset after a stable link | 30 s | short re-scan before a try | 2 s |
| graceful stop flush | 2 s | | |

A lost link is retried three times with a toast per try; a link that stayed up for 30 s gets its budget back.
Before retrying a vanished device the hub scans briefly again, because on Windows a device that dropped off
needs the scanner to see it once more (an iron-heart lesson).

## 6. Zwift Click and Play

Detected by the Zwift service, by manufacturer id `0x094A` with controller type 9 (Click), 2 (Play right) or 3
(Play left), or by a `Zwift Click` / `Zwift Play` name, unless the device also advertises FTMS or is named
`Zwift Hub` (that is a trainer). The session subscribes `0002` and `0004`, writes the `RideOn` hello to `0003`
and runs a pure state machine (`zwift/controller.rs`) with a 180 ms per-button debouncer; the key exchange is
ECDH P-256, HKDF-SHA256 and AES-CCM (4-byte tag, 8-byte nonce) in `zwift/crypto.rs`, and a bare `RideOn` answer
means plain packets. Shifts become `SensorEvent::Shift(±1)`; the app switches the virtual gearbox on at the
first one. Buttons: Click `+` harder, `-` easier; Play right `Shift` or `Y` harder, Play left `Shift` or `B`
easier (`play_shift_buttons` is the single place to change it). A short haptic pattern acknowledges a shift.
This is unofficial interoperability work (see `NOTICE.md`); **there is no record of it running against a real controller**.

## 7. Android

`btleplug` 0.13's Android backend (droidplug) keeps its adapter in a global and **panics** when
`platform::init` was never called, so `android_ble::init_backend` runs on the hub thread before the first scan
(a failure is reported as a notice, the simulated rider stays). The activity asks for `BLUETOOTH_SCAN` and
`BLUETOOTH_CONNECT` on Android 12+ and for fine location on older versions
(`MainActivity.requestBlePermissions`); a scan without the permission fails visibly and the Devices dialog's
"Keresés indítása" asks again. The crate uses `jni` 0.22 (btleplug's version) while `ride-app` uses 0.21: the
two environments must not be mixed. See [`ANDROID.md`](ANDROID.md). Never exercised on a real phone.

## 8. The hex log

`<data>/ble.log` is **always on** (`start_hub` hands the path to the hub; rotated to `ble.log.1` at start-up
beyond 8 MB). One line per packet, `#` for decisions of the session:

```
2026-11-01T22:37:28.184Z  WRITE  2ad9  11000000002814
2026-11-01T22:37:28.206Z  NOTIFY  2a5b  02f0235f93
2026-11-01T22:35:52.103Z  #  a trainer ismételten reset-állapotot jelez
```

(The first line is a Set Indoor Bike Simulation write: wind 0, grade 0.00 %, crr 0.0040, cw 0.20.)

Directions are `NOTIFY`, `INDICATE`, `READ` and `WRITE`. Control Point responses read `80 <request opcode>
<result>`; result `01` success, `02` op code not supported, `03` invalid parameter, `04` operation failed,
`05` control not permitted. The `RIDE_BLE_LOG` variable no longer does anything.

## 9. What a real trainer taught us

A capture of the author's trainer (a generic FTMS "VE200"), `ble.log.1`, 2026-11-01 13:22 to 2026-11-02 11:43,
168 177 lines:

| Observation | Count / fact |
|---|---|
| Indoor Bike Data, CSC and Cycling Power notifications | 47 110, 47 110, 47 109: the trainer serves three roles on one connection |
| Request Control writes, all answered `80 00 01` | 2 400, of which 2 342 within two hours of 2026-11-01 (the status storm before the fix); one per session afterwards |
| Set Target Power (ERG) writes, all accepted | 928 on 2026-11-01 evening and 2026-11-02 morning (targets such as 143 W and 147 W) |
| Start/Resume | 27, accepted |
| Stop/Pause | 6, **every one answered "op code not supported"** (`80 08 02`); harmless because a neutral grade is written first |
| Set Indoor Bike Simulation | 6, all neutral (0 % grade, written 0.25 s before a Stop/Pause) |
| Indoor Bike Data "trailing bytes" parse failures | 12 comment lines, all before the tail rule above existed |

**Not in the capture: terrain simulation.** Not one Set Indoor Bike Simulation write carries a non-zero grade,
so that the trainer follows the hills of a route has never been confirmed on hardware, only ERG has. The logic
is thoroughly tested against the fake transport and against the spec's byte vectors.

## 10. Verification status

| | Automated | Real device |
|---|---|---|
| FTMS codecs | differential tests: `ftms` crate vs vendored `cycling-ble` vs hand-built spec vectors, every relevant Indoor Bike Data flag combination, Control Point vectors | the capture above |
| Control session | fake-transport behaviour tests (handshake, keep-alive, storms, reclaim, pause, reconnect replay) | ERG yes, terrain no |
| CPS / CSC / HR decoding | vendored upstream contract tests, wrap-around tests | CPS and CSC from the trainer; **no HR strap, no separate power meter** |
| Zwift | NIST AES-CCM vectors, pure state machine tests | none |
| Android Bluetooth | init contract test | none |
| macOS CoreBluetooth, Linux BlueZ | none (CI only builds) | none |

## 11. Troubleshooting a trainer that does not respond

1. Open `ble.log`. No `WRITE 2ad9` lines at all: the trainer was not connected in the **Trainer** role (a
   trainer connected only as a power meter is read, not driven; the app warns when a workout starts).
2. `WRITE 2ad9 00` repeating: Request Control is not confirmed; look for `INDICATE 2ad9 800001`. Without it
   the session assumes control after three tries and writes anyway.
3. `INDICATE 2ad9 80 xx 05`: control not permitted, another app (Zwift, a phone app) holds the trainer.
4. A `#` line about repeated reset or stopped states: the storm handling is active, which is expected on
   firmware like the captured one.
5. Nothing arrives on `NOTIFY 2ad2`: the link is dead; the watchdog reconnects after 10 s.

## 12. Adding support for something new

Add the service UUID and a decoder (`sensor::Decoder`), claim the role in `multi.rs`, map it into
`merge.rs` priorities if it feeds a field, write the byte-level tests next to the decoder and a fake-transport
test in `hub_tests.rs`, extend `scan::classify`, and add a licence header and a `NOTICE.md` entry for any
ported code. Keep every number you rely on traceable to the specification or to a capture.
