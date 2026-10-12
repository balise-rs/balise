# Repository commands. `just --list` shows them.

# Check, tag the current commit, push it and publish the crates: `just publish 0.1.0-alpha.2`
publish version remote="public":
    cargo run --quiet -p release -- {{version}} {{remote}}
