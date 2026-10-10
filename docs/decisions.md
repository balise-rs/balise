# Decisions

Status: living document.

Decisions taken while building Balise, with their reason. Most recent last. Options still to be measured live in the [log of alternatives](alternatives.md).

| Date | Decision | Reason |
| --- | --- | --- |
| 2026-10-10 | Crate names `balise` and `balise-*` reserved on crates.io, including `balise-macros` | Route attributes and derives need a separate proc-macro crate, so its name is reserved with the others |
| 2026-10-10 | npm scope `@balise-rs` for front-end packages (`@balise-rs/react`…) | `@balise` and the unscoped `balise` are taken on npm |
| 2026-10-10 | GitHub organization `balise-rs` | `balise` is taken on GitHub; same name as the npm scope, one identity everywhere |
