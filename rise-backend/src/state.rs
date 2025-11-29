use crate::settings::Settings;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub http_client: Arc<reqwest::Client>,
    pub pocketbase_url: String,
}

impl AppState {
    pub async fn new(settings: &Settings) -> Self {
        let http_client = reqwest::Client::new();

        Self {
            settings: Arc::new(settings.clone()),
            http_client: Arc::new(http_client),
            pocketbase_url: settings.pocketbase.url.clone(),
        }
    }
}