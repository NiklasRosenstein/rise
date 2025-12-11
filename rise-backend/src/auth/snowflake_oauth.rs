//! Snowflake OAuth2 client for Rise platform authentication
//!
//! This module handles OAuth2 authentication with Snowflake for projects
//! that have `snowflake_enabled = true`. Rise acts as an OAuth2 client,
//! obtaining and managing tokens on behalf of users.

use anyhow::{Context, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

/// Snowflake OAuth token response
#[derive(Debug, Deserialize)]
pub struct SnowflakeTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    /// Seconds until the access token expires
    pub expires_in: u64,
    /// Scope granted (may differ from requested)
    #[serde(default)]
    pub scope: Option<String>,
}

/// Snowflake OAuth error response
#[derive(Debug, Deserialize)]
pub struct SnowflakeOAuthError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

/// Snowflake OAuth2 client
pub struct SnowflakeOAuthClient {
    /// Snowflake account identifier (e.g., "xy12345.eu-west-1")
    account: String,
    /// OAuth client ID from Snowflake security integration
    client_id: String,
    /// OAuth client secret
    client_secret: String,
    /// OAuth redirect URI (must match security integration)
    redirect_uri: String,
    /// OAuth scopes to request (default: "session:role-any")
    scopes: String,
    /// HTTP client for making requests
    http_client: HttpClient,
}

impl SnowflakeOAuthClient {
    /// Create a new Snowflake OAuth client
    pub fn new(
        account: String,
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        scopes: String,
    ) -> Self {
        Self {
            account,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
            http_client: HttpClient::new(),
        }
    }

    /// Get the Snowflake OAuth authorize URL base
    fn authorize_url_base(&self) -> String {
        format!(
            "https://{}.snowflakecomputing.com/oauth/authorize",
            self.account
        )
    }

    /// Get the Snowflake OAuth token URL
    fn token_url(&self) -> String {
        format!(
            "https://{}.snowflakecomputing.com/oauth/token-request",
            self.account
        )
    }

    /// Build the Snowflake authorization URL for user redirect
    ///
    /// # Arguments
    /// * `state` - CSRF protection state parameter (should be stored in session)
    ///
    /// # Returns
    /// The full authorization URL to redirect the user to
    pub fn build_authorize_url(&self, state: &str) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", self.scopes.as_str()),
            ("state", state),
        ];

        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("{}?{}", self.authorize_url_base(), query)
    }

    /// Exchange an authorization code for tokens
    ///
    /// # Arguments
    /// * `code` - The authorization code received from Snowflake callback
    ///
    /// # Returns
    /// Token response containing access and refresh tokens
    pub async fn exchange_code(&self, code: &str) -> Result<SnowflakeTokenResponse> {
        #[derive(Serialize)]
        struct TokenRequest<'a> {
            grant_type: &'a str,
            code: &'a str,
            redirect_uri: &'a str,
        }

        let request = TokenRequest {
            grant_type: "authorization_code",
            code,
            redirect_uri: &self.redirect_uri,
        };

        let response = self
            .http_client
            .post(self.token_url())
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&request)
            .send()
            .await
            .context("Failed to send token exchange request to Snowflake")?;

        if !response.status().is_success() {
            let status = response.status();
            let error: SnowflakeOAuthError = response.json().await.unwrap_or(SnowflakeOAuthError {
                error: "unknown_error".to_string(),
                error_description: Some("Failed to parse error response".to_string()),
            });

            anyhow::bail!(
                "Snowflake token exchange failed ({}): {} - {}",
                status,
                error.error,
                error.error_description.unwrap_or_default()
            );
        }

        let token_response: SnowflakeTokenResponse = response
            .json()
            .await
            .context("Failed to parse Snowflake token response")?;

        Ok(token_response)
    }

    /// Refresh an access token using a refresh token
    ///
    /// # Arguments
    /// * `refresh_token` - The refresh token to use
    ///
    /// # Returns
    /// New token response with refreshed access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<SnowflakeTokenResponse> {
        #[derive(Serialize)]
        struct RefreshRequest<'a> {
            grant_type: &'a str,
            refresh_token: &'a str,
            redirect_uri: &'a str,
        }

        let request = RefreshRequest {
            grant_type: "refresh_token",
            refresh_token,
            redirect_uri: &self.redirect_uri,
        };

        let response = self
            .http_client
            .post(self.token_url())
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&request)
            .send()
            .await
            .context("Failed to send token refresh request to Snowflake")?;

        if !response.status().is_success() {
            let status = response.status();
            let error: SnowflakeOAuthError = response.json().await.unwrap_or(SnowflakeOAuthError {
                error: "unknown_error".to_string(),
                error_description: Some("Failed to parse error response".to_string()),
            });

            anyhow::bail!(
                "Snowflake token refresh failed ({}): {} - {}",
                status,
                error.error,
                error.error_description.unwrap_or_default()
            );
        }

        let token_response: SnowflakeTokenResponse = response
            .json()
            .await
            .context("Failed to parse Snowflake refresh token response")?;

        Ok(token_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_authorize_url() {
        let client = SnowflakeOAuthClient::new(
            "test-account.eu-west-1".to_string(),
            "test-client-id".to_string(),
            "test-secret".to_string(),
            "https://rise.dev/.rise/oauth/callback".to_string(),
            "session:role-any".to_string(),
        );

        let url = client.build_authorize_url("test-state-123");

        assert!(url
            .starts_with("https://test-account.eu-west-1.snowflakecomputing.com/oauth/authorize?"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=test-state-123"));
        assert!(url.contains("scope=session%3Arole-any"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Frise.dev%2F.rise%2Foauth%2Fcallback"));
    }

    #[test]
    fn test_token_url() {
        let client = SnowflakeOAuthClient::new(
            "xy12345.eu-west-1".to_string(),
            "client".to_string(),
            "secret".to_string(),
            "https://example.com/callback".to_string(),
            "session:role-any".to_string(),
        );

        assert_eq!(
            client.token_url(),
            "https://xy12345.eu-west-1.snowflakecomputing.com/oauth/token-request"
        );
    }
}
