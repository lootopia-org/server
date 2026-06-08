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
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enroll/begin", post(totp_enroll_begin))
        .route("/auth/totp/enroll/verify", post(totp_enroll_verify))
        .route("/auth/totp/disable", post(totp_disable))
        .route(
            "/auth/webauthn/register/begin",
            post(webauthn_register_begin),
        )
        .route(
            "/auth/webauthn/register/complete",
            post(webauthn_register_complete),
        )
        .route("/auth/webauthn/credentials", get(credentials))
}
