use balise::prelude::*;

#[derive(Output)]
struct Row {
    id: u32,
}

#[get("/exports", page(default = 500, max = 100))]
async fn export(_page: PageParams) -> Vec<Row> {
    Vec::new()
}

#[get("/orders", page(max = 50))]
async fn list() -> Vec<Row> {
    Vec::new()
}

fn main() {}
