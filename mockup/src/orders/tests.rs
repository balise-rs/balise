//! Tests for the /orders routes. Validation rules are tested by generated tests.

use balise::test::*;

use super::api::{CreateOrder, OrderView};
use super::{Order, OrderStatus};

#[balise::test]
async fn create_returns_201(app: TestApp) {
    app.sign_in_as_new_user().await;

    let input = CreateOrder { quantity: 2, note: None };
    let res = app.post("/orders").body(input).send().await;

    res.assert_status(201);
    let order: OrderView = res.json();
    assert_eq!(order.quantity, 2);
    assert_eq!(order.status, OrderStatus::Pending);
}

#[balise::test]
async fn list_shows_only_own_orders(app: TestApp) {
    let user = app.sign_in_as_new_user().await;
    Order::fake().owned_by(&user).create_many(app.db(), 3).await;
    Order::fake().create_many(app.db(), 2).await;

    let res = app.get("/orders").send().await;

    res.assert_status(200);
    res.assert_json_len("items", 3);
}

#[balise::test]
async fn get_hides_other_users_orders(app: TestApp) {
    app.sign_in_as_new_user().await;
    let other = Order::fake().create(app.db()).await;

    let res = app.get(format!("/orders/{}", other.id)).send().await;

    res.assert_status(404);
}

#[balise::test]
async fn cancel_twice_is_a_conflict(app: TestApp) {
    let user = app.sign_in_as_new_user().await;
    let order = Order::fake()
        .owned_by(&user)
        .with(|o| o.status = OrderStatus::Cancelled)
        .create(app.db())
        .await;

    let res = app.post(format!("/orders/{}/cancel", order.id)).send().await;

    res.assert_status(409);
    res.assert_error_code("ORD0001");
}
