use balise::prelude::*;

struct CreateOrder;

#[post("/orders")]
async fn create(_input: CreateOrder) {}

fn main() {}
