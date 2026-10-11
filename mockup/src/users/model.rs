//! The User model. `#[auth::user]` checks that the fields the `auth` module needs are present;
//! every other field is yours.

use balise::prelude::*;

#[derive(Model)]
#[auth::user]
pub struct User {
    pub id: UserId,
    #[field(unique, max_length = 254)]
    pub email: String,
    pub password_hash: PasswordHash,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}
