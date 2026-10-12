//! The /orders routes.

mod api;
mod model;

use balise::prelude::*;

use api::{
    CreateOrder, OrderDeleted, OrderFilters, OrderLineView, OrderList, OrderView, UpdateOrder,
};
use model::Order;

#[get("/orders")]
async fn list(filters: Query<OrderFilters>) -> OrderList {
    let items = (1..=filters.per_page)
        .map(Order::sample)
        .filter(|order| {
            filters
                .status
                .as_ref()
                .is_none_or(|status| *status == order.status)
        })
        .map(OrderView::from)
        .collect();
    OrderList { items }
}

#[get("/orders/{id}")]
async fn get(id: Path<u32>) -> OrderView {
    Order::sample(id).into()
}

#[post("/orders")]
async fn create(input: Body<CreateOrder>) -> OrderView {
    Order {
        quantity: input.quantity,
        note: input.note,
        contact: input.contact,
        ..Order::sample(1)
    }
    .into()
}

#[patch("/orders/{id}")]
async fn update(id: Path<u32>, input: Body<UpdateOrder>) -> OrderView {
    let mut order = Order::sample(id);
    if let Patch::Set(quantity) = input.quantity {
        order.quantity = quantity;
    }
    if let Patch::Set(note) = input.note {
        order.note = note;
    }
    order.into()
}

#[delete("/orders/{id}")]
async fn delete(id: Path<u32>) -> OrderDeleted {
    OrderDeleted { id }
}

#[get("/orders/{order_id}/lines/{line}")]
async fn line((order_id, line): Path<(u32, u32)>) -> OrderLineView {
    OrderLineView { order_id, line }
}
