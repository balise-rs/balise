//! `balise dev`: the dev environment, with rebuild on change.

use std::io::IsTerminal;

use balise_core::error_codes::CLI0006;

use crate::args::{Opt, Parsed};
use crate::{Command, Context, Failure, Report, dev, project, style};

pub const COMMAND: Command = Command {
    name: "dev",
    summary: "Run the app in dev: rebuilt and restarted at each change, one stream of logs.",
    arguments: "",
    options: &[
        Opt::flag(
            "--native",
            "build and run on this machine, with its Rust, without Docker",
        ),
        Opt::flag(
            "--inside",
            "the supervisor inside the dev container; run by the container",
        ),
    ],
    exclusive: &[&["--native", "--inside"]],
    run,
};

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    if !parsed.has("--native") && !parsed.has("--inside") {
        return Err(Failure::usage(
            CLI0006,
            &[
                ("what", "Running in Docker"),
                ("version", env!("CARGO_PKG_VERSION")),
            ],
            &[("instead", "use `balise dev --native`")],
        ));
    }
    let root = project::require(&context.cwd, "dev")?;
    dev::catch_signals();
    let mut out = std::io::stdout();
    let color = style::wanted(std::io::stdout().is_terminal());
    dev::supervise(&root, &mut out, color, &dev::stop_requested)?;
    Ok(crate::text("balise dev stopped.".into()))
}
