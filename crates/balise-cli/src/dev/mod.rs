//! The dev supervisor, the same on the machine (`--native`) and in the container
//! (`--inside`): it watches the files, rebuilds, restarts the app, and writes one stream of
//! logs. While a build runs, or fails, the last version that built keeps answering.

mod app;
mod build;
pub mod docker;
mod signal;
mod watch;

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use balise_core::error_codes::{DEV0001, DEV0002};
use balise_core::event::{Event, Level};

use crate::{Failure, project};
use app::App;
use build::Built;
use watch::Watcher;

pub use signal::install as catch_signals;

/// How often the files are read.
const POLL: Duration = Duration::from_millis(300);

/// A change is complete once the files stop changing for this long: an editor saving several
/// files is one change.
const SETTLE: Duration = Duration::from_millis(150);

/// An app that stops sooner than this after starting would stop again: `balise dev` waits
/// for a change instead of starting it again.
const STARTUP: Duration = Duration::from_secs(2);

/// How much longer than `shutdown.timeout` the app is given to stop before it is killed.
pub const GRACE_MARGIN: Duration = Duration::from_secs(2);

/// Runs the supervisor of the project at `root` until `stop` says so.
pub fn supervise(
    root: &Path,
    out: &mut dyn Write,
    color: bool,
    stop: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    let name = project::package_name(root).unwrap_or_else(|| "app".to_string());
    let target = project::target_dir(root);
    let binary = target
        .join("debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let runs = target.join("balise-dev");
    // The app gets the time its own shutdown takes, and a little more. The app inherits
    // this process's variables: the configuration is read with them.
    let variables: Vec<(String, String)> = std::env::vars().collect();
    let config = project::config(root, &variables);
    let grace = config.shutdown_timeout + GRACE_MARGIN;
    // In a container, the port is published on the machine as it is.
    let ready = format!(
        "Ready: http://localhost:{}{}",
        config.port,
        config.api_base()
    );
    balise_core::journal::open(root);
    let say = |out: &mut dyn Write, event: Event| {
        let _ = writeln!(out, "{}", event.render_text(color));
        let _ = out.flush();
        balise_core::journal::event(&event);
    };

    let mut watcher = Watcher::new(root);
    let mut app: Option<App> = None;
    let mut generation = 0;
    // Whether the app started and does not listen yet.
    let mut starting = false;
    let mut rebuild = Some(format!("Building {name}"));
    loop {
        if stop() {
            if let Some(app) = app.take() {
                app.stop(grace);
            }
            return Ok(());
        }
        if let Some(reason) = rebuild.take() {
            say(out, Event::new(Level::Info, reason));
            match build::build(root)? {
                Built::Ok { seconds, warnings } => {
                    let warnings = warnings.map(|w| format!(", {w}")).unwrap_or_default();
                    let restart = if app.is_some() {
                        "; restarting the app"
                    } else {
                        ""
                    };
                    say(
                        out,
                        Event::new(
                            Level::Info,
                            format!("Built in {seconds:.1} s{warnings}{restart}"),
                        ),
                    );
                    if let Some(old) = app.take() {
                        old.stop(grace);
                    }
                    generation += 1;
                    app = Some(start(&binary, root, &runs, generation)?);
                    starting = true;
                }
                Built::Failed { errors, lines } => {
                    for line in lines {
                        let _ = writeln!(out, "  {line}");
                    }
                    say(
                        out,
                        Event::coded(Level::Warn, DEV0002, &[("errors", &errors)]),
                    );
                }
            }
        }
        std::thread::sleep(POLL);
        if starting && app.is_some() && listens(config.port) {
            starting = false;
            say(out, Event::new(Level::Info, ready.as_str()));
        }

        let mut changes = watcher.changes();
        if !changes.is_empty() {
            let since = Instant::now();
            while since.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(SETTLE);
                let more = watcher.changes();
                if more.is_empty() {
                    break;
                }
                changes.extend(more);
            }
            changes.sort();
            changes.dedup();
            let what = match changes.as_slice() {
                [one] => format!("{} changed", one.display()),
                many => format!("{} files changed", many.len()),
            };
            rebuild = Some(format!("Rebuilding: {what}"));
        }

        if let Some(running) = app.as_mut()
            && let Some(status) = running.exited()
        {
            let uptime = running.uptime();
            app = None;
            say(
                out,
                Event::coded(Level::Error, DEV0001, &[("status", &app::describe(status))]),
            );
            if uptime > STARTUP && rebuild.is_none() {
                say(out, Event::new(Level::Info, "Starting the app again"));
                generation += 1;
                app = Some(start(&binary, root, &runs, generation)?);
                starting = true;
            }
        }
    }
}

/// Whether the app accepts connections: a connection alone, which the app does not log.
fn listens(port: u16) -> bool {
    let address = (std::net::Ipv4Addr::LOCALHOST, port).into();
    std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok()
}

fn start(binary: &Path, root: &Path, runs: &Path, generation: u64) -> Result<App, Failure> {
    App::start(binary, root, runs, generation).map_err(|error| {
        Failure::problem(
            balise_core::error_codes::DEV0003,
            &[(
                "cause",
                &format!("cannot start {}: {error}", binary.display()),
            )],
            &[],
        )
    })
}

/// Whether Ctrl-C or `SIGTERM` asked the supervisor to stop.
pub fn stop_requested() -> bool {
    signal::requested()
}
