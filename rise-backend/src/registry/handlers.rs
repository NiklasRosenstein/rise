use axum::{
    Json,
    extract::{State, Query},
    http::StatusCode,
};

use crate::state::AppState;
use super::models::{GetRegistryCredsRequest, GetRegistryCredsResponse};

/// Get registry credentials for a project
pub async fn get_registry_credentials(
    State(state): State<AppState>,
    Query(params): Query<GetRegistryCredsRequest>,
    headers: axum::http::HeaderMap,
) -> Result<Json<GetRegistryCredsResponse>, (StatusCode, String)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: No authentication token provided".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid Authorization header format".to_string()))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to verify token: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid or expired token".to_string()));
    }

    // Check if registry is configured
    let registry_provider = state.registry_provider.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "No registry configured".to_string()))?;

    // Get credentials from the registry provider
    // The repository name is typically the project name
    let repository = params.project.clone();

    let credentials = registry_provider
        .get_credentials(&repository)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get registry credentials: {}", e)
        ))?;

    Ok(Json(GetRegistryCredsResponse {
        credentials,
        repository,
    }))
}
