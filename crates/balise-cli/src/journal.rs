//! Reading the dev journal, which the app built by `balise dev` and the supervisor write in
//! `.balise/dev/` (see `balise_core::journal`).

use std::path::Path;

use balise_core::journal::{DIR, FILE, PREVIOUS};
use balise_core::json::{self, Value};

use crate::value::{get, number, text};

/// One line of the journal.
pub struct Record {
    pub value: Value,
}

impl Record {
    pub fn kind(&self) -> &str {
        text(&self.value, "kind").unwrap_or_default()
    }

    /// Milliseconds since the Unix epoch.
    pub fn ms(&self) -> u64 {
        number(&self.value, "ms") as u64
    }

    /// For an event: the event, as the JSON logs write it.
    pub fn event(&self) -> Option<&Value> {
        get(&self.value, "event")
    }

    /// For an event: as the terminal shows it.
    pub fn text(&self) -> &str {
        text(&self.value, "text").unwrap_or_default()
    }

    pub fn request_id(&self) -> Option<&str> {
        match self.kind() {
            "route" => text(&self.value, "request_id"),
            _ => self.event().and_then(|event| text(event, "request_id")),
        }
    }
}

/// Every record, oldest first; `None` when the journal does not exist yet.
pub fn read(root: &Path) -> Option<Vec<Record>> {
    let dir = root.join(DIR);
    let mut found = false;
    let mut records = Vec::new();
    for name in [PREVIOUS, FILE] {
        let Ok(content) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        found = true;
        records.extend(
            content
                .lines()
                .filter_map(|line| json::parse(line.as_bytes()).ok())
                .map(|value| Record { value }),
        );
    }
    found.then_some(records)
}
