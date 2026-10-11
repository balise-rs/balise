use balise::prelude::*;

struct Money;

#[derive(Input)]
struct CreateOrder {
    price: Money,
}

#[derive(Output)]
struct OrderView {
    price: Money,
}

fn main() {}
