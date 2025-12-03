use axum::{Router, routing::{post, patch}};
use crate::state::AppState;

pub fn deployment_routes() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(super::handlers::create_deployment))
        .route("/deployments/{deployment_id}/status", patch(super::handlers::update_deployment_status))
}
