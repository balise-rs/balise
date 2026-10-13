[package]
name = "{{name}}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
balise = {{balise_dependency}}

# Enabled by `balise dev`. A release build never has it.
[features]
dev = ["balise/dev"]

[lints.rust]
deprecated = "deny"

# Checked by `balise check`, which runs clippy; `clippy.toml` lists what is disallowed.
[lints.clippy]
await_holding_lock = "deny"
disallowed_types = "deny"
disallowed_macros = "deny"
disallowed_methods = "deny"

# Fast edit-compile loop: dependencies optimized once, your code compiled quickly.
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
opt-level = 2
debug = false

# Production backtraces keep their files and lines.
[profile.release]
debug = "line-tables-only"
