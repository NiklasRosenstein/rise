use axum::{
    Json,
    extract::{State, Extension},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::db::{models::User, users};
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub struct CodeExchangeRequest {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

/// Exchange authorization code for token (OAuth2 PKCE flow)
#[instrument(skip(state, payload))]
pub async fn code_exchange(
    State(state): State<AppState>,
    Json(payload): Json<CodeExchangeRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    // Exchange authorization code for tokens using PKCE
    let token_info = state
        .oauth_client
        .exchange_code_pkce(
            &payload.code,
            &payload.code_verifier,
            &payload.redirect_uri,
        )
        .await
        .map_err(|e| {
            tracing::warn!("OAuth2 code exchange failed: {}", e);
            (StatusCode::UNAUTHORIZED, format!("Code exchange failed: {}", e))
        })?;

    tracing::info!("Code exchange successful");

    // Return the ID token (which contains user claims)
    Ok(Json(LoginResponse {
        token: token_info.id_token,
    }))
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub email: String,
}

/// Get current user info from auth middleware
#[instrument(skip(_state))]
pub async fn me(
    State(_state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Json<MeResponse>, (StatusCode, String)> {
    // User is injected by auth middleware
    Ok(Json(MeResponse {
        id: user.id.to_string(),
        email: user.email,
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

/// Lookup users by email addresses
#[instrument(skip(state))]
pub async fn users_lookup(
    State(state): State<AppState>,
    Extension(_user): Extension<User>,
    Json(payload): Json<UsersLookupRequest>,
) -> Result<Json<UsersLookupResponse>, (StatusCode, String)> {
    let mut user_infos = Vec::new();

    for email in payload.emails {
        // Query database for user by email
        let user = users::find_by_email(&state.db_pool, &email)
            .await
            .map_err(|e| {
                tracing::error!("Database error looking up user {}: {}", email, e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            })?;

        match user {
            Some(u) => {
                user_infos.push(UserInfo {
                    id: u.id.to_string(),
                    email: u.email,
                });
            }
            None => {
                return Err((StatusCode::NOT_FOUND, format!("User not found: {}", email)));
            }
        }
    }

    Ok(Json(UsersLookupResponse { users: user_infos }))
}
