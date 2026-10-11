//! Inputs and outputs of the auth routes: the contract with the front end.

use balise::prelude::*;

#[derive(Input)]
pub struct SignUp {
    #[input(email)]
    pub email: String,
    #[input(length(min = 12, max = 128))]
    pub password: String,
}

#[derive(Input)]
pub struct SignIn {
    pub email: String,
    pub password: String,
}

#[derive(Input)]
pub struct RequestPasswordReset {
    #[input(email)]
    pub email: String,
}

#[derive(Input)]
pub struct ResetPassword {
    pub token: String,
    #[input(length(min = 12, max = 128))]
    pub password: String,
}

#[derive(Output)]
pub struct CurrentUserView {
    pub id: UserId,
    pub email: String,
}
