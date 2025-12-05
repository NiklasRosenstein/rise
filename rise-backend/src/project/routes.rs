use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects", get(handlers::list_projects))
        .route("/projects", post(handlers::create_project))
        .route("/projects/{id_or_name}", get(handlers::get_project))
        .route("/projects/{id_or_name}", put(handlers::update_project))
        .route("/projects/{id_or_name}", delete(handlers::delete_project))
}
