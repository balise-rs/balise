//! Colors for a person at a terminal, with the conventions of Balise's logs: codes in bold,
//! `fix:` in green, `code` in cyan, what is secondary dimmed. Agents, `--json` and
//! `NO_COLOR` get the plain text: colors would only cost them tokens.

const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const CYAN: &str = "36";
const BOLD: &str = "1";
const DIM: &str = "2";

fn paint(text: &str, style: &str) -> String {
    format!("\x1b[{style}m{text}\x1b[0m")
}

/// Whether to color what goes to standard output or standard error: a terminal, or
/// `CLICOLOR_FORCE`, which `balise dev` sets for the supervisor in its container when the
/// terminal on the machine wants colors.
pub fn wanted(terminal: bool) -> bool {
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    (terminal || set("CLICOLOR_FORCE")) && !set("NO_COLOR")
}

/// `error` as the first word of a failure.
pub fn error_label() -> String {
    paint("error", &format!("{BOLD};{RED}"))
}

/// Explicit colors, for what a command shows a person and styles itself: the questions and
/// the result of `balise new`. Without colors, the text is unchanged.
#[derive(Clone, Copy)]
pub struct Painter(pub bool);

impl Painter {
    fn paint(self, text: &str, style: &str) -> String {
        match self.0 {
            true => paint(text, style),
            false => text.to_string(),
        }
    }
    pub fn bold(self, text: &str) -> String {
        self.paint(text, BOLD)
    }
    pub fn dim(self, text: &str) -> String {
        self.paint(text, DIM)
    }
    /// A command to run, or a value to type.
    pub fn command(self, text: &str) -> String {
        self.paint(text, CYAN)
    }
    pub fn success(self, text: &str) -> String {
        self.paint(text, &format!("{BOLD};{GREEN}"))
    }
    pub fn warn(self, text: &str) -> String {
        self.paint(text, YELLOW)
    }
    pub fn accent(self, text: &str) -> String {
        self.paint(text, &format!("{BOLD};{CYAN}"))
    }
}

/// Colors the text of a result or an error, line by line. A line a command already styled is
/// left as it is.
pub fn colorize(text: &str) -> String {
    text.lines()
        .map(|line| match line.starts_with("    ") {
            _ if line.contains('\x1b') => line.to_string(),
            // Indented by four spaces: an example, as in explanations.
            true => paint(line, DIM),
            false => colorize_line(line),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn colorize_line(line: &str) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let content = line.trim_start();
    if let Some(rest) = content.strip_prefix("fix:") {
        return format!("{indent}{}{}", paint("fix:", GREEN), inline(rest));
    }
    if content.starts_with("more:") {
        return format!("{indent}{}", paint(content, DIM));
    }
    // `CLI0003: …` or `HTTP0003  Title`: the code in bold.
    if let Some(code_end) = code_at_start(content) {
        let (code, rest) = content.split_at(code_end);
        return format!("{indent}{}{}", paint(code, BOLD), inline(rest));
    }
    // `Next:`, `Defaults applied:`: headings.
    if indent.is_empty() && content.ends_with(':') && !content.contains('`') {
        return paint(content, BOLD);
    }
    format!("{indent}{}", inline(content))
}

/// The length of an error code at the start of `text`: letters, then four digits.
fn code_at_start(text: &str) -> Option<usize> {
    let letters = text.chars().take_while(char::is_ascii_uppercase).count();
    let digits = text[letters..]
        .chars()
        .take_while(char::is_ascii_digit)
        .count();
    ((2..=4).contains(&letters) && digits == 4).then_some(letters + 4)
}

/// `` `code` `` in cyan, backticks kept as the plain text has them.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut parts = text.split('`');
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    let mut inside = true;
    for part in parts {
        match inside {
            true => out.push_str(&paint(&format!("`{part}`"), CYAN)),
            false => out.push_str(part),
        }
        inside = !inside;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn colors_never_change_the_text() {
        let text = "CLI0003: `balise new` needs a name\n  fix: Run `balise new shop`.\n  more: balise explain CLI0003\n\nNext:\n  cd shop\n\nExample:\n\n    [\"imports::create\"]\n    body_size = \"20MB\"\n\nAfter.";
        assert_eq!(plain(&colorize(text)), text);
    }

    #[test]
    fn each_part_has_its_color() {
        let colored = colorize("HTTP0003  No route\n  fix: Run `balise routes`.\nNext:");
        assert!(colored.starts_with("\x1b[1mHTTP0003\x1b[0m  No route"));
        assert!(colored.contains("\x1b[32mfix:\x1b[0m Run \x1b[36m`balise routes`\x1b[0m."));
        assert!(colored.ends_with("\x1b[1mNext:\x1b[0m"));
    }
}
