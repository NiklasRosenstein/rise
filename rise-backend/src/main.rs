use rise_backend::settings::Settings;
use rise_backend::run;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
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

    run(settings).await;
}