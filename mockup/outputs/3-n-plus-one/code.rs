// src/orders/mod.rs, lines 50 to 64: a dashboard of the 20 latest orders with each customer's email.
// Not in the mock-up: written the way an agent would, with the N+1 on line 60.

#[get("/admin/orders/latest")]
async fn latest(admin: Admin, db: Db) -> Result<Vec<AdminOrderView>> {
    let orders = Order::query()
        .all_owners("admin dashboard: latest orders")
        .order_by(|o| o.created_at.desc())
        .limit(20)
        .all(db)
        .await?;
    let mut views = Vec::new();
    for order in orders {
        let customer = User::get(db, order.user_id).await?;
        views.push(AdminOrderView::new(order, customer));
    }
    Ok(views)
}
