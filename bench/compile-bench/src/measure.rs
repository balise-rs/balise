//! `compile-bench measure`: compile times on a project, the way an agent's loop experiences them.
//!
//! The project marks where to edit with two comment lines:
//! - `// bench:body` inside a function body: replaced by a statement;
//! - `// bench:field` inside an input struct, and `// bench:model` inside a model struct:
//!   replaced by a public field, so these structs must never be built with a literal without
//!   `..` (struct update).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// A slowdown is flagged when it exceeds both thresholds, so noise on short timings is ignored.
const THRESHOLD_PERCENT: f64 = 10.0;
const THRESHOLD_SECONDS: f64 = 0.1;

pub const BODY_MARKER: &str = "// bench:body";
pub const FIELD_MARKER: &str = "// bench:field";
pub const MODEL_MARKER: &str = "// bench:model";

struct Args {
    project: PathBuf,
    runs: usize,
    baseline: Option<PathBuf>,
    out: Option<PathBuf>,
}

pub const USAGE: &str = "compile-bench measure <project> [--runs N] [--baseline FILE] [--out FILE]";

pub fn main(args: impl Iterator<Item = String>) -> Result<(), String> {
    parse_args(args).and_then(run)
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        project: PathBuf::new(),
        runs: 5,
        baseline: None,
        out: None,
    };
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--runs" => parsed.runs = value()?.parse().map_err(|_| "--runs needs a number")?,
            "--baseline" => parsed.baseline = Some(value()?.into()),
            "--out" => parsed.out = Some(value()?.into()),
            _ if parsed.project.as_os_str().is_empty() => parsed.project = arg.into(),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    if parsed.project.as_os_str().is_empty() || parsed.runs == 0 {
        return Err(format!("usage: {USAGE}"));
    }
    Ok(parsed)
}

fn run(args: Args) -> Result<(), String> {
    let project = &args.project;
    let body = Marker::find(project, BODY_MARKER)?;
    let field = Marker::find(project, FIELD_MARKER)?;
    let model = Marker::find(project, MODEL_MARKER)?;
    // Read before measuring: `--out` may overwrite the baseline file.
    let baseline = match &args.baseline {
        Some(path) if path.exists() => Some(read_results(path)?),
        _ => None,
    };

    let mut results = BTreeMap::new();
    results.insert("commit".to_string(), commit());
    results.insert(
        "rustc".to_string(),
        output(project, "rustc", &["--version"])?,
    );

    let mut record = |name: &str, seconds: f64| {
        println!("{name:<18} {seconds:>8.2} s");
        results.insert(name.to_string(), format!("{seconds:.2}"));
    };

    cargo(project, "clean")?;
    record("cold_check", cargo(project, "check")?);
    cargo(project, "clean")?;
    record("cold_build", cargo(project, "build")?);

    record(
        "body_edit_check",
        median(args.runs, |i| {
            body.apply(&format!("let _bench = {i};"))?;
            cargo(project, "check")
        })?,
    );
    record(
        "body_edit_build",
        median(args.runs, |i| {
            body.apply(&format!("let _bench = {i};"))?;
            cargo(project, "build")
        })?,
    );
    record(
        "field_edit_check",
        median(args.runs, |i| {
            field.apply(&format!("pub bench_field_{i}: u32,"))?;
            cargo(project, "check")
        })?,
    );
    record(
        "model_edit_check",
        median(args.runs, |i| {
            model.apply(&format!("pub bench_field_{i}: u32,"))?;
            cargo(project, "check")
        })?,
    );
    body.restore()?;
    field.restore()?;
    model.restore()?;

    if let Some(baseline) = baseline {
        compare(&baseline, &results);
    }
    if let Some(out) = &args.out {
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text: String = results.iter().map(|(k, v)| format!("{k} {v}\n")).collect();
        fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}

/// A file with a marker line, restored to its original content when dropped.
struct Marker {
    file: PathBuf,
    original: String,
    marker: &'static str,
}

impl Marker {
    fn find(project: &Path, marker: &'static str) -> Result<Marker, String> {
        let mut files = Vec::new();
        collect_rust_files(&project.join("src"), &mut files)?;
        for file in files {
            let original =
                fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            if original.lines().any(|line| line.trim() == marker) {
                return Ok(Marker {
                    file,
                    original,
                    marker,
                });
            }
        }
        Err(format!("no `{marker}` line in {}", project.display()))
    }

    /// Writes the original content with the marker line replaced by `code`.
    fn apply(&self, code: &str) -> Result<(), String> {
        let edited: String = self
            .original
            .lines()
            .map(|line| match line.trim() == self.marker {
                true => line.replace(self.marker, &format!("{code} {}", self.marker)),
                false => line.to_string(),
            })
            .map(|line| line + "\n")
            .collect();
        fs::write(&self.file, edited).map_err(|e| format!("{}: {e}", self.file.display()))
    }

    fn restore(&self) -> Result<(), String> {
        fs::write(&self.file, &self.original).map_err(|e| format!("{}: {e}", self.file.display()))
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Runs `step` once to warm up, then `runs` times, and returns the median duration.
fn median(runs: usize, mut step: impl FnMut(usize) -> Result<f64, String>) -> Result<f64, String> {
    step(0)?;
    let mut durations = (1..=runs).map(&mut step).collect::<Result<Vec<_>, _>>()?;
    durations.sort_by(f64::total_cmp);
    Ok(durations[durations.len() / 2])
}

/// Runs a cargo command on the project and returns its duration in seconds.
fn cargo(project: &Path, command: &str) -> Result<f64, String> {
    let start = Instant::now();
    let out = Command::new("cargo")
        .args([command, "--quiet"])
        .current_dir(project)
        .output()
        .map_err(|e| format!("cargo {command}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo {command} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(start.elapsed().as_secs_f64())
}

fn output(dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn commit() -> String {
    std::env::var("GITHUB_SHA")
        .ok()
        .or_else(|| output(Path::new("."), "git", &["rev-parse", "HEAD"]).ok())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_results(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

fn compare(baseline: &BTreeMap<String, String>, results: &BTreeMap<String, String>) {
    let unknown = String::from("unknown");
    println!(
        "\nCompared to {}:",
        baseline.get("commit").unwrap_or(&unknown)
    );
    for (name, value) in results {
        let (Some(before), Ok(after)) = (baseline.get(name), value.parse::<f64>()) else {
            continue;
        };
        let Ok(before) = before.parse::<f64>() else {
            continue;
        };
        if before <= 0.0 {
            continue;
        }
        let delta = (after - before) / before * 100.0;
        let flag = if delta > THRESHOLD_PERCENT && after - before > THRESHOLD_SECONDS {
            "  WARNING: slower"
        } else {
            ""
        };
        println!("{name:<18} {before:>8.2} s -> {after:>8.2} s  {delta:>+6.1} %{flag}");
    }
}

pub fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(())
}
