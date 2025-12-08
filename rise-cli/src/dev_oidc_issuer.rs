//! Local OIDC issuer for testing service account authentication
//!
//! This module provides a simple OIDC-compliant token issuer that can be used
//! to test service account authentication without needing GitLab CI or GitHub Actions.

use anyhow::{Context, Result, anyhow};
use axum::{
    Router,
    extract::{Query, State},
    response::Json,
    routing::get,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::{RsaPrivateKey, RsaPublicKey, pkcs1::EncodeRsaPrivateKey};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::SystemTime};

/// Shared state for the OIDC issuer
struct IssuerState {
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
    issuer_url: String,
}

/// OIDC Discovery document
#[derive(Serialize)]
struct OidcDiscovery {
    issuer: String,
    jwks_uri: String,
    token_endpoint: String,
    response_types_supported: Vec<String>,
    subject_types_supported: Vec<String>,
    id_token_signing_alg_values_supported: Vec<String>,
}

/// JWKS response
#[derive(Serialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

/// JSON Web Key
#[derive(Serialize)]
struct Jwk {
    kty: String,
    #[serde(rename = "use")]
    key_use: String,
    kid: String,
    alg: String,
    n: String,
    e: String,
}

/// Token response
#[derive(Serialize)]
struct TokenResponse {
    token: String,
}

/// Token request query parameters
#[derive(Deserialize)]
struct TokenQuery {
    #[serde(flatten)]
    claims: HashMap<String, String>,
}

/// JWT claims structure
#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    iat: u64,
    exp: u64,
    #[serde(flatten)]
    custom: HashMap<String, String>,
}

/// Generate a token with the given claims
fn generate_token(state: &IssuerState, claims_map: &HashMap<String, String>) -> Result<String> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Extract sub or use default
    let sub = claims_map
        .get("sub")
        .cloned()
        .unwrap_or_else(|| "test-subject".to_string());

    // Build custom claims (exclude iss, sub, iat, exp as we set those)
    let mut custom = claims_map.clone();
    custom.remove("sub");
    custom.remove("iss");
    custom.remove("iat");
    custom.remove("exp");

    let claims = JwtClaims {
        iss: state.issuer_url.clone(),
        sub,
        iat: now,
        exp: now + 3600, // 1 hour expiry
        custom,
    };

    // Convert RSA private key to PEM format for jsonwebtoken
    let pem = state
        .private_key
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .context("Failed to encode private key to PEM")?;

    let encoding_key =
        EncodingKey::from_rsa_pem(pem.as_bytes()).context("Failed to create encoding key")?;

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("local-key".to_string());

    let token = encode(&header, &claims, &encoding_key).context("Failed to encode JWT")?;

    Ok(token)
}

/// Parse claims from a comma-separated string (e.g., "aud=foo,sub=bar")
fn parse_claims_string(claims_str: &str) -> Result<HashMap<String, String>> {
    let mut claims = HashMap::new();

    for pair in claims_str.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let parts: Vec<&str> = pair.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid claim format '{}'. Expected 'key=value'",
                pair
            ));
        }

        claims.insert(parts[0].to_string(), parts[1].to_string());
    }

    Ok(claims)
}

/// Handler for /.well-known/openid-configuration
async fn discovery_handler(State(state): State<Arc<IssuerState>>) -> Json<OidcDiscovery> {
    Json(OidcDiscovery {
        issuer: state.issuer_url.clone(),
        jwks_uri: format!("{}/keys", state.issuer_url),
        token_endpoint: format!("{}/token", state.issuer_url),
        response_types_supported: vec!["token".to_string()],
        subject_types_supported: vec!["public".to_string()],
        id_token_signing_alg_values_supported: vec!["RS256".to_string()],
    })
}

/// Handler for /keys (JWKS endpoint)
async fn jwks_handler(State(state): State<Arc<IssuerState>>) -> Result<Json<JwksResponse>, String> {
    // Get the public key components using the rsa crate's traits
    use rsa::traits::PublicKeyParts;
    let n_bytes = state.public_key.n().to_bytes_be();
    let e_bytes = state.public_key.e().to_bytes_be();

    let n = URL_SAFE_NO_PAD.encode(&n_bytes);
    let e = URL_SAFE_NO_PAD.encode(&e_bytes);

    Ok(Json(JwksResponse {
        keys: vec![Jwk {
            kty: "RSA".to_string(),
            key_use: "sig".to_string(),
            kid: "local-key".to_string(),
            alg: "RS256".to_string(),
            n,
            e,
        }],
    }))
}

/// Handler for /token endpoint
async fn token_handler(
    State(state): State<Arc<IssuerState>>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<TokenResponse>, String> {
    let token = generate_token(&state, &query.claims)
        .map_err(|e| format!("Failed to generate token: {}", e))?;

    Ok(Json(TokenResponse { token }))
}

/// Run the local OIDC issuer
pub async fn run(port: u16, token_claims: Option<String>) -> Result<()> {
    // Generate RSA keypair
    tracing::info!("Generating RSA keypair...");
    let mut rng = rand::thread_rng();
    let private_key =
        RsaPrivateKey::new(&mut rng, 2048).context("Failed to generate RSA private key")?;
    let public_key = RsaPublicKey::from(&private_key);

    let issuer_url = format!("http://localhost:{}", port);

    let state = Arc::new(IssuerState {
        private_key,
        public_key,
        issuer_url: issuer_url.clone(),
    });

    // If --token was provided, generate and print a token immediately
    if let Some(claims_str) = token_claims {
        let claims = parse_claims_string(&claims_str)?;
        let token = generate_token(&state, &claims)?;
        println!("Token: {}", token);
    }

    // Build the router
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery_handler))
        .route("/keys", get(jwks_handler))
        .route("/token", get(token_handler))
        .with_state(state);

    // Start the server
    println!("Local OIDC issuer running at {}", issuer_url);
    println!(
        "  Discovery: {}/.well-known/openid-configuration",
        issuer_url
    );
    println!("  JWKS:      {}/keys", issuer_url);
    println!("  Token:     {}/token?aud=...&sub=...", issuer_url);

    let listener = tokio::net::TcpListener::bind(format!("localhost:{}", port))
        .await
        .context("Failed to bind to port")?;

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}
