use super::models::{
    AuthorizeFlowQuery, CallbackRequest, OAuthExchangeState, OAuthExtensionSpec,
    OAuthExtensionStatus, OAuthFlowType, OAuthState, TokenResponse,
};
use crate::db::{extensions as db_extensions, projects as db_projects, user_oauth_tokens};
use crate::server::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::Engine;
use chrono::{Duration, Utc};
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

/// Generate a random state token for CSRF protection
fn generate_state_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

/// Generate a random session ID
fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
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

/// Extract session ID from cookie header
fn extract_session_id_from_cookie(cookie_header: Option<&str>) -> Option<String> {
    // TODO: Use proper cookie parsing library
    cookie_header?.split(';').find_map(|cookie| {
        let cookie = cookie.trim();
        if cookie.starts_with("rise_oauth_session=") {
            Some(cookie.trim_start_matches("rise_oauth_session=").to_string())
        } else {
            None
        }
    })
}

/// Validate redirect URI against allowed origins
async fn validate_redirect_uri(
    pool: &sqlx::PgPool,
    redirect_uri: &str,
    project_id: uuid::Uuid,
    project_name: &str,
    api_url: &str,
) -> Result<(), String> {
    let redirect_url =
        Url::parse(redirect_uri).map_err(|e| format!("Invalid redirect URI: {}", e))?;

    let host = redirect_url
        .host_str()
        .ok_or("Missing host in redirect URI")?;
    let port = redirect_url.port();

    // Allow localhost for local development (any port)
    if host == "localhost" || host == "127.0.0.1" {
        return Ok(());
    }

    // Parse the API URL to extract the base domain
    let api_parsed = Url::parse(api_url).map_err(|e| format!("Invalid API URL: {}", e))?;
    let api_host = api_parsed.host_str().ok_or("Missing host in API URL")?;

    // Construct allowed redirect domains
    let mut allowed_domains = vec![];

    // Allow the main Rise domain (where API and UI are hosted)
    // This is needed for the "Test OAuth Flow" button in the UI
    allowed_domains.push(api_host.to_string());

    // Allow project subdomain: {project}.{domain}
    // e.g., oauth-fragment-flow.rise.example.com
    allowed_domains.push(format!("{}.{}", project_name, api_host));

    // For localhost development: project.apps.rise.local
    if api_host == "localhost" || api_host == "127.0.0.1" {
        allowed_domains.push(format!("{}.apps.rise.local", project_name));
    }

    // Fetch and allow project's custom domains
    match crate::db::custom_domains::list_project_custom_domains(pool, project_id).await {
        Ok(custom_domains) => {
            for custom_domain in custom_domains {
                allowed_domains.push(custom_domain.domain);
            }
        }
        Err(e) => {
            warn!(
                "Failed to fetch custom domains for project validation: {:?}",
                e
            );
            // Continue without custom domains rather than failing the request
        }
    }

    // Check if the redirect host matches any allowed domain (with or without port)
    let host_with_port = if let Some(p) = port {
        format!("{}:{}", host, p)
    } else {
        host.to_string()
    };

    for allowed in &allowed_domains {
        if host == allowed || host_with_port == *allowed {
            return Ok(());
        }
    }

    Err(format!(
        "Invalid redirect URI: not authorized for this project (allowed: localhost, {})",
        allowed_domains.join(", ")
    ))
}

/// Initiate OAuth authorization flow
///
/// GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize
///
/// Query params:
/// - redirect_uri (optional): Where to redirect after auth (for local dev/custom domains)
/// - state (optional): Application's CSRF state parameter (passed through to final redirect)
/// - flow (optional): "fragment" (default, for SPAs) or "exchange" (for backend apps)
pub async fn authorize(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Query(req): Query<AuthorizeFlowQuery>,
    headers: axum::http::HeaderMap,
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

    // Extract session ID from cookie (if present)
    let cookie_header = headers.get(header::COOKIE).and_then(|h| h.to_str().ok());
    let existing_session_id = extract_session_id_from_cookie(cookie_header);

    debug!("Existing session ID from cookie: {:?}", existing_session_id);

    // Check for cached token if we have a session ID
    if let Some(ref session_id) = existing_session_id {
        match user_oauth_tokens::get_by_session(
            &state.db_pool,
            project.id,
            &extension_name,
            session_id,
        )
        .await
        {
            Ok(Some(cached_token)) => {
                debug!("Found cached OAuth token for session {}", session_id);

                // Check if token is still valid or can be refreshed
                if let Some(expires_at) = cached_token.expires_at {
                    if expires_at > Utc::now() {
                        // Token is still valid, use it immediately
                        info!("Cached token still valid, redirecting with cached token");

                        // Update last_accessed_at
                        if let Err(e) =
                            user_oauth_tokens::update_last_accessed(&state.db_pool, cached_token.id)
                                .await
                        {
                            warn!("Failed to update last_accessed_at: {:?}", e);
                        }

                        // Decrypt tokens
                        let encryption_provider = state.encryption_provider.as_ref().ok_or((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Encryption provider not configured".to_string(),
                        ))?;

                        let access_token = encryption_provider
                            .decrypt(&cached_token.access_token_encrypted)
                            .await
                            .map_err(|e| {
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    format!("Failed to decrypt access token: {}", e),
                                )
                            })?;

                        let id_token =
                            if let Some(ref id_token_encrypted) = cached_token.id_token_encrypted {
                                Some(
                                    encryption_provider
                                        .decrypt(id_token_encrypted)
                                        .await
                                        .map_err(|e| {
                                            (
                                                StatusCode::INTERNAL_SERVER_ERROR,
                                                format!("Failed to decrypt ID token: {}", e),
                                            )
                                        })?,
                                )
                            } else {
                                None
                            };

                        // Determine final redirect URI
                        let final_redirect_uri = req.redirect_uri.unwrap_or_else(|| {
                            // Default to project's primary URL
                            format!(
                                "https://{}.{}",
                                project_name,
                                state.public_url.trim_start_matches("https://api.")
                            )
                        });

                        // Build redirect URL based on requested flow type
                        let mut redirect_url = Url::parse(&final_redirect_uri).map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Invalid redirect URI: {}", e),
                            )
                        })?;

                        match req.flow {
                            OAuthFlowType::Fragment => {
                                // Fragment flow: Return tokens in URL fragment
                                let expires_in = (expires_at - Utc::now()).num_seconds();
                                let mut fragment_parts = vec![
                                    format!("access_token={}", access_token),
                                    format!("token_type=Bearer"),
                                    format!("expires_in={}", expires_in),
                                ];

                                if let Some(id_token) = id_token {
                                    fragment_parts.push(format!("id_token={}", id_token));
                                }

                                if let Some(app_state) = req.state {
                                    fragment_parts.push(format!("state={}", app_state));
                                }

                                redirect_url.set_fragment(Some(&fragment_parts.join("&")));
                            }
                            OAuthFlowType::Exchange => {
                                // Exchange flow: Generate exchange token for backend to retrieve
                                let exchange_token = generate_state_token();

                                let exchange_state = OAuthExchangeState {
                                    project_id: project.id,
                                    extension_name: extension_name.clone(),
                                    session_id: session_id.clone(),
                                    created_at: Utc::now(),
                                };

                                // Store exchange token in cache (5-minute TTL)
                                state
                                    .oauth_exchange_store
                                    .insert(exchange_token.clone(), exchange_state)
                                    .await;

                                // Return exchange token in query parameter
                                redirect_url
                                    .query_pairs_mut()
                                    .append_pair("exchange_token", &exchange_token);

                                // Pass through application's CSRF state if provided
                                if let Some(app_state) = req.state {
                                    redirect_url
                                        .query_pairs_mut()
                                        .append_pair("state", &app_state);
                                }
                            }
                        }

                        return Ok(Redirect::to(redirect_url.as_str()).into_response());
                    } else if cached_token.refresh_token_encrypted.is_some() {
                        // Token expired but we have a refresh token - refresh it
                        debug!("Cached token expired, attempting refresh");
                        // TODO: Implement inline token refresh
                        // For now, fall through to full OAuth flow
                    } else {
                        // Token expired and no refresh token - delete it
                        debug!("Cached token expired without refresh token, deleting");
                        if let Err(e) =
                            user_oauth_tokens::delete(&state.db_pool, cached_token.id).await
                        {
                            warn!("Failed to delete expired token: {:?}", e);
                        }
                    }
                }
            }
            Ok(None) => {
                debug!("No cached token found for session {}", session_id);
            }
            Err(e) => {
                warn!("Error checking for cached token: {:?}", e);
            }
        }
    }

    // No valid cached token - proceed with full OAuth flow

    // Determine final redirect URI
    let final_redirect_uri = if let Some(ref uri) = req.redirect_uri {
        // Validate redirect URI
        validate_redirect_uri(
            &state.db_pool,
            uri,
            project.id,
            &project_name,
            &state.public_url,
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
        session_id: existing_session_id,
        flow_type: req.flow,
        code_verifier,
        created_at: Utc::now(),
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
            "{}://{}:{}/api/v1/oauth/callback/{}/{}",
            api_url.scheme(),
            api_host,
            port,
            project_name,
            extension_name
        )
    } else {
        format!(
            "{}://{}/api/v1/oauth/callback/{}/{}",
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
/// GET /api/v1/oauth/callback/{project}/{extension}
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

    // Resolve client_secret from environment variable
    let env_vars = crate::db::env_vars::list_project_env_vars(&state.db_pool, project.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let env_var = env_vars
        .iter()
        .find(|var| var.key == spec.client_secret_ref)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Environment variable '{}' not found for OAuth client secret",
                    spec.client_secret_ref
                ),
            )
        })?;

    let client_secret = if env_var.is_secret {
        let encryption_provider = state.encryption_provider.as_ref().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Encryption provider not configured".to_string(),
        ))?;

        encryption_provider
            .decrypt(&env_var.value)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to decrypt OAuth client secret: {}", e),
                )
            })?
    } else {
        env_var.value.clone()
    };

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
            "{}://{}:{}/api/v1/oauth/callback/{}/{}",
            api_url.scheme(),
            api_host,
            port,
            project_name,
            extension_name
        )
    } else {
        format!(
            "{}://{}/api/v1/oauth/callback/{}/{}",
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

    // Determine session ID (use existing or generate new)
    let session_id = oauth_state.session_id.unwrap_or_else(generate_session_id);

    // Store user OAuth token in database
    let expires_at = Some(Utc::now() + Duration::seconds(token_response.expires_in));
    user_oauth_tokens::upsert(
        &state.db_pool,
        project.id,
        &extension_name,
        &session_id,
        &access_token_encrypted,
        refresh_token_encrypted.as_deref(),
        id_token_encrypted.as_deref(),
        expires_at,
    )
    .await
    .map_err(|e| {
        error!("Failed to store user OAuth token: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to store token: {}", e),
        )
    })?;

    info!("Successfully stored OAuth token for session {}", session_id);

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
    let final_redirect_uri = oauth_state.redirect_uri.ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Missing redirect URI in state".to_string(),
    ))?;

    // Build redirect URL - different flow based on oauth_state.flow_type
    let mut redirect_url = Url::parse(&final_redirect_uri).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid redirect URI: {}", e),
        )
    })?;

    match oauth_state.flow_type {
        OAuthFlowType::Fragment => {
            // Fragment flow: Return tokens directly in URL fragment (for SPAs)
            let mut fragment_parts = vec![
                format!("access_token={}", token_response.access_token),
                format!("token_type={}", token_response.token_type),
                format!("expires_in={}", token_response.expires_in),
            ];

            if let Some(id_token) = token_response.id_token {
                fragment_parts.push(format!("id_token={}", id_token));
            }

            // Pass through application's CSRF state
            if let Some(app_state) = oauth_state.application_state {
                fragment_parts.push(format!("state={}", app_state));
            }

            redirect_url.set_fragment(Some(&fragment_parts.join("&")));

            debug!("Fragment flow: Returning tokens in URL fragment");
        }
        OAuthFlowType::Exchange => {
            // Exchange flow: Generate exchange token for backend to retrieve (for server-rendered apps)
            let exchange_token = generate_state_token(); // Reuse same random token generator

            let exchange_state = OAuthExchangeState {
                project_id: project.id,
                extension_name: extension_name.clone(),
                session_id: session_id.clone(),
                created_at: Utc::now(),
            };

            // Store exchange token in cache (5-minute TTL, single-use)
            state
                .oauth_exchange_store
                .insert(exchange_token.clone(), exchange_state)
                .await;

            // Add exchange token as query parameter
            redirect_url
                .query_pairs_mut()
                .append_pair("exchange_token", &exchange_token);

            // Pass through application's CSRF state
            if let Some(app_state) = oauth_state.application_state {
                redirect_url
                    .query_pairs_mut()
                    .append_pair("state", &app_state);
            }

            info!(
                "Exchange flow: Generated exchange token for session {}",
                session_id
            );
        }
    }

    // Set session cookie
    let cookie_value = format!(
        "rise_oauth_session={}; HttpOnly; Secure; SameSite=Lax; Max-Age={}; Path=/",
        session_id,
        90 * 24 * 60 * 60 // 90 days
    );

    // Build response with cookie
    let mut response = Redirect::to(redirect_url.as_str()).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie_value.parse().unwrap());

    Ok(response)
}

/// Exchange a temporary token for OAuth credentials (Exchange Flow)
///
/// POST /api/v1/projects/{project}/extensions/{extension}/oauth/exchange
///
/// Query params:
/// - exchange_token: Temporary exchange token from callback
///
/// This endpoint is called by backend applications that received an exchange_token
/// from the OAuth callback. They exchange it for the actual OAuth credentials.
/// Requires service account authentication (RISE_SERVICE_ACCOUNT_TOKEN).
pub async fn exchange_credentials(
    State(state): State<AppState>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Query(params): Query<super::models::ExchangeTokenRequest>,
) -> Result<Json<super::models::CredentialsResponse>, (StatusCode, String)> {
    debug!(
        "Exchange credentials request for project={}, extension={}",
        project_name, extension_name
    );

    // Get project
    let project = db_projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Retrieve and validate exchange token (single-use, 5-minute TTL)
    let exchange_state = state
        .oauth_exchange_store
        .get(&params.exchange_token)
        .await
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Invalid or expired exchange token".to_string(),
        ))?;

    // Invalidate exchange token immediately (single-use)
    state
        .oauth_exchange_store
        .invalidate(&params.exchange_token)
        .await;

    debug!(
        "Exchange token validated and invalidated for session {}",
        exchange_state.session_id
    );

    // Verify project and extension match
    if exchange_state.project_id != project.id || exchange_state.extension_name != extension_name {
        return Err((
            StatusCode::BAD_REQUEST,
            "Exchange token does not match project/extension".to_string(),
        ));
    }

    // Get tokens from database
    let token = user_oauth_tokens::get_by_session(
        &state.db_pool,
        project.id,
        &extension_name,
        &exchange_state.session_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "OAuth token not found for this session".to_string(),
    ))?;

    // Update last_accessed_at
    if let Err(e) = user_oauth_tokens::update_last_accessed(&state.db_pool, token.id).await {
        warn!("Failed to update last_accessed_at: {:?}", e);
    }

    // Decrypt tokens
    let encryption_provider = state.encryption_provider.as_ref().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Encryption provider not configured".to_string(),
    ))?;

    let access_token = encryption_provider
        .decrypt(&token.access_token_encrypted)
        .await
        .map_err(|e| {
            error!("Failed to decrypt access token: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to decrypt access token: {}", e),
            )
        })?;

    let refresh_token = match &token.refresh_token_encrypted {
        Some(refresh_token_encrypted) => Some(
            encryption_provider
                .decrypt(refresh_token_encrypted)
                .await
                .map_err(|e| {
                    error!("Failed to decrypt refresh token: {:?}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to decrypt refresh token: {}", e),
                    )
                })?,
        ),
        None => None,
    };

    info!(
        "Exchange successful for session {} on project {} extension {}",
        exchange_state.session_id, project_name, extension_name
    );

    Ok(Json(super::models::CredentialsResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_at: token.expires_at.unwrap_or_else(Utc::now),
        refresh_token, // Include refresh token for backend apps
    }))
}
