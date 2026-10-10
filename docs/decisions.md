# Decisions

Status: living document.

Decisions taken while building Balise, with their reason. Most recent last. Options still to be measured live in the [log of alternatives](alternatives.md).

| Date | Decision | Reason |
| --- | --- | --- |
| 2026-10-10 | Crate names `balise` and `balise-*` reserved on crates.io, including `balise-macros` | Route attributes and derives need a separate proc-macro crate, so its name is reserved with the others |
| 2026-10-10 | npm scope `@balise-rs` for front-end packages (`@balise-rs/react`…) | `@balise` and the unscoped `balise` are taken on npm |
| 2026-10-10 | GitHub organization `balise-rs` | `balise` is taken on GitHub; same name as the npm scope, one identity everywhere |
| 2026-10-10 | Error codes declared with a `macro_rules!` in `balise-core/src/error_codes/`, one file per area prefix | No build script or parser, so no compile cost; one file per area keeps the registry readable |
| 2026-10-10 | Each code is a typed constant (`error_codes::EXT0001`) that only the registry can create | Raising an unregistered code from Rust does not compile; a test scanning the sources catches the rest (strings, comments) |
| 2026-10-10 | No doc page per error code: `balise explain` and MCP read the explanation from the registry | A page would repeat the registry; agents get the same text without leaving the loop. A website can be generated from the registry later if needed |
