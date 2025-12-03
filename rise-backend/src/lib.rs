pub mod auth;
pub mod settings;
pub mod state;
pub mod db;
pub mod project;
pub mod team;
pub mod registry;
pub mod deployment;

#[cfg(test)]
mod lib_tests;

use axum::{Router, middleware};
use state::AppState;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;
use anyhow::Result;

pub async fn run(settings: settings::Settings) -> Result<()> {
    let state = AppState::new(&settings).await?;

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", axum::routing::get(health_check))
        .merge(auth::routes::public_routes());

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .merge(auth::routes::protected_routes())
        .merge(project::routes::routes())
        .merge(team::routes::team_routes())
        .merge(registry::routes::routes())
        .merge(deployment::routes::deployment_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::auth_middleware,
        ));

    let app = public_routes
        .merge(protected_routes)
        .with_state(state.clone())
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}