use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/code/exchange", post(handlers::code_exchange))
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}

/// Public routes that don't require authentication
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/authorize", post(handlers::authorize))
        .route("/auth/code/exchange", post(handlers::code_exchange))
        .route("/auth/signin", get(handlers::oauth_signin))
        .route("/auth/callback", get(handlers::oauth_callback))
        .route("/auth/ingress", get(handlers::ingress_auth))
        .route("/auth/logout", get(handlers::oauth_logout))
}

/// Protected routes that require authentication
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}
