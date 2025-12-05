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
    Router::new().route("/auth/code/exchange", post(handlers::code_exchange))
}

/// Protected routes that require authentication
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}
