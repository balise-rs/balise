struct Cache;

#[balise::state]
fn cache(size: u32) -> Cache {
    let _ = size;
    Cache
}

#[balise::state]
fn nothing() {}

fn main() {}
