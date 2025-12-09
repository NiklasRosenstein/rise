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
use crate::state::AppState;

/// Extract Bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Result<String, (StatusCode, String)> {
    let auth_header = headers
        .get("Authorization")
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "Missing Authorization header".to_string(),
        ))?
        .to_str()
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid Authorization header".to_string(),
            )
        })?;

    if !auth_header.starts_with("Bearer ") {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid Authorization header format".to_string(),
        ));
    }

    Ok(auth_header[7..].to_string())
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
            tracing::error!("Failed to find service accounts by issuer: {}", e);
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
                tracing::error!("Failed to deserialize service account claims: {}", e);
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
            tracing::error!("Failed to find user for service account: {}", e);
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

    // Extract token from Authorization header
    let token = extract_bearer_token(&headers)?;
    tracing::debug!(
        "Auth middleware: extracted bearer token (length={})",
        token.len()
    );

    // Peek at the issuer to determine authentication method
    let issuer = {
        // Decode header to check if JWT is well-formed
        decode_header(&token).map_err(|e| {
            tracing::warn!("Failed to decode JWT header: {}", e);
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
                tracing::warn!("Failed to decode JWT payload: {}", e);
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid token encoding".to_string(),
                )
            })?;

        let claims: MinimalClaims = serde_json::from_slice(&decoded).map_err(|e| {
            tracing::warn!("Failed to parse JWT claims: {}", e);
            (StatusCode::UNAUTHORIZED, "Invalid token claims".to_string())
        })?;

        claims.iss
    };

    tracing::debug!(
        "Auth middleware: token issuer='{}', configured issuer='{}'",
        issuer,
        state.auth_settings.issuer
    );

    let user = if issuer == state.auth_settings.issuer {
        // User authentication via configured OIDC provider
        tracing::debug!("Auth middleware: authenticating as user via configured OIDC provider");

        // Build expected claims for user auth (just validate aud matches client_id)
        let mut expected_claims = HashMap::new();
        expected_claims.insert("aud".to_string(), state.auth_settings.client_id.clone());

        tracing::debug!(
            "Auth middleware: validating JWT with issuer='{}', expected aud='{}'",
            state.auth_settings.issuer,
            state.auth_settings.client_id
        );

        let claims_value = state
            .jwt_validator
            .validate(&token, &state.auth_settings.issuer, &expected_claims)
            .await
            .map_err(|e| {
                tracing::warn!("Auth middleware: JWT validation failed: {}", e);
                (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e))
            })?;

        tracing::debug!("Auth middleware: JWT validation successful");

        // Deserialize to typed Claims to get email
        let claims: crate::auth::jwt::Claims =
            serde_json::from_value(claims_value).map_err(|e| {
                tracing::warn!("Failed to parse user claims: {}", e);
                (
                    StatusCode::UNAUTHORIZED,
                    format!("Invalid token claims: {}", e),
                )
            })?;

        tracing::debug!("JWT validated for user: {}", claims.email);

        users::find_or_create(&state.db_pool, &claims.email)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find/create user: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?
    } else {
        // Service account authentication (new flow)
        tracing::debug!("Authenticating as service account from issuer: {}", issuer);
        authenticate_service_account(&state, &token, &issuer).await?
    };

    tracing::debug!("User authenticated: {} ({})", user.email, user.id);

    // Insert user into request extensions for handlers to access
    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

/// Optional authentication middleware - allows unauthenticated requests but injects User if token is present
pub async fn optional_auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Response {
    // Try to extract token
    if let Ok(token) = extract_bearer_token(&headers) {
        // Build expected claims for user auth
        let mut expected_claims = HashMap::new();
        expected_claims.insert("aud".to_string(), state.auth_settings.client_id.clone());

        // Try to validate token and find/create user
        if let Ok(claims_value) = state
            .jwt_validator
            .validate(&token, &state.auth_settings.issuer, &expected_claims)
            .await
        {
            if let Ok(claims) = serde_json::from_value::<crate::auth::jwt::Claims>(claims_value) {
                if let Ok(user) = users::find_or_create(&state.db_pool, &claims.email).await {
                    req.extensions_mut().insert(user);
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

        let token = extract_bearer_token(&headers).unwrap();
        assert_eq!(token, "my-token-here");
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        let headers = HeaderMap::new();
        let result = extract_bearer_token(&headers);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_bearer_token_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_static("Basic user:pass"));

        let result = extract_bearer_token(&headers);
        assert!(result.is_err());
    }
}
