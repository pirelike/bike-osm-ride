//! The browser panel: a real Firefox window docked into the ride screen, so a series or any other
//! content can run next to the ride.
//!
//! Firefox is started with a profile of its own (`-no-remote`, so it never joins the Firefox the
//! user may already have open, and its logins and settings survive between rides), its window is
//! found, turned into a child window of the app window and kept on top of the panel's body. The
//! web page UI of the app cannot draw over a native child window, so the shell hides it while a
//! dialog is open or a panel is being dragged. Closing the panel closes Firefox.
//!
//! Windows only (the app targets Windows); elsewhere every call reports that it is unavailable.

use std::path::{Path, PathBuf};

/// Where Firefox's window should be, in physical pixels of the app window's client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Slot {
    /// A slot from CSS pixels and the device pixel ratio, pulled in by `inset` CSS pixels on every
    /// side so the panel's borders and the dividers stay reachable around the native window.
    pub fn from_css(x: f64, y: f64, w: f64, h: f64, dpr: f64, inset: f64) -> Option<Slot> {
        let ok = [x, y, w, h, dpr].iter().all(|v| v.is_finite()) && dpr > 0.0;
        if !ok || w - 2.0 * inset < 40.0 || h - 2.0 * inset < 40.0 {
            return None;
        }
        Some(Slot {
            x: ((x + inset) * dpr).round() as i32,
            y: ((y + inset) * dpr).round() as i32,
            w: ((w - 2.0 * inset) * dpr).round() as i32,
            h: ((h - 2.0 * inset) * dpr).round() as i32,
        })
    }
}

/// The Firefox that is installed, if any.
pub fn firefox_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(Path::new(&base).join("Mozilla Firefox").join("firefox.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::process::Command;
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, MapWindowPoints, SetWindowRgn};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetParent, GetWindowLongPtrW, GetWindowRect, IsWindow,
        IsWindowVisible, PostMessageW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SW_HIDE, SW_RESTORE, SW_SHOWNA, WM_CLOSE, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPSIBLINGS,
        WS_DLGFRAME, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_WINDOWEDGE,
        WS_MAXIMIZE, WS_MAXIMIZEBOX, WS_MINIMIZE, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME, WS_VISIBLE,
    };

    use super::Slot;

    const USER_CHROME: &str = ".titlebar-buttonbox-container { display: none !important; }\n";
    const USER_PREFS: &str =
        "user_pref(\"toolkit.legacyUserProfileCustomizations.stylesheets\", true);\n\
        user_pref(\"browser.aboutwelcome.enabled\", false);\n\
        user_pref(\"browser.shell.checkDefaultBrowser\", false);\n\
        user_pref(\"browser.startup.homepage_override.mstone\", \"ignore\");\n\
        user_pref(\"datareporting.policy.dataSubmissionPolicyBypassNotification\", true);\n";

    /// The docked Firefox window, once there is one.
    static DOCKED: Mutex<Option<isize>> = Mutex::new(None);
    /// Where the docked window was last put and whether it is shown (see [`enforce`]).
    static PLACED: Mutex<Option<(Slot, bool)>> = Mutex::new(None);

    fn remember(slot: Slot, visible: bool) {
        if let Ok(mut placed) = PLACED.lock() {
            *placed = Some((slot, visible));
        }
    }

    fn class_of(hwnd: HWND) -> String {
        let mut buf = [0u16; 64];
        // SAFETY: the buffer is valid for 64 u16 and the length passed is 64.
        let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Every visible top-level Firefox window right now.
    pub fn firefox_windows() -> Vec<isize> {
        unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> i32 {
            // SAFETY: `lparam` is the pointer to the Vec below, alive for the whole EnumWindows call.
            let found = unsafe { &mut *(lparam as *mut Vec<isize>) };
            // SAFETY: plain window queries on a handle EnumWindows just handed out.
            if unsafe { IsWindowVisible(hwnd) } != 0 && class_of(hwnd) == "MozillaWindowClass" {
                found.push(hwnd as isize);
            }
            TRUE
        }
        let mut found: Vec<isize> = Vec::new();
        // SAFETY: the callback only touches `found`, which outlives the call.
        unsafe { EnumWindows(Some(each), &mut found as *mut Vec<isize> as LPARAM) };
        found
    }

    /// Starts Firefox with the app's own profile.
    pub fn launch(exe: &Path, profile: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(profile.join("chrome"))?;
        // The panel has its own title bar and close button: hide Firefox's window buttons (a
        // click on them would minimise or close a window that is not a window any more), and
        // skip the first-run pages.
        std::fs::write(profile.join("chrome").join("userChrome.css"), USER_CHROME)?;
        std::fs::write(profile.join("user.js"), USER_PREFS)?;
        Command::new(exe)
            .arg("-no-remote")
            .arg("-profile")
            .arg(profile)
            .arg("-new-window")
            .arg("about:home")
            .spawn()
            .map(|_| ())
    }

    pub fn is_docked() -> bool {
        DOCKED
            .lock()
            .ok()
            .and_then(|d| *d)
            .is_some_and(|h| unsafe { IsWindow(h as HWND) } != 0)
    }

    /// Clips the window to `slot`: Firefox cannot be made smaller than its own minimum size, and
    /// what does not fit must not spill over the neighbouring panels.
    fn clip(child: HWND, slot: Slot) {
        // SAFETY: the region is handed over to the window, which owns (and frees) it.
        unsafe {
            let region = CreateRectRgn(0, 0, slot.w, slot.h);
            SetWindowRgn(child, region, 1);
        }
    }

    /// Turns the Firefox window `hwnd` into a child of `parent` at `slot`.
    pub fn dock(hwnd: isize, parent: isize, slot: Slot) {
        let (child, parent) = (hwnd as HWND, parent as HWND);
        // SAFETY: both handles are live windows; the calls only change their style and place.
        unsafe {
            ShowWindow(child, SW_RESTORE);
            let style = GetWindowLongPtrW(child, GWL_STYLE) as u32;
            // a remembered "maximised" state would make the child cover the whole app window
            let frame = WS_POPUP
                | WS_MAXIMIZE
                | WS_MINIMIZE
                | WS_CAPTION
                | WS_THICKFRAME
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_BORDER
                | WS_DLGFRAME;
            // child style first, then the new parent (what the SetParent documentation asks for)
            SetWindowLongPtrW(
                child,
                GWL_STYLE,
                ((style & !frame) | WS_CHILD | WS_CLIPSIBLINGS | WS_VISIBLE) as isize,
            );
            let ex = GetWindowLongPtrW(child, GWL_EXSTYLE) as u32;
            let ex_frame =
                WS_EX_APPWINDOW | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME;
            SetWindowLongPtrW(child, GWL_EXSTYLE, (ex & !ex_frame) as isize);
            SetParent(child, parent);
            SetWindowPos(
                child,
                HWND_TOP,
                slot.x,
                slot.y,
                slot.w,
                slot.h,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            clip(child, slot);
        }
        if let Ok(mut docked) = DOCKED.lock() {
            *docked = Some(hwnd);
        }
        remember(slot, true);
    }

    /// Where the docked window is now, in its parent's client coordinates.
    fn current_rect(child: HWND) -> Option<Slot> {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: `r` is a valid RECT; the parent handle comes from the live child.
        unsafe {
            if GetWindowRect(child, &mut r) == 0 {
                return None;
            }
            let parent = GetParent(child);
            let mut corners = [
                POINT {
                    x: r.left,
                    y: r.top,
                },
                POINT {
                    x: r.right,
                    y: r.bottom,
                },
            ];
            MapWindowPoints(std::ptr::null_mut(), parent, corners.as_mut_ptr(), 2);
            Some(Slot {
                x: corners[0].x,
                y: corners[0].y,
                w: corners[1].x - corners[0].x,
                h: corners[1].y - corners[0].y,
            })
        }
    }

    /// Firefox restores its remembered position, size and maximised state a moment after it
    /// starts (and sometimes later); this puts the docked window back over the panel and strips
    /// a maximised / minimised state again. Called periodically while the panel is open.
    pub fn enforce() {
        let Some(hwnd) = DOCKED.lock().ok().and_then(|d| *d) else {
            return;
        };
        let Some((slot, visible)) = PLACED.lock().ok().and_then(|p| *p) else {
            return;
        };
        let child = hwnd as HWND;
        // SAFETY: plain queries and style changes on a window that may be gone (calls then fail).
        unsafe {
            if IsWindow(child) == 0 || !visible {
                return;
            }
            let style = GetWindowLongPtrW(child, GWL_STYLE) as u32;
            let state = WS_MAXIMIZE | WS_MINIMIZE | WS_POPUP | WS_CAPTION | WS_THICKFRAME;
            let restyled = style & state != 0;
            if restyled {
                SetWindowLongPtrW(
                    child,
                    GWL_STYLE,
                    ((style & !state) | WS_CHILD | WS_CLIPSIBLINGS | WS_VISIBLE) as isize,
                );
            }
            if restyled || current_rect(child) != Some(slot) {
                SetWindowPos(
                    child,
                    HWND_TOP,
                    slot.x,
                    slot.y,
                    slot.w,
                    slot.h,
                    SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                clip(child, slot);
            }
        }
    }

    /// Moves the docked window; `visible = false` hides it without closing Firefox.
    pub fn place(slot: Slot, visible: bool) {
        let Some(hwnd) = DOCKED.lock().ok().and_then(|d| *d) else {
            return;
        };
        let child = hwnd as HWND;
        // SAFETY: a plain move of a window that may have been closed meanwhile (the call then fails).
        unsafe {
            if IsWindow(child) == 0 {
                return;
            }
            SetWindowPos(
                child,
                HWND_TOP,
                slot.x,
                slot.y,
                slot.w,
                slot.h,
                SWP_NOACTIVATE | if visible { SWP_SHOWWINDOW } else { 0 },
            );
            clip(child, slot);
            ShowWindow(child, if visible { SW_SHOWNA } else { SW_HIDE });
        }
        remember(slot, visible);
    }

    pub fn hide() {
        // the watchdog must not bring a hidden window back
        if let Ok(mut placed) = PLACED.lock() {
            if let Some((_, visible)) = placed.as_mut() {
                *visible = false;
            }
        }
        let Some(hwnd) = DOCKED.lock().ok().and_then(|d| *d) else {
            return;
        };
        // SAFETY: see `place`.
        unsafe {
            if IsWindow(hwnd as HWND) != 0 {
                ShowWindow(hwnd as HWND, SW_HIDE);
            }
        }
    }

    /// Asks Firefox to close (it restores its tabs the next time) and forgets the window.
    pub fn close() {
        if let Ok(mut placed) = PLACED.lock() {
            *placed = None;
        }
        let taken = DOCKED.lock().ok().and_then(|mut d| d.take());
        if let Some(hwnd) = taken {
            // SAFETY: posting a message to a window that may be gone is harmless.
            unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    use super::Slot;

    pub fn firefox_windows() -> Vec<isize> {
        Vec::new()
    }
    pub fn launch(_exe: &Path, _profile: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("only available on Windows"))
    }
    pub fn is_docked() -> bool {
        false
    }
    pub fn dock(_hwnd: isize, _parent: isize, _slot: Slot) {}
    pub fn place(_slot: Slot, _visible: bool) {}
    pub fn hide() {}
    pub fn close() {}
    pub fn enforce() {}
}

pub use imp::{close, dock, enforce, firefox_windows, hide, is_docked, launch, place};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_physical_pixels_inside_the_panel() {
        let s = Slot::from_css(100.0, 50.0, 400.0, 300.0, 1.5, 4.0).expect("slot");
        assert_eq!(
            s,
            Slot {
                x: 156,
                y: 81,
                w: 588,
                h: 438
            }
        );
    }

    #[test]
    fn a_tiny_or_broken_panel_gets_no_slot() {
        assert_eq!(Slot::from_css(0.0, 0.0, 30.0, 300.0, 1.0, 4.0), None);
        assert_eq!(Slot::from_css(0.0, 0.0, 300.0, 40.0, 1.0, 4.0), None);
        assert_eq!(Slot::from_css(f64::NAN, 0.0, 300.0, 300.0, 1.0, 4.0), None);
        assert_eq!(Slot::from_css(0.0, 0.0, 300.0, 300.0, 0.0, 4.0), None);
    }
}
