//! The app's process: started from a copy of the last binary that built, stopped the way an
//! orchestrator stops it.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

/// The app while it runs.
pub struct App {
    child: Child,
    started: Instant,
    /// The copy it runs from, removed once it stops.
    binary: PathBuf,
}

impl App {
    /// Starts `binary` from the project's folder. It runs from a copy, so that the next build
    /// can replace the binary while this one runs, on every system.
    pub fn start(binary: &Path, root: &Path, runs: &Path, generation: u64) -> std::io::Result<App> {
        std::fs::create_dir_all(runs)?;
        let name = binary.file_name().unwrap_or_default().to_string_lossy();
        let copy = runs.join(format!("{generation}-{name}"));
        std::fs::copy(binary, &copy)?;
        let child = Command::new(&copy).current_dir(root).spawn()?;
        Ok(App {
            child,
            started: Instant::now(),
            binary: copy,
        })
    }

    /// How long it has run.
    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// Whether it stopped on its own, and how.
    pub fn exited(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Asks it to stop (`SIGTERM`, as an orchestrator does), then kills it after `grace`.
    pub fn stop(mut self, grace: Duration) {
        terminate(&self.child);
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if self.exited().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.binary);
    }
}

#[cfg(unix)]
fn terminate(child: &Child) {
    // SAFETY: sending a signal to a process this supervisor started and has not reaped.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate(_: &Child) {
    // Without signals, the app is killed after the grace period: nothing more to do here.
}

/// What `status` says, for messages: `exit code 101`, `signal 11`.
pub fn describe(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "no exit code".to_string(),
    }
}
