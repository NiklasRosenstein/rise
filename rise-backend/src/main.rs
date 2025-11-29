mod settings;

use axum::{
    routing::get,
    Router,
};
use settings::Settings;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Starting server on {}:{}", settings.server.host, settings.server.port);

    // build our application with a single route
    let app = Router::new().route("/", get(|| async { "Hello, Rise!" }));

    // run it with hyper on localhost:3000
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}