//! The /orders routes: the example resource. Remove it with `balise remove resource orders`.

mod api;
mod model;

use balise::prelude::*;

use api::{CreateOrder, OrderError, OrderFilters, OrderView, UpdateOrder};
use model::{Order, Orders};

#[get("/orders")]
async fn list(
    page: PageParams,
    filters: Query<OrderFilters>,
    orders: State<Shared<Orders>>,
) -> Vec<OrderView> {
    let orders = orders.read().await;
    orders
        .all()
        .filter(|order| {
            filters
                .status
                .as_ref()
                .is_none_or(|status| *status == order.status)
        })
        .take(page.per_page() as usize)
        .map(OrderView::from)
        .collect()
}

#[get("/orders/{id}")]
async fn get(id: Path<u32>, orders: State<Shared<Orders>>) -> Result<OrderView> {
    let orders = orders.read().await;
    let order = orders.find(id).ok_or(OrderError::NotFound { id })?;
    Ok(OrderView::from(order))
}

#[post("/orders")]
async fn create(
    input: Body<CreateOrder>,
    orders: State<Shared<Orders>>,
) -> Result<Created<OrderView>> {
    let mut orders = orders.write().await;
    let order = orders.insert(Order {
        quantity: input.quantity,
        note: input.note,
        contact: input.contact,
        ..Order::default()
    });
    balise::log::info!(order_id = order.id, "Order created");
    Ok(Created(OrderView::from(order)))
}

#[patch("/orders/{id}")]
async fn update(
    id: Path<u32>,
    input: Body<UpdateOrder>,
    orders: State<Shared<Orders>>,
) -> Result<OrderView> {
    let mut orders = orders.write().await;
    let order = orders.find_mut(id).ok_or(OrderError::NotFound { id })?;
    if order.status == "cancelled" {
        return Err(OrderError::Cancelled { id }.into());
    }
    if let Patch::Set(quantity) = input.quantity {
        order.quantity = quantity;
    }
    if let Patch::Set(note) = input.note {
        order.note = note;
    }
    Ok(OrderView::from(&*order))
}

#[delete("/orders/{id}")]
async fn delete(id: Path<u32>, orders: State<Shared<Orders>>) -> Result<NoContent> {
    orders
        .write()
        .await
        .remove(id)
        .ok_or(OrderError::NotFound { id })?;
    Ok(NoContent)
}
