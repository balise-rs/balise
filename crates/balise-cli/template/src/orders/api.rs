//! Inputs, outputs and errors of the /orders routes: the contract with the front end.

use balise::prelude::*;

use super::model::Order;

#[derive(Input)]
pub struct CreateOrder {
    #[input(default = 1, min = 1, max = 100)]
    pub quantity: u32,
    #[input(length(max = 500))]
    pub note: Option<String>,
    #[input(email)]
    pub contact: String,
}

#[derive(Input)]
pub struct UpdateOrder {
    #[input(min = 1, max = 100)]
    pub quantity: Patch<u32>,
    pub note: Patch<Option<String>>,
}

#[derive(Input)]
pub struct OrderFilters {
    #[input(one_of("pending", "cancelled"))]
    pub status: Option<String>,
}

#[derive(Output)]
pub struct OrderView {
    pub id: u32,
    pub quantity: u32,
    pub note: Option<String>,
    pub contact: String,
    pub status: String,
}

impl From<&Order> for OrderView {
    fn from(order: &Order) -> OrderView {
        OrderView {
            id: order.id,
            quantity: order.quantity,
            note: order.note.clone(),
            contact: order.contact.clone(),
            status: order.status.clone(),
        }
    }
}

#[derive(AppError)]
pub enum OrderError {
    /// The order was deleted, or never existed.
    #[error(
        status = 404,
        code = "ORD0001",
        message = "Order {id} does not exist",
        fix = "Check the order's id: `GET /orders` lists the orders."
    )]
    NotFound { id: u32 },
    /// Cancelled orders are kept for the history, and cannot change.
    #[error(
        status = 409,
        code = "ORD0002",
        message = "Order {id} is cancelled",
        fix = "Create a new order instead of changing a cancelled one."
    )]
    Cancelled { id: u32 },
}
