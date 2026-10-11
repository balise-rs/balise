//! Orders of the signed-in user.

mod api;
mod model;
#[cfg(test)]
mod tests;

use balise::prelude::*;

use api::{CreateOrder, OrderError, OrderFilters, OrderView, UpdateOrder};
pub use model::{Order, OrderId, OrderStatus};

#[get("/orders")]
async fn list(filters: Query<OrderFilters>, page: Query<PageParams>, user: CurrentUser, db: Db) -> Result<Page<OrderView>> {
    let mut query = Order::query()
        .owned_by(&user)
        .order_by(|o| o.created_at.desc());
    if let Some(status) = filters.status {
        query = query.filter(|o| o.status.eq(status));
    }
    Ok(query.page(db, page).await?.into())
}

#[get("/orders/{id}")]
async fn get(id: Path<OrderId>, user: CurrentUser, db: Db) -> Result<OrderView> {
    Ok(Order::get(db, *id).owned_by(&user).await?.into())
}

#[post("/orders")]
async fn create(input: Body<CreateOrder>, user: CurrentUser, db: Db) -> Result<Created<OrderView>> {
    let order = Order::create(db, input).owned_by(&user).await?;
    Ok(Created(order.into()))
}

#[patch("/orders/{id}")]
async fn update(id: Path<OrderId>, input: Body<UpdateOrder>, user: CurrentUser, db: Db) -> Result<OrderView> {
    let order = Order::get(db, *id).owned_by(&user).await?;
    Ok(order.update(db, input).await?.into())
}

#[delete("/orders/{id}")]
async fn delete(id: Path<OrderId>, user: CurrentUser, db: Db) -> Result<NoContent> {
    Order::get(db, *id).owned_by(&user).await?.delete(db).await?;
    Ok(NoContent)
}

#[post("/orders/{id}/cancel")]
async fn cancel(id: Path<OrderId>, user: CurrentUser, db: Db) -> Result<OrderView> {
    let order = Order::get(db, *id).owned_by(&user).await?;
    if !order.is_pending() {
        return Err(OrderError::NotPending.into());
    }
    let order = order.update(db, |o| o.status = OrderStatus::Cancelled).await?;
    Ok(order.into())
}
