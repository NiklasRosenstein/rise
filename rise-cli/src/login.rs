use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use std::time::Duration;

// Password-based authentication
pub async fn handle_password_login(
    http_client: &Client,
    backend_url: &str,
    username: &str,
    password: &str,
    config: &mut Config,
    url_to_save: Option<&str>,
) -> Result<()> {
    #[derive(Debug, Serialize)]
    struct LoginRequest {
        identity: String,
        password: String,
    }

    #[derive(Debug, Deserialize)]
    struct LoginResponse {
        token: String,
    }

    let login_request = LoginRequest {
        identity: username.to_string(),
        password: password.to_string(),
    };

    let url = format!("{}/login", backend_url);

    println!("Authenticating with {}...", backend_url);

    let response = http_client
        .post(&url)
        .json(&login_request)
        .send()
        .await
        .context("Failed to send login request")?;

    if response.status().is_success() {
        let login_response: LoginResponse = response.json().await.context("Failed to decode login response")?;

        // Store the backend URL if provided
        if let Some(url) = url_to_save {
            config.set_backend_url(url.to_string())
                .context("Failed to save backend URL")?;
        }

        // Store the token
        config.set_token(login_response.token)
            .context("Failed to save authentication token")?;

        println!("✓ Login successful! Welcome back, {}!", username);
        println!("  Token saved to: {}", Config::config_path()?.display());
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Login failed (status {}): {}", status, error_text);
    }

    Ok(())
}

// Device flow authentication (browser-based)
pub async fn handle_device_login(
    http_client: &Client,
    backend_url: &str,
    config: &mut Config,
    url_to_save: Option<&str>,
) -> Result<()> {
    #[derive(Debug, Deserialize)]
    struct DeviceInitResponse {
        device_code: String,
        user_code: String,
        verification_uri: String,
        expires_in: i64,
        interval: i64,
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "status")]
    enum DevicePollResponse {
        #[serde(rename = "pending")]
        Pending { message: String },
        #[serde(rename = "authorized")]
        Authorized { token: String, username: String },
        #[serde(rename = "expired")]
        Expired { message: String },
    }

    // Step 1: Initialize device flow
    let init_url = format!("{}/auth/device/init", backend_url);
    let init_response: DeviceInitResponse = http_client
        .post(&init_url)
        .send()
        .await
        .context("Failed to initialize device flow")?
        .json()
        .await
        .context("Failed to parse device init response")?;

    // Step 2: Display auth URL and open browser
    let auth_url = format!("{}?code={}", init_response.verification_uri, init_response.user_code);
    println!("Opening browser to authenticate...");
    println!("If the browser doesn't open, visit: {}", auth_url);
    println!("Code: {}", init_response.user_code);

    if let Err(e) = webbrowser::open(&auth_url) {
        println!("Failed to open browser automatically: {}", e);
    }

    // Step 3: Poll for authorization
    println!("\nWaiting for authentication...");
    let poll_url = format!("{}/auth/device/poll", backend_url);
    let poll_interval = Duration::from_secs(init_response.interval as u64);
    let timeout = Duration::from_secs(init_response.expires_in as u64);
    let start_time = std::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            anyhow::bail!("Authentication timeout - please try again");
        }

        tokio::time::sleep(poll_interval).await;

        let response: DevicePollResponse = http_client
            .get(&poll_url)
            .query(&[("device_code", &init_response.device_code)])
            .send()
            .await
            .context("Failed to poll device status")?
            .json()
            .await
            .context("Failed to parse poll response")?;

        match response {
            DevicePollResponse::Authorized { token, username } => {
                // Store the backend URL if provided
                if let Some(url) = url_to_save {
                    config.set_backend_url(url.to_string())
                        .context("Failed to save backend URL")?;
                }

                config.set_token(token)
                    .context("Failed to save authentication token")?;
                println!("\n✓ Login successful! Welcome back, {}!", username);
                println!("  Token saved to: {}", Config::config_path()?.display());
                return Ok(());
            }
            DevicePollResponse::Expired { message } => {
                anyhow::bail!("Device code expired: {}", message);
            }
            DevicePollResponse::Pending { .. } => {
                // Continue polling
                print!(".");
                use std::io::Write;
                std::io::stdout().flush()?;
            }
        }
    }
}
