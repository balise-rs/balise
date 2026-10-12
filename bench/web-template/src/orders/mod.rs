//! The /orders routes.

mod api;
mod model;

use balise::prelude::*;

use api::{
    AttachReceipt, CreateOrder, OrderError, OrderFilters, OrderLineView, OrderList, OrderView,
    ReceiptView, UpdateOrder,
};
use model::Order;

#[get("/orders", page(max = 50))]
async fn list(page: PageParams, filters: Query<OrderFilters>) -> OrderList {
    let items = (1..=page.per_page())
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
async fn create(input: Body<CreateOrder>) -> Result<Created<OrderView>> {
    let order = Order {
        quantity: input.quantity,
        note: input.note,
        contact: input.contact,
        ..Order::sample(1)
    };
    balise::log::info!(
        order_id = order.id,
        quantity = order.quantity,
        "Order created"
    );
    Ok(Created(order.into()))
}

#[patch("/orders/{id}")]
async fn update(id: Path<u32>, input: Body<UpdateOrder>) -> Result<OrderView> {
    let mut order = Order::sample(id);
    if order.status == "cancelled" {
        return Err(OrderError::Cancelled { id }.into());
    }
    if let Patch::Set(quantity) = input.quantity {
        order.quantity = quantity;
    }
    if let Patch::Set(note) = input.note {
        order.note = note;
    }
    Ok(order.into())
}

#[post("/orders/{id}/receipt")]
async fn attach_receipt(id: Path<u32>, input: Body<AttachReceipt>) -> Result<Created<ReceiptView>> {
    let size = input.receipt.size();
    let stored = input.receipt.store("receipts").await?;
    Ok(Created(ReceiptView {
        order_id: id,
        label: input.label,
        path: stored.path().to_string(),
        size,
    }))
}

#[delete("/orders/{id}")]
async fn delete(_id: Path<u32>) -> NoContent {
    NoContent
}

#[get("/orders/{order_id}/lines/{line}")]
async fn line((order_id, line): Path<(u32, u32)>) -> OrderLineView {
    OrderLineView { order_id, line }
}
