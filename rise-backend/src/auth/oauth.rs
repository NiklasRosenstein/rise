use anyhow::{anyhow, Context, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OAuth2 token response from OIDC provider
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub id_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Raw token response from OIDC provider (includes id_token)
#[derive(Debug, Deserialize)]
struct DexTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    id_token: Option<String>,
    #[allow(dead_code)]
    refresh_token: Option<String>,
}

/// Device authorization response from OIDC provider
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Raw device auth response from OIDC provider
#[derive(Debug, Deserialize)]
struct DexDeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
}

/// OAuth2 client for OIDC provider
/// Note: Named DexOAuthClient for historical reasons, works with any OIDC provider
pub struct DexOAuthClient {
    issuer: String,
    client_id: String,
    client_secret: String,
    http_client: HttpClient,
}

impl DexOAuthClient {
    /// Create a new OAuth2 client for the configured OIDC provider
    pub fn new(issuer: String, client_id: String, client_secret: String) -> Result<Self> {
        Ok(Self {
            issuer,
            client_id,
            client_secret,
            http_client: HttpClient::new(),
        })
    }

    /// Exchange username and password for tokens (Resource Owner Password Grant)
    pub async fn password_grant(&self, email: &str, password: &str) -> Result<TokenInfo> {
        let token_url = format!("{}/token", self.issuer);

        let mut params = HashMap::new();
        params.insert("grant_type", "password");
        params.insert("username", email);
        params.insert("password", password);
        params.insert("scope", "openid email profile offline_access");

        let response = self
            .http_client
            .post(&token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&params)
            .send()
            .await
            .context("Failed to send token request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!(
                "Token request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let token_response: DexTokenResponse = response
            .json()
            .await
            .context("Failed to parse token response")?;

        let id_token = token_response
            .id_token
            .ok_or_else(|| anyhow!("No id_token in response"))?;

        Ok(TokenInfo {
            access_token: token_response.access_token,
            id_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in.unwrap_or(3600),
        })
    }

    /// Initiate device authorization flow
    pub async fn device_flow_start(&self) -> Result<DeviceAuthResponse> {
        let device_auth_url = format!("{}/device/code", self.issuer);

        let mut params = HashMap::new();
        params.insert("client_id", self.client_id.as_str());
        params.insert("scope", "openid email profile offline_access");

        let response = self
            .http_client
            .post(&device_auth_url)
            .form(&params)
            .send()
            .await
            .context("Failed to request device code")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!(
                "Device auth request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let device_response: DexDeviceAuthResponse = response
            .json()
            .await
            .context("Failed to parse device auth response")?;

        Ok(DeviceAuthResponse {
            device_code: device_response.device_code.clone(),
            user_code: device_response.user_code,
            verification_uri: device_response.verification_uri.clone(),
            verification_uri_complete: device_response
                .verification_uri_complete
                .unwrap_or(device_response.verification_uri),
            expires_in: device_response.expires_in.unwrap_or(600),
            interval: device_response.interval.unwrap_or(5),
        })
    }

    /// Poll for device authorization completion
    pub async fn device_flow_poll(&self, device_code: &str) -> Result<Option<TokenInfo>> {
        let token_url = format!("{}/token", self.issuer);

        let mut params = HashMap::new();
        params.insert("grant_type", "urn:ietf:params:oauth:grant-type:device_code");
        params.insert("device_code", device_code);

        let response = self
            .http_client
            .post(&token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&params)
            .send()
            .await
            .context("Failed to poll device token")?;

        if response.status().is_success() {
            let token_response: DexTokenResponse = response
                .json()
                .await
                .context("Failed to parse token response")?;

            let id_token = token_response
                .id_token
                .ok_or_else(|| anyhow!("No id_token in response"))?;

            Ok(Some(TokenInfo {
                access_token: token_response.access_token,
                id_token,
                token_type: token_response.token_type,
                expires_in: token_response.expires_in.unwrap_or(3600),
            }))
        } else if response.status() == 400 {
            // Parse error response to check if it's authorization_pending
            let error_text = response.text().await.unwrap_or_default();
            if error_text.contains("authorization_pending") || error_text.contains("slow_down") {
                Ok(None)
            } else {
                Err(anyhow!("Device authorization failed: {}", error_text))
            }
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(anyhow!(
                "Device token request failed with status {}: {}",
                status,
                error_text
            ))
        }
    }

    /// Exchange authorization code for tokens (PKCE flow)
    pub async fn exchange_code_pkce(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenInfo> {
        let token_url = format!("{}/token", self.issuer);

        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", redirect_uri);
        params.insert("code_verifier", code_verifier);

        let response = self
            .http_client
            .post(&token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&params)
            .send()
            .await
            .context("Failed to exchange authorization code")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!(
                "Code exchange failed with status {}: {}",
                status,
                error_text
            ));
        }

        let token_response: DexTokenResponse = response
            .json()
            .await
            .context("Failed to parse token response")?;

        let id_token = token_response
            .id_token
            .ok_or_else(|| anyhow!("No id_token in response"))?;

        Ok(TokenInfo {
            access_token: token_response.access_token,
            id_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in.unwrap_or(3600),
        })
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }
}
