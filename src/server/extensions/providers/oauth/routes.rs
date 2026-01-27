use super::handlers;
use crate::server::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

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
        // RFC 6749-compliant token endpoint
        .route(
            "/projects/{project}/extensions/{extension}/oauth/token",
            post(handlers::token_endpoint),
        )
        // Legacy exchange endpoint (deprecated, kept for backwards compatibility)
        .route(
            "/projects/{project}/extensions/{extension}/oauth/exchange",
            get(handlers::exchange_credentials),
        )
}
