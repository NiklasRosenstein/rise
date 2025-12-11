use super::{handlers, snowflake_handlers};
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/code/exchange", post(handlers::code_exchange))
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}

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
        // Snowflake OAuth routes
        .route(
            "/.rise/snowflake/oauth/start",
            get(snowflake_handlers::snowflake_oauth_start),
        )
        .route(
            "/.rise/snowflake/oauth/callback",
            get(snowflake_handlers::snowflake_oauth_callback),
        )
        .route(
            "/.rise/snowflake/auth/me",
            get(snowflake_handlers::snowflake_auth_me),
        )
        .route(
            "/.rise/snowflake/auth/logout",
            post(snowflake_handlers::snowflake_auth_logout),
        )
}

/// Protected routes that require authentication
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::me))
        .route("/users/lookup", post(handlers::users_lookup))
}
