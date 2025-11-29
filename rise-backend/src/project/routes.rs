use axum::{routing::post, Router};
use crate::state::AppState;
use super::handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects", post(handlers::create_project))
}