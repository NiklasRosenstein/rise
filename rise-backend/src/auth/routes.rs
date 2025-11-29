use axum::{
    routing::{post, get},
    Router,
};
use crate::state::AppState;
use super::{handlers, device};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/auth/device/init", post(device::device_init))
        .route("/auth/device/poll", get(device::device_poll))
        .route("/auth/device/authorize", post(device::device_authorize))
        .route("/device-auth", get(device::device_auth_page))
}
