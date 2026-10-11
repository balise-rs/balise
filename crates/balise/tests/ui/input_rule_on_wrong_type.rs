use balise::prelude::*;

#[derive(Input)]
struct CreateOrder {
    #[input(email)]
    quantity: u32,
}

fn main() {}
