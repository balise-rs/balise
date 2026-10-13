//! The commands that read a project, answered by its dev build: `routes`, `why`,
//! `config check`; and `explain config`, `policy set`, which need no build.

use std::path::{Path, PathBuf};
use std::sync::Once;

use balise_cli::{Context, Machine, run};
use balise_core::json::{self, Value};

/// A machine with Rust.
struct Rust;

impl Machine for Rust {
    fn installed(&self, program: &str) -> bool {
        program == "cargo"
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

fn balise(cwd: &Path, args: &[&str], interactive: bool) -> (String, u8) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut input: &[u8] = b"";
    let mut asked = Vec::new();
    let mut context = Context {
        cwd: cwd.to_path_buf(),
        interactive,
        input: &mut input,
        output: &mut asked,
        color: false,
        machine: &Rust,
    };
    run(&args, &mut context)
}

/// A new project with the example, named after the test, in a target folder shared by the
/// tests so that Balise compiles once.
fn project(test: &str) -> PathBuf {
    static TARGET: Once = Once::new();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    TARGET.call_once(|| {
        // SAFETY: set once, before any test of this file runs cargo.
        unsafe { std::env::set_var("CARGO_TARGET_DIR", workspace.join("target/new-projects")) };
    });
    let cwd = std::env::temp_dir().join("balise-project-tests").join(test);
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    let name = format!("app_{test}");
    let path = workspace.to_str().unwrap();
    let (out, code) = balise(
        &cwd,
        &["new", &name, "--no-front", "--balise-path", path],
        false,
    );
    assert_eq!(code, 0, "{out}");
    cwd.join(name)
}

fn json_of(out: &str) -> Value {
    json::parse(out.as_bytes()).unwrap_or_else(|e| panic!("{e}: {out}"))
}

#[test]
fn routes_and_why_answer_as_the_app_routes() {
    let root = project("routes");
    let (out, code) = balise(&root, &["routes"], false);
    assert_eq!(code, 0, "{out}");
    let first = out.lines().next().unwrap();
    assert!(
        first.starts_with("GET     /api/orders       orders::list"),
        "{out}"
    );
    assert!(first.ends_with("body 2 MB · 30 s · page 20/100"), "{out}");
    assert!(out.ends_with("5 routes under /api"), "{out}");

    let (out, _) = balise(&root, &["why", "GET", "/api/orders/5"], false);
    assert!(
        out.starts_with("GET /api/orders/5 → orders::get (src/orders/mod.rs:"),
        "{out}"
    );
    assert!(out.contains("  params  id = 5\n"), "{out}");

    let (out, code) = balise(&root, &["why", "get", "/orders/5"], false);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        "HTTP0003  GET /orders/5 answers 404: no route matches\n  instead: GET /api/orders/5 (every route is under `/api`)"
    );
    let (out, _) = balise(&root, &["why", "DELETE", "/api/orders"], false);
    assert_eq!(
        out,
        "HTTP0008  DELETE /api/orders answers 405: the path exists, with GET, POST only"
    );
    let (out, _) = balise(&root, &["why", "GET", "/api/ordres", "--json"], false);
    let value = json_of(&out);
    let Some(Value::Array(near)) = field(&value, "near") else {
        panic!("{out}")
    };
    assert_eq!(
        field(&near[0], "reason"),
        Some(&Value::String("`ordres` is not `orders`".into()))
    );
}

#[test]
fn config_check_is_the_apps() {
    let root = project("config");
    let (out, code) = balise(&root, &["config", "check", "--env", "prod"], false);
    assert_eq!(
        (out.as_str(), code),
        ("✓ The configuration is valid for prod", 0)
    );
    std::fs::write(root.join("balise.toml"), "port = \"many\"\n").unwrap();
    let (out, code) = balise(&root, &["config", "check"], false);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("CFG0002"), "{out}");
    assert!(
        out.ends_with("✗ 1 error in the configuration for dev"),
        "{out}"
    );
}

#[test]
fn a_project_that_does_not_build_cannot_answer() {
    let root = project("broken");
    std::fs::write(
        root.join("src/main.rs"),
        "mod orders;\n\nfn main() {\n    balise::run!(\n}\n",
    )
    .unwrap();
    let (out, code) = balise(&root, &["routes"], false);
    assert_eq!(code, 1);
    assert!(
        out.starts_with("DEV0009: `balise routes` asks the app's dev build, and the app does not build\n  fix: Run `balise check`"),
        "{out}"
    );
}

#[test]
fn every_setting_is_explained_with_its_default_and_variable() {
    let (out, code) = balise(Path::new("."), &["explain", "config"], false);
    assert_eq!(code, 0);
    assert!(
        out.contains(
            "\nport = 3000\n  a port number from 1 to 65535; variable BALISE_PORT or PORT\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("\nlog.format = \"text\" in dev; \"json\" in test, prod\n"),
        "{out}"
    );
}

#[test]
fn policy_set_changes_one_type_in_a_terminal_only() {
    let cwd = std::env::temp_dir().join("balise-project-tests/policy");
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(cwd.join(".balise")).unwrap();
    std::fs::write(cwd.join("Cargo.toml"), "[package]\nname = \"p\"\n").unwrap();
    std::fs::write(cwd.join("balise.toml"), "").unwrap();
    let (out, code) = balise(&cwd, &["policy", "set", "budget", "agent"], false);
    assert_eq!(code, 1);
    assert!(out.starts_with("CLI0009"), "{out}");
    let (out, code) = balise(&cwd, &["policy", "set", "budgets", "agent"], true);
    assert_eq!(code, 2);
    assert!(
        out.starts_with("CLI0004: `budgets` is not a valid value"),
        "{out}"
    );
    let (out, code) = balise(&cwd, &["policy", "set", "budget", "agent"], true);
    assert_eq!(
        (out.as_str(), code),
        (
            "budget: the agent decides from now on (.balise/policy.toml).",
            0
        )
    );
    let policy = std::fs::read_to_string(cwd.join(".balise/policy.toml")).unwrap();
    assert!(
        policy.contains("budget = \"agent\"\naccept = \"human\""),
        "{policy}"
    );
    assert!(
        policy.contains("# Preset: none, set type by type."),
        "{policy}"
    );
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}
