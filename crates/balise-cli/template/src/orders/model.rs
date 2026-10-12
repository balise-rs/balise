//! The orders, kept in memory until the app stores them: they are lost when it restarts.

#[derive(Default)]
pub struct Order {
    pub id: u32,
    pub quantity: u32,
    pub note: Option<String>,
    pub contact: String,
    pub status: String,
}

/// Every order, shared by the handlers: read with `State<Shared<Orders>>`.
#[derive(Default)]
pub struct Orders {
    orders: Vec<Order>,
    last_id: u32,
}

#[balise::state]
fn orders() -> balise::Shared<Orders> {
    balise::Shared::new(Orders::default())
}

impl Orders {
    pub fn all(&self) -> impl Iterator<Item = &Order> {
        self.orders.iter()
    }

    pub fn find(&self, id: u32) -> Option<&Order> {
        self.orders.iter().find(|order| order.id == id)
    }

    pub fn find_mut(&mut self, id: u32) -> Option<&mut Order> {
        self.orders.iter_mut().find(|order| order.id == id)
    }

    /// Adds the order with the next id, pending.
    pub fn insert(&mut self, order: Order) -> &Order {
        self.last_id += 1;
        self.orders.push(Order {
            id: self.last_id,
            status: "pending".to_string(),
            ..order
        });
        self.orders.last().expect("just pushed")
    }

    pub fn remove(&mut self, id: u32) -> Option<Order> {
        let index = self.orders.iter().position(|order| order.id == id)?;
        Some(self.orders.remove(index))
    }
}
