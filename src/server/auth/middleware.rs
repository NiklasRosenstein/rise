use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose, Engine as _};
use jsonwebtoken::decode_header;
use serde::Deserialize;
use std::collections::HashMap;

use crate::db::{service_accounts, users, User};
use crate::server::auth::cookie_helpers;
use crate::server::state::AppState;

/// Check if a JWT issuer is a Rise-issued JWT
///
/// Rise JWTs have `iss` set to the Rise public URL (e.g., "https://rise.example.com").
/// This helper checks for exact match or scheme prefix match.
fn is_rise_issued_jwt(issuer: &str, public_url: &str) -> bool {
    // Exact match
    if issuer == public_url {
        return true;
    }

    // Check if issuer starts with the public_url's base (handles port differences)
    if let Some(public_base) = public_url.strip_suffix(|c: char| c.is_ascii_digit() || c == ':') {
        if issuer.starts_with(public_base) {
            return true;
        }
    }

    false
}

/// Extract Bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    Some(auth_header[7..].to_string())
}

/// Extract Rise JWT from cookie
fn extract_rise_jwt_from_cookie(headers: &HeaderMap) -> Option<String> {
    cookie_helpers::extract_rise_jwt_cookie(headers)
}

/// Minimal JWT claims structure just to peek at the issuer
#[derive(Debug, Deserialize)]
struct MinimalClaims {
    iss: String,
}

/// Authenticate as a service account using external OIDC provider
async fn authenticate_service_account(
    state: &AppState,
    token: &str,
    issuer: &str,
) -> Result<User, (StatusCode, String)> {
    // Find all service accounts with this issuer
    let service_accounts = service_accounts::find_by_issuer(&state.db_pool, issuer)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find service accounts by issuer: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    if service_accounts.is_empty() {
        tracing::warn!("No service accounts found for issuer: {}", issuer);
        return Err((
            StatusCode::UNAUTHORIZED,
            "No service accounts configured for this issuer".to_string(),
        ));
    }

    // Validate all service accounts and collect matches
    let mut matching_accounts = Vec::new();

    for sa in &service_accounts {
        // Convert JSONB claims to HashMap
        let claims: HashMap<String, String> =
            serde_json::from_value(sa.claims.clone()).map_err(|e| {
                tracing::error!("Failed to deserialize service account claims: {:#}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Invalid service account configuration".to_string(),
                )
            })?;

        // Try to validate with this service account's claims
        if state
            .jwt_validator
            .validate(token, issuer, &claims)
            .await
            .is_ok()
        {
            matching_accounts.push(sa);
        }
    }

    // Check for collisions
    if matching_accounts.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "No service account matched the provided token claims".to_string(),
        ));
    }

    if matching_accounts.len() > 1 {
        let sa_ids: Vec<String> = matching_accounts
            .iter()
            .map(|sa| sa.id.to_string())
            .collect();
        tracing::error!(
            "Multiple service accounts matched JWT: {:?}. This indicates ambiguous claim configuration.",
            sa_ids
        );
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Multiple service accounts ({}) matched this token. \
                 This indicates ambiguous claim configuration. \
                 Each service account must have unique claim requirements.",
                matching_accounts.len()
            ),
        ));
    }

    // Exactly one match - authenticate
    let sa = matching_accounts[0];
    tracing::info!(
        "Service account authenticated: {} for project {}",
        sa.id,
        sa.project_id
    );

    let user = users::find_by_id(&state.db_pool, sa.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find user for service account: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?
        .ok_or_else(|| {
            tracing::error!("Service account user not found: {}", sa.user_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Service account user not found".to_string(),
            )
        })?;

    Ok(user)
}

/// Authentication middleware that validates JWT and injects User into request extensions
/// Supports both user authentication (via configured OIDC provider) and service account authentication (via external OIDC providers)
///
/// Authentication methods (in order of precedence):
/// 1. Rise JWT from `rise_jwt` cookie (HS256 for UI, RS256 for ingress)
/// 2. IdP token from Authorization Bearer header (for backward compatibility)
pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    tracing::debug!(
        "Auth middleware: validating request to {}",
        req.uri().path()
    );

    // Try to extract token from cookie first (new primary method)
    let token = if let Some(cookie_token) = extract_rise_jwt_from_cookie(&headers) {
        tracing::debug!(
            "Auth middleware: found Rise JWT in cookie (length={})",
            cookie_token.len()
        );
        cookie_token
    } else if let Some(bearer_token) = extract_bearer_token(&headers) {
        tracing::debug!(
            "Auth middleware: found Bearer token in Authorization header (length={})",
            bearer_token.len()
        );
        bearer_token
    } else {
        tracing::warn!("Auth middleware: no authentication token found");
        return Err((
            StatusCode::UNAUTHORIZED,
            "Missing authentication token (cookie or Authorization header)".to_string(),
        ));
    };

    // Peek at the issuer to determine authentication method
    let issuer = {
        // Decode header to check if JWT is well-formed
        decode_header(&token).map_err(|e| {
            tracing::warn!("Failed to decode JWT header: {:#}", e);
            (
                StatusCode::UNAUTHORIZED,
                format!("Invalid token format: {}", e),
            )
        })?;

        // Decode payload without validation to peek at issuer
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err((StatusCode::UNAUTHORIZED, "Invalid JWT format".to_string()));
        }

        let payload = parts[1];
        let decoded = general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|e| {
                tracing::warn!("Failed to decode JWT payload: {:#}", e);
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid token encoding".to_string(),
                )
            })?;

        let claims: MinimalClaims = serde_json::from_slice(&decoded).map_err(|e| {
            tracing::warn!("Failed to parse JWT claims: {:#}", e);
            (StatusCode::UNAUTHORIZED, "Invalid token claims".to_string())
        })?;

        claims.iss
    };

    tracing::debug!(
        "Auth middleware: token issuer='{}', configured issuer='{}', rise public_url='{}'",
        issuer,
        state.auth_settings.issuer,
        state.public_url
    );

    let user = if is_rise_issued_jwt(&issuer, &state.public_url) {
        // Rise-issued JWT (HS256 or RS256) - validate with JwtSigner
        // This is the primary authentication path for both CLI and UI users
        tracing::debug!("Auth middleware: authenticating with Rise-issued JWT");

        // Verify Rise JWT (skips audience validation for now - we trust our own JWTs)
        let claims = state.jwt_signer.verify_jwt_skip_aud(&token).map_err(|e| {
            tracing::warn!("Auth middleware: Rise JWT validation failed: {:#}", e);
            (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e))
        })?;

        tracing::debug!("Auth middleware: Rise JWT validation successful");

        // Extract email from Rise JWT claims
        let email = &claims.email;

        tracing::debug!("Rise JWT validated for user: {}", email);

        users::find_or_create(&state.db_pool, email)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find/create user: {:#}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?
    } else {
        // External issuer - service account authentication
        tracing::debug!("Authenticating as service account from issuer: {}", issuer);
        authenticate_service_account(&state, &token, &issuer).await?
    };

    tracing::debug!("User authenticated: {} ({})", user.email, user.id);

    // Insert user into request extensions for handlers to access
    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

/// Optional authentication middleware - allows unauthenticated requests but injects User if token is present
#[allow(dead_code)]
pub async fn optional_auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Response {
    // Try to extract token from cookie first, then Authorization header
    let token = extract_rise_jwt_from_cookie(&headers).or_else(|| extract_bearer_token(&headers));

    if let Some(token) = token {
        // Peek at issuer
        if decode_header(&token).is_ok() {
            let parts: Vec<&str> = token.split('.').collect();
            if parts.len() == 3 {
                if let Ok(decoded) = general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
                    if let Ok(claims) = serde_json::from_slice::<MinimalClaims>(&decoded) {
                        // Only accept Rise-issued JWTs for optional auth
                        if is_rise_issued_jwt(&claims.iss, &state.public_url) {
                            // Try to validate Rise JWT
                            if let Ok(rise_claims) = state.jwt_signer.verify_jwt_skip_aud(&token) {
                                let email = &rise_claims.email;
                                if let Ok(user) = users::find_or_create(&state.db_pool, email).await
                                {
                                    req.extensions_mut().insert(user);
                                }
                            }
                        }
                        // Note: Service account tokens are not supported in optional auth
                    }
                }
            }
        }
    }

    // Continue regardless of authentication status
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_bearer_token_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_static("Bearer my-token-here"),
        );

        let token = extract_bearer_token(&headers);
        assert_eq!(token, Some("my-token-here".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        let headers = HeaderMap::new();
        let result = extract_bearer_token(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_bearer_token_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_static("Basic user:pass"));

        let result = extract_bearer_token(&headers);
        assert_eq!(result, None);
    }
}
