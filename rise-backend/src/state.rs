use crate::settings::Settings;
use std::sync::Arc;
use pocketbase_sdk::client::Client as PocketbaseClient;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub http_client: Arc<reqwest::Client>,
    pub pocketbase_url: String,
    pub pb_client: Arc<PocketbaseClient>,
}

impl AppState {
    pub async fn new(settings: &Settings) -> Self {
        let http_client = reqwest::Client::new();
        let pb_client = PocketbaseClient::new(&settings.pocketbase.url);

        Self {
            settings: Arc::new(settings.clone()),
            http_client: Arc::new(http_client),
            pocketbase_url: settings.pocketbase.url.clone(),
            pb_client: Arc::new(pb_client),
        }
    }
}