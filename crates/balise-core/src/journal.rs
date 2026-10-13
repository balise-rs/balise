//! The dev journal: in dev builds, every event, and what routing decided for each request,
//! appended as JSON lines to `.balise/dev/journal.jsonl` in the project. `balise logs` and
//! `balise trace` read it, whether the app runs or not, across its restarts.
//!
//! Only the file's layout is compiled everywhere, for the CLI that reads it; the writer is
//! behind the `dev` feature, so a production build does not even check whether to write.
//!
//! Each line is one record: `{"kind":"event","ms":…,"text":"…","event":{…}}`, the event as
//! the JSON logs write it and as text; or `{"kind":"route","ms":…,"request_id":…,…}`.

/// The journal's folder, in the project.
pub const DIR: &str = ".balise/dev";
/// The current file, and the previous one: past this size, the current one replaces it.
pub const FILE: &str = "journal.jsonl";
pub const PREVIOUS: &str = "journal.1.jsonl";
pub const MAX_SIZE: u64 = 4 * 1024 * 1024;

#[cfg(feature = "dev")]
pub use writer::{event, open, route};

#[cfg(feature = "dev")]
mod writer {
    use std::fs::{File, OpenOptions};
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    use super::{DIR, FILE, MAX_SIZE, PREVIOUS};
    use crate::event::{Event, Level};
    use crate::json::{Number, Value, write_string, write_value};
    use crate::log::{REDACTED, is_secret};

    struct Journal {
        dir: PathBuf,
        file: Option<File>,
        written: u64,
    }

    static JOURNAL: OnceLock<Mutex<Journal>> = OnceLock::new();

    /// Starts writing in the project at `root`. Until then, nothing is written.
    pub fn open(root: &Path) {
        let dir = root.join(DIR);
        let _ = JOURNAL.set(Mutex::new(Journal {
            dir,
            file: None,
            written: 0,
        }));
    }

    /// Records an event; `trace` ones are too many to keep.
    pub fn event(event: &Event) {
        if event.level == Level::Trace {
            return;
        }
        let mut line = String::from(r#"{"kind":"event","ms":"#);
        line.push_str(&event.time_ms.to_string());
        line.push_str(r#","text":"#);
        write_string(&mut line, &event.render_text(false));
        line.push_str(r#","event":"#);
        line.push_str(&event.render_json());
        line.push('}');
        append(&line);
    }

    /// Records what routing decided for a request: the route, its parameters and limits.
    #[allow(clippy::too_many_arguments)]
    pub fn route(
        time_ms: u64,
        request_id: &str,
        method: &str,
        path: &str,
        query: &str,
        route: &crate::registry::Route,
        params: &[(String, String)],
        limits: &crate::limits::Limits,
    ) {
        let text = |text: &str| Value::String(text.to_string());
        let count = |n: u64| Value::Number(Number::Unsigned(n));
        let shown = |name: &str, value: &str| match is_secret(name) {
            true => text(REDACTED),
            false => text(value),
        };
        let query = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                match is_secret(name) {
                    true => format!("{name}={REDACTED}"),
                    false => format!("{name}={value}"),
                }
            })
            .collect::<Vec<_>>()
            .join("&");
        let record = Value::Object(vec![
            ("kind".into(), text("route")),
            ("ms".into(), count(time_ms)),
            ("request_id".into(), text(request_id)),
            ("method".into(), text(method)),
            ("path".into(), text(path)),
            ("query".into(), text(&query)),
            ("route".into(), text(route.path)),
            ("handler".into(), text(&route.handler_name())),
            ("location".into(), text(&route.location())),
            (
                "params".into(),
                Value::Object(
                    params
                        .iter()
                        .map(|(name, value)| (name.clone(), shown(name, value)))
                        .collect(),
                ),
            ),
            ("body_size".into(), count(limits.body_size as u64)),
            (
                "timeout_ms".into(),
                count(limits.timeout.as_millis() as u64),
            ),
        ]);
        let mut line = String::new();
        write_value(&mut line, &record);
        append(&line);
    }

    /// Appends a line; past `MAX_SIZE`, the file becomes the previous one. The app and the
    /// supervisor both write: each line is one write, appended.
    fn append(line: &str) {
        let Some(journal) = JOURNAL.get() else {
            return;
        };
        let mut journal = journal.lock().unwrap_or_else(|e| e.into_inner());
        let journal = &mut *journal;
        let path = journal.dir.join(FILE);
        if journal.written > MAX_SIZE {
            journal.written = 0;
            journal.file = None;
            // Another process may have rotated it already.
            if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_SIZE) {
                let _ = std::fs::rename(&path, journal.dir.join(PREVIOUS));
            }
        }
        if journal.file.is_none() {
            let _ = std::fs::create_dir_all(&journal.dir);
            journal.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok();
            journal.written = std::fs::metadata(&path).map_or(0, |meta| meta.len());
        }
        let Some(file) = journal.file.as_mut() else {
            return;
        };
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        if file.write_all(&bytes).is_ok() {
            journal.written += bytes.len() as u64;
        }
    }
}
