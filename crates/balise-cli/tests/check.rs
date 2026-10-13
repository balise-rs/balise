//! `balise check` on generated projects: a clean one passes, and the mistakes Balise knows
//! come out with their codes, where the fix goes.

use std::path::{Path, PathBuf};
use std::sync::Once;

use balise_cli::{Context, Machine, run};

/// A machine with Rust, or with nothing.
struct Fake {
    rust: bool,
}

impl Machine for Fake {
    fn installed(&self, program: &str) -> bool {
        self.rust && program == "cargo"
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

const RUST: Fake = Fake { rust: true };

fn balise(cwd: &Path, args: &[&str], machine: &Fake) -> (String, u8) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut input: &[u8] = b"";
    let mut asked = Vec::new();
    let mut context = Context {
        cwd: cwd.to_path_buf(),
        interactive: false,
        input: &mut input,
        output: &mut asked,
        color: false,
        machine,
    };
    run(&args, &mut context)
}

/// A new project without the example, in a folder named after the test. Projects share
/// one target folder, so that Balise compiles once.
fn project(test: &str) -> PathBuf {
    project_with(test, &["--no-example"])
}

fn project_with(test: &str, options: &[&str]) -> PathBuf {
    static TARGET: Once = Once::new();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    TARGET.call_once(|| {
        // SAFETY: set once, before any test of this file runs cargo.
        unsafe { std::env::set_var("CARGO_TARGET_DIR", workspace.join("target/new-projects")) };
    });
    let cwd = std::env::temp_dir().join("balise-check-tests").join(test);
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    // One name per project: in a shared target folder, cargo tells packages by their name.
    let name = format!("app_{test}");
    let mut args = vec![
        "new",
        &name,
        "--no-front",
        "--balise-path",
        workspace.to_str().unwrap(),
    ];
    args.extend(options);
    let (out, code) = balise(&cwd, &args, &RUST);
    assert_eq!(code, 0, "{out}");
    cwd.join(name)
}

/// Adds `src/bad.rs` to the project.
fn add(root: &Path, code: &str) {
    std::fs::write(root.join("src/bad.rs"), code).unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        "mod bad;\n\nfn main() {\n    balise::run!();\n}\n",
    )
    .unwrap();
}

/// The codes and lines `balise check --json` reports.
fn found(root: &Path) -> Vec<(String, usize)> {
    let (out, code) = balise(root, &["check", "--json"], &RUST);
    assert_eq!(code, 1, "{out}");
    let value = balise_core::json::parse(out.as_bytes()).unwrap();
    let checked = balise_cli::check::Checked::from_value(&value).unwrap();
    checked
        .diagnostics
        .into_iter()
        .filter(|d| d.code != "dead_code")
        .map(|d| (d.code, d.line))
        .collect()
}

#[test]
fn a_new_project_passes() {
    let root = project_with("clean", &[]);
    let (out, code) = balise(&root, &["check"], &RUST);
    assert_eq!((out.as_str(), code), ("✓ 0 errors · 0 warnings", 0));
}

#[test]
fn names_and_parameters_balise_knows() {
    let root = project("names");
    add(
        &root,
        r#"use balise::prelude::*;

#[derive(Output)]
struct Hello {
    text: String,
}

#[get("/json")]
async fn json(body: Json<u32>) -> Hello {
    Hello { text: String::new() }
}

#[get("/file")]
async fn file(_file: std::fs::File) -> Hello {
    Hello { text: private::secret() }
}

mod private {
    fn secret() -> String {
        String::new()
    }
}
"#,
    );
    std::fs::create_dir_all(root.join("src/lost")).unwrap();
    std::fs::write(root.join("src/lost/mod.rs"), "").unwrap();
    let found = found(&root);
    for expected in [
        ("CMP0021", 9),
        ("CMP0006", 14),
        ("CMP0023", 15),
        ("CMP0020", 1),
    ] {
        let expected = (expected.0.to_string(), expected.1);
        assert!(found.contains(&expected), "{expected:?} in {found:?}");
    }
    let (out, _) = balise(&root, &["check"], &RUST);
    assert!(
        out.contains(
            "error CMP0021  Balise has no `Json`: it comes from Axum and Actix Web\n      at src/bad.rs:9 in bad::json\n"
        ),
        "{out}"
    );
    assert!(out.ends_with("✗ 4 errors · 0 warnings"), "{out}");
}

#[test]
fn state_logs_and_threads_go_through_balise() {
    let root = project("lints");
    add(
        &root,
        r#"use balise::prelude::*;

static COUNT: std::sync::Mutex<u32> = std::sync::Mutex::new(0);
static mut OTHER: u32 = 0;

#[derive(Output)]
struct Hello {
    text: String,
}

#[get("/print")]
async fn print() -> Hello {
    println!("{:?}", COUNT.lock());
    let _ = std::thread::spawn(|| ());
    Hello { text: String::new() }
}
"#,
    );
    let found = found(&root);
    for expected in [
        ("CMP0024", 3),
        ("CMP0024", 4),
        ("CMP0025", 13),
        ("CMP0026", 14),
    ] {
        let expected = (expected.0.to_string(), expected.1);
        assert!(found.contains(&expected), "{expected:?} in {found:?}");
    }
}

#[test]
fn a_value_kept_across_an_await_is_shown_where_it_is_declared() {
    let root = project("send");
    add(
        &root,
        r#"use balise::prelude::*;

#[derive(Output)]
struct Hello {
    text: String,
}

#[get("/rc")]
async fn rc() -> Hello {
    let shared = std::rc::Rc::new(1);
    let _ = balise::blocking(|| Ok::<(), balise::Error>(())).await;
    Hello { text: format!("{shared}") }
}
"#,
    );
    assert_eq!(found(&root), [("CMP0022".to_string(), 10)]);
    let (out, _) = balise(&root, &["check"], &RUST);
    assert!(
        out.contains("error CMP0022  `shared` is kept across an `.await`, and its type `std::rc::Rc<i32>` cannot move between threads\n      at src/bad.rs:10 in bad::rc\n"),
        "{out}"
    );
}

#[test]
fn without_rust_or_docker_nothing_compiles() {
    let root = project("nothing");
    let (out, code) = balise(&root, &["check"], &Fake { rust: false });
    assert_eq!(code, 1);
    assert!(
        out.starts_with("CLI0013: `balise check` needs Rust or Docker"),
        "{out}"
    );
}

#[test]
fn rust_codes_are_explained_by_rustc() {
    let (out, code) = balise(Path::new("."), &["explain", "e0308"], &RUST);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("E0308  (explained by rustc)\n\nExpected type did not match"),
        "{out}"
    );
}

/// Runs `balise check --hook` with the hook's JSON on standard input; returns the exit code
/// and what it wrote for the agent.
fn hook(root: &Path, input: &str) -> (u8, String) {
    let args: Vec<String> = ["check", "--hook"].iter().map(|a| a.to_string()).collect();
    let mut input = input.as_bytes();
    let mut said = Vec::new();
    let mut context = Context {
        cwd: root.to_path_buf(),
        interactive: false,
        input: &mut input,
        output: &mut said,
        color: false,
        machine: &RUST,
    };
    let (out, code) = run(&args, &mut context);
    assert_eq!(out, "");
    (code, String::from_utf8(said).unwrap())
}

#[test]
fn the_end_of_task_hook_blocks_once_on_what_is_left() {
    let root = project("hook");
    add(
        &root,
        "use balise::prelude::*;\n\n#[get(\"/n\")]\nasync fn n() -> NoContent {\n    let n: u32 = \"a\";\n    NoContent\n}\n",
    );
    let (code, said) = hook(
        &root,
        r#"{"hook_event_name":"Stop","stop_hook_active":false}"#,
    );
    assert_eq!(code, 2);
    assert!(
        said.starts_with("balise check: 1 error · 0 warnings left. Fix them before finishing:\n\nerror E0308 at src/bad.rs:5 in bad::n: mismatched types\n"),
        "{said}"
    );
    // Blocked once already: the agent may finish.
    let (code, said) = hook(
        &root,
        r#"{"hook_event_name":"Stop","stop_hook_active":true}"#,
    );
    assert_eq!((code, said.as_str()), (0, ""));
    let (code, said) = hook(&root, r#"{"hook_event_name":"PostToolUse"}"#);
    assert_eq!((code, said.as_str()), (0, ""));
    std::fs::write(
        root.join("src/bad.rs"),
        "use balise::prelude::*;\n\n#[get(\"/n\")]\nasync fn n() -> NoContent {\n    NoContent\n}\n",
    )
    .unwrap();
    let (code, said) = hook(
        &root,
        r#"{"hook_event_name":"Stop","stop_hook_active":false}"#,
    );
    assert_eq!((code, said.as_str()), (0, ""));
}
