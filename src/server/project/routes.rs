use super::handlers;
use crate::server::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/access-classes",
            get(handlers::list_access_classes),
        )
        .route("/projects", get(handlers::list_projects))
        .route("/projects", post(handlers::create_project))
        .route("/projects/{id_or_name}", get(handlers::get_project))
        .route("/projects/{id_or_name}", put(handlers::update_project))
        .route("/projects/{id_or_name}", delete(handlers::delete_project))
        .route(
            "/projects/{id_or_name}/app-users",
            get(handlers::list_app_users),
        )
        .route(
            "/projects/{id_or_name}/app-users",
            post(handlers::add_app_user),
        )
        .route(
            "/projects/{id_or_name}/app-users/{identifier_type}/{identifier_value}",
            delete(handlers::remove_app_user),
        )
}
