//! `balise new`, as an agent and as a person run it; the project it creates compiles.

use std::path::{Path, PathBuf};
use std::process::Command;

use balise_cli::{Context, Machine, run};

/// A machine whose tools the test chooses; programs it has run for real.
struct Fake {
    git: bool,
    docker: Option<bool>,
}

impl Machine for Fake {
    fn installed(&self, program: &str) -> bool {
        match program {
            "git" => self.git,
            "docker" => self.docker.is_some(),
            _ => true,
        }
    }

    fn docker_running(&self) -> bool {
        self.docker == Some(true)
    }

    fn os(&self) -> String {
        "Ubuntu 24.04".into()
    }

    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<(), String> {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|error| error.to_string())?;
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

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// A fresh folder to create projects in.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("balise-new-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `balise <args>` in `cwd`, answering `answers` to questions when `interactive`.
fn balise(
    cwd: &Path,
    args: &[&str],
    interactive: bool,
    answers: &str,
    machine: &Fake,
) -> (String, u8, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut input = answers.as_bytes();
    let mut asked = Vec::new();
    let mut context = Context {
        cwd: cwd.to_path_buf(),
        interactive,
        input: &mut input,
        output: &mut asked,
        machine,
    };
    let (out, code) = run(&args, &mut context);
    (out, code, String::from_utf8(asked).unwrap())
}

const EVERYTHING: Fake = Fake {
    git: true,
    docker: Some(true),
};

#[test]
fn an_agent_must_choose_the_front_end() {
    let cwd = folder("front");
    let (out, code, _) = balise(&cwd, &["new", "shop"], false, "", &EVERYTHING);
    assert_eq!(code, 2);
    assert!(
        out.starts_with("CLI0003: `balise new` needs a front end: `--front <framework>` or `--no-front`\n  fix: Run it again with what is missing: `balise new shop --no-front`."),
        "{out}"
    );
    let (out, code, _) = balise(
        &cwd,
        &["new", "shop", "--front", "react"],
        false,
        "",
        &EVERYTHING,
    );
    assert_eq!(code, 2);
    assert!(
        out.starts_with("CLI0006: The `react` front end is not available in Balise"),
        "{out}"
    );
    assert!(!cwd.join("shop").exists());
}

#[test]
fn names_and_folders_are_checked() {
    let cwd = folder("names");
    for name in ["Shop", "1shop", "my shop", "std", "balise"] {
        let (out, code, _) = balise(&cwd, &["new", name, "--no-front"], false, "", &EVERYTHING);
        assert_eq!(code, 2, "{name}");
        assert!(out.starts_with("CLI0007"), "{out}");
    }
    std::fs::create_dir_all(cwd.join("shop/src")).unwrap();
    let (out, code, _) = balise(&cwd, &["new", "shop", "--no-front"], false, "", &EVERYTHING);
    assert_eq!(code, 1);
    assert!(out.starts_with("CLI0008"), "{out}");
}

#[test]
fn an_agent_gets_defaults_and_a_strict_policy() {
    let cwd = folder("agent");
    let workspace = workspace();
    let args = [
        "new",
        "shop",
        "--no-front",
        "--balise-path",
        workspace.to_str().unwrap(),
    ];
    let machine = Fake {
        git: false,
        docker: None,
    };
    let (out, code, asked) = balise(&cwd, &args, false, "", &machine);
    assert_eq!(code, 0, "{out}");
    assert!(asked.is_empty(), "no question: {asked}");
    for expected in [
        "Created shop/",
        "Defaults applied:\n  example   orders",
        "rm -r shop && balise new shop --no-front",
        "Policy: strict",
        "balise policy preset autonomous",
        "git and Docker not installed",
        "shop/.balise/prompts/install-git.md",
    ] {
        assert!(out.contains(expected), "{expected}\n---\n{out}");
    }
    let root = cwd.join("shop");
    let policy = std::fs::read_to_string(root.join(".balise/policy.toml")).unwrap();
    assert!(policy.contains("budget = \"human\""));
    let prompt = std::fs::read_to_string(root.join(".balise/prompts/install-docker.md")).unwrap();
    assert!(prompt.starts_with("I am on Ubuntu 24.04 and I need Docker"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("name = \"shop\""));
    assert!(manifest.contains("balise = { path = "), "{manifest}");
}

#[test]
fn a_person_is_asked_and_gets_the_equivalent_command() {
    let cwd = folder("person");
    // Example: no; policy: autonomous (default); confirm: yes.
    let (out, code, asked) = balise(&cwd, &["new", "shop"], true, "n\n\n\n", &EVERYTHING);
    assert_eq!(code, 0, "{out}");
    assert!(asked.contains("Add the example resource"), "{asked}");
    assert!(asked.contains("1) autonomous"), "{asked}");
    assert!(
        !asked.contains("Front end?"),
        "a question with one answer is not asked"
    );
    assert!(
        out.contains(
            "The same project without questions:\n  balise new shop --no-front --no-example"
        ),
        "{out}"
    );
    assert!(!out.contains("Defaults applied"), "{out}");
    let root = cwd.join("shop");
    assert!(!root.join("src/orders").exists());
    assert!(root.join(".git").exists());
    let commits = Command::new("git")
        .args(["rev-list", "--all", "--count"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&commits.stdout).trim(),
        "0",
        "nothing committed"
    );
    let policy = std::fs::read_to_string(root.join(".balise/policy.toml")).unwrap();
    assert!(policy.contains("Preset: autonomous"));
}

#[test]
fn json_gives_the_same_data() {
    let cwd = folder("json");
    let (out, code, _) = balise(&cwd, &["new", "--json", "shop"], false, "", &EVERYTHING);
    assert_eq!(code, 2);
    assert!(out.starts_with(r#"{"error":{"code":"CLI0003""#), "{out}");
}

/// The template's lockfile pins third-party crates as the workspace does. To update it after
/// a change of dependencies: `BALISE_UPDATE_TEMPLATE_LOCK=1 cargo test -p balise-cli`.
#[test]
fn the_template_lockfile_follows_the_workspace() {
    let lock = std::fs::read_to_string(workspace().join("Cargo.lock")).unwrap();
    let mut blocks = lock.split("\n[[package]]\n");
    let mut expected = blocks.next().unwrap().to_string();
    for block in blocks.filter(|block| block.contains("\nsource = ")) {
        expected.push_str("\n[[package]]\n");
        expected.push_str(block);
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("template/Cargo.lock.tpl");
    if std::env::var_os("BALISE_UPDATE_TEMPLATE_LOCK").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path).unwrap();
    assert!(
        actual == expected,
        "template/Cargo.lock.tpl is out of date: BALISE_UPDATE_TEMPLATE_LOCK=1 cargo test -p balise-cli"
    );
}

/// The generated project compiles and is formatted, with and without the example.
#[test]
fn generated_projects_compile() {
    let cwd = folder("compile");
    let workspace = workspace();
    let path = workspace.to_str().unwrap();
    for (name, extra) in [("full", None), ("bare", Some("--no-example"))] {
        let mut args = vec!["new", name, "--no-front", "--balise-path", path];
        args.extend(extra);
        let (out, code, _) = balise(&cwd, &args, false, "", &EVERYTHING);
        assert_eq!(code, 0, "{out}");
        let root = cwd.join(name);
        for (command, args) in [
            ("cargo", vec!["check", "--quiet"]),
            ("cargo", vec!["fmt", "--check"]),
        ] {
            let status = Command::new(command)
                .args(&args)
                .current_dir(&root)
                .env("CARGO_TARGET_DIR", workspace.join("target/new-projects"))
                .status()
                .unwrap();
            assert!(status.success(), "{command} {args:?} in {name}");
        }
    }
}
