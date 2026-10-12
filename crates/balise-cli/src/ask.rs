//! Questions asked in a terminal: numbered choices, one line per answer, no dependency.

use crate::Context;

/// Asks to choose among `options`, each `(value, description)`; an empty answer takes
/// `default`. Returns the index chosen. At the end of the input, the default.
pub fn choose(
    context: &mut Context,
    question: &str,
    options: &[(&str, &str)],
    default: usize,
) -> usize {
    let _ = writeln!(context.output, "\n{question}");
    for (i, (value, description)) in options.iter().enumerate() {
        let _ = writeln!(context.output, "  {}) {value:<12} {description}", i + 1);
    }
    loop {
        let _ = write!(context.output, "[{}]: ", default + 1);
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
            "Answer with a number from 1 to {}.",
            options.len()
        );
    }
}

/// Asks a yes-or-no question; an empty answer takes `default`.
pub fn confirm(context: &mut Context, question: &str, default: bool) -> bool {
    let hint = if default { "Y/n" } else { "y/N" };
    loop {
        let _ = write!(context.output, "{question} [{hint}]: ");
        let _ = context.output.flush();
        let Some(answer) = read_line(context) else {
            return default;
        };
        match answer.to_ascii_lowercase().as_str() {
            "" => return default,
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => {
                let _ = writeln!(context.output, "Answer `y` or `n`.");
            }
        }
    }
}

/// Asks for a line of text; `None` at the end of the input.
pub fn line(context: &mut Context, question: &str) -> Option<String> {
    let _ = write!(context.output, "{question}: ");
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
