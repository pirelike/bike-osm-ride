//! Keeps the machine awake (no sleep, no screen blanking) while a ride is running.
//!
//! Every platform has its own mechanism, and all of them are tied to the process: when the app
//! exits — even by crashing — the system drops the request on its own, so a forgotten ride can
//! never keep a PC awake forever.
//!
//! * **Windows**: `SetThreadExecutionState` is per thread, so every call must come from the same
//!   thread (the UI thread, where the whole controller runs). The state is released when the
//!   thread ends.
//! * **Linux**: `org.freedesktop.ScreenSaver.Inhibit` on the session bus (works under GNOME,
//!   KDE, Xfce, … — the interface is the de-facto standard). The cookie is released with
//!   `UnInhibit`; closing the bus connection releases it too.
//! * **macOS**: an IOKit power assertion (`IOPMAssertionCreateWithName` with
//!   `PreventUserIdleDisplaySleep`), released with `IOPMAssertionRelease`.
//! * **Android**: `FLAG_KEEP_SCREEN_ON` on the activity's window, set through JNI
//!   (`android_picker::set_keep_screen_on`). The flag belongs to the window, so it goes away with
//!   it. Not verified on a device (see `docs/ANDROID.md`).

#[cfg(windows)]
mod imp {
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
    const ES_CONTINUOUS: u32 = 0x8000_0000;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(es_flags: u32) -> u32;
    }

    pub fn set(on: bool) -> bool {
        let flags = if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: plain Win32 call with a flag value, no pointers involved.
        unsafe { SetThreadExecutionState(flags) != 0 }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::cell::RefCell;
    use std::time::Duration;

    use dbus::blocking::Connection;

    /// Shown by the desktop in its power settings next to the inhibition.
    const APP: &str = "bike-osm-ride";
    /// Looked up at call time: the catalog is not a `const` and the language can change.
    fn reason() -> &'static str {
        ride_i18n::t("app.wake.tekeres_folyamatban")
    }
    const SERVICE: &str = "org.freedesktop.ScreenSaver";
    const PATH: &str = "/org/freedesktop/ScreenSaver";
    const INTERFACE: &str = "org.freedesktop.ScreenSaver";
    /// The screen saver answers in microseconds; a stuck one must not freeze the UI for long.
    const TIMEOUT: Duration = Duration::from_secs(1);

    thread_local! {
        /// The live inhibition. Dropping the connection (release, exit, crash) also drops the
        /// inhibition, which is why the cookie is stored together with it.
        static INHIBIT: RefCell<Option<(Connection, u32)>> = const { RefCell::new(None) };
    }

    pub fn set(on: bool) -> bool {
        if on {
            INHIBIT.with(|slot| {
                let Ok(mut slot) = slot.try_borrow_mut() else {
                    return false;
                };
                if slot.is_some() {
                    return true; // already inhibited: never stack two cookies
                }
                match inhibit() {
                    Some(pair) => {
                        *slot = Some(pair);
                        true
                    }
                    None => false,
                }
            })
        } else {
            let taken =
                INHIBIT.with(|slot| slot.try_borrow_mut().ok().and_then(|mut slot| slot.take()));
            match taken {
                None => true, // nothing to release
                Some((conn, cookie)) => release(&conn, cookie),
            }
        }
    }

    fn inhibit() -> Option<(Connection, u32)> {
        let conn = Connection::new_session().ok()?;
        let (cookie,): (u32,) = conn
            .with_proxy(SERVICE, PATH, TIMEOUT)
            .method_call(INTERFACE, "Inhibit", (APP, reason()))
            .ok()?;
        Some((conn, cookie))
    }

    fn release(conn: &Connection, cookie: u32) -> bool {
        let released: Result<(), dbus::Error> = conn
            .with_proxy(SERVICE, PATH, TIMEOUT)
            .method_call(INTERFACE, "UnInhibit", (cookie,));
        released.is_ok()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;

    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::string::CFString;

    /// `kIOPMAssertionTypePreventUserIdleDisplaySleep`: keep the display (and with it the system)
    /// awake, which is what a rider staring at the map wants.
    const ASSERTION: &str = "PreventUserIdleDisplaySleep";
    /// `kIOPMAssertionLevelOn`
    const LEVEL_ON: u32 = 255;
    /// `kIOReturnSuccess`
    const SUCCESS: i32 = 0;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFTypeRef,
            assertion_level: u32,
            assertion_name: CFTypeRef,
            assertion_id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(assertion_id: u32) -> i32;
    }

    thread_local! {
        /// The live assertion id. The system drops the assertion when the process exits.
        static ASSERTION_ID: RefCell<Option<u32>> = const { RefCell::new(None) };
    }

    pub fn set(on: bool) -> bool {
        if on {
            ASSERTION_ID.with(|slot| {
                let Ok(mut slot) = slot.try_borrow_mut() else {
                    return false;
                };
                if slot.is_some() {
                    return true; // already asserted: never stack two ids
                }
                // The two CFStrings must outlive the call: they are moved into the assertion id
                // only on success, so keep them alive until we return.
                let kind = CFString::new(ASSERTION);
                let name =
                    CFString::new(ride_i18n::t("app.wake.bike_osm_ride_tekeres_folyamatban"));
                let mut id: u32 = 0;
                // SAFETY: both strings are alive for the whole call and the API writes `id`
                // only on success (which we check before using it).
                let status = unsafe {
                    IOPMAssertionCreateWithName(
                        kind.as_concrete_TypeRef() as CFTypeRef,
                        LEVEL_ON,
                        name.as_concrete_TypeRef() as CFTypeRef,
                        &mut id,
                    )
                };
                if status == SUCCESS {
                    *slot = Some(id);
                    true
                } else {
                    false
                }
            })
        } else {
            let taken = ASSERTION_ID
                .with(|slot| slot.try_borrow_mut().ok().and_then(|mut slot| slot.take()));
            match taken {
                None => true, // nothing to release
                // SAFETY: `id` came from a successful create and is released exactly once.
                Some(id) => (unsafe { IOPMAssertionRelease(id) }) == SUCCESS,
            }
        }
    }
}

#[cfg(target_os = "android")]
mod imp {
    /// `FLAG_KEEP_SCREEN_ON` on the activity's window (`MainActivity.setKeepScreenOn`).
    pub fn set(on: bool) -> bool {
        crate::android_picker::set_keep_screen_on(on).is_ok()
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "macos",
    target_os = "android"
)))]
mod imp {
    /// The remaining unixes: nothing to do, the platform manages it.
    pub fn set(_on: bool) -> bool {
        true
    }
}

/// Requests (`true`) or releases (`false`) the keep-awake state. Returns whether the OS accepted it.
pub fn set(on: bool) -> bool {
    imp::set(on)
}

#[cfg(test)]
mod tests {
    #[test]
    fn can_be_toggled() {
        let on = super::set(true);
        let off = super::set(false);
        // Windows always has the API; on Linux there may be no session bus (CI, containers) and
        // on macOS the request can be refused, which is fine — but nothing may panic.
        if cfg!(windows) {
            assert!(on, "SetThreadExecutionState(true) must succeed");
            assert!(off, "SetThreadExecutionState(false) must succeed");
        }
        let _ = (on, off);
    }

    #[test]
    fn releasing_twice_is_fine() {
        assert!(super::set(false));
        assert!(super::set(false));
    }
}
