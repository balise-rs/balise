//! Watching the project's files by reading their dates at a regular interval.
//!
//! Notifications from the system do not always cross the mount between a machine and a
//! Docker container; reading the dates always works, and costs little for a project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The files whose change rebuilds the app, besides everything under `src/`.
const FILES: &[&str] = &["Cargo.toml", "balise.toml", "budgets.toml"];

/// The state of the watched files, to compare with the next one.
pub struct Watcher {
    root: PathBuf,
    files: BTreeMap<PathBuf, (SystemTime, u64)>,
}

impl Watcher {
    pub fn new(root: &Path) -> Watcher {
        let files = scan(root);
        Watcher {
            root: root.to_path_buf(),
            files,
        }
    }

    /// The files changed, added or removed since the last call, relative to the project.
    pub fn changes(&mut self) -> Vec<PathBuf> {
        let now = scan(&self.root);
        let mut changed: Vec<PathBuf> = now
            .iter()
            .filter(|(path, state)| self.files.get(*path) != Some(state))
            .map(|(path, _)| path.clone())
            .collect();
        changed.extend(
            self.files
                .keys()
                .filter(|path| !now.contains_key(*path))
                .cloned(),
        );
        self.files = now;
        changed
            .into_iter()
            .map(|path| {
                path.strip_prefix(&self.root)
                    .map(Path::to_path_buf)
                    .unwrap_or(path)
            })
            .collect()
    }
}

fn scan(root: &Path) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    let mut files = BTreeMap::new();
    let mut paths: Vec<PathBuf> = FILES.iter().map(|name| root.join(name)).collect();
    collect(&root.join("src"), &mut paths);
    for path in paths {
        if let Ok(meta) = std::fs::metadata(&path) {
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            files.insert(path, (modified, meta.len()));
        }
    }
    files
}

fn collect(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => collect(&path, paths),
            false => paths.push(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_seen_once() {
        let root = std::env::temp_dir().join("balise-watch-tests");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/orders")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "a").unwrap();
        std::fs::write(root.join("src/main.rs"), "a").unwrap();
        let mut watcher = Watcher::new(&root);
        assert!(watcher.changes().is_empty());
        std::fs::write(root.join("src/orders/mod.rs"), "new").unwrap();
        std::fs::write(root.join("src/main.rs"), "longer").unwrap();
        let mut changes = watcher.changes();
        changes.sort();
        assert_eq!(
            changes,
            [
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/orders/mod.rs")
            ]
        );
        assert!(watcher.changes().is_empty());
        std::fs::remove_file(root.join("src/main.rs")).unwrap();
        assert_eq!(watcher.changes(), [PathBuf::from("src/main.rs")]);
    }
}
