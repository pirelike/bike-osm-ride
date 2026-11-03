//! Panic containment for calls into the other workstream crates.
//!
//! The shell talks to the route loader, the persistence layer, the ride engine and the
//! sensor hub. None of them may bring the whole window down: a panic inside one of those
//! calls (a bug, or a not-yet-implemented stub) is turned into an error value that the
//! caller reports as a toast and then carries on.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Runs `f` and converts a panic into `Err(message)`. `what` names the call site in the log.
///
/// Every panic caught here also goes to the crash log (`crash.rs`), so an Android report without
/// a console still says what happened.
pub fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        let message = panic_message(payload.as_ref());
        eprintln!("[ride-app] {what}: panic: {message}");
        crate::crash::record(what, &message);
        message
    })
}

/// Best-effort text of a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "ismeretlen belső hiba".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_values_through() {
        assert_eq!(guarded("ok", || 41 + 1), Ok(42));
    }

    #[test]
    fn contains_str_and_string_panics() {
        let a = guarded("str", || -> u8 { panic!("kapcsolat") });
        assert_eq!(a, Err("kapcsolat".to_string()));
        let n = 7;
        let b = guarded("string", || -> u8 { panic!("érték: {n}") });
        assert_eq!(b, Err("érték: 7".to_string()));
    }
}
