use crate::state::AppState;
use axum::{
    routing::{get, patch, post},
    Router,
};

pub fn deployment_routes() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(super::handlers::create_deployment))
        .route(
            "/deployments/{deployment_id}/status",
            patch(super::handlers::update_deployment_status),
        )
        .route(
            "/projects/{project_name}/deployments",
            get(super::handlers::list_deployments),
        )
        .route(
            "/projects/{project_name}/deployments/stop",
            post(super::handlers::stop_deployments_by_group),
        )
        .route(
            "/projects/{project_name}/deployments/{deployment_id}",
            get(super::handlers::get_deployment_by_project),
        )
        .route(
            "/projects/{project_name}/deployments/{deployment_id}/rollback",
            post(super::handlers::rollback_deployment),
        )
}
