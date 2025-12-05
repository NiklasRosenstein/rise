use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::db::users;
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

/// Authentication middleware that validates JWT and injects User into request extensions
pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    // Extract token from Authorization header
    let token = extract_bearer_token(&headers)?;

    // Validate JWT and extract claims
    let claims = state.jwt_validator.validate(&token).await.map_err(|e| {
        tracing::warn!("JWT validation failed: {}", e);
        (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e))
    })?;

    tracing::debug!("JWT validated for user: {}", claims.email);

    // Find or create user in database based on email from claims
    let user = users::find_or_create(&state.db_pool, &claims.email)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find/create user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    tracing::debug!("User found/created: {} ({})", user.email, user.id);

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
        // Try to validate token and find/create user
        if let Ok(claims) = state.jwt_validator.validate(&token).await {
            if let Ok(user) = users::find_or_create(&state.db_pool, &claims.email).await {
                req.extensions_mut().insert(user);
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
