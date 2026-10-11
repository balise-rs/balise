//! Inputs and outputs of the /orders routes: the contract with the front end.

use balise::prelude::*;

use super::model::OrderStatus;

#[derive(Input)]
pub struct CreateOrder {
    #[input(default = 1, min = 1, max = 100)]
    pub quantity: u32,
    pub note: Option<String>,
}

#[derive(Input)]
#[input(patch)]
pub struct UpdateOrder {
    #[input(min = 1, max = 100)]
    pub quantity: u32,
    pub note: Option<String>,
}

#[derive(Input)]
pub struct OrderFilters {
    pub status: Option<OrderStatus>,
}

#[derive(AppError)]
pub enum OrderError {
    #[error(status = 409, code = "ORD0001", fix = "Only pending orders can be cancelled.")]
    NotPending,
}

#[derive(Output)]
pub struct OrderView {
    pub id: OrderId,
    pub quantity: u32,
    pub note: Option<String>,
    pub status: OrderStatus,
    pub created_at: DateTime,
}
