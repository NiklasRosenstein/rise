use super::handlers;
use crate::server::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub fn domain_routes() -> Router<AppState> {
    Router::new()
        // Domain management
        .route(
            "/projects/{project_id_or_name}/domains",
            post(handlers::add_domain),
        )
        .route(
            "/projects/{project_id_or_name}/domains",
            get(handlers::list_domains),
        )
        .route(
            "/projects/{project_id_or_name}/domains/{domain_name}",
            delete(handlers::delete_domain),
        )
        .route(
            "/projects/{project_id_or_name}/domains/{domain_name}/verify",
            post(handlers::verify_domain),
        )
        .route(
            "/projects/{project_id_or_name}/domains/{domain_name}/challenges",
            get(handlers::get_challenges),
        )
        .route(
            "/projects/{project_id_or_name}/domains/{domain_name}/certificate",
            post(handlers::request_certificate),
        )
}
