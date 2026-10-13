//! The machine the CLI runs on: which tools it has, and how to run them. Behind a trait, so
//! that tests decide what the machine has.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What the CLI needs to know about, or do on, the machine.
pub trait Machine {
    /// Whether a program answers `--version`.
    fn installed(&self, program: &str) -> bool;
    /// Whether the Docker daemon answers.
    fn docker_running(&self) -> bool;
    /// The operating system, as prompts name it: `macOS`, `Windows`, `Ubuntu 24.04`.
    fn os(&self) -> String;
    /// Runs a program in `dir`, its output hidden; `Err` holds what it printed on error.
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<(), String>;
    /// Runs a program in `dir` with more environment variables, and returns what it printed.
    fn output(
        &self,
        program: &str,
        args: &[&str],
        dir: &Path,
        variables: &[(String, String)],
    ) -> Result<String, String>;
}

/// The real machine.
pub struct System;

impl Machine for System {
    fn installed(&self, program: &str) -> bool {
        Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn docker_running(&self) -> bool {
        // `docker info` waits for a daemon that does not answer: a few seconds at most.
        let Ok(mut child) = Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return false,
            }
        }
        let _ = child.kill();
        false
    }

    fn os(&self) -> String {
        match std::env::consts::OS {
            "macos" => "macOS".into(),
            "windows" => "Windows".into(),
            "linux" => linux_distribution().unwrap_or_else(|| "Linux".into()),
            other => other.into(),
        }
    }

    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<(), String> {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|error| error.to_string())?;
        match output.status.success() {
            true => Ok(()),
            false => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        }
    }

    fn output(
        &self,
        program: &str,
        args: &[&str],
        dir: &Path,
        variables: &[(String, String)],
    ) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .envs(variables.iter().map(|(name, value)| (name, value)))
            .output()
            .map_err(|error| error.to_string())?;
        match output.status.success() {
            true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
            false => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        }
    }
}

/// `Ubuntu 24.04`, from `/etc/os-release`.
fn linux_distribution() -> Option<String> {
    let release = std::fs::read_to_string("/etc/os-release").ok()?;
    release.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        Some(value.trim_matches('"').to_string())
    })
}

/// A tool `balise new` checks, and the two prompts given to the agent when it is missing.
pub struct Tool {
    pub program: &'static str,
    pub name: &'static str,
    /// Why Balise needs it, in a few words.
    pub why: &'static str,
}

pub const GIT: Tool = Tool {
    program: "git",
    name: "git",
    why: "to version the project, so that every change can be reviewed and undone",
};

pub const DOCKER: Tool = Tool {
    program: "docker",
    name: "Docker",
    why: "to run the app, its database and its front end in dev without installing them",
};

impl Tool {
    /// A prompt asking the agent how to install the tool on this system.
    pub fn install_prompt(&self, os: &str) -> String {
        format!(
            "I am on {os} and I need {name} for a Balise project, {why}. \
             Explain how to install it on my system, step by step, and how to check that it works.\n",
            name = self.name,
            why = self.why
        )
    }

    /// A prompt asking the agent for a short tutorial on the tool.
    pub fn learn_prompt(&self) -> String {
        format!(
            "I am starting a Balise project, which uses {name} {why}. \
             Give me a short tutorial: what {name} is, why Balise uses it, and the few commands worth knowing.\n",
            name = self.name,
            why = self.why
        )
    }
}
