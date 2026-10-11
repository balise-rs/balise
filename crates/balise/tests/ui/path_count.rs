// Stand-ins for the real extractors, so the only error is the macro's.
#![allow(dead_code)]
struct Path<T>(T);
struct Body<T>(T);

#[balise::get("/shops/{shop_id}/orders/{id}")]
async fn get(_id: Path<u8>) {}

fn main() {}
