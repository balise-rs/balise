//! What the application's binary does when it starts: serve, or run `config check`.

use balise_core::config::load::{self, Files, Mode};
use balise_core::config::{Config, Environment};
use balise_core::error_codes::CFG0008;
use balise_core::event::{Event, Level};
use balise_core::log::{self, Format, Logger};

/// Starts the application. `build` is the environment the binary was built for.
///
/// Without arguments, it loads the configuration and serves. `config check [--env NAME]`
/// checks the configuration with the strictest rules and exits.
pub fn start(files: Files, build: Environment) -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let variables: Vec<(String, String)> = std::env::vars().collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => serve(files, build, &variables),
        ["config", "check", rest @ ..] => std::process::exit(check(files, build, &variables, rest)),
        _ => {
            let event = Event::coded(Level::Error, CFG0008, &[("command", &args.join(" "))]);
            println!("{}", event.render_text(color()));
            std::process::exit(2)
        }
    }
}

fn serve(files: Files, build: Environment, variables: &[(String, String)]) -> ! {
    let environment = match load::environment(build, variables) {
        Ok(environment) => environment,
        Err(event) => fail(&[*event], build),
    };
    let loaded = match load::load(&files, environment, build, variables, Mode::Start) {
        Ok(loaded) => loaded,
        Err(errors) => fail(&errors, environment),
    };
    let config = loaded.config;
    log::install(Logger::stdout(config.log_format, config.log_level));
    for warning in loaded.warnings {
        warning.emit();
    }
    crate::run(config)
}

/// Reports what stops the startup, in the format of the environment, and exits.
fn fail(events: &[Event], environment: Environment) -> ! {
    let format = Config::defaults(environment).log_format;
    log::install(Logger::stdout(format, Level::Info));
    for event in events {
        event.clone().emit();
    }
    std::process::exit(1)
}

/// `config check [--env NAME]`: every problem is an error, whatever the environment. Returns
/// the exit code.
fn check(files: Files, build: Environment, variables: &[(String, String)], args: &[&str]) -> i32 {
    let environment = match args {
        [] => load::environment(build, variables),
        ["--env", name] => load::environment(build, &[("BALISE_ENV".into(), name.to_string())]),
        _ => {
            let command = format!("config check {}", args.join(" "));
            let event = Event::coded(Level::Error, CFG0008, &[("command", &command)]);
            println!("{}", event.render_text(color()));
            return 2;
        }
    };
    let environment = match environment {
        Ok(environment) => environment,
        Err(event) => {
            println!("{}", event.render_text(color()));
            return 1;
        }
    };
    match load::load(&files, environment, build, variables, Mode::Check) {
        Ok(_) => {
            println!("✓ The configuration is valid for {}", environment.name());
            0
        }
        Err(errors) => {
            for event in &errors {
                println!("{}", event.render_text(color()));
            }
            let noun = if errors.len() == 1 { "error" } else { "errors" };
            println!(
                "✗ {} {noun} in the configuration for {}",
                errors.len(),
                environment.name()
            );
            1
        }
    }
}

/// Colors for a person at a terminal, never with `NO_COLOR`.
fn color() -> bool {
    Logger::stdout(Format::Text, Level::Info).color
}
