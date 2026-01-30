use super::handlers;
use crate::server::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Create OAuth routes with short OIDC paths
///
/// All endpoints are now consolidated under `/oidc/{project}/{extension}/...`:
/// - `GET /oidc/{project}/{extension}/authorize` - Initiate OAuth flow
/// - `GET /oidc/{project}/{extension}/callback` - Handle OAuth provider callback
/// - `POST /oidc/{project}/{extension}/token` - Exchange code for tokens (RFC 6749)
/// - `GET /oidc/{project}/{extension}/.well-known/openid-configuration` - OIDC discovery
/// - `GET /oidc/{project}/{extension}/jwks` - JWKS
pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        // OAuth flow endpoints (short paths)
        .route(
            "/oidc/{project}/{extension}/authorize",
            get(handlers::authorize),
        )
        .route(
            "/oidc/{project}/{extension}/callback",
            get(handlers::callback),
        )
        // RFC 6749-compliant token endpoint with CORS support
        .route(
            "/oidc/{project}/{extension}/token",
            post(handlers::token_endpoint).options(handlers::token_endpoint_options),
        )
        // OIDC discovery endpoints (proxied from upstream)
        .route(
            "/oidc/{project}/{extension}/.well-known/openid-configuration",
            get(handlers::oidc_discovery),
        )
        .route("/oidc/{project}/{extension}/jwks", get(handlers::oidc_jwks))
}
