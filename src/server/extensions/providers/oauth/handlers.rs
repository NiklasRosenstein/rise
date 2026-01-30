use super::models::{
    AuthorizeFlowQuery, CallbackRequest, OAuth2ErrorResponse, OAuth2TokenResponse, OAuthCodeState,
    OAuthExtensionSpec, OAuthExtensionStatus, OAuthState, TokenRequest, TokenResponse,
};
use crate::db::{extensions as db_extensions, projects as db_projects};
use crate::server::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::Engine;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use url::Url;

/// Generate a random state token for CSRF protection
fn generate_state_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

/// Generate a PKCE code verifier (random string)
fn generate_code_verifier() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..128)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a PKCE code challenge from a code verifier (SHA256 hash, base64url encoded)
fn generate_code_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();

    // Base64url encode (no padding)
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

/// Validate CORS origin against allowed origins for a project
///
/// Returns the allowed origin if valid, None otherwise.
/// Allowed origins:
/// - localhost (any port) for local development
/// - Rise public URL
/// - Project deployment URLs (including custom domains)
async fn validate_cors_origin(
    pool: &sqlx::PgPool,
    origin: &str,
    project: &crate::db::models::Project,
    rise_public_url: &str,
    deployment_backend: &Arc<dyn crate::server::deployment::controller::DeploymentBackend>,
) -> Option<String> {
    let origin_url = match Url::parse(origin) {
        Ok(url) => url,
        Err(_) => return None,
    };

    // Allow localhost for local development (any port)
    if let Some(host) = origin_url.host_str() {
        if host == "localhost" || host == "127.0.0.1" {
            return Some(origin.to_string());
        }
    }

    // Allow if origin matches Rise public URL (same origin)
    if let Ok(rise_url) = Url::parse(rise_public_url) {
        if origin_url.host() == rise_url.host()
            && origin_url.port() == rise_url.port()
            && origin_url.scheme() == rise_url.scheme()
        {
            return Some(origin.to_string());
        }
    }

    // Check project's deployment URLs
    let all_deployments =
        match crate::db::deployments::get_active_deployments_for_project(pool, project.id).await {
            Ok(deployments) => deployments,
            Err(e) => {
                warn!(
                    "Failed to fetch active deployments for project {}: {:?}",
                    project.name, e
                );
                return None;
            }
        };

    for deployment in &all_deployments {
        match deployment_backend
            .get_deployment_urls(deployment, project)
            .await
        {
            Ok(urls) => {
                // Check primary URL
                if !urls.primary_url.is_empty() {
                    if let Ok(deployment_url) = Url::parse(&urls.primary_url) {
                        if origin_url.host() == deployment_url.host()
                            && origin_url.port() == deployment_url.port()
                            && origin_url.scheme() == deployment_url.scheme()
                        {
                            return Some(origin.to_string());
                        }
                    }
                }

                // Check custom domain URLs
                for custom_url in &urls.custom_domain_urls {
                    if let Ok(custom_domain_url) = Url::parse(custom_url) {
                        if origin_url.host() == custom_domain_url.host()
                            && origin_url.port() == custom_domain_url.port()
                            && origin_url.scheme() == custom_domain_url.scheme()
                        {
                            return Some(origin.to_string());
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to get deployment URLs for deployment {}: {:?}",
                    deployment.deployment_id, e
                );
            }
        }
    }

    None
}

/// Create CORS response headers for allowed origin
fn cors_headers(origin: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(origin_value) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_value);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("86400"), // 24 hours
    );
    headers
}

/// Validate redirect URI against allowed origins
async fn validate_redirect_uri(
    pool: &sqlx::PgPool,
    redirect_uri: &str,
    project: &crate::db::models::Project,
    rise_public_url: &str,
    deployment_backend: &Arc<dyn crate::server::deployment::controller::DeploymentBackend>,
) -> Result<(), String> {
    let redirect_url =
        Url::parse(redirect_uri).map_err(|e| format!("Invalid redirect URI: {}", e))?;

    // Allow localhost for local development (any port and path)
    if let Some(host) = redirect_url.host_str() {
        if host == "localhost" || host == "127.0.0.1" {
            return Ok(());
        }
    }

    // Allow any redirect URL beginning with the Rise public URL
    if redirect_uri.starts_with(rise_public_url) {
        return Ok(());
    }

    // Get project's deployment URLs from the deployment backend
    // Check all active deployments (including staging/non-default groups)
    let all_deployments =
        match crate::db::deployments::get_active_deployments_for_project(pool, project.id).await {
            Ok(deployments) => deployments,
            Err(e) => {
                warn!(
                    "Failed to fetch active deployments for project {}: {:?}",
                    project.name, e
                );
                vec![]
            }
        };

    // Check if redirect URI starts with any deployment URL (primary or custom domain)
    for deployment in &all_deployments {
        match deployment_backend
            .get_deployment_urls(deployment, project)
            .await
        {
            Ok(urls) => {
                // Check primary URL
                if !urls.primary_url.is_empty() && redirect_uri.starts_with(&urls.primary_url) {
                    return Ok(());
                }

                // Check custom domain URLs
                for custom_url in &urls.custom_domain_urls {
                    if redirect_uri.starts_with(custom_url) {
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to get deployment URLs for deployment {}: {:?}",
                    deployment.deployment_id, e
                );
            }
        }
    }

    Err(format!(
        "Invalid redirect URI: not authorized for this project. Allowed: localhost, URLs starting with Rise public URL ({}), or any active deployment URL",
        rise_public_url
    ))
}

/// Initiate OAuth authorization flow
///
/// GET /oidc/{project}/{extension}/authorize
///
/// Query params:
/// - redirect_uri (optional): Where to redirect after auth (for local dev/custom domains)
/// - state (optional): Application's CSRF state parameter (passed through to final redirect)
/// - code_challenge (optional): PKCE code challenge for public clients (SPAs)
/// - code_challenge_method (optional): PKCE method ("S256" or "plain", defaults to "S256")
pub async fn authorize(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Query(req): Query<AuthorizeFlowQuery>,
) -> Result<Response, (StatusCode, String)> {
    debug!(
        "OAuth authorize request for project={}, extension={}",
        project_name, extension_name
    );

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Get OAuth extension
    let extension =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((
                StatusCode::NOT_FOUND,
                "OAuth extension not configured".to_string(),
            ))?;

    // Verify extension type is oauth
    if extension.extension_type != "oauth" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Extension '{}' is not an OAuth extension (type: {})",
                extension_name, extension.extension_type
            ),
        ));
    }

    // Parse spec
    let spec: OAuthExtensionSpec = serde_json::from_value(extension.spec.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid spec: {}", e),
        )
    })?;

    // Validate PKCE parameters if provided
    if let Some(ref code_challenge) = req.code_challenge {
        // RFC 7636: code_challenge must be 43-128 characters, base64url charset
        if code_challenge.len() < 43 || code_challenge.len() > 128 {
            return Err((
                StatusCode::BAD_REQUEST,
                "code_challenge must be 43-128 characters".to_string(),
            ));
        }
        if !code_challenge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "code_challenge contains invalid characters (must be base64url)".to_string(),
            ));
        }

        let method = req.code_challenge_method.as_deref().unwrap_or("S256");
        if method != "S256" && method != "plain" {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Unsupported code_challenge_method '{}'. Only 'S256' and 'plain' are supported.",
                    method
                ),
            ));
        }
    }

    // Determine final redirect URI
    let final_redirect_uri = if let Some(ref uri) = req.redirect_uri {
        // Validate redirect URI
        validate_redirect_uri(
            &state.db_pool,
            uri,
            &project,
            &state.public_url,
            &state.deployment_backend,
        )
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        uri.clone()
    } else {
        // Default to project's primary URL
        // Parse API URL to construct project URL
        let api_url = Url::parse(&state.public_url).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid API URL configuration: {}", e),
            )
        })?;

        let api_host = api_url.host_str().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Missing host in API URL".to_string(),
        ))?;

        let project_host = if let Some(base_domain) = api_host.strip_prefix("api.") {
            // api.domain.com -> project.domain.com
            format!("{}.{}", project_name, base_domain)
        } else if api_host == "localhost" || api_host == "127.0.0.1" {
            // localhost -> project.apps.rise.local
            format!("{}.apps.rise.local", project_name)
        } else {
            // domain.com -> project.domain.com
            format!("{}.{}", project_name, api_host)
        };

        let scheme = api_url.scheme();

        // For deployed Rise apps, use port 8080 (the default app port)
        // For production with proper DNS, don't include port
        if api_host == "localhost" || api_host == "127.0.0.1" {
            // Deployed locally - use port 8080
            format!("{}://{}:8080/", scheme, project_host)
        } else {
            // Production - no port (handled by ingress)
            format!("{}://{}/", scheme, project_host)
        }
    };

    // Generate CSRF state token
    let state_token = generate_state_token();

    // Generate PKCE code verifier and challenge
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    // Store OAuth state in cache
    let oauth_state = OAuthState {
        redirect_uri: Some(final_redirect_uri),
        application_state: req.state,
        project_name: project_name.clone(),
        extension_name: extension_name.clone(),
        code_verifier,
        created_at: Utc::now(),
        client_code_challenge: req.code_challenge,
        client_code_challenge_method: req.code_challenge_method,
    };

    // Store state in cache (TTL configured on cache builder)
    state
        .oauth_state_store
        .insert(state_token.clone(), oauth_state)
        .await;

    // Compute callback redirect URI for this extension
    // Use the same scheme and host as the API URL
    let api_url = Url::parse(&state.public_url).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid API URL configuration: {}", e),
        )
    })?;

    let api_host = api_url.host_str().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing host in API URL".to_string(),
    ))?;

    let redirect_uri = if let Some(port) = api_url.port() {
        format!(
            "{}://{}:{}/oidc/{}/{}/callback",
            api_url.scheme(),
            api_host,
            port,
            project_name,
            extension_name
        )
    } else {
        format!(
            "{}://{}/oidc/{}/{}/callback",
            api_url.scheme(),
            api_host,
            project_name,
            extension_name
        )
    };

    // Build authorization URL
    let mut auth_url = Url::parse(&spec.authorization_endpoint).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid authorization endpoint: {}", e),
        )
    })?;

    auth_url
        .query_pairs_mut()
        .append_pair("client_id", &spec.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &spec.scopes.join(" "))
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");

    debug!("Redirecting to OAuth provider: {}", auth_url.as_str());

    // Redirect to OAuth provider
    Ok(Redirect::to(auth_url.as_str()).into_response())
}

/// Handle OAuth callback from provider
///
/// GET /oidc/{project}/{extension}/callback
///
/// Query params:
/// - code: Authorization code from provider
/// - state: CSRF state token
pub async fn callback(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Query(req): Query<CallbackRequest>,
) -> Result<Response, (StatusCode, String)> {
    debug!(
        "OAuth callback for project={}, extension={}",
        project_name, extension_name
    );

    // Retrieve and validate state
    let oauth_state = state
        .oauth_state_store
        .get(&req.state)
        .await
        .ok_or((StatusCode::BAD_REQUEST, "Invalid state token".to_string()))?;

    // Verify project and extension match
    if oauth_state.project_name != project_name || oauth_state.extension_name != extension_name {
        return Err((StatusCode::BAD_REQUEST, "State mismatch".to_string()));
    }

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Get OAuth extension
    let extension =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((
                StatusCode::NOT_FOUND,
                "OAuth extension not configured".to_string(),
            ))?;

    // Parse spec
    let spec: OAuthExtensionSpec = serde_json::from_value(extension.spec.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid spec: {}", e),
        )
    })?;

    // Resolve OAuth provider's client secret (prefers encrypted in spec, falls back to env var ref)
    let encryption_provider = state.encryption_provider.as_ref().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Encryption provider not configured".to_string(),
    ))?;

    use super::provider::{OAuthProvider, OAuthProviderConfig};
    let oauth_provider = OAuthProvider::new(OAuthProviderConfig {
        db_pool: state.db_pool.clone(),
        encryption_provider: encryption_provider.clone(),
        http_client: reqwest::Client::new(),
        api_domain: state.public_url.clone(),
    });

    let client_secret = oauth_provider
        .resolve_oauth_client_secret(project.id, &spec)
        .await
        .map_err(|e| {
            error!("Failed to resolve OAuth client secret: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve OAuth client secret: {}", e),
            )
        })?;

    // Compute callback redirect URI - must match exactly what was sent in authorize request
    let api_url = Url::parse(&state.public_url).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid API URL configuration: {}", e),
        )
    })?;

    let api_host = api_url.host_str().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing host in API URL".to_string(),
    ))?;

    let redirect_uri = if let Some(port) = api_url.port() {
        format!(
            "{}://{}:{}/oidc/{}/{}/callback",
            api_url.scheme(),
            api_host,
            port,
            project_name,
            extension_name
        )
    } else {
        format!(
            "{}://{}/oidc/{}/{}/callback",
            api_url.scheme(),
            api_host,
            project_name,
            extension_name
        )
    };

    // Exchange authorization code for tokens (with PKCE code verifier)
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&spec.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &req.code),
            ("client_id", &spec.client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &oauth_state.code_verifier),
        ])
        .send()
        .await
        .map_err(|e| {
            error!("Token exchange request failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Token exchange request failed: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read error response".to_string());
        error!(
            "Token exchange failed with status {}: {}",
            status, error_text
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Token exchange failed with status {}: {}",
                status, error_text
            ),
        ));
    }

    let token_response: TokenResponse = response.json().await.map_err(|e| {
        error!("Failed to parse token response: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse token response: {}", e),
        )
    })?;

    // Encrypt tokens
    let encryption_provider = state.encryption_provider.as_ref().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Encryption provider not configured".to_string(),
    ))?;

    let access_token_encrypted = encryption_provider
        .encrypt(&token_response.access_token)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encrypt access token: {}", e),
            )
        })?;

    let refresh_token_encrypted = match &token_response.refresh_token {
        Some(refresh_token) => Some(encryption_provider.encrypt(refresh_token).await.map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to encrypt refresh token: {}", e),
                )
            },
        )?),
        None => None,
    };

    let id_token_encrypted = match &token_response.id_token {
        Some(id_token) => Some(encryption_provider.encrypt(id_token).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encrypt ID token: {}", e),
            )
        })?),
        None => None,
    };

    // Calculate token expiration
    let expires_at = Some(Utc::now() + Duration::seconds(token_response.expires_in));

    // Update extension status
    let status = OAuthExtensionStatus {
        redirect_uri: Some(redirect_uri),
        configured_at: Some(Utc::now()),
        auth_verified: true,
        error: None,
    };

    db_extensions::update_status(
        &state.db_pool,
        project.id,
        &extension_name,
        &serde_json::to_value(&status).unwrap(),
    )
    .await
    .map_err(|e| {
        warn!("Failed to update extension status: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update status: {}", e),
        )
    })?;

    // Clear state token from cache
    state.oauth_state_store.invalidate(&req.state).await;

    // Determine final redirect URI
    let final_redirect_uri = oauth_state.redirect_uri.clone().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing redirect URI in state".to_string(),
    ))?;

    debug!(
        "OAuth callback: final_redirect_uri={}, oauth_state={:?}",
        final_redirect_uri, oauth_state
    );

    // Build redirect URL with authorization code (RFC 6749)
    let mut redirect_url = Url::parse(&final_redirect_uri).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid redirect URI: {}", e),
        )
    })?;

    // Detect if this is a test flow (redirect to Rise UI) vs real application flow
    let is_test_flow = final_redirect_uri.starts_with(&state.public_url)
        || final_redirect_uri.starts_with("http://localhost:")
        || final_redirect_uri.starts_with("http://127.0.0.1:");

    if !is_test_flow {
        // Real application flow: Generate authorization code for token exchange
        let authorization_code = generate_state_token();

        // Store encrypted tokens in authorization code state (5-minute TTL, single-use)
        let code_state = OAuthCodeState {
            project_id: project.id,
            extension_name: extension_name.clone(),
            created_at: Utc::now(),
            code_challenge: oauth_state.client_code_challenge.clone(),
            code_challenge_method: oauth_state.client_code_challenge_method.clone(),
            access_token_encrypted,
            refresh_token_encrypted,
            id_token_encrypted,
            expires_at,
        };

        state
            .oauth_code_store
            .insert(authorization_code.clone(), code_state)
            .await;

        // Add authorization code as query parameter (RFC 6749)
        redirect_url
            .query_pairs_mut()
            .append_pair("code", &authorization_code);

        // Pass through application's CSRF state
        if let Some(app_state) = oauth_state.application_state {
            redirect_url
                .query_pairs_mut()
                .append_pair("state", &app_state);
        }

        info!(
            "Generated authorization code for project {} extension {}",
            project_name, extension_name
        );
    } else {
        // Test flow: Skip code generation, just redirect back to UI
        info!(
            "Completed test OAuth flow for project {} extension {} (no code generated)",
            project_name, extension_name
        );
    }

    info!(
        "OAuth callback complete: redirecting to {}",
        redirect_url.as_str()
    );

    Ok(Redirect::to(redirect_url.as_str()).into_response())
}

/// Validate PKCE code_verifier against code_challenge
/// Returns true if valid, false otherwise
fn validate_pkce(code_verifier: &str, code_challenge: &str, code_challenge_method: &str) -> bool {
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;

    match code_challenge_method {
        "S256" => {
            // SHA256 hash the verifier
            let mut hasher = Sha256::new();
            hasher.update(code_verifier.as_bytes());
            let hash = hasher.finalize();

            // Base64url encode (no padding)
            let computed_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);

            // Constant-time comparison
            computed_challenge
                .as_bytes()
                .ct_eq(code_challenge.as_bytes())
                .into()
        }
        "plain" => {
            // Direct comparison
            code_verifier
                .as_bytes()
                .ct_eq(code_challenge.as_bytes())
                .into()
        }
        _ => false,
    }
}

/// Create OAuth2 error response
fn oauth2_error(
    error: &str,
    description: Option<String>,
) -> (StatusCode, Json<OAuth2ErrorResponse>) {
    let status_code = match error {
        "invalid_request" => StatusCode::BAD_REQUEST,
        "invalid_client" => StatusCode::UNAUTHORIZED,
        "invalid_grant" => StatusCode::BAD_REQUEST,
        "unsupported_grant_type" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status_code,
        Json(OAuth2ErrorResponse {
            error: error.to_string(),
            error_description: description,
        }),
    )
}

/// RFC 6749-compliant token endpoint with per-project CORS support
///
/// POST /oidc/{project}/{extension}/token
///
/// Grant types:
/// - authorization_code: Exchange authorization code for tokens
/// - refresh_token: Refresh access token
///
/// Client authentication:
/// - Confidential clients: Use client_id + client_secret
/// - Public clients: Use client_id + code_verifier (PKCE)
///
/// CORS:
/// - Validates Origin header against project-specific allowed origins
/// - Allows localhost (any port), Rise public URL, project deployment URLs
pub async fn token_endpoint(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    debug!(
        "Token endpoint request for project={}, extension={}",
        project_name, extension_name
    );

    // Extract Origin header for CORS validation (will be validated after we get the project)
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Inner function to handle the actual token logic
    let result = token_endpoint_inner(&state, &project_name, &extension_name, &headers, body).await;

    // Get CORS headers if Origin was provided and project exists
    let cors_headers = if let Some(ref origin_str) = origin {
        // We need to get the project to validate CORS
        if let Ok(Some(project)) = db_projects::find_by_name(&state.db_pool, &project_name).await {
            validate_cors_origin(
                &state.db_pool,
                origin_str,
                &project,
                &state.public_url,
                &state.deployment_backend,
            )
            .await
            .map(|allowed| cors_headers(&allowed))
        } else {
            None
        }
    } else {
        None
    };

    // Build response with CORS headers
    // For error responses, always include CORS headers if Origin was provided (even if validation failed)
    // This ensures proper CORS error handling in the browser
    match result {
        Ok(token_response) => {
            let mut response = (StatusCode::OK, Json(token_response)).into_response();
            if let Some(cors) = cors_headers {
                response.headers_mut().extend(cors);
            }
            response
        }
        Err((status, error_json)) => {
            let mut response = (status, error_json).into_response();
            // For errors, use validated CORS headers if available, otherwise echo back Origin
            if let Some(cors) = cors_headers {
                response.headers_mut().extend(cors);
            } else if let Some(origin_str) = origin {
                // Even if CORS validation failed, include CORS headers so browser gets proper error
                response.headers_mut().extend(cors_headers(&origin_str));
            }
            response
        }
    }
}

/// Inner implementation of token endpoint logic
async fn token_endpoint_inner(
    state: &AppState,
    project_name: &str,
    extension_name: &str,
    headers: &axum::http::HeaderMap,
    body: String,
) -> Result<OAuth2TokenResponse, (StatusCode, Json<OAuth2ErrorResponse>)> {
    // Parse request body (support both form-urlencoded and JSON)
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/x-www-form-urlencoded");

    let req: TokenRequest = if content_type.contains("application/json") {
        serde_json::from_str(&body).map_err(|e| {
            oauth2_error(
                "invalid_request",
                Some(format!("Invalid JSON request body: {}", e)),
            )
        })?
    } else {
        // Parse as form-urlencoded
        serde_urlencoded::from_str(&body).map_err(|e| {
            oauth2_error(
                "invalid_request",
                Some(format!("Invalid form-urlencoded request body: {}", e)),
            )
        })?
    };

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, project_name)
        .await
        .map_err(|e| {
            error!("Database error: {:?}", e);
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?
        .ok_or_else(|| oauth2_error("invalid_request", Some("Project not found".to_string())))?;

    // Get OAuth extension
    let extension =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, extension_name)
            .await
            .map_err(|e| {
                error!("Database error: {:?}", e);
                oauth2_error("server_error", Some("Internal server error".to_string()))
            })?
            .ok_or_else(|| {
                oauth2_error(
                    "invalid_request",
                    Some("OAuth extension not configured".to_string()),
                )
            })?;

    // Verify extension type
    if extension.extension_type != "oauth" {
        return Err(oauth2_error(
            "invalid_request",
            Some(format!(
                "Extension '{}' is not an OAuth extension",
                extension_name
            )),
        ));
    }

    // Parse spec
    let spec: OAuthExtensionSpec = serde_json::from_value(extension.spec.clone()).map_err(|e| {
        error!("Invalid extension spec: {:?}", e);
        oauth2_error(
            "server_error",
            Some("Invalid extension configuration".to_string()),
        )
    })?;

    // Validate client_id
    let rise_client_id = spec.rise_client_id.as_ref().ok_or_else(|| {
        error!("Rise client ID not configured for extension");
        oauth2_error(
            "server_error",
            Some("OAuth extension not fully configured".to_string()),
        )
    })?;

    if &req.client_id != rise_client_id {
        return Err(oauth2_error(
            "invalid_client",
            Some("Invalid client_id".to_string()),
        ));
    }

    // Grant-specific authentication validation
    let has_client_secret = req.client_secret.is_some();
    let has_code_verifier = req.code_verifier.is_some();

    match req.grant_type.as_str() {
        "authorization_code" => {
            // authorization_code grant: REQUIRE client_secret OR code_verifier (PKCE)
            if !has_client_secret && !has_code_verifier {
                return Err(oauth2_error(
                    "invalid_request",
                    Some("Missing client authentication: provide either client_secret (confidential clients) or code_verifier (public clients with PKCE)".to_string()),
                ));
            }
            // For authorization_code grant, client_secret and code_verifier are mutually exclusive
            if has_client_secret && has_code_verifier {
                return Err(oauth2_error(
                    "invalid_request",
                    Some("Client authentication methods are mutually exclusive: provide either client_secret (confidential clients) or code_verifier (public clients), not both".to_string()),
                ));
            }
        }
        "refresh_token" => {
            // refresh_token grant: ALLOW client_secret (confidential) or no auth (public)
            // REJECT code_verifier (PKCE is only for authorization_code grant)
            if has_code_verifier {
                return Err(oauth2_error(
                    "invalid_request",
                    Some("code_verifier not supported for refresh_token grant (PKCE is only for authorization_code)".to_string()),
                ));
            }
            // Note: client_secret is optional for refresh_token grant (public clients)
        }
        _ => {
            // Unknown grant type will be rejected later
        }
    }

    // If client_secret provided, validate it
    if let Some(ref client_secret) = req.client_secret {
        // Get encryption provider
        let encryption_provider = state.encryption_provider.as_ref().ok_or_else(|| {
            error!("Encryption provider not configured");
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?;

        // Resolve stored Rise client secret (prefer encrypted in spec, fall back to env var ref)
        let stored_secret = if let Some(ref encrypted) = spec.rise_client_secret_encrypted {
            // Decrypt from spec
            encryption_provider.decrypt(encrypted).await.map_err(|e| {
                error!("Failed to decrypt Rise client secret from spec: {:?}", e);
                oauth2_error("server_error", Some("Internal server error".to_string()))
            })?
        } else if let Some(ref rise_client_secret_ref) = spec.rise_client_secret_ref {
            // Legacy: Get from env vars
            use crate::db::env_vars as db_env_vars;
            let env_vars = db_env_vars::list_project_env_vars(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    error!("Failed to list env vars: {:?}", e);
                    oauth2_error("server_error", Some("Internal server error".to_string()))
                })?;

            let env_var = env_vars
                .iter()
                .find(|v| v.key == *rise_client_secret_ref)
                .ok_or_else(|| {
                    error!(
                        "Rise client secret env var not found: {}",
                        rise_client_secret_ref
                    );
                    oauth2_error(
                        "invalid_client",
                        Some("Client credentials not configured".to_string()),
                    )
                })?;

            encryption_provider
                .decrypt(&env_var.value)
                .await
                .map_err(|e| {
                    error!("Failed to decrypt Rise client secret from env var: {:?}", e);
                    oauth2_error("server_error", Some("Internal server error".to_string()))
                })?
        } else {
            error!("No Rise client secret configured (rise_client_secret_encrypted or rise_client_secret_ref required)");
            return Err(oauth2_error(
                "server_error",
                Some("OAuth extension not fully configured".to_string()),
            ));
        };

        // Constant-time comparison
        use subtle::ConstantTimeEq;
        let is_valid: bool = client_secret
            .as_bytes()
            .ct_eq(stored_secret.as_bytes())
            .into();
        if !is_valid {
            return Err(oauth2_error(
                "invalid_client",
                Some("Invalid client_secret".to_string()),
            ));
        }
    }

    // Route to grant-specific handlers (unwrap Json wrapper for inner return)
    match req.grant_type.as_str() {
        "authorization_code" => handle_authorization_code_grant(
            state.clone(),
            project,
            extension_name.to_string(),
            spec,
            req,
        )
        .await
        .map(|Json(response)| response),
        "refresh_token" => handle_refresh_token_grant(
            state.clone(),
            project,
            extension_name.to_string(),
            spec,
            req,
        )
        .await
        .map(|Json(response)| response),
        _ => Err(oauth2_error(
            "unsupported_grant_type",
            Some(format!("Unsupported grant_type: {}", req.grant_type)),
        )),
    }
}

/// Handle authorization_code grant type
async fn handle_authorization_code_grant(
    state: AppState,
    project: crate::db::models::Project,
    extension_name: String,
    spec: OAuthExtensionSpec,
    req: TokenRequest,
) -> Result<Json<OAuth2TokenResponse>, (StatusCode, Json<OAuth2ErrorResponse>)> {
    // SECURITY: Ensure mutual exclusivity of authentication methods
    // This is defensive programming - the check should already have happened in token_endpoint_inner
    if req.client_secret.is_some() && req.code_verifier.is_some() {
        return Err(oauth2_error(
            "invalid_request",
            Some("Authentication methods must be mutually exclusive".to_string()),
        ));
    }

    // Validate required parameters
    let code = req.code.ok_or_else(|| {
        oauth2_error(
            "invalid_request",
            Some("Missing required parameter: code".to_string()),
        )
    })?;

    // SECURITY: Consume authorization code immediately (atomic get-and-remove)
    // This prevents race conditions where the same code could be used twice.
    // Validations happen after removal - if they fail, code is already consumed.
    // This is acceptable per RFC 6749: codes MUST be single-use.
    let code_state = state.oauth_code_store.remove(&code).await.ok_or_else(|| {
        oauth2_error(
            "invalid_grant",
            Some("Invalid or expired authorization code".to_string()),
        )
    })?;

    // Verify project and extension match
    if code_state.project_id != project.id || code_state.extension_name != extension_name {
        return Err(oauth2_error(
            "invalid_grant",
            Some("Authorization code mismatch".to_string()),
        ));
    }

    // CRITICAL: If code_verifier provided, challenge must have been provided during authz
    // This prevents bypassing authentication by providing code_verifier without prior code_challenge
    if req.code_verifier.is_some() && code_state.code_challenge.is_none() {
        return Err(oauth2_error(
            "invalid_request",
            Some("code_verifier requires prior code_challenge during authorization".to_string()),
        ));
    }

    // PKCE validation if code_challenge was provided during authorization
    if let Some(ref code_challenge) = code_state.code_challenge {
        // PKCE flow - require code_verifier
        let code_verifier = req.code_verifier.ok_or_else(|| {
            oauth2_error(
                "invalid_request",
                Some("Missing code_verifier for PKCE flow".to_string()),
            )
        })?;

        // RFC 7636: code_verifier must be 43-128 characters, unreserved charset [A-Za-z0-9-._~]
        if code_verifier.len() < 43 || code_verifier.len() > 128 {
            return Err(oauth2_error(
                "invalid_request",
                Some("code_verifier must be 43-128 characters".to_string()),
            ));
        }
        if !code_verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c))
        {
            return Err(oauth2_error(
                "invalid_request",
                Some("code_verifier contains invalid characters".to_string()),
            ));
        }

        let code_challenge_method = code_state
            .code_challenge_method
            .as_deref()
            .unwrap_or("S256");

        if !validate_pkce(&code_verifier, code_challenge, code_challenge_method) {
            return Err(oauth2_error(
                "invalid_grant",
                Some("PKCE validation failed".to_string()),
            ));
        }

        debug!("PKCE validation successful");
    }

    // Note: For non-PKCE flows, client_secret was already validated in token_endpoint()

    // Decrypt tokens from code_state (tokens stored directly in authorization code cache)
    let encryption_provider = state.encryption_provider.as_ref().ok_or_else(|| {
        error!("Encryption provider not configured");
        oauth2_error("server_error", Some("Internal server error".to_string()))
    })?;

    let access_token = encryption_provider
        .decrypt(&code_state.access_token_encrypted)
        .await
        .map_err(|e| {
            error!("Failed to decrypt access token: {:?}", e);
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?;

    let refresh_token = if let Some(ref encrypted) = code_state.refresh_token_encrypted {
        Some(encryption_provider.decrypt(encrypted).await.map_err(|e| {
            error!("Failed to decrypt refresh token: {:?}", e);
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?)
    } else {
        None
    };

    let id_token = if let Some(ref encrypted) = code_state.id_token_encrypted {
        Some(encryption_provider.decrypt(encrypted).await.map_err(|e| {
            error!("Failed to decrypt ID token: {:?}", e);
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?)
    } else {
        None
    };

    // Calculate expires_in (seconds from now)
    let expires_in = code_state.expires_at.map(|expires_at| {
        let now = Utc::now();
        let duration = expires_at.signed_duration_since(now);
        duration.num_seconds().max(0) // Don't return negative values
    });

    // Build scope from extension spec
    let scope = if spec.scopes.is_empty() {
        None
    } else {
        Some(spec.scopes.join(" "))
    };

    info!(
        "Authorization code grant successful for project {} extension {}",
        project.name, extension_name
    );

    Ok(Json(OAuth2TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in,
        refresh_token,
        scope,
        id_token,
    }))
}

/// Handle refresh_token grant type
async fn handle_refresh_token_grant(
    state: AppState,
    project: crate::db::models::Project,
    extension_name: String,
    spec: OAuthExtensionSpec,
    req: TokenRequest,
) -> Result<Json<OAuth2TokenResponse>, (StatusCode, Json<OAuth2ErrorResponse>)> {
    // Validate required parameters
    let refresh_token = req.refresh_token.ok_or_else(|| {
        oauth2_error(
            "invalid_request",
            Some("Missing required parameter: refresh_token".to_string()),
        )
    })?;

    // Call OAuth provider's refresh_token method
    use super::provider::{OAuthProvider, OAuthProviderConfig};

    let oauth_provider = OAuthProvider::new(OAuthProviderConfig {
        db_pool: state.db_pool.clone(),
        encryption_provider: state.encryption_provider.clone().ok_or_else(|| {
            error!("Encryption provider not configured");
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?,
        http_client: reqwest::Client::new(),
        api_domain: state.public_url.clone(),
    });

    // Get upstream OAuth client secret (prefers encrypted in spec, falls back to env var ref)
    let client_secret = oauth_provider
        .resolve_oauth_client_secret(project.id, &spec)
        .await
        .map_err(|e| {
            error!("Failed to resolve OAuth client secret: {:?}", e);
            oauth2_error("server_error", Some("Internal server error".to_string()))
        })?;

    // Refresh tokens with upstream provider
    let token_response = oauth_provider
        .refresh_token(&spec, &client_secret, &refresh_token)
        .await
        .map_err(|e| {
            error!("Failed to refresh token with upstream provider: {:?}", e);
            oauth2_error("invalid_grant", Some("Failed to refresh token".to_string()))
        })?;

    // Calculate expires_in
    let expires_in = Some(token_response.expires_in);

    // Build scope
    let scope = if spec.scopes.is_empty() {
        None
    } else {
        Some(spec.scopes.join(" "))
    };

    info!(
        "Refresh token grant successful for project {} extension {}",
        project.name, extension_name
    );

    Ok(Json(OAuth2TokenResponse {
        access_token: token_response.access_token,
        token_type: token_response.token_type,
        expires_in,
        refresh_token: token_response.refresh_token,
        scope,
        id_token: token_response.id_token,
    }))
}

/// CORS preflight handler for token endpoint
///
/// OPTIONS /oidc/{project}/{extension}/token
///
/// Validates the Origin header against project-specific allowed origins:
/// - localhost (any port) for local development
/// - Rise public URL
/// - Project deployment URLs (including custom domains)
pub async fn token_endpoint_options(
    State(state): State<AppState>,
    Path((project_name, _extension_name)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Get Origin header
    let origin = match headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) {
        Some(o) => o,
        None => {
            // No Origin header - not a CORS request, return empty 204
            return StatusCode::NO_CONTENT.into_response();
        }
    };

    // Get project to validate origin
    let project = match db_projects::find_by_name(&state.db_pool, &project_name).await {
        Ok(Some(p)) => p,
        _ => {
            // Project not found - reject CORS
            return StatusCode::FORBIDDEN.into_response();
        }
    };

    // Validate origin against project's allowed origins
    match validate_cors_origin(
        &state.db_pool,
        origin,
        &project,
        &state.public_url,
        &state.deployment_backend,
    )
    .await
    {
        Some(allowed_origin) => {
            // Origin is allowed - return CORS headers
            let cors = cors_headers(&allowed_origin);
            (StatusCode::NO_CONTENT, cors).into_response()
        }
        None => {
            // Origin not allowed
            debug!(
                "CORS origin '{}' not allowed for project '{}'",
                origin, project_name
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}

/// Get upstream issuer URL from spec or derive from token_endpoint
fn get_upstream_issuer(spec: &OAuthExtensionSpec) -> Option<String> {
    // Use spec.issuer if provided, otherwise derive from token_endpoint
    spec.issuer.clone().or_else(|| {
        // Try to derive issuer from token_endpoint by removing common path suffixes
        // e.g., "http://dex:5556/dex/token" -> "http://dex:5556/dex"
        let token_url = Url::parse(&spec.token_endpoint).ok()?;
        let mut path = token_url.path().to_string();
        // Remove common token endpoint suffixes
        for suffix in ["/token", "/token-request", "/oauth/token", "/oauth2/token"] {
            if path.ends_with(suffix) {
                path = path.trim_end_matches(suffix).to_string();
                break;
            }
        }
        let mut issuer_url = token_url.clone();
        issuer_url.set_path(&path);
        // Remove query and fragment
        issuer_url.set_query(None);
        issuer_url.set_fragment(None);
        Some(issuer_url.to_string().trim_end_matches('/').to_string())
    })
}

/// Proxy OIDC discovery document from upstream provider
///
/// GET /oidc/{project}/{extension}/.well-known/openid-configuration
///
/// Returns the OIDC discovery document with URLs rewritten to point to Rise's OIDC proxy:
/// - issuer -> {RISE_PUBLIC_URL}/oidc/{project}/{extension}
/// - authorization_endpoint -> {RISE_PUBLIC_URL}/oidc/{project}/{extension}/authorize
/// - token_endpoint -> {RISE_PUBLIC_URL}/oidc/{project}/{extension}/token
/// - jwks_uri -> {RISE_PUBLIC_URL}/oidc/{project}/{extension}/jwks
pub async fn oidc_discovery(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug!(
        "OIDC discovery request for project={}, extension={}",
        project_name, extension_name
    );

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Get OAuth extension
    let extension =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((
                StatusCode::NOT_FOUND,
                "OAuth extension not configured".to_string(),
            ))?;

    // Verify extension type is oauth
    if extension.extension_type != "oauth" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Extension '{}' is not an OAuth extension (type: {})",
                extension_name, extension.extension_type
            ),
        ));
    }

    // Parse spec
    let spec: OAuthExtensionSpec = serde_json::from_value(extension.spec.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid spec: {}", e),
        )
    })?;

    // Get upstream issuer
    let upstream_issuer = get_upstream_issuer(&spec).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to determine upstream OIDC issuer".to_string(),
    ))?;

    // Fetch upstream discovery document
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        upstream_issuer.trim_end_matches('/')
    );

    let http_client = reqwest::Client::new();
    let response = http_client.get(&discovery_url).send().await.map_err(|e| {
        error!(
            "Failed to fetch OIDC discovery from {}: {:?}",
            discovery_url, e
        );
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to fetch OIDC discovery: {}", e),
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read error response".to_string());
        error!(
            "OIDC discovery failed with status {}: {}",
            status, error_text
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Upstream OIDC discovery failed: {}", status),
        ));
    }

    let mut discovery: serde_json::Value = response.json().await.map_err(|e| {
        error!("Failed to parse OIDC discovery response: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse OIDC discovery".to_string(),
        )
    })?;

    // Build Rise OIDC base URL
    let rise_oidc_base = format!(
        "{}/oidc/{}/{}",
        state.public_url.trim_end_matches('/'),
        project_name,
        extension_name
    );

    // Rewrite URLs to point to Rise's OIDC proxy
    if let Some(obj) = discovery.as_object_mut() {
        obj.insert(
            "issuer".to_string(),
            serde_json::Value::String(rise_oidc_base.clone()),
        );
        obj.insert(
            "authorization_endpoint".to_string(),
            serde_json::Value::String(format!("{}/authorize", rise_oidc_base)),
        );
        obj.insert(
            "token_endpoint".to_string(),
            serde_json::Value::String(format!("{}/token", rise_oidc_base)),
        );
        obj.insert(
            "jwks_uri".to_string(),
            serde_json::Value::String(format!("{}/jwks", rise_oidc_base)),
        );
    }

    info!(
        "Returning OIDC discovery for {}/{} with Rise OIDC base: {}",
        project_name, extension_name, rise_oidc_base
    );

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json(discovery),
    ))
}

/// Proxy JWKS from upstream provider
///
/// GET /oidc/{project}/{extension}/jwks
///
/// Fetches the JWKS from the upstream OAuth provider and returns it.
pub async fn oidc_jwks(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug!(
        "OIDC JWKS request for project={}, extension={}",
        project_name, extension_name
    );

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Get OAuth extension
    let extension =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((
                StatusCode::NOT_FOUND,
                "OAuth extension not configured".to_string(),
            ))?;

    // Verify extension type is oauth
    if extension.extension_type != "oauth" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Extension '{}' is not an OAuth extension (type: {})",
                extension_name, extension.extension_type
            ),
        ));
    }

    // Parse spec
    let spec: OAuthExtensionSpec = serde_json::from_value(extension.spec.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid spec: {}", e),
        )
    })?;

    // Get upstream issuer
    let upstream_issuer = get_upstream_issuer(&spec).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to determine upstream OIDC issuer".to_string(),
    ))?;

    // Fetch upstream discovery to get jwks_uri
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        upstream_issuer.trim_end_matches('/')
    );

    let http_client = reqwest::Client::new();
    let discovery_response = http_client.get(&discovery_url).send().await.map_err(|e| {
        error!(
            "Failed to fetch OIDC discovery from {}: {:?}",
            discovery_url, e
        );
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to fetch OIDC discovery: {}", e),
        )
    })?;

    if !discovery_response.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "Upstream OIDC discovery failed".to_string(),
        ));
    }

    let discovery: serde_json::Value = discovery_response.json().await.map_err(|e| {
        error!("Failed to parse OIDC discovery response: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse OIDC discovery".to_string(),
        )
    })?;

    // Get jwks_uri from discovery
    let jwks_uri = discovery.get("jwks_uri").and_then(|v| v.as_str()).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "No jwks_uri in OIDC discovery".to_string(),
    ))?;

    // Fetch JWKS
    let jwks_response = http_client.get(jwks_uri).send().await.map_err(|e| {
        error!("Failed to fetch JWKS from {}: {:?}", jwks_uri, e);
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to fetch JWKS: {}", e),
        )
    })?;

    if !jwks_response.status().is_success() {
        let status = jwks_response.status();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Upstream JWKS fetch failed: {}", status),
        ));
    }

    let jwks: serde_json::Value = jwks_response.json().await.map_err(|e| {
        error!("Failed to parse JWKS response: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse JWKS".to_string(),
        )
    })?;

    info!(
        "Returning JWKS for {}/{} from upstream: {}",
        project_name, extension_name, jwks_uri
    );

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json(jwks),
    ))
}
