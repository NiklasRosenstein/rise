use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::Config;

pub async fn handle_login(
    http_client: &Client,
    backend_url: &str,
    username: &str,
    password: &str,
    config: &mut Config,
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
