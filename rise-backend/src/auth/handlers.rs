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
use crate::frontend::StaticAssets;
use crate::state::AppState;
use axum::{
    extract::{Extension, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::Tera;
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

            // Build authorization URL with typed parameters
            let params = crate::auth::oauth::AuthorizeParams {
                client_id: &state.auth_settings.client_id,
                redirect_uri: &redirect_uri,
                response_type: "code",
                scope: "openid email profile offline_access",
                code_challenge: &code_challenge,
                code_challenge_method: &code_challenge_method,
                state: None,
            };

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
    /// Optional project name for ingress authentication flow
    pub project: Option<String>,
}

/// Pre-authentication page for ingress auth
///
/// Shows the user which project they're about to authenticate for before
/// starting the OAuth flow. This provides better UX by explaining what's happening.
#[instrument(skip(state, params))]
pub async fn signin_page(
    State(state): State<AppState>,
    Query(params): Query<SigninQuery>,
) -> Result<Response, (StatusCode, String)> {
    let project_name = params.project.as_deref().unwrap_or("Unknown");
    let redirect_url = params
        .redirect
        .or(params.rd)
        .unwrap_or_else(|| "/".to_string());

    tracing::info!(
        project = %project_name,
        has_redirect = !redirect_url.is_empty(),
        "Signin page requested"
    );

    // Load template from embedded assets
    let template_content = StaticAssets::get("auth-signin.html.tera")
        .ok_or_else(|| {
            tracing::error!("auth-signin.html.tera template not found");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template not found".to_string(),
            )
        })?
        .data;

    let template_str = std::str::from_utf8(&template_content).map_err(|e| {
        tracing::error!("Failed to parse template as UTF-8: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Template encoding error".to_string(),
        )
    })?;

    // Create Tera instance and add template
    let mut tera = Tera::default();
    tera.add_raw_template("auth-signin.html.tera", template_str)
        .map_err(|e| {
            tracing::error!("Failed to parse template: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template error".to_string(),
            )
        })?;

    // Build continue URL (to oauth_signin_start)
    let mut continue_params = vec![];
    if let Some(ref project) = params.project {
        continue_params.push(format!("project={}", urlencoding::encode(project)));
    }
    if !redirect_url.is_empty() {
        continue_params.push(format!("redirect={}", urlencoding::encode(&redirect_url)));
    }
    let continue_url = format!(
        "{}/auth/signin/start?{}",
        state.public_url.trim_end_matches('/'),
        continue_params.join("&")
    );

    // Render template
    let mut context = tera::Context::new();
    context.insert("project_name", project_name);
    context.insert("continue_url", &continue_url);
    context.insert("redirect_url", &redirect_url);

    let html = tera
        .render("auth-signin.html.tera", &context)
        .map_err(|e| {
            tracing::error!("Failed to render template: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template rendering error".to_string(),
            )
        })?;

    Ok(Html(html).into_response())
}

/// Initiate OAuth2 login flow for ingress auth (start of OAuth flow)
///
/// This handler starts the OAuth2 authorization code flow with PKCE.
/// It generates a PKCE verifier/challenge pair, stores the state, and
/// redirects the user to the OIDC provider for authentication.
#[instrument(skip(state, params))]
pub async fn oauth_signin_start(
    State(state): State<AppState>,
    Query(params): Query<SigninQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    // Prefer rd (full URL) over redirect (path only)
    let redirect_url = params.rd.or(params.redirect);
    tracing::info!(
        project = ?params.project,
        has_redirect = redirect_url.is_some(),
        "OAuth signin initiated"
    );

    // Generate PKCE parameters
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state_token = generate_state_token();

    // Store PKCE state with redirect URL and project name for later retrieval
    let oauth_state = OAuth2State {
        code_verifier: code_verifier.clone(),
        redirect_url,
        project_name: params.project.clone(), // For ingress auth flow
    };
    state.token_store.save(state_token.clone(), oauth_state);

    // Build OAuth2 authorization URL
    let callback_url = format!("{}/auth/callback", state.public_url.trim_end_matches('/'));

    let params = crate::auth::oauth::AuthorizeParams {
        client_id: &state.auth_settings.client_id,
        redirect_uri: &callback_url,
        response_type: "code",
        scope: "openid email profile",
        code_challenge: &code_challenge,
        code_challenge_method: "S256",
        state: Some(&state_token),
    };

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
#[instrument(skip(state, params))]
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

    // Determine redirect URL
    let redirect_url = oauth_state.redirect_url.unwrap_or_else(|| "/".to_string());

    // Check if this is an ingress auth flow (has project context)
    let (cookie, is_ingress_auth) = if let Some(ref project) = oauth_state.project_name {
        tracing::info!(
            "Issuing Rise JWT for ingress auth (project context: {})",
            project
        );

        // Issue Rise JWT (NOT project-scoped - the cookie is shared across all *.rise.dev subdomains)
        let rise_jwt = state
            .jwt_signer
            .sign_ingress_jwt(&claims, Some(exp))
            .map_err(|e| {
                tracing::error!("Failed to sign Rise JWT: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to create authentication token".to_string(),
                )
            })?;

        let cookie =
            cookie_helpers::create_ingress_jwt_cookie(&rise_jwt, &state.cookie_settings, max_age);

        (cookie, true)
    } else {
        tracing::info!("Using IdP token for session");

        // Regular OAuth flow (not ingress auth)
        let cookie = cookie_helpers::create_session_cookie(
            &token_info.id_token,
            &state.cookie_settings,
            max_age,
        );

        (cookie, false)
    };

    // For ingress auth flow, render success page with auto-redirect
    if is_ingress_auth {
        let project_name = oauth_state
            .project_name
            .unwrap_or_else(|| "Unknown".to_string());

        // Load success template
        let template_content = StaticAssets::get("auth-success.html.tera")
            .ok_or_else(|| {
                tracing::error!("auth-success.html.tera template not found");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Template not found".to_string(),
                )
            })?
            .data;

        let template_str = std::str::from_utf8(&template_content).map_err(|e| {
            tracing::error!("Failed to parse template as UTF-8: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template encoding error".to_string(),
            )
        })?;

        // Create Tera instance and add template
        let mut tera = Tera::default();
        tera.add_raw_template("auth-success.html.tera", template_str)
            .map_err(|e| {
                tracing::error!("Failed to parse template: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Template error".to_string(),
                )
            })?;

        // Render success template
        let mut context = tera::Context::new();
        context.insert("success", &true);
        context.insert("project_name", &project_name);
        context.insert("redirect_url", &redirect_url);

        let html = tera
            .render("auth-success.html.tera", &context)
            .map_err(|e| {
                tracing::error!("Failed to render template: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Template rendering error".to_string(),
                )
            })?;

        tracing::info!(
            "Setting ingress JWT cookie and showing success page for project: {}",
            project_name
        );

        // Build response with cookie and HTML
        let response = (
            StatusCode::OK,
            [("Set-Cookie", cookie.as_str())],
            Html(html),
        )
            .into_response();

        Ok(response)
    } else {
        tracing::info!("Setting session cookie and redirecting to {}", redirect_url);

        // For regular OAuth flow, immediate redirect
        let response = (
            StatusCode::FOUND,
            [("Location", redirect_url.as_str()), ("Set-Cookie", &cookie)],
        )
            .into_response();

        Ok(response)
    }
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
#[instrument(skip(state, params, headers))]
pub async fn ingress_auth(
    State(state): State<AppState>,
    Query(params): Query<IngressAuthQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    // Log only safe, relevant information (excluding sensitive cookies, tokens, etc.)
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");

    tracing::debug!(
        project = %params.project,
        request_id = %request_id,
        "Ingress auth check"
    );

    // Extract and validate Rise JWT (required)
    let rise_jwt = cookie_helpers::extract_ingress_jwt_cookie(&headers).ok_or_else(|| {
        tracing::debug!("No ingress JWT cookie found");
        (StatusCode::UNAUTHORIZED, "No session cookie".to_string())
    })?;

    let ingress_claims = state
        .jwt_signer
        .verify_ingress_jwt(&rise_jwt)
        .map_err(|e| {
            tracing::warn!("Invalid or expired ingress JWT: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired session".to_string(),
            )
        })?;

    let email = ingress_claims.email;

    // Find or create user in database
    let user = users::find_or_create(&state.db_pool, &email)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding/creating user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    tracing::debug!(
        project = %params.project,
        user_id = %user.id,
        user_email = %user.email,
        "Rise JWT validated"
    );

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
        tracing::debug!(
            project = %params.project,
            user_id = %user.id,
            user_email = %user.email,
            "Public project access granted"
        );
        return Ok((
            StatusCode::OK,
            [
                ("X-Auth-Request-Email", email),
                ("X-Auth-Request-User", user.id.to_string()),
            ],
        )
            .into_response());
    }

    // For private projects, check access permissions
    let has_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    if has_access {
        tracing::debug!(
            project = %params.project,
            user_id = %user.id,
            user_email = %user.email,
            "Private project access granted"
        );
        Ok((
            StatusCode::OK,
            [
                ("X-Auth-Request-Email", email),
                ("X-Auth-Request-User", user.id.to_string()),
            ],
        )
            .into_response())
    } else {
        tracing::warn!(
            project = %params.project,
            user_id = %user.id,
            user_email = %user.email,
            "Private project access denied"
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
