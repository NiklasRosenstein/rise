use anyhow::{Result, Context};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use crate::config::Config;

#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default = "default_expires_in")]
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_expires_in() -> u64 {
    600 // 10 minutes
}

fn default_interval() -> u64 {
    5 // 5 seconds
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    id_token: String,
    token_type: String,
    #[serde(default = "default_token_expires_in")]
    expires_in: u64,
}

fn default_token_expires_in() -> u64 {
    3600 // 1 hour
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Handle device authorization flow by communicating directly with Dex
pub async fn handle_device_flow(
    http_client: &Client,
    dex_url: &str,
    client_id: &str,
    config: &mut Config,
    backend_url_to_save: Option<&str>,
) -> Result<()> {
    // Step 1: Initialize device flow with Dex
    let device_auth_url = format!("{}/device/code", dex_url);

    let mut params = std::collections::HashMap::new();
    params.insert("client_id", client_id);
    params.insert("scope", "openid email profile offline_access");

    println!("Initializing device authorization flow...");

    let device_response: DeviceAuthResponse = http_client
        .post(&device_auth_url)
        .form(&params)
        .send()
        .await
        .context("Failed to initialize device flow with Dex")?
        .json()
        .await
        .context("Failed to parse device auth response")?;

    // Step 2: Display user code and open browser
    let verification_url = device_response
        .verification_uri_complete
        .as_ref()
        .unwrap_or(&device_response.verification_uri);

    println!("\nOpening browser to authenticate...");
    println!("If the browser doesn't open, visit: {}", verification_url);
    println!("Enter code: {}", device_response.user_code);

    if let Err(e) = webbrowser::open(verification_url) {
        println!("Failed to open browser automatically: {}", e);
    }

    // Step 3: Poll Dex for authorization
    println!("\nWaiting for authentication...");
    let token_url = format!("{}/token", dex_url);
    let poll_interval = Duration::from_secs(device_response.interval);
    let timeout = Duration::from_secs(device_response.expires_in);
    let start_time = std::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            anyhow::bail!("Authentication timeout - please try again");
        }

        tokio::time::sleep(poll_interval).await;

        let mut token_params = std::collections::HashMap::new();
        token_params.insert("grant_type", "urn:ietf:params:oauth:grant-type:device_code");
        token_params.insert("device_code", device_response.device_code.as_str());
        token_params.insert("client_id", client_id);

        let response = http_client
            .post(&token_url)
            .form(&token_params)
            .send()
            .await
            .context("Failed to poll Dex token endpoint")?;

        if response.status().is_success() {
            // Successfully got the token
            let token_response: TokenResponse = response
                .json()
                .await
                .context("Failed to parse token response")?;

            // Store the backend URL if provided
            if let Some(url) = backend_url_to_save {
                config.set_backend_url(url.to_string())
                    .context("Failed to save backend URL")?;
            }

            // Store the ID token
            config.set_token(token_response.id_token)
                .context("Failed to save authentication token")?;

            println!("\n✓ Login successful!");
            println!("  Token saved to: {}", Config::config_path()?.display());
            return Ok(());
        } else if response.status() == 400 {
            // Check if it's authorization_pending or slow_down
            let error_response: Result<TokenErrorResponse, _> = response.json().await;

            match error_response {
                Ok(err) if err.error == "authorization_pending" || err.error == "slow_down" => {
                    // Continue polling
                    print!(".");
                    use std::io::Write;
                    std::io::stdout().flush()?;
                }
                Ok(err) => {
                    anyhow::bail!("Device authorization failed: {} - {}", err.error, err.error_description.unwrap_or_default());
                }
                Err(_) => {
                    anyhow::bail!("Device authorization failed with status 400");
                }
            }
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!("Device token request failed with status {}: {}", status, error_text);
        }
    }
}
