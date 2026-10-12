use balise::prelude::*;

#[derive(Input)]
struct Upload {
    #[input(max_size = "1MB", types("image/png"))]
    photo: File,
}

#[get("/uploads")]
async fn search(_filters: Query<Upload>) -> NoContent {
    NoContent
}

fn main() {}
