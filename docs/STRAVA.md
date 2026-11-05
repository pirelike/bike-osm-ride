# Strava upload

"Feltöltés Stravára" in the end-of-ride dialog, next to "Csak mentés TCX-ként". **It does not use the Strava API**,
and it must not be rewritten to use it. This document records why, how it works, and what to do when it breaks.
Checked against `crates/ride-app/src/strava.rs`, `controller.rs`, `android_share.rs` and the dialog on
2026-11-04.

> **Status: never run against the live Strava site, on any platform.** The design is covered by unit tests of the
> pure parts (script generation, the words the page reports, window geometry). Whether the form fields are still
> named as the script expects is unknown until someone with a Strava account tries it.

## 1. Why not the API

Since June 2026 creating a Strava API application needs a paid subscription, and a client secret shared by an
open-source app cannot stay secret. The maintainer does not want to subscribe and wants the upload to run in the
background, with the fields of Strava's "Add Activity" form (title, description, virtual or real ride, how it
felt, private notes, who can see it, mute from the feed) filled in the summary dialog first. So the app does what
a person does on the website. Do not suggest registering an API app again. `StravaUploaderPy` (GPL-3.0) was looked at
as a reference for the web form's field names; none of its code may be copied (permissive-only rule).

## 2. The details form (`ride-ui-dialogs/src/ride_details.rs`)

`RideDetails` is edited in the summary dialog and kept in `AppState::ride_details`: `title` (empty becomes a
time-of-day name such as "Délutáni edzés"), `description`, `virtual_ride` (Virtuális túra, the default, or
Kerékpározás), `feel` ("Hogy esett?": five choices that map onto Strava's 1-10 perceived exertion, optional), `notes` (private), `visibility` (Mindenki / Követők / Csak én
→ `strava_value()`), `mute`.

## 3. Desktop: a hidden WebView

1. The TCX is rendered (`ride_sim::tcx`) and a **copy is written to `<data>/strava/<suggested name>.tcx`** first, so a
   failed upload never loses the ride.
2. `StravaView::open` creates a **second WebView as a child of the app window**, with its own profile folder
   (`<data>/strava-webview`, so the sign-in lasts between rides; the app never sees the password), pointing at
   `https://www.strava.com/upload/select`, **hidden** (`with_visible(false)`), and injects `SCRIPT` into every
   page it loads. Its bounds follow the window below a 52 px bar the shell keeps for itself (`BAR_HEIGHT`).
3. The script (runs only in the top frame on `*.strava.com`; `__NAME__`, `__XML__`, `__DETAILS__` are replaced by JSON
   literals) is a small state machine; the phase lives in `sessionStorage`, which survives navigation:
   - not on `/upload`: first time, navigate there once; if a login page (`/login`, `/session`, `/oauth`, `/signup`)
     shows up, report `login`;
   - phase `''`: find `input[type=file]`, hand it the TCX as a `File` through a `DataTransfer`, fire `input` and
     `change`, set `picked`, report `selected`;
   - phase `picked`: wait until the form exists, fill it (`fill()`: title by name/id/placeholder heuristics; sport
     `select` chosen by text "virtual" or "ride"; description; private notes; perceived exertion; visibility by
     text in English or Hungarian; the "hide from home" checkbox), report `fields:{...}` for the log, set
     `saving`, and press the first enabled button that reads "save" or "mentés" after 800 ms;
   - on `/activities/<id>` while `saving`: set `saved`, report `saved`.
   - it polls every 400 ms and gives up after 240 tries (96 s) before saving, 60 tries (24 s) after, reporting
     `debug:{...}` with the URL, phase and a dump of every input, textarea, select and button of the page.
4. Rust reads the words through `window.ipc` (`parse_word` → `Word::{Login, Selected, Fields, Saved, Stuck}`):
   - `Saved`: toast "A túra felkerült a Stravára.", the view is dropped;
   - `Login`: the WebView is shown below the bar with "Jelentkezz be a Stravába; utána a feltöltés magától
     folytatódik." After the sign-in the same script continues;
   - `Stuck` (or 150 s of silence): the dump is written to **`<data>/strava/strava-debug.log`**, the view is shown so
     the rider can finish by hand or upload the saved copy, and the bar names the copy's path; "Kész" closes it.
   While it runs, the summary dialog stays usable.

## 4. Android: the share intent

wry on Android allows only one WebView, so there is no hidden upload there (the maintainer chose this).
`android_share.rs` writes the TCX into the shared Download folder through `MediaStore.Downloads` (Android 10+, the
only way to hand out a content URI without a `FileProvider` entry) and starts an `ACTION_SEND` intent
(`application/vnd.garmin.tcx+xml`) aimed at `com.strava`; without the Strava app the system chooser opens. The Strava
app takes no title or other details from the intent: the rider types them there, and the toast says so.

## 5. When it stops working

Strava can change its page without notice. Symptoms: the bar says the upload did not finish by itself, and
`strava-debug.log` holds a JSON dump. Fix the selectors in `SCRIPT` first: the dump lists every form control with
its name, id, aria-label, placeholder and `data-testid` (`fields`), the URL, and the phase. Typical changes: the file
input becoming dynamic, a renamed save button, a new confirmation step. Keep the Hungarian and English labels
working, keep the script free of the user's data except what it needs, and add the new field spelling to the
`the_words_of_the_page_are_told_apart` / script tests where it makes sense. Then try it for real once and note the
date in this document.

## 6. Privacy

Only the TCX and the fields the rider typed leave the machine, to strava.com, from a WebView the rider can see on
request. The sign-in cookie sits in `<data>/strava-webview`. Nothing is sent without the button being pressed.
