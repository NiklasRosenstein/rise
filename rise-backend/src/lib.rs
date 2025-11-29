pub mod auth;
pub mod settings;
pub mod state;

use axum::Router;
use state::AppState;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn run(settings: settings::Settings) {
    let state = AppState::new(&settings).await;

    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .with_state(state.clone())
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}
