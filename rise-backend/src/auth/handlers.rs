use axum::{
    Json,
    extract::State,
};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use anyhow::Result;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identity: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PocketbaseAuthResponse {
    pub token: String,
    pub record: PocketbaseUserRecord,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PocketbaseUserRecord {
    pub id: String,
    pub username: String,
    pub email: String,
    pub verified: bool,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, String> {
    let client = state.http_client.as_ref();
    let url = format!("{}/api/collections/users/auth-with-password", state.pocketbase_url);

    let mut map = std::collections::HashMap::new();
    map.insert("identity", payload.identity);
    map.insert("password", payload.password);

    let response = client
        .post(&url)
        .json(&map)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        let pb_auth_response: PocketbaseAuthResponse = response.json().await.map_err(|e| e.to_string())?;
        Ok(Json(LoginResponse { token: pb_auth_response.token }))
    } else {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("PocketBase authentication failed: {}", error_text))
    }
}