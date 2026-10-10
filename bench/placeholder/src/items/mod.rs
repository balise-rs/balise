//! One resource shaped like a Balise one: a model, an input, an output and five handlers.

// `..Item::default()` everywhere, so the benchmark can add a field to the model.
#![allow(dead_code, clippy::needless_update)]

#[derive(Debug, Default, Clone)]
struct Item {
    id: i64,
    name: String,
    price_cents: i64,
    // bench:model
}

#[derive(Debug, Default)]
struct CreateItem {
    name: String,
    price_cents: i64,
    // bench:field
}

#[derive(Debug)]
struct ItemView {
    id: i64,
    name: String,
}

impl From<Item> for ItemView {
    fn from(item: Item) -> Self {
        ItemView {
            id: item.id,
            name: item.name,
        }
    }
}

pub fn dispatch(route: &str) {
    match route {
        "list" => println!("{:?}", list()),
        "get" => println!("{:?}", get(1)),
        "create" => println!("{:?}", create(CreateItem::default())),
        "update" => println!("{:?}", update(1, CreateItem::default())),
        "delete" => delete(1),
        _ => {}
    }
}

fn list() -> Vec<ItemView> {
    (1..=3)
        .map(|id| {
            Item {
                id,
                ..Item::default()
            }
            .into()
        })
        .collect()
}

fn get(id: i64) -> ItemView {
    Item {
        id,
        ..Item::default()
    }
    .into()
}

fn create(input: CreateItem) -> ItemView {
    // bench:body
    Item {
        id: 0,
        name: input.name,
        price_cents: input.price_cents,
        ..Item::default()
    }
    .into()
}

fn update(id: i64, input: CreateItem) -> ItemView {
    Item {
        id,
        name: input.name,
        price_cents: input.price_cents,
        ..Item::default()
    }
    .into()
}

fn delete(id: i64) {
    println!("deleted {id}");
}
