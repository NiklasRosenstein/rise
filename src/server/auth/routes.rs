use super::handlers;
use crate::server::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Public routes that don't require authentication
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/authorize", post(handlers::authorize))
        .route("/auth/code/exchange", post(handlers::code_exchange))
        .route("/auth/device/exchange", post(handlers::device_exchange))
        .route("/auth/signin", get(handlers::signin_page))
        .route("/auth/signin/start", get(handlers::oauth_signin_start))
        .route("/auth/callback", get(handlers::oauth_callback))
        .route("/auth/ingress", get(handlers::ingress_auth))
        .route("/auth/logout", get(handlers::oauth_logout))
}

/// Routes for `/.rise/auth/*` path (for custom domain support via Ingress routing)
///
/// These routes are mounted at the root level (not under `/api/v1`) to allow
/// custom domains to route auth requests through their Ingress to the Rise backend.
/// This enables cookie-based authentication for custom domains where cookie sharing
/// with the Rise backend domain is not possible due to browser security restrictions.
///
/// Flow:
/// 1. User visits custom domain → signin page at /.rise/auth/signin
/// 2. Signin start redirects to IdP with callback URL on main Rise domain
/// 3. After IdP callback on main domain, redirect to /.rise/auth/complete#token=xxx
/// 4. Landing page (GET) extracts token from fragment and POSTs it securely
/// 5. Complete handler (POST) sets cookie on custom domain and returns success page
pub fn rise_auth_routes() -> Router<AppState> {
    Router::new()
        .route("/.rise/auth/signin", get(handlers::signin_page))
        .route(
            "/.rise/auth/signin/start",
            get(handlers::oauth_signin_start),
        )
        // GET serves landing page that extracts token from fragment
        // POST receives token in body and completes auth flow
        .route(
            "/.rise/auth/complete",
            get(handlers::oauth_complete_landing).post(handlers::oauth_complete),
        )
}

/// Protected routes that require authentication
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}
