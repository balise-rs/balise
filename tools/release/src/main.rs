//! Publishes a version of Balise: `just publish <version> [remote]`.
//!
//! Checks first, so that nothing is half done: a valid semantic version, above every
//! existing tag and not tagged yet; nothing uncommitted, nothing stashed; the manifests at
//! that version; a successful `cargo publish --dry-run`. After a confirmation, tags the
//! current commit, pushes the branch and the tag to the remote, then publishes the crates.

use std::cmp::Ordering;
use std::io::Write;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (version, remote) = match args.as_slice() {
        [version] => (version.as_str(), "public"),
        [version, remote] => (version.as_str(), remote.as_str()),
        _ => {
            return fail(
                "usage: just publish <version> [remote], such as `just publish 0.1.0-alpha.2`",
            );
        }
    };
    match release(version, remote) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&message),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}

fn release(text: &str, remote: &str) -> Result<(), String> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let version = Version::parse(text).ok_or_else(|| {
        format!("`{text}` is not a semantic version, such as `0.1.0` or `0.1.0-alpha.2`")
    })?;
    let tag = format!("v{text}");

    step("Checking the working tree");
    let status = git(&["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err(format!("uncommitted changes; commit them first:\n{status}"));
    }
    let stash = git(&["stash", "list"])?;
    if !stash.trim().is_empty() {
        return Err(format!(
            "the stash is not empty; apply or drop it first:\n{stash}"
        ));
    }
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if branch == "HEAD" {
        return Err("HEAD is detached: check out the branch to publish".into());
    }

    step("Checking the manifests");
    let manifest = std::fs::read_to_string("Cargo.toml").map_err(|e| format!("Cargo.toml: {e}"))?;
    let problems = manifest_problems(&manifest, text);
    if !problems.is_empty() {
        return Err(format!(
            "Cargo.toml is not at {text}; set it, then commit:\n{}",
            problems.join("\n")
        ));
    }

    step("Checking the tags");
    git(&["remote", "get-url", remote]).map_err(|_| {
        format!("no remote named `{remote}`: add it with `git remote add {remote} <url>`")
    })?;
    let local = git(&["tag", "--list", "v*"])?;
    let remote_tags = git(&["ls-remote", "--tags", remote])?;
    let mut tags: Vec<String> = local.lines().map(str::to_string).collect();
    tags.extend(
        remote_tags
            .lines()
            .filter_map(|line| line.split("refs/tags/").nth(1))
            .map(|tag| tag.trim_end_matches("^{}").to_string()),
    );
    if tags.contains(&tag) {
        return Err(format!("the tag {tag} already exists"));
    }
    let latest = tags
        .iter()
        .filter_map(|tag| Version::parse(tag.strip_prefix('v')?).map(|v| (v, tag)))
        .max_by(|(a, _), (b, _)| a.cmp(b));
    if let Some((latest, latest_tag)) = latest
        && version <= latest
    {
        return Err(format!("{text} is not above the latest tag, {latest_tag}"));
    }

    step("Packaging every crate (dry run)");
    run("cargo", &["publish", "--workspace", "--dry-run"])?;

    print!(
        "\nPublish {text} to crates.io, from {branch} to `{remote}`? It cannot be undone. [y/N]: "
    );
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    if !matches!(answer.trim(), "y" | "yes") {
        return Err("nothing was done".into());
    }

    step(&format!("Tagging {tag}"));
    run(
        "git",
        &[
            "tag",
            "--annotate",
            &tag,
            "--message",
            &format!("Balise {text}"),
        ],
    )?;
    let resume = |done: &str| format!("{done}; finish by hand from there");
    step(&format!("Pushing {branch} and {tag} to {remote}"));
    run(
        "git",
        &["push", remote, &format!("HEAD:refs/heads/{branch}")],
    )
    .map_err(|e| {
        resume(&format!(
            "{e}\nthe tag {tag} exists locally, nothing was pushed"
        ))
    })?;
    run("git", &["push", remote, &tag])
        .map_err(|e| resume(&format!("{e}\n{branch} is pushed, the tag is not")))?;
    step("Publishing");
    run("cargo", &["publish", "--workspace"]).map_err(|e| {
        resume(&format!(
            "{e}\nthe tag is pushed; crates already published stay, run `cargo publish --workspace` again for the others"
        ))
    })?;
    println!("\nBalise {text} is published.");
    Ok(())
}

fn step(name: &str) {
    println!("==> {name}");
}

/// Runs a git command and returns its output.
fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    match output.status.success() {
        true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        false => Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// Runs a command with its output shown.
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("`{program} {}` failed", args.join(" "))),
    }
}

/// What in the root `Cargo.toml` is not at `version`: the workspace's version, and the exact
/// versions (`=…`) of the internal dependencies.
fn manifest_problems(manifest: &str, version: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut section = "";
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
            continue;
        }
        if section == "[workspace.package]"
            && let Some(value) = trimmed.strip_prefix("version = ")
            && value.trim_matches('"') != version
        {
            problems.push(format!("  [workspace.package] version = {value}"));
        }
        if let Some((_, rest)) = trimmed.split_once("version = \"=")
            && rest.split('"').next() != Some(version)
        {
            problems.push(format!("  {trimmed}"));
        }
    }
    problems
}

/// A semantic version: `1.2.3`, or `1.2.3-alpha.2`. Build metadata is not used.
#[derive(Debug, PartialEq, Eq)]
struct Version {
    core: [u64; 3],
    pre: Vec<Identifier>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    /// Numbers come before words.
    Number(u64),
    Word(String),
}

impl Version {
    fn parse(text: &str) -> Option<Version> {
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let numbers: Vec<u64> = core.split('.').map(number).collect::<Option<_>>()?;
        let core: [u64; 3] = numbers.try_into().ok()?;
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|part| match part.chars().all(|c| c.is_ascii_digit()) {
                    true => number(part).map(Identifier::Number),
                    false => part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                        .then(|| Identifier::Word(part.to_string()))
                        .filter(|_| !part.is_empty()),
                })
                .collect::<Option<_>>()?,
        };
        Some(Version { core, pre })
    }
}

/// A number without leading zeros, as semver requires.
fn number(text: &str) -> Option<u64> {
    let valid = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    valid.then(|| text.parse().ok()).flatten()
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                // A pre-release comes before its release.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_follow_semver_order() {
        let ordered = [
            "0.1.0-alpha.1",
            "0.1.0-alpha.2",
            "0.1.0-alpha.10",
            "0.1.0-beta",
            "0.1.0-beta.1",
            "0.1.0-rc.1",
            "0.1.0",
            "0.1.1",
            "0.2.0",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn invalid_versions_are_refused() {
        for text in [
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0.0-",
            "1.0.0-alpha..1",
            "1.0.0-alpha_1",
            "a.b.c",
        ] {
            assert!(Version::parse(text).is_none(), "{text}");
        }
    }

    #[test]
    fn the_manifest_must_be_at_the_version() {
        let manifest = "[workspace.package]\nversion = \"0.1.0-alpha.1\"\n\n[workspace.dependencies]\nbalise = { path = \"crates/balise\", version = \"=0.1.0-alpha.1\" }\n";
        assert!(manifest_problems(manifest, "0.1.0-alpha.1").is_empty());
        assert_eq!(manifest_problems(manifest, "0.1.0-alpha.2").len(), 2);
    }
}
