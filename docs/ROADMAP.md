# Roadmap

Features the rider has asked for and that are **not built**. Nothing here is a promise or a design that was
verified; each entry says what is wanted, what looks hard, and what to check first. When one is started, move its
reasoning into [`DECISIONS.md`](DECISIONS.md) and delete it from here. Not wanted, on purpose: 3D, route planning,
Strava upload (see `DECISIONS.md`).

Last updated 2026-11-05.

## 1. Data overlay over other full-screen content

**Wanted** (2026-11-05): while the rider watches a film or a video full screen on the same machine, a window with a
transparent background floats above it and shows the ride data, so the rider can watch and still see power, speed,
cadence, heart rate and, with a workout, the current block.

**Shape of a first version (desktop only)**

- A second, small window from the same process, so it reads the same signals as the main window and needs no IPC.
  Undecorated, transparent, always on top, not in the task bar, **click-through** (cursor events ignored), so the
  video player below keeps its keyboard and mouse.
- Content: a few metric tiles chosen in the settings (not the whole dock), large type, one corner of the screen,
  size and corner adjustable; a toggle in the top bar and a global shortcut. It follows the ride state (hidden when
  there is no ride).
- Reuse `ride-ui-hud` tiles with a second stylesheet; the overlay must stay legible on any picture, which means a
  solid panel behind the numbers with a chosen see-through level, not bare text.

**Things that look hard or may rule it out. Check them first, with a throw-away prototype.**

- **Exclusive full screen.** A game or player that owns the display exclusively (DirectX/Vulkan exclusive mode) draws
  over any other window. Borderless full screen and browser full screen (YouTube, streaming sites) are normally
  fine on Windows; this has to be tried with the rider's real player and browser.
- **Transparency on every platform.** Dioxus desktop / wry / tao support a transparent window; WebView2 needs a
  transparent background colour set and has had bugs combined with undecorated windows. macOS needs a window level
  above full-screen spaces; Linux under Wayland has **no** always-on-top for ordinary clients, so there it works on
  X11 only or not at all.
- **House rules for CSS.** The lints forbid `blur(`, `backdrop-filter`, `opacity` and hard-coded colours
  ([`DESIGN.md`](DESIGN.md)). A see-through panel needs a token for it (a colour-mix of an existing token with
  `transparent`) and the lint extended on purpose, with the contrast gates run against the worst-case picture
  (white and black).
- **Click-through plus a way to move it.** `set_ignore_cursor_events(true)` means the window cannot be dragged;
  position comes from the settings or from a short "edit mode" that turns click-through off.
- **Focus.** The window must never take focus from the player (`WS_EX_NOACTIVATE` on Windows).

**Android** is a different feature: a floating window over other apps needs the `SYSTEM_ALERT_WINDOW` permission
(a manual grant in system settings) or Picture-in-Picture; the pinned manifest and the privacy text would change.
Decide separately, after the desktop version shows the idea is worth it.

**Docs to touch when built:** `README.md`, `USER_GUIDE.md` (a section, the settings table), `CHANGELOG.md`, `UI.md`
(the second window), `DESIGN.md` (the see-through token), `ARCHITECTURE.md`, `PRIVACY.md` unchanged (nothing leaves
the machine), the verification table in `CLAUDE.md` (which player and browser were tried).
