//! Tests for the auth routes.

use balise::test::*;

use super::api::{RequestPasswordReset, SignUp};
use crate::users::User;

#[balise::test]
async fn sign_up_then_me(app: TestApp) {
    let res = app
        .post("/auth/sign-up")
        .body(SignUp { email: "ada@example.com".into(), password: "correct horse battery".into() })
        .send()
        .await;
    res.assert_status(201);

    let res = app.get("/auth/me").send().await;
    res.assert_status(200);
    res.assert_json_includes(json!({ "email": "ada@example.com" }));
}

#[balise::test]
async fn me_requires_sign_in(app: TestApp) {
    app.get("/auth/me").send().await.assert_status(401);
}

#[balise::test]
async fn password_reset_sends_a_link(app: TestApp) {
    let user = User::fake().with_password("old password long").create(app.db()).await;

    app.post("/auth/password-reset")
        .body(RequestPasswordReset { email: user.email.clone() })
        .send()
        .await
        .assert_status(204);

    let mail = app.mails().last_to(&user.email);
    assert!(mail.link("/password-reset").is_some());
}
