//! "Feltöltés Stravára": the finished ride goes to strava.com through Strava's own upload page,
//! not through the API (the API needs a paid subscription to register an app).
//!
//! A second WebView is a child of the app window. It stays **hidden** while the upload runs:
//! the user signs in to Strava once (the WebView keeps its own profile folder, so the sign-in
//! lasts, and the app never sees the password), and from then on a script injected into every
//! page of that WebView does the whole upload by itself: it hands the TCX to the file input of
//! `/upload`, fills in the title, description, sport, notes and the rest from [`RideDetails`],
//! presses Save and waits for the activity page.
//!
//! The page is Strava's and can change. Whenever the script cannot go on it reports a status
//! word (see [`Word`]) and the shell shows the WebView below its own strip (`BAR_HEIGHT`), so the
//! user can finish by hand; the TCX copy stays on disk.

use std::path::Path;
use std::rc::Rc;

use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::wry::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use dioxus::desktop::wry::{Rect, WebContext, WebView, WebViewBuilder};
use dioxus::desktop::{DesktopContext, WryEventHandler};
use ride_ui_dialogs::RideDetails;

/// Strava's own file upload page.
const UPLOAD_URL: &str = "https://www.strava.com/upload/select";

/// Logical pixels at the top of the window the shell keeps for its own bar.
pub const BAR_HEIGHT: f64 = 52.0;

/// Runs in every page of the upload WebView. `__NAME__`, `__XML__` and `__DETAILS__` are
/// replaced with JSON. `sessionStorage` (per WebView, survives navigation) keeps the phase:
/// '' (nothing yet), 'picked' (file selected), 'saving' (Save pressed), 'saved'.
const SCRIPT: &str = r#"
(function () {
  if (window.top !== window || !/(^|\.)strava\.com$/.test(location.hostname)) return;
  var NAME = __NAME__, XML = __XML__, D = __DETAILS__;
  function get(k) { try { return sessionStorage.getItem(k) || ''; } catch (e) { return ''; } }
  function set(k, v) { try { sessionStorage.setItem(k, v); } catch (e) {} }
  function say(m) { try { window.ipc.postMessage(m); } catch (e) {} }
  function key(e) {
    return [e.name, e.id, e.getAttribute('aria-label'), e.placeholder, e.getAttribute('data-testid')].join(' ');
  }
  function dump() {
    return [].slice.call(document.querySelectorAll('input,textarea,select,button'), 0, 80).map(function (e) {
      return (e.tagName + '|' + (e.type || '') + '|' + key(e).replace(/\s+/g, ' ').trim() + '|' + (e.innerText || '').slice(0, 24));
    });
  }
  function fail(reason, extra) {
    say('debug:' + JSON.stringify({ reason: reason, url: location.href, phase: get('ride_phase'), extra: extra || null, fields: dump() }));
  }
  if (get('ride_phase') === 'saved') return;
  var path = location.pathname;
  if (/^\/(login|session|oauth|signup)/.test(path)) { say('login'); return; }
  if (/^\/activities\/\d+/.test(path) && get('ride_phase') === 'saving') { set('ride_phase', 'saved'); say('saved'); return; }
  if (!/^\/upload/.test(path)) {
    if (!get('ride_nav')) { set('ride_nav', '1'); location.replace('/upload/select'); } else { say('login'); }
    return;
  }

  function title() {
    if (D.title) return D.title;
    var h = new Date().getHours();
    return (h < 5 ? 'Éjszakai' : h < 12 ? 'Reggeli' : h < 18 ? 'Délutáni' : h < 22 ? 'Esti' : 'Éjszakai') + ' edzés';
  }
  function setValue(el, v) {
    var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype
      : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }
  function field(rx, tags) {
    var els = document.querySelectorAll(tags || 'input,textarea,select');
    for (var i = 0; i < els.length; i++) {
      var e = els[i];
      if (e.type === 'file' || e.type === 'hidden') continue;
      if (rx.test(key(e))) return e;
    }
    return null;
  }
  function pick(sel, test) {
    for (var i = 0; i < sel.options.length; i++) {
      var o = sel.options[i];
      if (test(o.text + ' ' + o.value, o.value)) { setValue(sel, o.value); return true; }
    }
    return false;
  }
  var VIS = { everyone: /everyone|mindenki/i, followers_only: /follow|követ/i, only_me: /only.?me|csak/i };

  function fill() {
    var t = field(/activity\[name\]|title|(^|[^a-z])name([^a-z]|$)/i, 'input');
    if (!t) return null;
    var report = { title: true };
    setValue(t, title());
    var sport = field(/sport|activity\[type\]|activity_type|(^|[^a-z])type([^a-z]|$)/i, 'select');
    report.sport = !!sport && pick(sport, function (text) { return D.virtual ? /virtual/i.test(text) : /^\s*ride\b/i.test(text); });
    if (D.description) {
      var d = field(/description/i, 'textarea,input');
      report.description = !!d; if (d) setValue(d, D.description);
    }
    if (D.notes) {
      var n = field(/private|note/i, 'textarea,input');
      report.notes = !!n; if (n) setValue(n, D.notes);
    }
    if (D.feel) {
      var f = field(/exertion|feel/i, 'select,input');
      report.feel = !!f;
      if (f && f.tagName === 'SELECT') pick(f, function (text, v) { return String(v) === String(D.feel); });
      else if (f) setValue(f, String(D.feel));
    }
    var v = field(/visibility/i, 'select');
    if (v) report.visibility = pick(v, function (text) { return VIS[D.visibility].test(text); });
    var m = field(/hide_from_home|mute/i, 'input');
    if (m && m.type === 'checkbox') { report.mute = true; if (m.checked !== D.mute) m.click(); }
    return report;
  }
  function save() {
    var b = document.querySelectorAll('button,input[type=submit]');
    for (var i = 0; i < b.length; i++) {
      var txt = (b[i].innerText || b[i].value || '').trim();
      if (/^(save|mentés)/i.test(txt) && !b[i].disabled) { b[i].click(); return true; }
    }
    return false;
  }

  var tries = 0, since = 0;
  (function tick() {
    var phase = get('ride_phase');
    if (/^\/activities\/\d+/.test(location.pathname) && phase === 'saving') { set('ride_phase', 'saved'); say('saved'); return; }
    if (phase === '') {
      var input = document.querySelector('input[type="file"]');
      if (input) {
        try {
          var data = new DataTransfer();
          data.items.add(new File([XML], NAME, { type: 'application/vnd.garmin.tcx+xml' }));
          input.files = data.files;
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          set('ride_phase', 'picked'); say('selected'); tries = 0;
        } catch (e) { fail('file', String(e)); return; }
      }
    } else if (phase === 'picked') {
      var report = fill();
      if (report) {
        say('fields:' + JSON.stringify(report));
        set('ride_phase', 'saving'); tries = 0;
        setTimeout(function () { if (!save()) fail('no-save-button'); }, 800);
      }
    }
    if (++tries > (phase === 'saving' ? 60 : 240)) { fail('timeout-' + (phase || 'file')); return; }
    setTimeout(tick, 400);
  })();
})();
"#;

/// The injected script for one ride.
fn script(file_name: &str, tcx: &str, details: &RideDetails) -> String {
    let literal = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let details = serde_json::json!({
        "title": details.title.trim(),
        "description": details.description.trim(),
        "virtual": details.virtual_ride,
        "feel": details.feel,
        "notes": details.notes.trim(),
        "visibility": details.visibility.strava_value(),
        "mute": details.mute,
    });
    SCRIPT
        .replace("__NAME__", &literal(file_name))
        .replace("__DETAILS__", &details.to_string())
        .replace("__XML__", &literal(tcx))
}

/// What the page script reports.
#[derive(Debug, PartialEq, Eq)]
pub enum Word {
    /// Strava wants a sign-in: the user has to see the page.
    Login,
    /// The TCX is in the file input.
    Selected,
    /// Which of the form's fields were found (only for the log).
    Fields(String),
    /// Save was pressed and the activity page came up: done.
    Saved,
    /// The script cannot go on; the payload is a JSON dump of the page for the log.
    Stuck(String),
}

pub fn parse_word(raw: &str) -> Word {
    match raw {
        "login" => Word::Login,
        "selected" => Word::Selected,
        "saved" => Word::Saved,
        _ => {
            if let Some(rest) = raw.strip_prefix("fields:") {
                Word::Fields(rest.to_string())
            } else if let Some(rest) = raw.strip_prefix("debug:") {
                Word::Stuck(rest.to_string())
            } else {
                Word::Stuck(raw.to_string())
            }
        }
    }
}

/// The part of the window below the bar, in physical pixels: `(y, width, height)`.
fn body_rect(width: u32, height: u32, scale: f64) -> (i32, u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let bar = (BAR_HEIGHT * scale).round() as u32;
    (bar as i32, width.max(1), height.saturating_sub(bar).max(1))
}

fn bounds(win: &DesktopContext) -> Rect {
    let size = win.window.inner_size();
    let (y, w, h) = body_rect(size.width, size.height, win.window.scale_factor());
    Rect {
        position: Position::Physical(PhysicalPosition::new(0, y)),
        size: Size::Physical(PhysicalSize::new(w, h)),
    }
}

/// The upload WebView, hidden until [`StravaView::show`]; dropping it closes it.
pub struct StravaView {
    webview: Rc<WebView>,
    handler: WryEventHandler,
    win: DesktopContext,
    /// Declared last so it outlives the WebView on drop.
    _context: WebContext,
}

impl StravaView {
    /// Starts the upload page, hidden. `on_word` gets what the page script reports.
    pub fn open(
        win: &DesktopContext,
        profile: &Path,
        file_name: &str,
        tcx: &str,
        details: &RideDetails,
        on_word: impl Fn(String) + 'static,
    ) -> Result<Self, String> {
        let mut context = WebContext::new(Some(profile.to_path_buf()));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(UPLOAD_URL)
            .with_visible(false)
            .with_bounds(bounds(win))
            .with_initialization_script(script(file_name, tcx, details))
            .with_ipc_handler(move |request| on_word(request.body().clone()))
            .build_as_child(&*win.window)
            .map_err(|err| err.to_string())?;
        let webview = Rc::new(webview);
        let follow = webview.clone();
        let window = win.clone();
        let handler = win.create_wry_event_handler(move |event, _| {
            if let Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                ..
            } = event
            {
                let _ = follow.set_bounds(bounds(&window));
            }
        });
        Ok(Self {
            webview,
            handler,
            win: win.clone(),
            _context: context,
        })
    }

    /// Puts the page below the shell's bar so the user can sign in or finish by hand.
    pub fn show(&self) {
        let _ = self.webview.set_bounds(bounds(&self.win));
        let _ = self.webview.set_visible(true);
    }
}

impl Drop for StravaView {
    fn drop(&mut self) {
        self.win.remove_wry_event_handler(self.handler);
        let _ = self.webview.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_starts_below_the_scaled_bar() {
        assert_eq!(body_rect(1600, 900, 1.0), (52, 1600, 848));
        assert_eq!(body_rect(2400, 1350, 1.5), (78, 2400, 1272));
    }

    #[test]
    fn a_tiny_or_odd_window_never_gives_a_zero_or_negative_size() {
        assert_eq!(body_rect(0, 10, 1.0), (52, 1, 1));
        assert_eq!(body_rect(800, 600, f64::NAN), (52, 800, 548));
    }

    #[test]
    fn the_script_embeds_the_file_and_the_details_as_json() {
        let details = RideDetails {
            title: " Esti \"kör\" ".into(),
            feel: Some(6),
            ..RideDetails::default()
        };
        let js = script("a\"b.tcx", "<x>\n\"</x>", &details);
        assert!(js.contains(r#"var NAME = "a\"b.tcx", XML = "<x>\n\"</x>", D = {"#));
        assert!(js.contains(r#""title":"Esti \"kör\"""#));
        assert!(js.contains(r#""virtual":true"#) && js.contains(r#""feel":6"#));
        assert!(js.contains(r#""visibility":"everyone""#));
        assert!(!js.contains("__NAME__") && !js.contains("__XML__") && !js.contains("__DETAILS__"));
    }

    #[test]
    fn the_words_of_the_page_are_told_apart() {
        assert_eq!(parse_word("login"), Word::Login);
        assert_eq!(parse_word("selected"), Word::Selected);
        assert_eq!(parse_word("saved"), Word::Saved);
        assert_eq!(parse_word("fields:{}"), Word::Fields("{}".into()));
        assert_eq!(
            parse_word("debug:{\"a\":1}"),
            Word::Stuck("{\"a\":1}".into())
        );
        assert_eq!(parse_word("whatever"), Word::Stuck("whatever".into()));
    }
}
