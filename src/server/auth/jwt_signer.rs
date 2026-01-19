use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD as BASE64URL};
use base64::Engine;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rsa::traits::PublicKeyParts;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Claims for Rise-issued JWTs (both UI and ingress authentication)
///
/// The `aud` claim determines the scope:
/// - For UI login: aud = Rise public URL (e.g., "https://rise.example.com")
/// - For project ingress: aud = project URL (e.g., "https://myapp.apps.rise.dev")
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiseClaims {
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
    /// Audience (Rise UI URL or project URL)
    pub aud: String,
}

/// Alias for backward compatibility
pub type IngressClaims = RiseClaims;

/// JWT signer supporting both HS256 (symmetric) and RS256 (asymmetric) algorithms
///
/// - HS256 is used for UI authentication (aud = Rise public URL)
/// - RS256 is used for project ingress authentication (aud = project URL)
///
/// The RS256 keys can be exposed via JWKS for deployed apps to validate tokens.
pub struct JwtSigner {
    // HS256 symmetric key for UI authentication
    hs256_encoding_key: EncodingKey,
    hs256_decoding_key: DecodingKey,
    
    // RS256 asymmetric key pair for ingress authentication
    rs256_encoding_key: Arc<EncodingKey>,
    rs256_decoding_key: Arc<DecodingKey>,
    rs256_public_key_pem: String,
    rs256_key_id: String,
    
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
    #[error("RSA key generation failed: {0}")]
    RsaKeyError(String),
    #[error("PEM encoding failed: {0}")]
    PemError(String),
}

impl JwtSigner {
    /// Create a new JWT signer with both HS256 and RS256 support
    ///
    /// # Arguments
    /// * `hs256_secret_base64` - Base64-encoded signing secret for HS256 (must be at least 32 bytes when decoded)
    /// * `issuer` - Issuer URL (typically the Rise backend URL)
    /// * `default_expiry_seconds` - Default expiration duration in seconds
    /// * `claims_to_include` - List of claim names to include from IdP token (e.g., ["sub", "email", "name"])
    ///
    /// This will generate a new RS256 key pair on initialization.
    pub fn new(
        hs256_secret_base64: &str,
        issuer: String,
        default_expiry_seconds: u64,
        claims_to_include: Vec<String>,
    ) -> Result<Self, JwtSignerError> {
        // Set up HS256 symmetric key
        let secret = BASE64.decode(hs256_secret_base64)?;

        if secret.len() < 32 {
            return Err(JwtSignerError::InvalidBase64(
                base64::DecodeError::InvalidLength(secret.len()),
            ));
        }

        let hs256_encoding_key = EncodingKey::from_secret(&secret);
        let hs256_decoding_key = DecodingKey::from_secret(&secret);

        // Generate RS256 key pair (2048-bit RSA key)
        use rsa::{RsaPrivateKey, RsaPublicKey};
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
        
        let mut rng = rand::thread_rng();
        let bits = 2048;
        let private_key = RsaPrivateKey::new(&mut rng, bits)
            .map_err(|e| JwtSignerError::RsaKeyError(e.to_string()))?;
        let public_key = RsaPublicKey::from(&private_key);

        // Encode keys to PEM format
        let private_key_pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .map_err(|e| JwtSignerError::PemError(e.to_string()))?
            .to_string();
        
        let public_key_pem = public_key
            .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
            .map_err(|e| JwtSignerError::PemError(e.to_string()))?;

        // Create encoding and decoding keys
        let rs256_encoding_key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
            .map_err(|e| JwtSignerError::RsaKeyError(e.to_string()))?;
        
        let rs256_decoding_key = DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
            .map_err(|e| JwtSignerError::RsaKeyError(e.to_string()))?;

        // Generate a key ID (SHA-256 hash of the public key)
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(public_key_pem.as_bytes());
        let hash = hasher.finalize();
        let rs256_key_id = format!("{:x}", hash)[..16].to_string();

        Ok(Self {
            hs256_encoding_key,
            hs256_decoding_key,
            rs256_encoding_key: Arc::new(rs256_encoding_key),
            rs256_decoding_key: Arc::new(rs256_decoding_key),
            rs256_public_key_pem: public_key_pem,
            rs256_key_id,
            issuer,
            default_expiry_seconds,
            claims_to_include,
        })
    }

    /// Get the RS256 public key in PEM format
    pub fn rs256_public_key_pem(&self) -> &str {
        &self.rs256_public_key_pem
    }

    /// Get the RS256 key ID
    pub fn rs256_key_id(&self) -> &str {
        &self.rs256_key_id
    }

    /// Generate JWKS (JSON Web Key Set) for RS256 public key
    ///
    /// This can be used by deployed applications to validate Rise-issued RS256 JWTs.
    pub fn generate_jwks(&self) -> Result<serde_json::Value, JwtSignerError> {
        // Parse the public key to extract n and e components
        use rsa::pkcs8::DecodePublicKey;
        use rsa::RsaPublicKey;
        
        let public_key = RsaPublicKey::from_public_key_pem(&self.rs256_public_key_pem)
            .map_err(|e| JwtSignerError::PemError(e.to_string()))?;
        
        // Extract n (modulus) and e (exponent) as base64url (RFC 7515 standard for JWK)
        let n = BASE64URL.encode(public_key.n().to_bytes_be());
        let e = BASE64URL.encode(public_key.e().to_bytes_be());

        Ok(serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": self.rs256_key_id,
                "n": n,
                "e": e,
            }]
        }))
    }

    /// Sign a new Rise JWT for UI authentication (HS256)
    ///
    /// This JWT is used for authenticating to the Rise UI itself.
    /// Uses HS256 symmetric encryption and sets aud to the Rise public URL.
    ///
    /// # Arguments
    /// * `idp_claims` - Claims from the IdP JWT (must contain at least "sub" and "email")
    /// * `user_id` - UUID of the user (for fetching team memberships)
    /// * `db_pool` - Database connection pool (for fetching team memberships)
    /// * `rise_public_url` - The Rise public URL (used as aud claim)
    /// * `expiry_override` - Optional expiry timestamp (if None, uses default_expiry_seconds)
    pub async fn sign_ui_jwt(
        &self,
        idp_claims: &serde_json::Value,
        user_id: uuid::Uuid,
        db_pool: &sqlx::PgPool,
        rise_public_url: &str,
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
        let groups = crate::db::teams::get_team_names_for_user(db_pool, user_id)
            .await
            .ok();

        let claims = RiseClaims {
            sub,
            email,
            name,
            groups,
            iat: now,
            exp,
            iss: self.issuer.clone(),
            aud: rise_public_url.to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(&header, &claims, &self.hs256_encoding_key)?;

        Ok(token)
    }

    /// Sign a new Rise JWT for project ingress authentication (RS256)
    ///
    /// This JWT is used for authenticating to deployed Rise projects.
    /// Uses RS256 asymmetric encryption and sets aud to the project URL.
    ///
    /// # Arguments
    /// * `idp_claims` - Claims from the IdP JWT (must contain at least "sub" and "email")
    /// * `user_id` - UUID of the user (for fetching team memberships)
    /// * `db_pool` - Database connection pool (for fetching team memberships)
    /// * `project_url` - The project URL (used as aud claim, e.g., "https://myapp.apps.rise.dev")
    /// * `expiry_override` - Optional expiry timestamp (if None, uses default_expiry_seconds)
    pub async fn sign_ingress_jwt(
        &self,
        idp_claims: &serde_json::Value,
        user_id: uuid::Uuid,
        db_pool: &sqlx::PgPool,
        project_url: &str,
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
        let groups = crate::db::teams::get_team_names_for_user(db_pool, user_id)
            .await
            .ok();

        let claims = RiseClaims {
            sub,
            email,
            name,
            groups,
            iat: now,
            exp,
            iss: self.issuer.clone(),
            aud: project_url.to_string(),
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.rs256_key_id.clone());
        let token = encode(&header, &claims, &self.rs256_encoding_key)?;

        Ok(token)
    }

    /// Verify and decode a Rise JWT (supports both HS256 UI tokens and RS256 ingress tokens)
    ///
    /// Returns the claims if the JWT is valid, or an error if verification fails.
    /// Automatically detects the algorithm from the JWT header.
    ///
    /// # Arguments
    /// * `token` - The JWT token string
    /// * `expected_aud` - The expected audience claim (e.g., Rise public URL or project URL)
    pub fn verify_jwt(
        &self,
        token: &str,
        expected_aud: &str,
    ) -> Result<RiseClaims, JwtSignerError> {
        // Decode header to check algorithm
        let header = jsonwebtoken::decode_header(token)?;
        
        match header.alg {
            Algorithm::HS256 => {
                let mut validation = Validation::new(Algorithm::HS256);
                validation.set_issuer(&[&self.issuer]);
                validation.set_audience(&[expected_aud]);

                let token_data = decode::<RiseClaims>(token, &self.hs256_decoding_key, &validation)?;
                Ok(token_data.claims)
            }
            Algorithm::RS256 => {
                let mut validation = Validation::new(Algorithm::RS256);
                validation.set_issuer(&[&self.issuer]);
                validation.set_audience(&[expected_aud]);

                let token_data = decode::<RiseClaims>(token, &self.rs256_decoding_key, &validation)?;
                Ok(token_data.claims)
            }
            _ => Err(JwtSignerError::SigningFailed(
                jsonwebtoken::errors::Error::from(
                    jsonwebtoken::errors::ErrorKind::InvalidAlgorithm
                )
            )),
        }
    }

    /// Verify and decode an ingress JWT (backward compatibility wrapper)
    ///
    /// This is a backward compatibility wrapper that verifies tokens
    /// without strict audience validation.
    ///
    /// # Security Note
    /// This method skips audience validation for backward compatibility during migration.
    /// It should only be used in the ingress_auth handler where project access is
    /// validated separately. For new code, use `verify_jwt` with explicit audience.
    ///
    /// # Deprecated
    /// Use `verify_jwt` with the actual project URL as audience instead.
    /// This method will be removed after all existing tokens have expired (2024-Q2).
    #[deprecated(note = "Use verify_jwt with project URL as audience")]
    pub fn verify_ingress_jwt(&self, token: &str) -> Result<RiseClaims, JwtSignerError> {
        // Try with new project URL audience first, then fall back to legacy "rise-ingress"
        // This provides backward compatibility during migration
        
        // Since we don't know the project URL here, we need to be more lenient
        // Just verify the signature and issuer, but skip audience validation
        let header = jsonwebtoken::decode_header(token)?;
        
        match header.alg {
            Algorithm::HS256 => {
                let mut validation = Validation::new(Algorithm::HS256);
                validation.set_issuer(&[&self.issuer]);
                validation.validate_aud = false; // Skip aud validation for backward compatibility

                let token_data = decode::<RiseClaims>(token, &self.hs256_decoding_key, &validation)?;
                Ok(token_data.claims)
            }
            Algorithm::RS256 => {
                let mut validation = Validation::new(Algorithm::RS256);
                validation.set_issuer(&[&self.issuer]);
                validation.validate_aud = false; // Skip aud validation for backward compatibility

                let token_data = decode::<RiseClaims>(token, &self.rs256_decoding_key, &validation)?;
                Ok(token_data.claims)
            }
            _ => Err(JwtSignerError::SigningFailed(
                jsonwebtoken::errors::Error::from(
                    jsonwebtoken::errors::ErrorKind::InvalidAlgorithm
                )
            )),
        }
    }
}

// Note: Full integration tests require a database connection and are in the tests/ directory.
// These unit tests verify basic JWT signing and verification without database access.

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
    fn test_create_signer() {
        let signer = create_test_signer();
        
        // Verify RS256 keys were generated
        assert!(!signer.rs256_public_key_pem().is_empty());
        assert!(!signer.rs256_key_id().is_empty());
        assert_eq!(signer.rs256_key_id().len(), 16); // Should be 16 hex chars
    }

    #[test]
    fn test_generate_jwks() {
        let signer = create_test_signer();
        
        let jwks = signer.generate_jwks().unwrap();
        
        // Verify JWKS structure
        assert!(jwks.get("keys").is_some());
        let keys = jwks.get("keys").unwrap().as_array().unwrap();
        assert_eq!(keys.len(), 1);
        
        let key = &keys[0];
        assert_eq!(key.get("kty").unwrap().as_str().unwrap(), "RSA");
        assert_eq!(key.get("use").unwrap().as_str().unwrap(), "sig");
        assert_eq!(key.get("alg").unwrap().as_str().unwrap(), "RS256");
        assert_eq!(key.get("kid").unwrap().as_str().unwrap(), signer.rs256_key_id());
        assert!(key.get("n").is_some());
        assert!(key.get("e").is_some());
    }

    #[test]
    #[allow(deprecated)]
    fn test_verify_hs256_jwt_backward_compat() {
        let signer = create_test_signer();

        // Create an HS256 JWT manually for testing backward compatibility
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = RiseClaims {
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
        let token = encode(&header, &claims, &signer.hs256_encoding_key).unwrap();

        // Use deprecated method for backward compatibility test
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
    fn test_verify_rs256_jwt() {
        let signer = create_test_signer();

        // Create an RS256 JWT for testing
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = RiseClaims {
            sub: "user456".to_string(),
            email: "user2@example.com".to_string(),
            name: None,
            groups: None,
            iat: now,
            exp: now + 3600,
            iss: "https://rise.test".to_string(),
            aud: "https://myapp.apps.rise.dev".to_string(),
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(signer.rs256_key_id().to_string());
        let token = encode(&header, &claims, &signer.rs256_encoding_key).unwrap();

        // Verify with expected audience
        let verified_claims = signer.verify_jwt(&token, "https://myapp.apps.rise.dev").unwrap();

        assert_eq!(verified_claims.sub, "user456");
        assert_eq!(verified_claims.email, "user2@example.com");
        assert_eq!(verified_claims.aud, "https://myapp.apps.rise.dev");
    }

    #[test]
    fn test_verify_jwt_wrong_audience() {
        let signer = create_test_signer();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = RiseClaims {
            sub: "user789".to_string(),
            email: "user3@example.com".to_string(),
            name: None,
            groups: None,
            iat: now,
            exp: now + 3600,
            iss: "https://rise.test".to_string(),
            aud: "https://myapp.apps.rise.dev".to_string(),
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(signer.rs256_key_id().to_string());
        let token = encode(&header, &claims, &signer.rs256_encoding_key).unwrap();

        // Try to verify with wrong audience - should fail
        let result = signer.verify_jwt(&token, "https://wrongapp.apps.rise.dev");
        assert!(result.is_err());
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
