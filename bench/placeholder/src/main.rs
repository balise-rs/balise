mod items;

fn main() {
    let route = std::env::args().nth(1).unwrap_or_default();
    items::dispatch(&route);
}
