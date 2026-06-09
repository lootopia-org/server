use crate::{
    auth::handlers::{
        credentials, forgot_password, login, logout, me, mfa_totp, register, resend,
        reset_password, totp_disable, totp_enroll_begin, totp_enroll_verify, verify_email,
        webauthn_login_begin, webauthn_login_complete, webauthn_register_begin,
        webauthn_register_complete,
    },
    AppState,
};
use axum::{
    routing::{get, post},
    Router,
};

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/verify-email", get(verify_email))
        .route("/resend-verification", post(resend))
        .route("/login", post(login))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/mfa/totp", post(mfa_totp))
        .route("/webauthn/login/begin", post(webauthn_login_begin))
        .route("/webauthn/login/complete", post(webauthn_login_complete))
}

pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route("/totp/enroll/begin", post(totp_enroll_begin))
        .route("/totp/enroll/verify", post(totp_enroll_verify))
        .route("/totp/disable", post(totp_disable))
        .route("/webauthn/register/begin", post(webauthn_register_begin))
        .route(
            "/webauthn/register/complete",
            post(webauthn_register_complete),
        )
        .route("/webauthn/credentials", get(credentials))
}
