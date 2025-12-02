use axum::{routing::get, Router};
use crate::state::AppState;
use super::handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/registry/credentials", get(handlers::get_registry_credentials))
}
