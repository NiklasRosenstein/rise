use crate::config::Config;
use crate::login::token_utils::format_token_expiration;
use anyhow::{Context, Result};
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

/// Generate PKCE code_verifier and code_challenge
fn generate_pkce_challenge() -> (String, String) {
    // Generate random code_verifier (43-128 characters)
    let random_bytes: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let code_verifier = URL_SAFE_NO_PAD.encode(&random_bytes);

    // Calculate code_challenge = BASE64URL(SHA256(code_verifier))
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let hash = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(hash);

    (code_verifier, code_challenge)
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Start local HTTP server to receive OAuth callback
async fn start_callback_server() -> Result<(String, tokio::sync::oneshot::Receiver<Result<String>>)>
{
    use std::sync::Arc;

    // Try multiple ports in case one is in use
    let ports = vec![8765, 8766, 8767];
    let mut last_error = None;

    for port in ports {
        let redirect_uri = format!("http://localhost:{}/callback", port);
        let (tx, rx) = oneshot::channel();
        let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

        let app = Router::new().route("/callback", get({
            let tx = Arc::clone(&tx);
            move |Query(params): Query<CallbackParams>| async move {
                let (result, html_response) = if let Some(code) = params.code {
                    (
                        Ok(code),
                        Html("<html><body><h1>✓ Authentication Successful!</h1><p>You can close this window and return to the terminal.</p></body></html>".to_string())
                    )
                } else if let Some(error) = params.error {
                    let error_msg = format!("OAuth error: {} - {}", error, params.error_description.unwrap_or_default());
                    (
                        Err(anyhow::anyhow!("{}", error_msg)),
                        Html(format!("<html><body><h1>✗ Authentication Failed</h1><p>{}</p></body></html>", error_msg))
                    )
                } else {
                    (
                        Err(anyhow::anyhow!("No code or error in callback")),
                        Html("<html><body><h1>✗ Authentication Failed</h1><p>No code or error in callback</p></body></html>".to_string())
                    )
                };

                // Send result through channel
                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(result);
                }

                html_response
            }
        }));

        // Try to bind to this port
        let addr = format!("localhost:{}", port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                // Successfully bound, start the server in the background
                tokio::spawn(async move {
                    let _ = axum::serve(listener, app).await;
                });
                return Ok((redirect_uri, rx));
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to bind to any port (tried 8765-8767): {}",
        last_error.unwrap()
    ))
}

#[derive(Debug, Serialize)]
struct AuthorizeRequest {
    flow: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_challenge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_challenge_method: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeResponse {
    authorization_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct CodeExchangeRequest {
    code: String,
    code_verifier: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct CodeExchangeResponse {
    token: String,
}

/// Handle OAuth2 authorization code flow with PKCE
pub async fn handle_authorization_code_flow(
    http_client: &Client,
    backend_url: &str,
    config: &mut Config,
    backend_url_to_save: Option<&str>,
) -> Result<()> {
    // Step 1: Generate PKCE codes
    let (code_verifier, code_challenge) = generate_pkce_challenge();

    // Step 2: Start local callback server
    let (redirect_uri, code_receiver) = start_callback_server()
        .await
        .context("Failed to start local callback server")?;

    // Step 3: Request authorization URL from backend
    println!("Requesting authorization URL from backend...");

    let authorize_request = AuthorizeRequest {
        flow: "code".to_string(),
        redirect_uri: Some(redirect_uri.clone()),
        code_challenge: Some(code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
    };

    let authorize_url = format!("{}/auth/authorize", backend_url);

    let response = http_client
        .post(&authorize_url)
        .json(&authorize_request)
        .send()
        .await
        .context("Failed to request authorization URL from backend")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to get authorization URL (status {}): {}",
            status,
            error_text
        );
    }

    let authorize_response: AuthorizeResponse = response
        .json()
        .await
        .context("Failed to parse authorization URL response")?;

    let auth_url = authorize_response
        .authorization_url
        .ok_or_else(|| anyhow::anyhow!("No authorization URL in response"))?;

    // Step 4: Open browser
    println!("Opening browser to authenticate...");
    println!("If the browser doesn't open, visit: {}", auth_url);

    if let Err(e) = webbrowser::open(auth_url.as_str()) {
        println!("Failed to open browser automatically: {}", e);
    }

    // Step 5: Wait for callback
    println!("\nWaiting for authentication...");

    let code = tokio::time::timeout(
        std::time::Duration::from_secs(300), // 5 minute timeout
        code_receiver,
    )
    .await
    .context("Timeout waiting for authentication")??
    .context("Failed to receive authorization code")?;

    println!("✓ Received authorization code");

    // Step 6: Exchange code with backend
    println!("Exchanging authorization code for token...");

    let exchange_request = CodeExchangeRequest {
        code,
        code_verifier,
        redirect_uri,
    };

    let exchange_url = format!("{}/auth/code/exchange", backend_url);

    let response = http_client
        .post(&exchange_url)
        .json(&exchange_request)
        .send()
        .await
        .context("Failed to exchange code with backend")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Code exchange failed (status {}): {}", status, error_text);
    }

    let exchange_response: CodeExchangeResponse = response
        .json()
        .await
        .context("Failed to parse code exchange response")?;

    // Store the backend URL if provided
    if let Some(url) = backend_url_to_save {
        config
            .set_backend_url(url.to_string())
            .context("Failed to save backend URL")?;
    }

    // Store the token
    config
        .set_token(exchange_response.token.clone())
        .context("Failed to save authentication token")?;

    println!("✓ Login successful!");
    println!("  Token saved to: {}", Config::config_path()?.display());

    // Display token expiration
    match format_token_expiration(&exchange_response.token) {
        Ok(expiration) => println!("  Token expires: {}", expiration),
        Err(e) => {
            // Don't fail the login if we can't parse expiration
            tracing::debug!("Failed to parse token expiration: {}", e);
        }
    }

    Ok(())
}
