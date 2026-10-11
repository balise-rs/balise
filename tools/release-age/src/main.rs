//! Reports crates in lockfiles that were published less than 7 days ago, with the newest
//! version old enough and the command to pin it. A signal, not a gate: it always exits 0.
//!
//! Usage: `release-age <Cargo.lock>…`. Needs `curl` and the network.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_AGE_DAYS: i64 = 7;

fn main() {
    let today = days_since_epoch_now();
    let mut young = 0;
    for lockfile in std::env::args().skip(1) {
        let Ok(text) = std::fs::read_to_string(&lockfile) else {
            println!("release-age: cannot read {lockfile}");
            continue;
        };
        for (name, version) in registry_packages(&text) {
            let Some(published) = published(&name, &version) else {
                println!("release-age: no date for {name} {version}");
                continue;
            };
            if today - published >= MIN_AGE_DAYS {
                continue;
            }
            young += 1;
            println!(
                "{lockfile}: {name} {version} was published {} days ago",
                today - published
            );
            match older_version(&name, &version, today) {
                Some(older) => println!("  cargo update -p {name}@{version} --precise {older}"),
                None => println!("  no older version of the same major is available"),
            }
        }
    }
    if young == 0 {
        println!("release-age: every version is at least {MIN_AGE_DAYS} days old");
    }
}

/// `(name, version)` of every package that comes from crates.io.
fn registry_packages(lockfile: &str) -> Vec<(String, String)> {
    lockfile
        .split("[[package]]")
        .filter(|block| block.contains("source = \"registry+"))
        .filter_map(|block| Some((field(block, "name")?, field(block, "version")?)))
        .collect()
}

fn field(block: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = \"");
    let line = block.lines().find(|line| line.starts_with(&prefix))?;
    Some(line[prefix.len()..].trim_end_matches('"').to_string())
}

fn crates_io(path: &str) -> Option<String> {
    let output = Command::new("curl")
        .args([
            "-sf",
            "-A",
            "balise-release-age",
            &format!("https://crates.io/api/v1/crates/{path}"),
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn published(name: &str, version: &str) -> Option<i64> {
    let json = crates_io(&format!("{name}/{version}"))?;
    days_of(&string_after(&json, "\"created_at\":\"")?)
}

/// The newest version of the same major, not yanked, not a pre-release, old enough.
fn older_version(name: &str, version: &str, today: i64) -> Option<String> {
    let json = crates_io(&format!("{name}/versions?per_page=50"))?;
    let major = version.split('.').next()?;
    json.split("{\"id\":").skip(1).find_map(|entry| {
        let num = string_after(entry, "\"num\":\"")?;
        let date = days_of(&string_after(entry, "\"created_at\":\"")?)?;
        let pre_release = num.split('+').next()?.contains('-');
        let ok = num.split('.').next() == Some(major)
            && !entry.contains("\"yanked\":true")
            && !pre_release
            && today - date >= MIN_AGE_DAYS;
        ok.then_some(num)
    })
}

fn string_after(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let end = text[start..].find('"')?;
    Some(text[start..start + end].to_string())
}

/// Days since 1970-01-01 of an RFC 3339 date (`2026-10-09T…`).
fn days_of(date: &str) -> Option<i64> {
    let mut parts = date.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_since_epoch_now() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (seconds / 86_400) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_become_days_since_the_epoch() {
        assert_eq!(days_of("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            days_of("2026-10-11T13:42:11.000Z").unwrap() - days_of("2026-10-04").unwrap(),
            7
        );
        assert_eq!(
            days_of("2024-03-01").unwrap() - days_of("2024-02-28").unwrap(),
            2
        );
    }

    #[test]
    fn only_registry_packages_are_read() {
        let lock = "[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"local\"\nversion = \"0.0.0\"\n";
        assert_eq!(
            registry_packages(lock),
            [("a".to_string(), "1.0.0".to_string())]
        );
    }
}
