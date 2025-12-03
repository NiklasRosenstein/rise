use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, ClientId, ClientSecret,
    DeviceAuthorizationUrl, ResourceOwnerPassword, ResourceOwnerUsername, Scope, TokenResponse,
    TokenUrl,
};
use anyhow::{Result, Context, anyhow};
use serde::{Deserialize, Serialize};

/// OAuth2 token response from Dex
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub id_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Device authorization response from Dex
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// OAuth2 client wrapper for Dex
pub struct DexOAuthClient {
    client: BasicClient,
    issuer: String,
}

impl DexOAuthClient {
    /// Create a new Dex OAuth2 client
    pub fn new(issuer: String, client_id: String, client_secret: String) -> Result<Self> {
        let auth_url = AuthUrl::new(format!("{}/auth", issuer))
            .context("Failed to create auth URL")?;
        let token_url = TokenUrl::new(format!("{}/token", issuer))
            .context("Failed to create token URL")?;
        let device_auth_url = DeviceAuthorizationUrl::new(format!("{}/device/code", issuer))
            .context("Failed to create device authorization URL")?;

        let client = BasicClient::new(
            ClientId::new(client_id),
            Some(ClientSecret::new(client_secret)),
            auth_url,
            Some(token_url),
        )
        .set_device_authorization_url(device_auth_url);

        Ok(Self { client, issuer })
    }

    /// Exchange username and password for tokens (Resource Owner Password Grant)
    pub async fn password_grant(&self, email: &str, password: &str) -> Result<TokenInfo> {
        let token_result = self
            .client
            .exchange_password(
                &ResourceOwnerUsername::new(email.to_string()),
                &ResourceOwnerPassword::new(password.to_string()),
            )
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("offline_access".to_string()))
            .request_async(async_http_client)
            .await
            .context("Failed to exchange password for token")?;

        // Extract tokens
        let access_token = token_result.access_token().secret().clone();
        let id_token = token_result
            .extra_fields()
            .get("id_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No id_token in response"))?
            .to_string();

        Ok(TokenInfo {
            access_token,
            id_token,
            token_type: "Bearer".to_string(),
            expires_in: token_result
                .expires_in()
                .map(|d| d.as_secs())
                .unwrap_or(3600),
        })
    }

    /// Initiate device authorization flow
    pub async fn device_flow_start(&self) -> Result<DeviceAuthResponse> {
        let details = self
            .client
            .exchange_device_code()
            .context("Failed to create device code request")?
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("offline_access".to_string()))
            .request_async(async_http_client)
            .await
            .context("Failed to request device code")?;

        Ok(DeviceAuthResponse {
            device_code: details.device_code().secret().clone(),
            user_code: details.user_code().secret().clone(),
            verification_uri: details.verification_uri().to_string(),
            verification_uri_complete: details
                .verification_uri_complete()
                .map(|uri| uri.secret().to_string())
                .unwrap_or_else(|| details.verification_uri().to_string()),
            expires_in: details
                .expires_in()
                .map(|d| d.as_secs())
                .unwrap_or(600),
            interval: details.interval().map(|d| d.as_secs()).unwrap_or(5),
        })
    }

    /// Poll for device authorization completion
    pub async fn device_flow_poll(&self, device_code: &str) -> Result<Option<TokenInfo>> {
        let token_result = self
            .client
            .exchange_device_access_token(&oauth2::DeviceCode::new(device_code.to_string()))
            .request_async(async_http_client, std::time::Duration::from_secs(0), None)
            .await;

        match token_result {
            Ok(token) => {
                let access_token = token.access_token().secret().clone();
                let id_token = token
                    .extra_fields()
                    .get("id_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("No id_token in response"))?
                    .to_string();

                Ok(Some(TokenInfo {
                    access_token,
                    id_token,
                    token_type: "Bearer".to_string(),
                    expires_in: token.expires_in().map(|d| d.as_secs()).unwrap_or(3600),
                }))
            }
            Err(err) => {
                // Check if it's an authorization_pending error (user hasn't authorized yet)
                let err_str = err.to_string();
                if err_str.contains("authorization_pending") || err_str.contains("slow_down") {
                    Ok(None) // Not ready yet
                } else {
                    Err(err.into()) // Actual error
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_client_creation() {
        let client = DexOAuthClient::new(
            "http://localhost:5556/dex".to_string(),
            "rise-backend".to_string(),
            "rise-backend-secret".to_string(),
        );
        assert!(client.is_ok());
    }
}
