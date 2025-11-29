use crate::settings::Settings;
use pocketbase_sdk::client::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub pb: Arc<Client>,
}

impl AppState {
    pub async fn new(settings: &Settings) -> Self {
        let pb_client = Client::new(&settings.pocketbase.url);
        // Here you would typically login as an admin or authenticated user
        // For now, we'll just create the client

        Self {
            settings: Arc<new(settings.clone()),
            pb: Arc<new(pb_client),
        }
    }
}
