// Stand-ins for the real extractors, so the only error is the macro's.
#![allow(dead_code)]
struct Path<T>(T);
struct Body<T>(T);

#[balise::post("/orders")]
async fn create(_a: Body<u8>, _b: Body<u8>) {}

fn main() {}
