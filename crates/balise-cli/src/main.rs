//! The `balise` command-line tool, installed on the host.

use std::env;
use std::process::ExitCode;

use balise_core::error_codes;

const USAGE: &str = "usage: balise explain <CODE>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, code] if command == "explain" => explain(code),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn explain(code: &str) -> ExitCode {
    match error_codes::find(&code.to_ascii_uppercase()) {
        Some(code) => {
            print!("{}", code.explain());
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("`{code}` is not a Balise error code.");
            ExitCode::FAILURE
        }
    }
}
