use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, put},
    Router,
};

/// Environment variables API routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Project environment variables
        .route(
            "/projects/:project_id_or_name/env/:key",
            put(handlers::set_project_env_var).delete(handlers::delete_project_env_var),
        )
        .route(
            "/projects/:project_id_or_name/env",
            get(handlers::list_project_env_vars),
        )
        // Deployment environment variables (read-only)
        .route(
            "/deployments/:deployment_id/env",
            get(handlers::list_deployment_env_vars),
        )
}
