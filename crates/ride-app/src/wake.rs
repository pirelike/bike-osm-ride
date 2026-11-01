//! Keeps Windows awake (no sleep, no screen blanking) while a ride is running.
//!
//! `SetThreadExecutionState` is per thread: every call must come from the same thread
//! (the UI thread, where the whole controller runs). The state is released automatically
//! when the thread ends, so a crash cannot leave the PC awake.

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

#[cfg(not(windows))]
mod imp {
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
        assert!(super::set(true));
        assert!(super::set(false));
    }
}
