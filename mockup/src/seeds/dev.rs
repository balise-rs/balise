//! The `dev` dataset, loaded on the first `balise dev`, so pages show credible data.

use balise::prelude::*;

use crate::orders::Order;

#[balise::seed]
async fn dev(db: Db) -> Result<()> {
    Order::fake().create_many(db, 25).await?;
    Ok(())
}
