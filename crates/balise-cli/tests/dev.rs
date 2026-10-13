//! `balise dev --native` on a generated project: it builds, restarts on change, keeps the
//! last version running while the code does not compile, and stops the app with itself.
//! `balise dev` in Docker: what it checks before starting the container.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use balise_cli::{Context, Machine, run};

/// A machine with Docker as the test says, and no container running.
struct Quiet {
    docker: Option<bool>,
}

impl Machine for Quiet {
    fn installed(&self, program: &str) -> bool {
        program == "docker" && self.docker.is_some()
    }
    fn docker_running(&self) -> bool {
        self.docker == Some(true)
    }
    fn os(&self) -> String {
        "Linux".into()
    }
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<(), String> {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|e| e.to_string())?;
        match output.status.success() {
            true => Ok(()),
            false => Err(String::from_utf8_lossy(&output.stderr).into_owned()),
        }
    }
    fn output(
        &self,
        _: &str,
        _: &[&str],
        _: &Path,
        _: &[(String, String)],
    ) -> Result<String, String> {
        Ok(String::new())
    }
}

/// Logs written by the supervisor, readable while it runs.
#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Write for Logs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Logs {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

fn get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

/// Waits until `check` holds, for `limit` at most.
fn wait(limit: Duration, what: &str, logs: &Logs, check: impl Fn() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(
            started.elapsed() < limit,
            "{what}\n--- logs:\n{}",
            logs.text()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn hello(text: &str) -> String {
    format!(
        "use balise::prelude::*;\n\n#[derive(Output)]\nstruct Hello {{\n    text: String,\n}}\n\n#[get(\"/hello\")]\nasync fn hello() -> Hello {{\n    Hello {{\n        text: \"{text}\".into(),\n    }}\n}}\n"
    )
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Runs `balise <args>` in `cwd`, without a terminal.
fn balise(cwd: &Path, args: &[&str], machine: &Quiet) -> (String, u8) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut input: &[u8] = b"";
    let mut asked = Vec::new();
    let mut context = Context {
        cwd: cwd.to_path_buf(),
        interactive: false,
        input: &mut input,
        output: &mut asked,
        machine,
    };
    run(&args, &mut context)
}

/// A new project without the example, in a fresh folder named after the test.
fn project(test: &str) -> PathBuf {
    let cwd = std::env::temp_dir().join("balise-dev-tests").join(test);
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    let workspace = workspace();
    let args = [
        "new",
        "app",
        "--no-front",
        "--no-example",
        "--balise-path",
        workspace.to_str().unwrap(),
    ];
    let (out, code) = balise(&cwd, &args, &Quiet { docker: None });
    assert_eq!(code, 0, "{out}");
    cwd.join("app")
}

#[test]
fn docker_must_be_installed_and_running() {
    let root = project("docker");
    let (out, code) = balise(&root, &["dev"], &Quiet { docker: None });
    assert_eq!(code, 1);
    assert!(out.starts_with("DEV0004: "), "{out}");
    let (out, code) = balise(
        &root,
        &["dev", "status"],
        &Quiet {
            docker: Some(false),
        },
    );
    assert_eq!(code, 1);
    assert!(out.starts_with("DEV0005: "), "{out}");
}

#[test]
fn a_taken_port_is_reported_before_docker_starts() {
    let root = project("port");
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let config = std::fs::read_to_string(root.join("balise.toml")).unwrap();
    std::fs::write(root.join("balise.toml"), format!("port = {port}\n{config}")).unwrap();
    let (out, code) = balise(&root, &["dev"], &Quiet { docker: Some(true) });
    assert_eq!(code, 1);
    let expected = format!(
        "DEV0006: Port {port} is already used by another program\n  fix: Stop the program that uses it, or set another port in `balise.toml`: `port = {}`.",
        port + 1
    );
    assert!(out.starts_with(&expected), "{out}");
}

#[test]
fn actions_and_options_are_checked() {
    let root = project("usage");
    let docker = Quiet { docker: Some(true) };
    let (out, code) = balise(&root, &["dev", "restart"], &docker);
    assert_eq!(code, 2);
    assert!(
        out.starts_with("CLI0004: `restart` is not a valid value for `dev`"),
        "{out}"
    );
    let (out, code) = balise(&root, &["dev", "stop", "--native"], &docker);
    assert_eq!(code, 2);
    assert!(
        out.starts_with("CLI0005: `stop` and `--native` cannot be used together"),
        "{out}"
    );
    let (out, code) = balise(&root, &["dev", "--native", "--detach"], &docker);
    assert_eq!(code, 2, "{out}");
}

#[test]
fn the_supervisor_rebuilds_and_keeps_the_last_version_running() {
    let workspace = workspace();
    let root = project("supervisor");
    std::fs::write(
        root.join("src/main.rs"),
        "mod hello;\n\nfn main() {\n    balise::run!();\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("src/hello.rs"), hello("v1")).unwrap();

    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // SAFETY: set before any thread of this test starts; the supervisor's children inherit them.
    unsafe {
        std::env::set_var("PORT", port.to_string());
        std::env::set_var("CARGO_TARGET_DIR", workspace.join("target/new-projects"));
    }
    static STOP: AtomicBool = AtomicBool::new(false);
    let logs = Logs::default();
    let supervisor = {
        let root = root.clone();
        let mut logs = logs.clone();
        std::thread::spawn(move || {
            balise_cli::dev::supervise(&root, &mut logs, false, &|| STOP.load(Ordering::SeqCst))
        })
    };
    let answers = |text: &str| {
        let expected = format!(r#"{{"text":"{text}"}}"#);
        move || get(port, "/api/hello").is_some_and(|r| r.ends_with(&expected))
    };

    wait(
        Duration::from_secs(300),
        "first build",
        &logs,
        answers("v1"),
    );
    assert!(logs.text().contains("Building app"), "{}", logs.text());

    std::fs::write(root.join("src/hello.rs"), hello("v2")).unwrap();
    wait(Duration::from_secs(120), "rebuild", &logs, answers("v2"));
    assert!(
        logs.text().contains("Rebuilding: src/hello.rs changed"),
        "{}",
        logs.text()
    );

    std::fs::write(root.join("src/hello.rs"), "fn broken( {}\n").unwrap();
    wait(Duration::from_secs(120), "failed build", &logs, || {
        logs.text().contains("DEV0002")
    });
    assert!(answers("v2")(), "the last version keeps running");

    std::fs::write(root.join("src/hello.rs"), hello("v3")).unwrap();
    wait(
        Duration::from_secs(120),
        "repaired build",
        &logs,
        answers("v3"),
    );

    STOP.store(true, Ordering::SeqCst);
    supervisor.join().unwrap().unwrap();
    assert!(
        get(port, "/api/hello").is_none(),
        "the app stops with the supervisor"
    );
}
