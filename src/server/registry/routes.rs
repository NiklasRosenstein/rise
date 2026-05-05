use super::handlers;
use crate::server::state::AppState;
use axum::{routing::get, Router};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/projects/{project_name}/deployments/{deployment_id}/registry-credentials",
        get(handlers::get_deployment_registry_credentials),
    )
}
