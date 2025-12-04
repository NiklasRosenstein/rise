use axum::{Router, routing::{post, patch, get}};
use crate::state::AppState;

pub fn deployment_routes() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(super::handlers::create_deployment))
        .route("/deployments/{deployment_id}/status", patch(super::handlers::update_deployment_status))
        .route("/projects/{project_name}/deployments", get(super::handlers::list_deployments))
        .route("/projects/{project_name}/deployments/{deployment_id}", get(super::handlers::get_deployment_by_project))
}
