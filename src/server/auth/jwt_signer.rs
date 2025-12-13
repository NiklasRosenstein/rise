use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Claims for Rise-issued ingress authentication JWTs
///
/// Note: These JWTs are NOT scoped to specific projects because the cookie
/// is set at the rise.dev domain and shared across all *.apps.rise.dev subdomains.
/// Project access is validated separately in the ingress_auth handler.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IngressClaims {
    /// User ID from IdP
    pub sub: String,
    /// User email
    pub email: String,
    /// User name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Rise team names the user is a member of (ALL teams, not just IdP-managed)
    /// Used for authorization and audit logging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
    /// Issued at timestamp
    pub iat: u64,
    /// Expiration timestamp
    pub exp: u64,
    /// Issuer (Rise backend URL)
    pub iss: String,
    /// Audience (always "rise-ingress")
    pub aud: String,
}

/// JWT signer for ingress authentication tokens
pub struct JwtSigner {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    default_expiry_seconds: u64,
    claims_to_include: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum JwtSignerError {
    #[error("Invalid base64 secret: {0}")]
    InvalidBase64(#[from] base64::DecodeError),
    #[error("JWT signing failed: {0}")]
    SigningFailed(#[from] jsonwebtoken::errors::Error),
    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),
    #[error("Missing required claim: {0}")]
    MissingClaim(String),
}

impl JwtSigner {
    /// Create a new JWT signer
    ///
    /// # Arguments
    /// * `secret_base64` - Base64-encoded signing secret (must be at least 32 bytes when decoded)
    /// * `issuer` - Issuer URL (typically the Rise backend URL)
    /// * `default_expiry_seconds` - Default expiration duration in seconds
    /// * `claims_to_include` - List of claim names to include from IdP token (e.g., ["sub", "email", "name"])
    pub fn new(
        secret_base64: &str,
        issuer: String,
        default_expiry_seconds: u64,
        claims_to_include: Vec<String>,
    ) -> Result<Self, JwtSignerError> {
        let secret = BASE64.decode(secret_base64)?;

        if secret.len() < 32 {
            return Err(JwtSignerError::InvalidBase64(
                base64::DecodeError::InvalidLength(secret.len()),
            ));
        }

        let encoding_key = EncodingKey::from_secret(&secret);
        let decoding_key = DecodingKey::from_secret(&secret);

        Ok(Self {
            encoding_key,
            decoding_key,
            issuer,
            default_expiry_seconds,
            claims_to_include,
        })
    }

    /// Sign a new ingress JWT for authenticated users
    ///
    /// Note: This JWT is NOT scoped to a specific project because the cookie is shared
    /// across all subdomains. Project access is validated in the ingress_auth handler.
    ///
    /// # Arguments
    /// * `idp_claims` - Claims from the IdP JWT (must contain at least "sub" and "email")
    /// * `user_id` - UUID of the user (for fetching team memberships)
    /// * `db_pool` - Database connection pool (for fetching team memberships)
    /// * `expiry_override` - Optional expiry timestamp (if None, uses default_expiry_seconds)
    pub async fn sign_ingress_jwt(
        &self,
        idp_claims: &serde_json::Value,
        user_id: uuid::Uuid,
        db_pool: &sqlx::PgPool,
        expiry_override: Option<u64>,
    ) -> Result<String, JwtSignerError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let exp = expiry_override.unwrap_or_else(|| now + self.default_expiry_seconds);

        // Extract required claims
        let sub = idp_claims
            .get("sub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JwtSignerError::MissingClaim("sub".to_string()))?
            .to_string();

        let email = idp_claims
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JwtSignerError::MissingClaim("email".to_string()))?
            .to_string();

        // Extract optional name claim if requested
        let name = if self.claims_to_include.contains(&"name".to_string()) {
            idp_claims
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        // Fetch user's team memberships for groups claim
        // This includes ALL teams (both IdP-managed and regular teams)
        let groups = crate::db::teams::get_team_names_for_user(db_pool, user_id)
            .await
            .ok(); // Ignore errors, groups claim is optional

        let claims = IngressClaims {
            sub,
            email,
            name,
            groups,
            iat: now,
            exp,
            iss: self.issuer.clone(),
            aud: "rise-ingress".to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(&header, &claims, &self.encoding_key)?;

        Ok(token)
    }

    /// Verify and decode an ingress JWT
    ///
    /// Returns the claims if the JWT is valid, or an error if verification fails
    pub fn verify_ingress_jwt(&self, token: &str) -> Result<IngressClaims, JwtSignerError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&["rise-ingress"]);

        let token_data = decode::<IngressClaims>(token, &self.decoding_key, &validation)?;

        Ok(token_data.claims)
    }
}

// Note: Tests are commented out because they require a database connection
// The sign_ingress_jwt function is now async and requires a database pool
// Integration tests should be used instead to test the full authentication flow

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_signer() -> JwtSigner {
        // Exactly 32 bytes encoded as base64
        let secret = BASE64.encode([0u8; 32]);
        JwtSigner::new(
            &secret,
            "https://rise.test".to_string(),
            3600,
            vec!["sub".to_string(), "email".to_string(), "name".to_string()],
        )
        .unwrap()
    }

    #[test]
    fn test_verify_jwt() {
        let signer = create_test_signer();

        // Create a JWT manually for testing verification
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = IngressClaims {
            sub: "user123".to_string(),
            email: "user@example.com".to_string(),
            name: Some("Test User".to_string()),
            groups: Some(vec!["team1".to_string(), "team2".to_string()]),
            iat: now,
            exp: now + 3600,
            iss: "https://rise.test".to_string(),
            aud: "rise-ingress".to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(&header, &claims, &signer.encoding_key).unwrap();

        let verified_claims = signer.verify_ingress_jwt(&token).unwrap();

        assert_eq!(verified_claims.sub, "user123");
        assert_eq!(verified_claims.email, "user@example.com");
        assert_eq!(verified_claims.name, Some("Test User".to_string()));
        assert_eq!(
            verified_claims.groups,
            Some(vec!["team1".to_string(), "team2".to_string()])
        );
        assert_eq!(verified_claims.aud, "rise-ingress");
    }

    #[test]
    fn test_invalid_secret_length() {
        let short_secret = BASE64.encode(b"short"); // Less than 32 bytes

        let result = JwtSigner::new(
            &short_secret,
            "https://rise.test".to_string(),
            3600,
            vec!["sub".to_string(), "email".to_string()],
        );

        assert!(result.is_err());
    }
}
