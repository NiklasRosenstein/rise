use super::handlers;
use crate::server::state::AppState;
use axum::{routing::get, Router};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{project}/environments",
            get(handlers::list_environments).post(handlers::create_environment),
        )
        .route(
            "/projects/{project}/environments/{name}",
            get(handlers::get_environment)
                .patch(handlers::update_environment)
                .delete(handlers::delete_environment),
        )
}
