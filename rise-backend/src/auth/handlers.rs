use axum::{
    Json,
    extract::State,
    http::{StatusCode, HeaderMap},
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

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub email: String,
}

#[instrument(skip(state, headers))]
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, (StatusCode, String)> {
    // Extract token from Authorization header
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid Authorization header format".to_string()))?;

    // Use the token to get user info from PocketBase
    let pb_client = state.pb_client.as_ref();

    // Set the auth token on the client
    let mut authenticated_client = pb_client.clone();
    authenticated_client.auth_token = Some(token.to_string());

    // Get current user info - PocketBase stores this in the auth store
    // We'll make a request to get the user's record
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);

    let client = reqwest::Client::new();
    let response = client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to verify token: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::UNAUTHORIZED, "Invalid or expired token".to_string()));
    }

    #[derive(Deserialize)]
    struct AuthRefreshResponse {
        record: UserRecord,
    }

    #[derive(Deserialize)]
    struct UserRecord {
        id: String,
        email: String,
    }

    let auth_response: AuthRefreshResponse = response.json().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse response: {}", e)))?;

    Ok(Json(MeResponse {
        id: auth_response.record.id,
        email: auth_response.record.email,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UsersLookupRequest {
    pub emails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct UsersLookupResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

#[instrument(skip(state))]
pub async fn users_lookup(
    State(state): State<AppState>,
    Json(payload): Json<UsersLookupRequest>,
) -> Result<Json<UsersLookupResponse>, (StatusCode, String)> {
    let pb_client = state.pb_client.as_ref();

    // Authenticate with test user (TODO: use proper JWT auth)
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    let mut users = Vec::new();

    for email in payload.emails {
        // Query PocketBase to find user by email
        let filter = format!("email='{}'", email.replace("'", "\\'"));

        #[derive(Deserialize, Default)]
        struct PbUser {
            id: String,
            email: String,
            #[serde(default)]
            created: String,
            #[serde(default)]
            updated: String,
            #[serde(default)]
            collectionId: String,
            #[serde(default)]
            collectionName: String,
        }

        let result = authenticated_client
            .records("users")
            .list()
            .filter(&filter)
            .call::<PbUser>()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to lookup user {}: {}", email, e)))?;

        if let Some(user) = result.items.into_iter().next() {
            users.push(UserInfo {
                id: user.id,
                email: user.email,
            });
        } else {
            return Err((StatusCode::NOT_FOUND, format!("User not found: {}", email)));
        }
    }

    Ok(Json(UsersLookupResponse { users }))
}