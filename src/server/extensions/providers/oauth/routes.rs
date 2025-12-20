use super::handlers;
use crate::server::state::AppState;
use axum::{routing::get, Router};

/// Create OAuth routes
pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        // OAuth flow endpoints
        .route(
            "/projects/{project}/extensions/{extension}/oauth/authorize",
            get(handlers::authorize),
        )
        .route(
            "/oauth/callback/{project}/{extension}",
            get(handlers::callback),
        )
        .route(
            "/projects/{project}/extensions/{extension}/oauth/exchange",
            get(handlers::exchange_credentials),
        )
}
