use balise::prelude::*;

#[derive(Input)]
struct CreateOrder {
    #[input(minimum = 1)]
    quantity: u32,
}

fn main() {}
