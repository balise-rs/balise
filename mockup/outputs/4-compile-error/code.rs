// src/orders/mod.rs, lines 13 to 21: `list` written without the owner.
// The agent forgot `user: CurrentUser` and `.owned_by(&user)`.

#[get("/orders")]
async fn list(filters: Query<OrderFilters>, page: Query<PageParams>, db: Db) -> Result<Page<OrderView>> {
    let mut query = Order::query()
        .order_by(|o| o.created_at.desc());
    if let Some(status) = filters.status {
        query = query.filter(|o| o.status.eq(status));
    }
    Ok(query.page(db, page).await?.into())
}
