use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;
use crate::workload_identity::handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project_name}/workload-identities",
            post(handlers::create_workload_identity).get(handlers::list_workload_identities),
        )
        .route(
            "/projects/{project_name}/workload-identities/{sa_id}",
            get(handlers::get_workload_identity).delete(handlers::delete_workload_identity),
        )
}
