//! `balise dev --native` on a generated project: it builds, restarts on change, keeps the
//! last version running while the code does not compile, and stops the app with itself.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use balise_cli::{Context, Machine, run};

struct Quiet;

impl Machine for Quiet {
    fn installed(&self, _: &str) -> bool {
        false
    }
    fn docker_running(&self) -> bool {
        false
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

#[test]
fn the_supervisor_rebuilds_and_keeps_the_last_version_running() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let cwd = std::env::temp_dir().join("balise-dev-tests");
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    let args: Vec<String> = ["new", "app", "--no-front", "--no-example", "--balise-path"]
        .iter()
        .map(|a| a.to_string())
        .chain([workspace.display().to_string()])
        .collect();
    let mut input: &[u8] = b"";
    let mut asked = Vec::new();
    let mut context = Context {
        cwd: cwd.clone(),
        interactive: false,
        input: &mut input,
        output: &mut asked,
        machine: &Quiet,
    };
    let (out, code) = run(&args, &mut context);
    assert_eq!(code, 0, "{out}");
    let root: PathBuf = cwd.join("app");
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
