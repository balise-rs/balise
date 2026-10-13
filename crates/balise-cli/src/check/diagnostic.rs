//! One diagnostic, read from the compiler's JSON and kept as Balise shows it: a code, a
//! message, where the fix goes, the lines there, the fix and where to learn more.

use std::path::Path;

use balise_core::error_codes::ErrorCode;
use balise_core::json::{Number, Value};

use super::{enrich, source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A line of code shown under a diagnostic, with the columns to underline (from 1, end
/// excluded).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// `CMP0006`, rustc's `E0308`, a lint's name (`unused_variables`), or `rustfmt`.
    pub code: String,
    pub message: String,
    /// Relative to the project, where the fix goes.
    pub file: Option<String>,
    pub line: usize,
    pub column: usize,
    /// `orders::list`.
    pub function: Option<String>,
    pub snippet: Vec<Line>,
    /// What the underline points at.
    pub label: Option<String>,
    pub fix: Option<String>,
    /// `balise explain CMP0006`.
    pub more: Option<String>,
}

impl Diagnostic {
    /// A diagnostic of Balise's own, without a location yet.
    pub fn coded(
        severity: Severity,
        code: &ErrorCode,
        message_args: &[(&str, &str)],
        fix_args: &[(&str, &str)],
    ) -> Diagnostic {
        Diagnostic {
            severity,
            code: code.code().to_string(),
            message: code.format_message(message_args),
            file: None,
            line: 0,
            column: 0,
            function: None,
            snippet: Vec::new(),
            label: None,
            fix: Some(code.format_fix(fix_args)),
            more: Some(format!("balise explain {}", code.code())),
        }
    }

    /// Takes Balise's code, message and fix, keeping where it is.
    pub fn recode(
        &mut self,
        code: &ErrorCode,
        message_args: &[(&str, &str)],
        fix_args: &[(&str, &str)],
    ) {
        let coded = Diagnostic::coded(self.severity, code, message_args, fix_args);
        self.code = coded.code;
        self.message = coded.message;
        self.fix = coded.fix;
        self.more = coded.more;
    }

    /// Moves the diagnostic to a span of the compiler's JSON.
    pub fn locate(&mut self, root: &Path, span: &Value) {
        let file = text(span, "file_name").unwrap_or_default().to_string();
        self.line = number(span, "line_start");
        self.column = number(span, "column_start");
        self.function = source::function_at(root, &file, self.line);
        self.label = text(span, "label").map(str::to_string);
        self.snippet = snippet(span);
        self.file = Some(file);
    }

    /// Reads one message of `cargo --message-format json`. `None` for what is not a
    /// diagnostic of the project: summaries, notes about failure.
    pub fn from_rustc(root: &Path, message: &Value) -> Option<Diagnostic> {
        let severity = match text(message, "level")? {
            "error" => Severity::Error,
            "warning" => Severity::Warning,
            _ => return None,
        };
        let full = text(message, "message")?;
        let spans = array(message, "spans");
        if spans.is_empty()
            && (full.starts_with("aborting due to")
                || full.ends_with("warnings emitted")
                || full.ends_with("warning emitted"))
        {
            return None;
        }
        let rustc_code = get(message, "code").and_then(|code| text(code, "code"));
        let mut diagnostic = Diagnostic {
            severity,
            code: rustc_code.unwrap_or_default().to_string(),
            message: full.to_string(),
            file: None,
            line: 0,
            column: 0,
            function: None,
            snippet: Vec::new(),
            label: None,
            fix: None,
            more: rustc_code
                .filter(|code| is_rustc_code(code))
                .map(|code| format!("balise explain {code}")),
        };
        // Balise's macros and traits write `CMP0006: message`, then `fix:` and `more:`.
        let (first, rest) = full.split_once('\n').unwrap_or((full, ""));
        if let Some((code, text)) = first.split_once(": ")
            && is_balise_code(code)
        {
            diagnostic.code = code.to_string();
            diagnostic.message = text.to_string();
            diagnostic.more = Some(format!("balise explain {code}"));
            for line in rest
                .lines()
                .chain(children_texts(message_children(message)))
            {
                let line = line.trim();
                if let Some(fix) = line.strip_prefix("fix: ") {
                    diagnostic.fix = Some(fix.to_string());
                } else if let Some(more) = line.strip_prefix("more: ") {
                    diagnostic.more = Some(more.to_string());
                }
            }
        } else {
            diagnostic.fix = helps(message);
        }
        if let Some(span) = location(spans) {
            diagnostic.locate(root, span);
        }
        enrich::rustc(root, message, &mut diagnostic);
        Some(diagnostic)
    }

    /// The diagnostic as data, for `--json` and the MCP.
    pub fn to_value(&self) -> Value {
        let optional = |value: &Option<String>| value.clone().map_or(Value::Null, Value::String);
        let count = |n: usize| Value::Number(Number::Unsigned(n as u64));
        let snippet = self
            .snippet
            .iter()
            .map(|line| {
                Value::Object(vec![
                    ("end".into(), count(line.end)),
                    ("line".into(), count(line.number)),
                    ("start".into(), count(line.start)),
                    ("text".into(), Value::String(line.text.clone())),
                ])
            })
            .collect();
        Value::Object(vec![
            ("code".into(), Value::String(self.code.clone())),
            ("column".into(), count(self.column)),
            ("file".into(), optional(&self.file)),
            ("fix".into(), optional(&self.fix)),
            ("function".into(), optional(&self.function)),
            ("label".into(), optional(&self.label)),
            ("line".into(), count(self.line)),
            ("message".into(), Value::String(self.message.clone())),
            ("more".into(), optional(&self.more)),
            (
                "severity".into(),
                Value::String(self.severity.name().into()),
            ),
            ("snippet".into(), Value::Array(snippet)),
        ])
    }

    /// Reads [`to_value`](Self::to_value)'s data back: `balise check` relays the dev
    /// container's.
    pub fn from_value(value: &Value) -> Option<Diagnostic> {
        let optional = |key| text(value, key).map(str::to_string);
        Some(Diagnostic {
            severity: match text(value, "severity")? {
                "error" => Severity::Error,
                _ => Severity::Warning,
            },
            code: optional("code")?,
            message: optional("message")?,
            file: optional("file"),
            line: number(value, "line"),
            column: number(value, "column"),
            function: optional("function"),
            snippet: array(value, "snippet")
                .iter()
                .map(|line| Line {
                    number: number(line, "line"),
                    text: text(line, "text").unwrap_or_default().to_string(),
                    start: number(line, "start"),
                    end: number(line, "end"),
                })
                .collect(),
            label: optional("label"),
            fix: optional("fix"),
            more: optional("more"),
        })
    }
}

/// `E0308`.
pub fn is_rustc_code(code: &str) -> bool {
    code.len() == 5 && code.starts_with('E') && code[1..].chars().all(|c| c.is_ascii_digit())
}

/// `CMP0006`, `HTTP0014`: letters, then four digits.
fn is_balise_code(code: &str) -> bool {
    let letters = code.chars().take_while(char::is_ascii_uppercase).count();
    (2..=4).contains(&letters)
        && code.len() == letters + 4
        && code[letters..].chars().all(|c| c.is_ascii_digit())
}

/// Where the fix goes: the primary span, followed back through macros to the code that
/// called them, since paths inside Balise or the registry mean nothing to the project.
fn location(spans: &[Value]) -> Option<&Value> {
    let primary = spans
        .iter()
        .find(|span| matches!(get(span, "is_primary"), Some(Value::Bool(true))))?;
    let mut span = primary;
    loop {
        if is_local(span) {
            return Some(span);
        }
        match get(span, "expansion").and_then(|expansion| get(expansion, "span")) {
            Some(caller) => span = caller,
            None => break,
        }
    }
    spans.iter().find(|span| is_local(span))
}

/// A file of the project: rustc writes them relative to it, and others absolute.
pub fn is_local(span: &Value) -> bool {
    text(span, "file_name").is_some_and(|file| {
        !file.starts_with('/') && !file.starts_with('<') && !file.contains(":\\")
    })
}

fn snippet(span: &Value) -> Vec<Line> {
    let first = number(span, "line_start");
    array(span, "text")
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, line)| Line {
            number: first + i,
            text: text(line, "text").unwrap_or_default().to_string(),
            start: number(line, "highlight_start"),
            end: number(line, "highlight_end"),
        })
        .collect()
}

pub fn message_children(message: &Value) -> &[Value] {
    array(message, "children")
}

fn children_texts(children: &[Value]) -> impl Iterator<Item = &str> {
    children.iter().filter_map(|child| text(child, "message"))
}

/// The compiler's suggestions, as one fix: `prefix it with an underscore: `_x``.
fn helps(message: &Value) -> Option<String> {
    let helps: Vec<String> = message_children(message)
        .iter()
        .filter(|child| text(child, "level") == Some("help"))
        .filter_map(|child| {
            let help = text(child, "message")?;
            if help.starts_with("for further information visit") {
                return None;
            }
            let replacement = array(child, "spans")
                .iter()
                .filter(|span| is_local(span))
                .find_map(|span| text(span, "suggested_replacement"))
                .filter(|r| !r.trim().is_empty() && !r.contains('\n') && r.len() <= 60);
            Some(match replacement {
                Some(replacement) => format!("{help}: `{}`", replacement.trim()),
                None => help.to_string(),
            })
        })
        .collect();
    if helps.is_empty() {
        return None;
    }
    let fix = helps.join("; ");
    let mut chars = fix.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    Some(format!("{first}{}.", chars.as_str().trim_end_matches('.')))
}

pub fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

pub fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match get(value, key)? {
        Value::String(text) => Some(text),
        _ => None,
    }
}

pub fn number(value: &Value, key: &str) -> usize {
    match get(value, key) {
        Some(Value::Number(Number::Integer(n))) => (*n).max(0) as usize,
        Some(Value::Number(Number::Unsigned(n))) => *n as usize,
        _ => 0,
    }
}

pub fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    match get(value, key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}
