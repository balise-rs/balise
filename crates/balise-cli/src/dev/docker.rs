//! `balise dev` in Docker. The project's `compose.yaml` runs the supervisor (`balise dev
//! --inside`) in a container; this side checks Docker and the port, gives compose what it
//! derives from the configuration, and relays the container's logs.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use balise_core::error_codes::{DEV0004, DEV0005, DEV0006, DEV0007};
use balise_core::router::HEALTH_PATHS;

use balise_core::json;

use crate::check::Checked;
use crate::tools::{DOCKER, Machine};
use crate::{Failure, project};

/// How much longer than `shutdown.timeout` Docker waits before it kills the container: the
/// supervisor's own margin, and the time it takes to stop.
const STOP_MARGIN: Duration = Duration::from_secs(5);

/// The service of `compose.yaml` that runs the app.
const SERVICE: &str = "app";

/// The project's dev environment in Docker.
pub struct Compose {
    root: PathBuf,
    port: u16,
    api_base: String,
    /// The variables `compose.yaml` reads, derived from the configuration.
    variables: Vec<(String, String)>,
}

impl Compose {
    /// Checks that Docker answers, and reads what compose needs from the project at `root`.
    pub fn new(root: &Path, machine: &dyn Machine, color: bool) -> Result<Compose, Failure> {
        if !machine.installed(DOCKER.program) {
            return Err(Failure::problem(DEV0004, &[], &[]));
        }
        if !machine.docker_running() {
            return Err(Failure::problem(DEV0005, &[], &[]));
        }
        // The container does not get this machine's variables: only the files count.
        let config = project::config(root, &[]);
        let grace = config.shutdown_timeout + STOP_MARGIN;
        let mut variables = vec![
            ("BALISE_VERSION", env!("CARGO_PKG_VERSION").to_string()),
            ("BALISE_DEV_PORT", config.port.to_string()),
            ("BALISE_DEV_STOP_GRACE", format!("{}ms", grace.as_millis())),
            ("BALISE_DEV_USER", user()),
        ];
        write_env(root, &variables);
        if color {
            variables.push(("CLICOLOR_FORCE", "1".to_string()));
        }
        Ok(Compose {
            root: root.to_path_buf(),
            port: config.port,
            api_base: config.api_base(),
            variables: variables
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        })
    }

    /// Where the API answers on this machine.
    pub fn url(&self) -> String {
        format!("http://localhost:{}{}", self.port, self.api_base)
    }

    /// Whether the app's container runs.
    pub fn running(&self, machine: &dyn Machine) -> Result<bool, Failure> {
        let args = ["compose", "ps", "--status", "running", "--quiet", SERVICE];
        let ids = machine
            .output("docker", &args, &self.root, &self.variables)
            .map_err(|cause| failed("ps", &cause))?;
        Ok(!ids.trim().is_empty())
    }

    /// Whether the app answers its health check.
    pub fn answers(&self) -> bool {
        let Ok(mut stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            HEALTH_PATHS[0]
        );
        let mut response = String::new();
        stream.write_all(request.as_bytes()).is_ok()
            && stream.read_to_string(&mut response).is_ok()
            && response.starts_with("HTTP/1.1 200")
    }

    /// Builds the image and starts the container: in the foreground, its logs relayed until
    /// Ctrl-C stops it, or in the background.
    pub fn up(&self, machine: &dyn Machine, detach: bool) -> Result<(), Failure> {
        // A container of this project already holds the port: compose attaches to it.
        if !self.running(machine)? && TcpListener::bind((Ipv4Addr::LOCALHOST, self.port)).is_err() {
            let other = self.port.saturating_add(1).to_string();
            return Err(Failure::problem(
                DEV0006,
                &[("port", &self.port.to_string())],
                &[("other", &other)],
            ));
        }
        let args: &[&str] = match detach {
            true => &["up", "--build", "--detach", "--wait"],
            false => &["up", "--build", "--no-log-prefix"],
        };
        // Docker would create the mount point of `target/` itself, as root on Linux, where
        // `cargo` on this machine could not write any more.
        let _ = std::fs::create_dir_all(self.root.join("target"));
        self.run(args)
    }

    /// Runs `balise check` in the dev container, the running one or a temporary one, and
    /// reads what it found.
    pub fn check(&self, running: bool, ci: bool) -> Result<Checked, Failure> {
        let mut args = vec!["--json"];
        if ci {
            args.push("--ci");
        }
        let (command, output) = self.run_in(running, r#"exec balise-cli check "$@""#, &args)?;
        let value = last_json(&output).ok_or_else(|| failed(command, &cause(&output)))?;
        if let Some(failure) = Failure::from_json(&value) {
            return Err(failure);
        }
        Checked::from_value(&value).ok_or_else(|| failed(command, "unexpected output"))
    }

    /// Runs a shell `script` with `args` in the dev container: `docker compose exec` in the
    /// running one, `docker compose run` in a temporary one. Its standard error is shown.
    pub fn run_in(
        &self,
        running: bool,
        script: &str,
        args: &[&str],
    ) -> Result<(&'static str, Output), Failure> {
        let (command, how): (&str, &[&str]) = match running {
            true => ("exec", &["exec", "-T", SERVICE]),
            false => ("run", &["run", "--rm", "--no-deps", "-T", SERVICE]),
        };
        // A temporary container starts from an image up to date; building it quietly keeps
        // its progress out of the answer.
        if !running {
            let built = Command::new("docker")
                .args(["compose", "build", "--quiet", SERVICE])
                .current_dir(&self.root)
                .envs(self.variables.iter().map(|(name, value)| (name, value)))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status()
                .map_err(|error| failed("build", &error.to_string()))?;
            if !built.success() {
                return Err(failed("build", &super::app::describe(built)));
            }
        }
        let output = Command::new("docker")
            .arg("compose")
            .args(how)
            .args(["sh", "-c", script, "sh"])
            .args(args)
            .current_dir(&self.root)
            .envs(self.variables.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|error| failed(command, &error.to_string()))?;
        Ok((command, output))
    }

    /// Stops and removes the container. The volumes stay: the next start does not compile
    /// everything again.
    pub fn down(&self, volumes: bool) -> Result<(), Failure> {
        match volumes {
            true => self.run(&["down", "--volumes"]),
            false => self.run(&["down"]),
        }
    }

    /// Runs `docker compose`, its output shown. Ctrl-C and `SIGTERM` reach it through this
    /// process, one by one: the first stops the container cleanly, a second one kills it.
    fn run(&self, args: &[&str]) -> Result<(), Failure> {
        super::catch_signals();
        let mut command = Command::new("docker");
        command
            .arg("compose")
            .args(args)
            .current_dir(&self.root)
            .envs(self.variables.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null());
        // Its own group: the terminal's Ctrl-C reaches it only once, through this process.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command
            .spawn()
            .map_err(|error| failed(args[0], &error.to_string()))?;
        let mut seen = 0;
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| failed(args[0], &e.to_string()))?
            {
                break status;
            }
            if let Some((received, signal)) = super::signal::received_since(seen) {
                // One signal at a time, so that compose reacts to each: the first stops the
                // container cleanly, the second forces it; past that, compose does not
                // answer, and is killed so that the terminal is given back.
                for count in seen + 1..=received {
                    match count {
                        1 => eprintln!("Stopping the app… Ctrl-C again to force."),
                        2 => eprintln!("Forcing… Ctrl-C again to leave without waiting."),
                        _ => {
                            let _ = child.kill();
                            let _ = child.wait();
                            eprintln!(
                                "Left without waiting for Docker: `balise dev status` says what runs, `balise dev stop` stops it."
                            );
                            return Ok(());
                        }
                    }
                    forward(&child, signal);
                }
                seen = received;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        match status.success() || super::stop_requested() {
            true => Ok(()),
            false => Err(failed(args[0], &super::app::describe(status))),
        }
    }
}

#[cfg(unix)]
fn forward(child: &Child, signal: i32) {
    // SAFETY: sending a signal to a process this one started and has not reaped.
    unsafe {
        libc::kill(child.id() as libc::pid_t, signal);
    }
}

#[cfg(not(unix))]
fn forward(_: &Child, _: i32) {
    // Ctrl-C reaches every process of the console on Windows: compose already has it.
}

/// The block of `.env` that `balise dev` writes, between these lines.
const ENV_START: &str =
    "# Written by `balise dev` from balise.toml, for `docker compose`: change balise.toml instead.";
const ENV_END: &str = "# End of what `balise dev` writes.";

/// Writes the variables `compose.yaml` reads in the project's `.env`, which compose reads on
/// its own: `docker compose` then works by hand too, with the values of the last `balise`
/// command. The rest of the file is the user's, and kept as it is. A failure only costs
/// `docker compose` by hand, so it is not reported.
fn write_env(root: &Path, variables: &[(&str, String)]) {
    let path = root.join(".env");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut block = format!("{ENV_START}\n");
    for (name, value) in variables {
        block.push_str(&format!("{name}={value}\n"));
    }
    block.push_str(ENV_END);
    let updated = match (current.find(ENV_START), current.find(ENV_END)) {
        (Some(start), Some(end)) if end > start => {
            format!(
                "{}{block}{}",
                &current[..start],
                &current[end + ENV_END.len()..]
            )
        }
        _ => {
            let mut text = current.clone();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&block);
            text.push('\n');
            text
        }
    };
    if updated != current {
        let _ = std::fs::write(&path, updated);
    }
}

/// What a command run without Rust on the machine says about it.
pub fn note(running: bool) -> &'static str {
    match running {
        true => {
            "No Rust on this machine: ran in the running dev container, which is slower. Installing Rust (https://rustup.rs) makes it faster."
        }
        false => {
            "No Rust on this machine: ran in a temporary dev container, which is slower. Installing Rust (https://rustup.rs), or keeping `balise dev` running, makes it faster."
        }
    }
}

/// The last line of standard output as JSON.
pub fn last_json(output: &Output) -> Option<json::Value> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.trim().lines().last()?;
    json::parse(last.as_bytes()).ok()
}

/// Why a run gave nothing usable: its last line, or how it ended.
pub fn cause(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.trim().lines().last() {
        Some(last) => last.to_string(),
        None => super::app::describe(output.status),
    }
}

fn failed(command: &str, cause: &str) -> Failure {
    Failure::problem(DEV0007, &[("command", command), ("cause", cause)], &[])
}

/// The user the container runs as: this machine's, so that what the app writes in the
/// project (uploads, crash reports) belongs to the user.
#[cfg(unix)]
fn user() -> String {
    // SAFETY: these calls only read the process's ids.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    format!("{uid}:{gid}")
}

#[cfg(not(unix))]
fn user() -> String {
    // Docker Desktop maps the files of the project to the user on its own.
    "root".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_block_is_written_once_and_the_rest_kept() {
        let root = std::env::temp_dir().join("balise-env-block");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        write_env(&root, &[("BALISE_DEV_PORT", "3000".to_string())]);
        write_env(&root, &[("BALISE_DEV_PORT", "3001".to_string())]);
        let text = std::fs::read_to_string(root.join(".env")).unwrap();
        assert_eq!(
            text,
            format!("SECRET=1\n\n{ENV_START}\nBALISE_DEV_PORT=3001\n{ENV_END}\n")
        );
    }
}
