//! Stands in for the `Order` model until the ORM: a plain struct, never stored.

#[derive(Default)]
pub struct Order {
    pub id: u32,
    pub quantity: u32,
    pub note: Option<String>,
    pub contact: String,
    pub status: String,
}

impl Order {
    pub fn sample(id: u32) -> Order {
        Order {
            id,
            quantity: 1,
            status: "pending".to_string(),
            ..Order::default()
        }
    }
}
