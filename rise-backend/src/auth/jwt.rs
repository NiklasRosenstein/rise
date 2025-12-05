use anyhow::{anyhow, Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// JWT claims from Dex ID token
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,          // Subject (user ID from Dex)
    pub email: String,        // User email
    pub email_verified: bool, // Email verification status
    pub iss: String,          // Issuer (Dex URL)
    pub aud: String,          // Audience (client ID)
    pub exp: usize,           // Expiration time
    pub iat: usize,           // Issued at
    #[serde(default)]
    pub name: Option<String>, // User's full name
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
        let token_data =
            decode::<Claims>(token, &key, &validation).context("Failed to validate JWT token")?;

        Ok(token_data.claims)
    }

    /// Initialize by fetching JWKS on startup
    pub async fn init(&self) -> Result<()> {
        self.fetch_jwks().await
    }
}

/// OIDC Discovery document
#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    jwks_uri: String,
}

/// JWKS cache entry for external issuers
#[derive(Clone)]
struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
    ttl: Duration,
}

impl JwksCache {
    fn new(keys: HashMap<String, DecodingKey>) -> Self {
        Self {
            keys,
            fetched_at: Instant::now(),
            ttl: Duration::from_secs(3600), // 1 hour default TTL
        }
    }

    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

/// JWT validator for external OIDC providers (GitLab, GitHub, etc.)
/// Supports multiple issuers with per-issuer JWKS caching
pub struct ExternalJwtValidator {
    jwks_cache: Arc<RwLock<HashMap<String, JwksCache>>>,
    http_client: reqwest::Client,
}

impl ExternalJwtValidator {
    pub fn new() -> Self {
        Self {
            jwks_cache: Arc::new(RwLock::new(HashMap::new())),
            http_client: reqwest::Client::new(),
        }
    }

    /// Discover JWKS URI from OIDC issuer
    async fn discover_jwks_uri(&self, issuer_url: &str) -> Result<String> {
        let discovery_url = format!("{}/.well-known/openid-configuration", issuer_url);

        tracing::debug!("Discovering OIDC configuration from {}", discovery_url);

        let response = self
            .http_client
            .get(&discovery_url)
            .send()
            .await
            .context("Failed to fetch OIDC discovery document")?;

        let discovery: OidcDiscovery = response
            .json()
            .await
            .context("Failed to parse OIDC discovery document")?;

        Ok(discovery.jwks_uri)
    }

    /// Fetch JWKS from issuer
    async fn fetch_jwks(&self, jwks_uri: &str) -> Result<HashMap<String, DecodingKey>> {
        tracing::debug!("Fetching JWKS from {}", jwks_uri);

        let response = self
            .http_client
            .get(jwks_uri)
            .send()
            .await
            .context("Failed to fetch JWKS")?;

        let jwks: JwksResponse = response
            .json()
            .await
            .context("Failed to parse JWKS response")?;

        let mut keys = HashMap::new();

        for jwk in jwks.keys {
            if jwk.kty == "RSA" && jwk.key_use == "sig" {
                let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                    .context("Failed to create decoding key from JWK")?;
                keys.insert(jwk.kid.clone(), decoding_key);
                tracing::debug!("Loaded JWK with kid: {}", jwk.kid);
            }
        }

        tracing::info!("Loaded {} signing keys from JWKS", keys.len());
        Ok(keys)
    }

    /// Get JWKS for an issuer (with caching)
    async fn get_jwks(&self, issuer_url: &str) -> Result<HashMap<String, DecodingKey>> {
        // Check if cache exists and is still valid
        {
            let cache = self.jwks_cache.read().await;
            if let Some(cached) = cache.get(issuer_url) {
                if !cached.is_expired() {
                    tracing::debug!("Using cached JWKS for {}", issuer_url);
                    return Ok(cached.keys.clone());
                } else {
                    tracing::debug!("JWKS cache expired for {}", issuer_url);
                }
            }
        }

        // Cache miss or expired - fetch JWKS
        tracing::info!("Fetching fresh JWKS for {}", issuer_url);

        // Discover JWKS URI
        let jwks_uri = self.discover_jwks_uri(issuer_url).await?;

        // Fetch JWKS
        let keys = self.fetch_jwks(&jwks_uri).await?;

        // Update cache
        {
            let mut cache = self.jwks_cache.write().await;
            cache.insert(issuer_url.to_string(), JwksCache::new(keys.clone()));
        }

        Ok(keys)
    }

    /// Validate custom claims (exact matching)
    fn validate_custom_claims(
        jwt_claims: &serde_json::Value,
        expected_claims: &HashMap<String, String>,
    ) -> Result<()> {
        let claims_obj = jwt_claims
            .as_object()
            .ok_or_else(|| anyhow!("JWT claims is not an object"))?;

        for (key, expected_value) in expected_claims {
            let actual_value = claims_obj
                .get(key)
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Claim '{}' not found or not a string", key))?;

            if actual_value != expected_value {
                return Err(anyhow!(
                    "Claim mismatch: '{}' expected '{}', got '{}'",
                    key,
                    expected_value,
                    actual_value
                ));
            }
        }

        Ok(())
    }

    /// Validate a JWT token from an external OIDC provider
    pub async fn validate(
        &self,
        token: &str,
        issuer_url: &str,
        expected_claims: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        // Decode header to get key ID
        let header = decode_header(token).context("Failed to decode JWT header")?;
        let kid = header
            .kid
            .ok_or_else(|| anyhow!("JWT header missing kid"))?;

        // Get JWKS for this issuer
        let keys = self.get_jwks(issuer_url).await?;

        // Get the decoding key
        let key = keys
            .get(&kid)
            .ok_or_else(|| anyhow!("Key {} not found in JWKS for issuer {}", kid, issuer_url))?;

        // Set up validation parameters
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[issuer_url]);
        // Disable audience validation as different providers use different audiences
        validation.validate_aud = false;

        // Validate and decode the token
        let token_data = decode::<serde_json::Value>(token, key, &validation)
            .context("Failed to validate JWT token")?;

        // Validate exp claim manually (should be handled by jsonwebtoken, but double-check)
        if let Some(exp) = token_data.claims.get("exp").and_then(|v| v.as_u64()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now > exp {
                return Err(anyhow!("Token has expired"));
            }
        }

        // Validate custom claims (exact matching)
        Self::validate_custom_claims(&token_data.claims, expected_claims)?;

        Ok(token_data.claims)
    }
}

impl Default for ExternalJwtValidator {
    fn default() -> Self {
        Self::new()
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
