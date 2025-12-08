use crate::auth::{
    cookie_helpers,
    token_storage::{
        generate_code_challenge, generate_code_verifier, generate_state_token, OAuth2State,
    },
};
use crate::db::{
    models::{ProjectVisibility, User},
    projects, users,
};
use crate::state::AppState;
use axum::{
    extract::{Extension, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub struct CodeExchangeRequest {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceExchangeRequest {
    pub device_code: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceExchangeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeRequest {
    /// For authorization code flow: the redirect URI
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// For authorization code flow: the PKCE code challenge
    #[serde(default)]
    pub code_challenge: Option<String>,
    /// For authorization code flow: the PKCE code challenge method
    #[serde(default)]
    pub code_challenge_method: Option<String>,
    /// Flow type: "code" for authorization code flow, "device" for device flow
    pub flow: String,
}

#[derive(Debug, Serialize)]
pub struct AuthorizeResponse {
    /// For authorization code flow: the full authorization URL to open in browser
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    /// For device flow: the device code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code: Option<String>,
    /// For device flow: the user code to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    /// For device flow: the verification URI to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri: Option<String>,
    /// For device flow: the complete verification URI (with user code embedded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    /// For device flow: how long the device code is valid (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// For device flow: how often to poll (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,
}

/// Build OAuth2 authorization URL or initiate device flow (for CLI)
#[instrument(skip(state))]
pub async fn authorize(
    State(state): State<AppState>,
    Json(payload): Json<AuthorizeRequest>,
) -> Result<Json<AuthorizeResponse>, (StatusCode, String)> {
    match payload.flow.as_str() {
        "code" => {
            // Authorization code flow with PKCE
            let redirect_uri = payload.redirect_uri.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "redirect_uri required for code flow".to_string(),
                )
            })?;
            let code_challenge = payload.code_challenge.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "code_challenge required for code flow".to_string(),
                )
            })?;
            let code_challenge_method = payload.code_challenge_method.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "code_challenge_method required for code flow".to_string(),
                )
            })?;

            // Build query parameters
            let params = vec![
                ("client_id", state.auth_settings.client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", "openid email profile offline_access"),
                ("code_challenge", code_challenge.as_str()),
                ("code_challenge_method", code_challenge_method.as_str()),
            ];

            let authorization_url = state.oauth_client.build_authorize_url(&params);

            Ok(Json(AuthorizeResponse {
                authorization_url: Some(authorization_url),
                device_code: None,
                user_code: None,
                verification_uri: None,
                verification_uri_complete: None,
                expires_in: None,
                interval: None,
            }))
        }
        "device" => {
            // Device authorization flow
            let device_response = state.oauth_client.device_flow_start().await.map_err(|e| {
                tracing::error!("Failed to start device flow: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to start device flow: {}", e),
                )
            })?;

            Ok(Json(AuthorizeResponse {
                authorization_url: None,
                device_code: Some(device_response.device_code),
                user_code: Some(device_response.user_code),
                verification_uri: Some(device_response.verification_uri.clone()),
                verification_uri_complete: Some(device_response.verification_uri_complete),
                expires_in: Some(device_response.expires_in),
                interval: Some(device_response.interval),
            }))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid flow type: {}", payload.flow),
        )),
    }
}

/// Exchange authorization code for token (OAuth2 PKCE flow)
#[instrument(skip(state, payload))]
pub async fn code_exchange(
    State(state): State<AppState>,
    Json(payload): Json<CodeExchangeRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    tracing::debug!(
        "Code exchange request: redirect_uri={}",
        payload.redirect_uri
    );

    // Exchange authorization code for tokens using PKCE
    let token_info = state
        .oauth_client
        .exchange_code_pkce(&payload.code, &payload.code_verifier, &payload.redirect_uri)
        .await
        .map_err(|e| {
            tracing::warn!("OAuth2 code exchange failed: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                format!("Code exchange failed: {}", e),
            )
        })?;

    tracing::info!(
        "Code exchange successful, token_type={}, expires_in={}",
        token_info.token_type,
        token_info.expires_in
    );

    // Decode and log token claims for debugging (without validating yet)
    if let Ok(header) = jsonwebtoken::decode_header(&token_info.id_token) {
        tracing::debug!("ID token header: {:?}", header);
    }

    // Try to decode payload for logging (this doesn't validate signature)
    let parts: Vec<&str> = token_info.id_token.split('.').collect();
    if parts.len() == 3 {
        if let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
            if let Ok(claims_str) = String::from_utf8(decoded) {
                tracing::debug!("ID token claims: {}", claims_str);
            }
        }
    }

    // Return the ID token (which contains user claims)
    Ok(Json(LoginResponse {
        token: token_info.id_token,
    }))
}

/// Exchange device code for token (Device Flow)
#[instrument(skip(state, payload))]
pub async fn device_exchange(
    State(state): State<AppState>,
    Json(payload): Json<DeviceExchangeRequest>,
) -> Json<DeviceExchangeResponse> {
    tracing::debug!(
        "Device exchange request: device_code={}...",
        &payload.device_code[..8.min(payload.device_code.len())]
    );

    // Poll the identity provider's token endpoint with the device code
    match state
        .oauth_client
        .device_flow_poll(&payload.device_code)
        .await
    {
        Ok(Some(token_info)) => {
            tracing::info!("Device authorization successful");
            Json(DeviceExchangeResponse {
                token: Some(token_info.id_token),
                error: None,
                error_description: None,
            })
        }
        Ok(None) => {
            // authorization_pending - user hasn't authorized yet
            tracing::debug!("Device authorization pending");
            Json(DeviceExchangeResponse {
                token: None,
                error: Some("authorization_pending".to_string()),
                error_description: None,
            })
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::warn!("Device authorization error: {}", error_msg);

            // Check for standard OAuth2 device flow errors
            let (error, description) = if error_msg.contains("slow_down") {
                ("slow_down".to_string(), None)
            } else {
                ("access_denied".to_string(), Some(error_msg))
            };

            Json(DeviceExchangeResponse {
                token: None,
                error: Some(error),
                error_description: description,
            })
        }
    }
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
    tracing::debug!("GET /me: user_id={}, email={}", user.id, user.email);
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
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
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

// ============================================================================
// OAuth2 Proxy Handlers for Ingress Authentication
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SigninQuery {
    /// Optional redirect URL to return to after authentication (path only)
    pub redirect: Option<String>,
    /// Optional full redirect URL from Nginx ingress (includes host)
    pub rd: Option<String>,
}

/// Initiate OAuth2 login flow for ingress auth
///
/// This handler starts the OAuth2 authorization code flow with PKCE.
/// It generates a PKCE verifier/challenge pair, stores the state, and
/// redirects the user to the OIDC provider for authentication.
#[instrument(skip(state))]
pub async fn oauth_signin(
    State(state): State<AppState>,
    Query(params): Query<SigninQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    // Prefer rd (full URL) over redirect (path only)
    let redirect_url = params.rd.or(params.redirect);
    tracing::info!("OAuth signin initiated, redirect={:?}", redirect_url);

    // Generate PKCE parameters
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state_token = generate_state_token();

    // Store PKCE state with redirect URL for later retrieval
    let oauth_state = OAuth2State {
        code_verifier: code_verifier.clone(),
        redirect_url,
    };
    state.token_store.save(state_token.clone(), oauth_state);

    // Build OAuth2 authorization URL
    let callback_url = format!("{}/auth/callback", state.public_url.trim_end_matches('/'));

    let params = vec![
        ("client_id", state.auth_settings.client_id.as_str()),
        ("redirect_uri", callback_url.as_str()),
        ("response_type", "code"),
        ("scope", "openid email profile"),
        ("code_challenge", code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state_token.as_str()),
    ];

    let auth_url = state.oauth_client.build_authorize_url(&params);

    tracing::debug!("Redirecting to OIDC provider for authentication");
    Ok(Redirect::to(&auth_url))
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// OAuth2 callback from OIDC provider
///
/// This handler receives the authorization code from the OIDC provider, exchanges it for tokens,
/// sets a session cookie, and redirects the user back to their original URL.
#[instrument(skip(state))]
pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackQuery>,
) -> Result<Response, (StatusCode, String)> {
    tracing::info!("OAuth callback received");

    // Retrieve PKCE state from token store
    let oauth_state = state.token_store.get(&params.state).ok_or_else(|| {
        tracing::warn!("Invalid or expired state token");
        (
            StatusCode::BAD_REQUEST,
            "Invalid or expired state token".to_string(),
        )
    })?;

    // Build callback URL (must match the one used in signin)
    let callback_url = format!("{}/auth/callback", state.public_url.trim_end_matches('/'));

    // Exchange authorization code for tokens
    let token_info = state
        .oauth_client
        .exchange_code_pkce(&params.code, &oauth_state.code_verifier, &callback_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to exchange code: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                format!("Code exchange failed: {}", e),
            )
        })?;

    tracing::info!("Successfully exchanged code for tokens");

    // Validate the JWT to extract expiry time
    let mut expected_claims = HashMap::new();
    expected_claims.insert("aud".to_string(), state.auth_settings.client_id.clone());

    let claims = state
        .jwt_validator
        .validate(
            &token_info.id_token,
            &state.auth_settings.issuer,
            &expected_claims,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate JWT: {}", e);
            (StatusCode::UNAUTHORIZED, "Invalid token".to_string())
        })?;

    // Calculate cookie max age from JWT expiry
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let exp = claims["exp"].as_u64().unwrap_or(now + 3600);
    let max_age = if exp > now {
        exp - now
    } else {
        3600 // Default to 1 hour if exp is in the past
    };

    // Create session cookie with JWT
    let cookie = cookie_helpers::create_session_cookie(
        &token_info.id_token,
        &state.cookie_settings,
        max_age,
    );

    // Determine redirect URL
    let redirect_url = oauth_state.redirect_url.unwrap_or_else(|| "/".to_string());

    tracing::info!("Setting session cookie and redirecting to {}", redirect_url);

    // Build response with Set-Cookie header and redirect
    let response = (
        StatusCode::FOUND,
        [("Location", redirect_url.as_str()), ("Set-Cookie", &cookie)],
    )
        .into_response();

    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct IngressAuthQuery {
    pub project: String,
}

/// Nginx ingress auth endpoint
///
/// This handler is called by Nginx for every request to a private project.
/// It validates the session cookie, checks JWT validity, and verifies
/// project access authorization.
#[instrument(skip(state))]
pub async fn ingress_auth(
    State(state): State<AppState>,
    Query(params): Query<IngressAuthQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    tracing::debug!("Ingress auth check for project: {}", params.project);

    // Extract session cookie
    let session_token = cookie_helpers::extract_session_cookie(&headers).ok_or_else(|| {
        tracing::debug!("No session cookie found");
        (StatusCode::UNAUTHORIZED, "No session cookie".to_string())
    })?;

    // Validate JWT
    let mut expected_claims = HashMap::new();
    expected_claims.insert("aud".to_string(), state.auth_settings.client_id.clone());

    let claims = state
        .jwt_validator
        .validate(
            &session_token,
            &state.auth_settings.issuer,
            &expected_claims,
        )
        .await
        .map_err(|e| {
            tracing::warn!("Invalid or expired JWT: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired session".to_string(),
            )
        })?;

    // Extract email from claims
    let email = claims["email"].as_str().ok_or_else(|| {
        tracing::error!("JWT missing email claim");
        (
            StatusCode::UNAUTHORIZED,
            "Invalid token: missing email".to_string(),
        )
    })?;

    // Find or create user in database
    let user = users::find_or_create(&state.db_pool, email)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding/creating user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    // Find project by name
    let project = projects::find_by_name(&state.db_pool, &params.project)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding project: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?
        .ok_or_else(|| {
            tracing::debug!("Project not found: {}", params.project);
            (StatusCode::NOT_FOUND, "Project not found".to_string())
        })?;

    // Check if project is public - if so, allow access without further checks
    if matches!(project.visibility, ProjectVisibility::Public) {
        tracing::debug!("Project is public, allowing access");
        return Ok((
            StatusCode::OK,
            [
                ("X-Auth-Request-Email", email),
                ("X-Auth-Request-User", user.id.to_string().as_str()),
            ],
        )
            .into_response());
    }

    // For private projects, check access permissions
    let has_access = projects::user_can_access(&state.db_pool, user.id, project.id)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    if has_access {
        tracing::debug!("User has access to private project");
        Ok((
            StatusCode::OK,
            [
                ("X-Auth-Request-Email", email),
                ("X-Auth-Request-User", user.id.to_string().as_str()),
            ],
        )
            .into_response())
    } else {
        tracing::warn!(
            "User {} denied access to private project {}",
            user.email,
            params.project
        );
        Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub struct LogoutQuery {
    /// Optional redirect URL after logout
    pub redirect: Option<String>,
}

/// Logout endpoint
///
/// Clears the session cookie and redirects the user.
#[instrument(skip(state))]
pub async fn oauth_logout(
    State(state): State<AppState>,
    Query(params): Query<LogoutQuery>,
) -> Result<Response, (StatusCode, String)> {
    tracing::info!("Logout initiated");

    // Clear the session cookie
    let cookie = cookie_helpers::clear_session_cookie(&state.cookie_settings);

    // Determine redirect URL
    let redirect_url = params.redirect.unwrap_or_else(|| "/".to_string());

    tracing::info!(
        "Clearing session cookie and redirecting to {}",
        redirect_url
    );

    // Build response with Set-Cookie header and redirect
    let response = (
        StatusCode::FOUND,
        [("Location", redirect_url.as_str()), ("Set-Cookie", &cookie)],
    )
        .into_response();

    Ok(response)
}
