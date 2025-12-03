use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// JWT claims from Dex ID token
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,           // Subject (user ID from Dex)
    pub email: String,         // User email
    pub email_verified: bool,  // Email verification status
    pub iss: String,           // Issuer (Dex URL)
    pub aud: String,           // Audience (client ID)
    pub exp: usize,            // Expiration time
    pub iat: usize,            // Issued at
    #[serde(default)]
    pub name: Option<String>,  // User's full name
}

/// JWKS (JSON Web Key Set) response from Dex
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

/// Individual JSON Web Key
#[derive(Debug, Deserialize, Clone)]
struct Jwk {
    #[serde(rename = "use")]
    key_use: String,
    kty: String,
    kid: String,
    alg: String,
    n: String,
    e: String,
}

/// JWT validator that fetches and caches JWKS from Dex
pub struct JwtValidator {
    issuer: String,
    jwks_uri: String,
    client_id: String,
    keys: Arc<RwLock<HashMap<String, DecodingKey>>>,
    http_client: reqwest::Client,
}

impl JwtValidator {
    /// Create a new JWT validator
    pub fn new(issuer: String, client_id: String) -> Self {
        let jwks_uri = format!("{}/keys", issuer);

        Self {
            issuer,
            jwks_uri,
            client_id,
            keys: Arc::new(RwLock::new(HashMap::new())),
            http_client: reqwest::Client::new(),
        }
    }

    /// Fetch JWKS from Dex and cache the keys
    async fn fetch_jwks(&self) -> Result<()> {
        tracing::debug!("Fetching JWKS from {}", self.jwks_uri);

        let response = self
            .http_client
            .get(&self.jwks_uri)
            .send()
            .await
            .context("Failed to fetch JWKS")?;

        let jwks: JwksResponse = response
            .json()
            .await
            .context("Failed to parse JWKS response")?;

        let mut keys = self.keys.write().await;
        keys.clear();

        for jwk in jwks.keys {
            if jwk.kty == "RSA" && jwk.key_use == "sig" {
                let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                    .context("Failed to create decoding key from JWK")?;
                keys.insert(jwk.kid.clone(), decoding_key);
                tracing::debug!("Loaded JWK with kid: {}", jwk.kid);
            }
        }

        tracing::info!("Loaded {} signing keys from JWKS", keys.len());
        Ok(())
    }

    /// Get decoding key for a specific key ID
    async fn get_key(&self, kid: &str) -> Result<DecodingKey> {
        // Try to get from cache first
        {
            let keys = self.keys.read().await;
            if let Some(key) = keys.get(kid) {
                return Ok(key.clone());
            }
        }

        // Key not found, refresh JWKS and try again
        tracing::info!("Key {} not found in cache, refreshing JWKS", kid);
        self.fetch_jwks().await?;

        let keys = self.keys.read().await;
        keys.get(kid)
            .cloned()
            .ok_or_else(|| anyhow!("Key {} not found in JWKS", kid))
    }

    /// Validate a JWT token and extract claims
    pub async fn validate(&self, token: &str) -> Result<Claims> {
        // Decode header to get key ID
        let header = decode_header(token).context("Failed to decode JWT header")?;
        let kid = header
            .kid
            .ok_or_else(|| anyhow!("JWT header missing kid"))?;

        // Get the decoding key
        let key = self.get_key(&kid).await?;

        // Set up validation parameters
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&[&self.issuer]);

        // Validate and decode the token
        let token_data = decode::<Claims>(token, &key, &validation)
            .context("Failed to validate JWT token")?;

        Ok(token_data.claims)
    }

    /// Initialize by fetching JWKS on startup
    pub async fn init(&self) -> Result<()> {
        self.fetch_jwks().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_jwt_validator_creation() {
        let validator = JwtValidator::new(
            "http://localhost:5556/dex".to_string(),
            "rise-backend".to_string(),
        );
        assert_eq!(validator.issuer, "http://localhost:5556/dex");
        assert_eq!(validator.jwks_uri, "http://localhost:5556/dex/keys");
        assert_eq!(validator.client_id, "rise-backend");
    }
}
