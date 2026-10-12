//! The Balise CLI of one version. Users run it through `balise`, which picks the version of
//! their project.

use std::io::Write;
use std::process::ExitCode;

use balise_cli::{Context, System, person_at_terminal, run};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut questions = std::io::stderr();
    let mut context = Context {
        cwd: std::env::current_dir().unwrap_or_default(),
        interactive: person_at_terminal(),
        input: &mut input,
        output: &mut questions,
        machine: &System,
    };
    let json = args.iter().any(|arg| arg == "--json");
    let (out, code) = run(&args, &mut context);
    // A result goes to standard output, as JSON for programs; an error otherwise goes to
    // standard error.
    match (code, json) {
        (0, _) | (_, true) => println!("{out}"),
        _ => {
            let _ = writeln!(std::io::stderr(), "error {out}");
        }
    }
    ExitCode::from(code)
}
