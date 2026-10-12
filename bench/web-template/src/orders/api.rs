//! Inputs and outputs of the /orders routes.

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
    #[input(default = 20, min = 1, max = 100)]
    pub per_page: u32,
}

#[derive(Output)]
#[output(from = Order)]
pub struct OrderView {
    pub id: u32,
    pub quantity: u32,
    pub note: Option<String>,
    pub contact: String,
    pub status: String,
}

#[derive(Output)]
pub struct OrderList {
    pub items: Vec<OrderView>,
}

#[derive(Output)]
pub struct OrderLineView {
    pub order_id: u32,
    pub line: u32,
}

#[derive(AppError)]
pub enum OrderError {
    /// Cancelled orders are kept for the history, and cannot change.
    #[error(
        status = 409,
        code = "ORD0001",
        message = "Order {id} is cancelled",
        fix = "Create a new order instead of changing a cancelled one."
    )]
    Cancelled { id: u32 },
}
