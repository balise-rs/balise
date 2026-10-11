//! The Order model.

use balise::prelude::*;

use crate::users::User;

#[derive(Model)]
#[model(owned_by = User, expected_rows = "1M")]
#[index(user, created_at)]
pub struct Order {
    pub id: OrderId,
    pub user: BelongsTo<User>,
    #[field(check = "quantity > 0")]
    pub quantity: u32,
    #[field(max_length = 500)]
    pub note: Option<String>,
    #[field(default = OrderStatus::Pending)]
    pub status: OrderStatus,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Enum)]
pub enum OrderStatus {
    Pending,
    Cancelled,
}

impl Order {
    pub fn is_pending(&self) -> bool {
        self.status == OrderStatus::Pending
    }
}
