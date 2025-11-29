pub mod auth;
pub mod settings;
pub mod state;
pub mod project;
pub mod team;

#[cfg(test)]
mod lib_tests;

use axum::Router;
use state::AppState;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;
use anyhow::Result;

pub async fn run(settings: settings::Settings) -> Result<()> {
    let state = AppState::new(&settings).await;

    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .merge(auth::routes::routes())
        .merge(project::routes::routes())
        .merge(team::routes::team_routes())
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