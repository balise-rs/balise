//! Ctrl-C and `SIGTERM` for the supervisor: it stops the app before it stops itself.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

/// Set when the supervisor is asked to stop.
pub static STOP: AtomicBool = AtomicBool::new(false);

/// How many signals arrived, and the last one: `balise dev` passes each on to compose.
static RECEIVED: AtomicUsize = AtomicUsize::new(0);
static LAST: AtomicI32 = AtomicI32::new(0);

/// Catches `SIGINT` and `SIGTERM`. In a terminal, Ctrl-C reaches the app too, which starts
/// its own shutdown; in a container, only the supervisor gets `SIGTERM`, and passes it on.
pub fn install() {
    #[cfg(unix)]
    {
        extern "C" fn handle(signal: libc::c_int) {
            STOP.store(true, Ordering::SeqCst);
            LAST.store(signal, Ordering::SeqCst);
            RECEIVED.fetch_add(1, Ordering::SeqCst);
        }
        let handler = handle as extern "C" fn(libc::c_int) as libc::sighandler_t;
        // SAFETY: the handler only stores into atomics, which is signal-safe.
        unsafe {
            libc::signal(libc::SIGINT, handler);
            libc::signal(libc::SIGTERM, handler);
        }
    }
}

pub fn requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

/// The signals received after the first `seen`.
pub fn received_since(seen: usize) -> Option<(usize, i32)> {
    let received = RECEIVED.load(Ordering::SeqCst);
    (received > seen).then(|| (received, LAST.load(Ordering::SeqCst)))
}
