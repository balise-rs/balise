mod auth;
mod orders;
mod users;

#[cfg(any(feature = "dev", feature = "test"))]
mod seeds;

fn main() {
    balise::run!();
}
