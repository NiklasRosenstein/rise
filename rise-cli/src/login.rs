use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

pub async fn handle_login(http_client: &Client, backend_url: &str) -> Result<()> {
    println!("Please enter your login credentials.");

    print!("Username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim().to_string();

    print!("Password: ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim().to_string();

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
        identity: username,
        password,
    };

    let url = format!("{}/auth/login", backend_url);

    let response = http_client
        .post(&url)
        .json(&login_request)
        .send()
        .await?;

    if response.status().is_success() {
        let login_response: LoginResponse = response.json().await?;
        println!("Login successful! Token: {}", login_response.token);
        // TODO: Store the token securely
    } else {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        eprintln!("Login failed: {}", error_text);
    }

    Ok(())
}
