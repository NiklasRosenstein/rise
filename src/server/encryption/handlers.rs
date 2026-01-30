use crate::server::auth::jwt::Claims;
use crate::server::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::{Deserialize, Serialize};

/// Request to encrypt a plaintext value
#[derive(Debug, Deserialize)]
pub struct EncryptRequest {
    pub plaintext: String,
}

/// Response containing encrypted value
#[derive(Debug, Serialize)]
pub struct EncryptResponse {
    pub encrypted: String,
}

/// Error responses for encrypt endpoint
#[derive(Debug, thiserror::Error)]
pub enum EncryptError {
    #[error("Rate limit exceeded. Retry after {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    #[error("Encryption provider not configured")]
    ProviderNotConfigured,

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
}

impl IntoResponse for EncryptError {
    fn into_response(self) -> axum::response::Response {
        match self {
            EncryptError::RateLimitExceeded { retry_after } => {
                let mut response =
                    (StatusCode::TOO_MANY_REQUESTS, self.to_string()).into_response();
                let _ = response.headers_mut().try_insert(
                    axum::http::header::RETRY_AFTER,
                    axum::http::HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                );
                response
            }
            EncryptError::ProviderNotConfigured => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string()).into_response()
            }
            EncryptError::EncryptionFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

/// POST /api/v1/encrypt - Encrypt a plaintext value
///
/// Rate limited to 100 requests per hour per user.
pub async fn encrypt_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<EncryptRequest>,
) -> Result<Json<EncryptResponse>, EncryptError> {
    // Check rate limit (100 req/hour per user)
    let key = format!("encrypt:{}", claims.sub);
    let count = state.encrypt_rate_limiter.get(&key).await.unwrap_or(0);

    if count >= 100 {
        return Err(EncryptError::RateLimitExceeded { retry_after: 3600 });
    }

    // Increment counter
    state
        .encrypt_rate_limiter
        .insert(key.clone(), count + 1)
        .await;

    // Encrypt using encryption provider
    let encryption_provider = state
        .encryption_provider
        .as_ref()
        .ok_or(EncryptError::ProviderNotConfigured)?;

    let encrypted = encryption_provider
        .encrypt(&req.plaintext)
        .await
        .map_err(|e| EncryptError::EncryptionFailed(e.to_string()))?;

    Ok(Json(EncryptResponse { encrypted }))
}
