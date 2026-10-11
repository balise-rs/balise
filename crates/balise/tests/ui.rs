//! Each mistake in a route attribute fails to compile with its `CMP` code and fix.
//! `TRYBUILD=overwrite cargo test -p balise --test ui` updates the expected outputs.

#[test]
fn route_attribute_errors() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
