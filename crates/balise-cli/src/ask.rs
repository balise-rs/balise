//! Questions asked in a terminal: numbered choices, one line per answer, no dependency.

use crate::Context;
use crate::style::Painter;

/// A question, after a mark that makes it stand out.
fn question(p: Painter, text: &str) -> String {
    format!("{} {}", p.accent("?"), p.bold(text))
}

/// Asks to choose among `options`, each `(value, description)`; an empty answer takes
/// `default`. Returns the index chosen. At the end of the input, the default.
pub fn choose(
    context: &mut Context,
    question: &str,
    options: &[(&str, &str)],
    default: usize,
) -> usize {
    let p = Painter(context.color);
    let _ = writeln!(context.output, "\n{}", self::question(p, question));
    for (i, (value, description)) in options.iter().enumerate() {
        let number = p.command(&format!("{})", i + 1));
        let value = p.bold(&format!("{value:<12}"));
        let _ = writeln!(context.output, "  {number} {value} {}", p.dim(description));
    }
    loop {
        let _ = write!(context.output, "{} ", p.dim(&format!("[{}]:", default + 1)));
        let _ = context.output.flush();
        let Some(answer) = read_line(context) else {
            return default;
        };
        if answer.is_empty() {
            return default;
        }
        if let Ok(n) = answer.parse::<usize>()
            && (1..=options.len()).contains(&n)
        {
            return n - 1;
        }
        if let Some(i) = options
            .iter()
            .position(|(value, _)| value.eq_ignore_ascii_case(&answer))
        {
            return i;
        }
        let _ = writeln!(
            context.output,
            "{}",
            p.warn(&format!(
                "Answer with a number from 1 to {}.",
                options.len()
            ))
        );
    }
}

/// Asks a yes-or-no question; an empty answer takes `default`.
pub fn confirm(context: &mut Context, question: &str, default: bool) -> bool {
    let p = Painter(context.color);
    let hint = if default { "[Y/n]:" } else { "[y/N]:" };
    // A question after a blank line keeps it.
    let (mut blank, question) = match question.strip_prefix('\n') {
        Some(question) => ("\n", question),
        None => ("", question),
    };
    loop {
        let _ = write!(
            context.output,
            "{blank}{} {} ",
            self::question(p, question),
            p.dim(hint)
        );
        blank = "";
        let _ = context.output.flush();
        let Some(answer) = read_line(context) else {
            return default;
        };
        match answer.to_ascii_lowercase().as_str() {
            "" => return default,
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => {
                let _ = writeln!(context.output, "{}", p.warn("Answer `y` or `n`."));
            }
        }
    }
}

/// Asks for a line of text; `None` at the end of the input.
pub fn line(context: &mut Context, question: &str) -> Option<String> {
    let p = Painter(context.color);
    let _ = write!(
        context.output,
        "{}{} ",
        self::question(p, question),
        p.bold(":")
    );
    let _ = context.output.flush();
    read_line(context).filter(|answer| !answer.is_empty())
}

fn read_line(context: &mut Context) -> Option<String> {
    let mut line = String::new();
    match context.input.read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}
