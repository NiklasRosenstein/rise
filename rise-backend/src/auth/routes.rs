use axum::{
    routing::{post, get},
    Router,
};
use crate::state::AppState;
use super::{handlers, device};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
        .route("/auth/device/init", post(device::device_init))
        .route("/auth/device/poll", get(device::device_poll))
        .route("/auth/device/authorize", post(device::device_authorize))
        .route("/device-auth", get(device::device_auth_page))
}

/// Public routes that don't require authentication
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/auth/device/init", post(device::device_init))
        .route("/auth/device/poll", get(device::device_poll))
        .route("/auth/device/authorize", post(device::device_authorize))
        .route("/device-auth", get(device::device_auth_page))
}

/// Protected routes that require authentication
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}
