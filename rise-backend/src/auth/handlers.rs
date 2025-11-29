use axum::{
    Json,
    extract::State,
};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identity: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[instrument(skip(state))]
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, String> {
    let pb_client = state.pb_client.as_ref();

    let authenticated_client = pb_client
        .auth_with_password(
            "users", // The name of your user collection
            &payload.identity,
            &payload.password,
        )
        .map_err(|e| format!("PocketBase authentication failed: {}", e.to_string()))?;

    let token = authenticated_client.auth_token.ok_or("Failed to get token from authenticated client".to_string())?;
    let response = LoginResponse { token };

    tracing::info!(?response, "Login successful");

    Ok(Json(response))
}