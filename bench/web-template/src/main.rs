//! The compile benchmark's "web" variant: what Balise can already build, without the ORM,
//! auth or mail. Each resource uses `Path`, `Query`, `Body`, `Input`, `Output` and `Patch`.

mod orders;

fn main() {
    balise::run();
}
