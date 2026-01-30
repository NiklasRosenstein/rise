use super::handlers;
use crate::server::state::AppState;
use axum::{routing::post, Router};

/// Register encryption-related routes
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/encrypt", post(handlers::encrypt_handler))
}
