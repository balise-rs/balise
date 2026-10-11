//! The `dev` dataset, loaded on the first `balise dev`, so pages show credible data.

use balise::prelude::*;

// balise:orders
use crate::orders::Order;
// /balise:orders
use crate::users::User;

#[balise::seed]
async fn dev(db: Db) -> Result<()> {
    // Sign in with dev@example.com / password.
    let user = User::fake()
        .with(|u| u.email = "dev@example.com".into())
        .with_password("password")
        .create(db)
        .await?;
    // balise:orders
    Order::fake().owned_by(&user).create_many(db, 25).await?;
    Order::fake().create_many(db, 10).await?;
    // /balise:orders
    Ok(())
}
