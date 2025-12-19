use super::handlers;
use crate::server::state::AppState;
use axum::{routing::get, Router};

/// Create OAuth routes
pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        // OAuth flow endpoints
        .route(
            "/api/v1/projects/{project}/extensions/{extension}/oauth/authorize",
            get(handlers::authorize),
        )
        .route(
            "/api/v1/oauth/callback/{project}/{extension}",
            get(handlers::callback),
        )
        .route(
            "/api/v1/projects/{project}/extensions/{extension}/oauth/exchange",
            get(handlers::exchange_credentials),
        )
}
