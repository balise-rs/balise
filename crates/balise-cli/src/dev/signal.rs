//! Ctrl-C and `SIGTERM` for the supervisor: it stops the app before it stops itself.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set when the supervisor is asked to stop.
pub static STOP: AtomicBool = AtomicBool::new(false);

/// Catches `SIGINT` and `SIGTERM`. In a terminal, Ctrl-C reaches the app too, which starts
/// its own shutdown; in a container, only the supervisor gets `SIGTERM`, and passes it on.
pub fn install() {
    #[cfg(unix)]
    {
        extern "C" fn handle(_: libc::c_int) {
            STOP.store(true, Ordering::SeqCst);
        }
        let handler = handle as extern "C" fn(libc::c_int) as libc::sighandler_t;
        // SAFETY: the handler only stores into an atomic, which is signal-safe.
        unsafe {
            libc::signal(libc::SIGINT, handler);
            libc::signal(libc::SIGTERM, handler);
        }
    }
}

pub fn requested() -> bool {
    STOP.load(Ordering::SeqCst)
}
