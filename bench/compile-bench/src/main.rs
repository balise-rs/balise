//! Compile-time benchmark: generates the reference project and measures compile times.

mod generate;
mod measure;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        Some("generate") => generate::main(args),
        Some("measure") => measure::main(args),
        _ => Err(format!(
            "usage:\n  {}\n  {}",
            generate::USAGE,
            measure::USAGE
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("compile-bench: {message}");
            ExitCode::FAILURE
        }
    }
}
