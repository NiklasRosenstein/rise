use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Claims for Rise-issued ingress authentication JWTs
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IngressClaims {
    /// User ID from IdP
    pub sub: String,
    /// User email
    pub email: String,
    /// User name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Project name (for scoping)
    pub project: String,
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

    /// Sign a new ingress JWT scoped to a specific project
    ///
    /// # Arguments
    /// * `idp_claims` - Claims from the IdP JWT (must contain at least "sub" and "email")
    /// * `project_name` - The project name to scope this JWT to
    /// * `expiry_override` - Optional expiry timestamp (if None, uses default_expiry_seconds)
    pub fn sign_ingress_jwt(
        &self,
        idp_claims: &serde_json::Value,
        project_name: &str,
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

        let claims = IngressClaims {
            sub,
            email,
            name,
            project: project_name.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_signer() -> JwtSigner {
        // 32-byte secret encoded as base64
        let secret = BASE64.encode(b"this-is-a-32-byte-test-secret!");
        JwtSigner::new(
            &secret,
            "https://rise.test".to_string(),
            3600,
            vec!["sub".to_string(), "email".to_string(), "name".to_string()],
        )
        .unwrap()
    }

    #[test]
    fn test_sign_and_verify_jwt() {
        let signer = create_test_signer();

        let idp_claims = json!({
            "sub": "user123",
            "email": "user@example.com",
            "name": "Test User"
        });

        let token = signer.sign_ingress_jwt(&idp_claims, "myapp", None).unwrap();

        let claims = signer.verify_ingress_jwt(&token).unwrap();

        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.email, "user@example.com");
        assert_eq!(claims.name, Some("Test User".to_string()));
        assert_eq!(claims.project, "myapp");
        assert_eq!(claims.aud, "rise-ingress");
    }

    #[test]
    fn test_jwt_without_name_claim() {
        let secret = BASE64.encode(b"this-is-a-32-byte-test-secret!");
        let signer = JwtSigner::new(
            &secret,
            "https://rise.test".to_string(),
            3600,
            vec!["sub".to_string(), "email".to_string()], // no "name"
        )
        .unwrap();

        let idp_claims = json!({
            "sub": "user123",
            "email": "user@example.com",
            "name": "Test User"
        });

        let token = signer.sign_ingress_jwt(&idp_claims, "myapp", None).unwrap();

        let claims = signer.verify_ingress_jwt(&token).unwrap();

        assert_eq!(claims.name, None); // name should not be included
    }

    #[test]
    fn test_missing_required_claim() {
        let signer = create_test_signer();

        let idp_claims = json!({
            "sub": "user123"
            // missing email
        });

        let result = signer.sign_ingress_jwt(&idp_claims, "myapp", None);

        assert!(matches!(
            result,
            Err(JwtSignerError::MissingClaim(claim)) if claim == "email"
        ));
    }

    #[test]
    fn test_custom_expiry() {
        let signer = create_test_signer();

        let idp_claims = json!({
            "sub": "user123",
            "email": "user@example.com"
        });

        let custom_exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 7200;

        let token = signer
            .sign_ingress_jwt(&idp_claims, "myapp", Some(custom_exp))
            .unwrap();

        let claims = signer.verify_ingress_jwt(&token).unwrap();

        assert_eq!(claims.exp, custom_exp);
    }

    #[test]
    fn test_project_scoping() {
        let signer = create_test_signer();

        let idp_claims = json!({
            "sub": "user123",
            "email": "user@example.com"
        });

        let token1 = signer
            .sign_ingress_jwt(&idp_claims, "project1", None)
            .unwrap();
        let token2 = signer
            .sign_ingress_jwt(&idp_claims, "project2", None)
            .unwrap();

        let claims1 = signer.verify_ingress_jwt(&token1).unwrap();
        let claims2 = signer.verify_ingress_jwt(&token2).unwrap();

        assert_eq!(claims1.project, "project1");
        assert_eq!(claims2.project, "project2");
        assert_ne!(token1, token2);
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
