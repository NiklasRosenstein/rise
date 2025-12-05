pub mod auth;
pub mod db;
pub mod deployment;
pub mod oci;
pub mod project;
pub mod registry;
pub mod settings;
pub mod state;
pub mod team;

#[cfg(test)]
mod lib_tests;

use anyhow::Result;
use axum::{middleware, Router};
use state::AppState;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn run(settings: settings::Settings) -> Result<()> {
    let state = AppState::new(&settings).await?;

    // Start deployment controller
    let controller = Arc::new(deployment::controller::DeploymentController::new(
        Arc::new(state.clone()),
    )?);
    controller.start();
    info!("Deployment controller started");

    // Start project controller
    let project_controller = Arc::new(project::ProjectController::new(Arc::new(state.clone())));
    project_controller.start();
    info!("Project controller started");

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
