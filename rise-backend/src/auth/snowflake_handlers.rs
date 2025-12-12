//! Snowflake OAuth handlers for Rise platform
//!
//! These handlers manage the OAuth2 flow for Snowflake authentication:
//! - `/snowflake/oauth/start` - Start OAuth flow, redirect to Snowflake
//! - `/snowflake/oauth/callback` - Handle callback from Snowflake
//! - `/snowflake/auth/me` - Get current Snowflake session info
//! - `/snowflake/auth/logout` - Clear Snowflake session

use crate::auth::token_storage::OAuth2State;
use crate::db::{projects, snowflake_sessions};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cookie name for Snowflake session
pub const SNOWFLAKE_SESSION_COOKIE: &str = "_rise_snowflake_session";

/// Query parameters for starting Snowflake OAuth flow
#[derive(Debug, Deserialize)]
pub struct OAuthStartParams {
    /// Project name requesting Snowflake access
    pub project: String,
    /// URL to redirect to after successful authentication
    pub redirect: String,
}

/// Query parameters for OAuth callback
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
    /// Authorization code from Snowflake
    pub code: String,
    /// State parameter for CSRF protection
    pub state: String,
}

/// Start Snowflake OAuth flow
///
/// This endpoint initiates the OAuth2 authorization code flow with Snowflake.
/// The user must already be authenticated with Rise (have a valid Rise session).
pub async fn snowflake_oauth_start(
    State(state): State<AppState>,
    Query(params): Query<OAuthStartParams>,
) -> Response {
    // Check if Snowflake OAuth is configured
    let snowflake_client = match &state.snowflake_oauth_client {
        Some(client) => client,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Snowflake OAuth is not configured",
            )
                .into_response();
        }
    };

    // Verify the project exists and has snowflake_enabled
    let project = match projects::find_by_name(&state.db_pool, &params.project).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Project not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find project: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to find project").into_response();
        }
    };

    if !project.snowflake_enabled {
        return (
            StatusCode::BAD_REQUEST,
            "Snowflake OAuth is not enabled for this project",
        )
            .into_response();
    }

    // Generate state parameter for CSRF protection
    let state_token = Uuid::new_v4().to_string();

    // Store OAuth state using the existing token store
    // We use code_verifier to store a marker, redirect_url for the redirect,
    // and project_name for the project
    let oauth_state = OAuth2State {
        code_verifier: "snowflake".to_string(), // Marker to identify Snowflake OAuth
        redirect_url: Some(params.redirect.clone()),
        project_name: Some(params.project.clone()),
    };

    // Store state in token store
    state.token_store.save(state_token.clone(), oauth_state);

    // Build Snowflake authorization URL
    let authorize_url = snowflake_client.build_authorize_url(&state_token);

    tracing::info!(
        "Starting Snowflake OAuth flow for project '{}', redirecting to Snowflake",
        params.project
    );

    Redirect::temporary(&authorize_url).into_response()
}

/// Handle Snowflake OAuth callback
///
/// This endpoint handles the callback from Snowflake after user authorization.
/// It exchanges the authorization code for tokens and stores them encrypted.
pub async fn snowflake_oauth_callback(
    State(state): State<AppState>,
    Query(params): Query<OAuthCallbackParams>,
) -> Response {
    // Check if Snowflake OAuth is configured
    let snowflake_client = match &state.snowflake_oauth_client {
        Some(client) => client,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Snowflake OAuth is not configured",
            )
                .into_response();
        }
    };

    // Check if encryption is configured (required for token storage)
    let encryption = match &state.encryption_provider {
        Some(enc) => enc,
        None => {
            tracing::error!("Encryption provider not configured - cannot store Snowflake tokens");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Encryption is required for Snowflake OAuth",
            )
                .into_response();
        }
    };

    // Retrieve and validate state
    let oauth_state = match state.token_store.get(&params.state) {
        Some(data) => data,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid or expired state parameter",
            )
                .into_response();
        }
    };

    // Verify this is a Snowflake OAuth state
    if oauth_state.code_verifier != "snowflake" {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid state - not a Snowflake OAuth flow",
        )
            .into_response();
    }

    let project_name = match &oauth_state.project_name {
        Some(name) => name.clone(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing project name in state").into_response();
        }
    };

    let redirect_url = oauth_state
        .redirect_url
        .clone()
        .unwrap_or_else(|| "/".to_string());

    // Exchange code for tokens
    let tokens = match snowflake_client.exchange_code(&params.code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to exchange Snowflake code: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Failed to exchange authorization code with Snowflake",
            )
                .into_response();
        }
    };

    // Encrypt tokens
    let access_token_encrypted = match encryption.encrypt(&tokens.access_token).await {
        Ok(enc) => enc,
        Err(e) => {
            tracing::error!("Failed to encrypt access token: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to encrypt tokens",
            )
                .into_response();
        }
    };

    let refresh_token_encrypted = match encryption.encrypt(&tokens.refresh_token).await {
        Ok(enc) => enc,
        Err(e) => {
            tracing::error!("Failed to encrypt refresh token: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to encrypt tokens",
            )
                .into_response();
        }
    };

    // Create new session (we use a placeholder email for now - should come from Rise auth)
    // TODO: Integrate with Rise authentication to get actual user email
    let user_email = "placeholder@example.com".to_string();
    let session_id = match snowflake_sessions::create_session(&state.db_pool, &user_email).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to create Snowflake session: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create session",
            )
                .into_response();
        }
    };

    // Calculate token expiry
    let expires_at = Utc::now() + Duration::seconds(tokens.expires_in as i64);

    // Store encrypted tokens
    if let Err(e) = snowflake_sessions::upsert_app_token(
        &state.db_pool,
        &session_id,
        &project_name,
        &access_token_encrypted,
        &refresh_token_encrypted,
        expires_at,
    )
    .await
    {
        tracing::error!("Failed to store Snowflake tokens: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store tokens").into_response();
    }

    tracing::info!(
        "Snowflake OAuth successful for project '{}', session: {}",
        project_name,
        &session_id[..8]
    );

    // Build session cookie
    let cookie_value = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax{}{}",
        SNOWFLAKE_SESSION_COOKIE,
        session_id,
        if state.cookie_settings.secure {
            "; Secure"
        } else {
            ""
        },
        if !state.cookie_settings.domain.is_empty() {
            format!("; Domain={}", state.cookie_settings.domain)
        } else {
            String::new()
        }
    );

    // Redirect to original destination with session cookie
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, redirect_url)
        .header(header::SET_COOKIE, cookie_value)
        .body(axum::body::Body::empty())
        .unwrap()
}

/// Session info response
#[derive(Debug, Serialize)]
pub struct SnowflakeSessionInfo {
    pub session_id: String,
    pub user_email: String,
    pub projects: Vec<String>,
}

/// Get current Snowflake session info
pub async fn snowflake_auth_me(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Extract session ID from cookie
    let session_id = match extract_session_id(&headers) {
        Some(id) => id,
        None => {
            return (StatusCode::UNAUTHORIZED, "No Snowflake session").into_response();
        }
    };

    // Get session from database
    let session = match snowflake_sessions::get_session(&state.db_pool, &session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, "Invalid or expired session").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get Snowflake session: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get session").into_response();
        }
    };

    // Get tokens for this session
    let tokens = match snowflake_sessions::list_session_tokens(&state.db_pool, &session_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to list session tokens: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list tokens").into_response();
        }
    };

    let projects: Vec<String> = tokens.into_iter().map(|t| t.project_name).collect();

    let info = SnowflakeSessionInfo {
        session_id: session_id[..8].to_string(), // Only show first 8 chars
        user_email: session.user_email,
        projects,
    };

    (StatusCode::OK, axum::Json(info)).into_response()
}

/// Query parameters for logout
#[derive(Debug, Deserialize)]
pub struct LogoutParams {
    /// Optional: Logout from specific project only
    pub project: Option<String>,
}

/// Logout from Snowflake session
pub async fn snowflake_auth_logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<LogoutParams>,
) -> Response {
    // Extract session ID from cookie
    let session_id = match extract_session_id(&headers) {
        Some(id) => id,
        None => {
            return (StatusCode::OK, "No session to logout from").into_response();
        }
    };

    if let Some(project) = params.project {
        // Logout from specific project only
        if let Err(e) =
            snowflake_sessions::delete_app_token(&state.db_pool, &session_id, &project).await
        {
            tracing::error!("Failed to delete project token: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to logout").into_response();
        }
        tracing::info!(
            "Logged out from Snowflake for project '{}', session: {}",
            project,
            &session_id[..8]
        );
        return (
            StatusCode::OK,
            format!("Logged out from project '{}'", project),
        )
            .into_response();
    }

    // Full logout - delete entire session
    if let Err(e) = snowflake_sessions::delete_session(&state.db_pool, &session_id).await {
        tracing::error!("Failed to delete session: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to logout").into_response();
    }

    tracing::info!("Full Snowflake logout, session: {}", &session_id[..8]);

    // Clear session cookie
    let clear_cookie = format!(
        "{}=; Path=/; HttpOnly; Max-Age=0{}{}",
        SNOWFLAKE_SESSION_COOKIE,
        if state.cookie_settings.secure {
            "; Secure"
        } else {
            ""
        },
        if !state.cookie_settings.domain.is_empty() {
            format!("; Domain={}", state.cookie_settings.domain)
        } else {
            String::new()
        }
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::SET_COOKIE, clear_cookie)
        .body(axum::body::Body::from("Logged out"))
        .unwrap()
}

/// Extract session ID from cookies
fn extract_session_id(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", SNOWFLAKE_SESSION_COOKIE)) {
            return Some(value.to_string());
        }
    }

    None
}
