//! Snowflake OAuth token refresh controller
//!
//! This controller runs in the background and proactively refreshes
//! Snowflake OAuth tokens before they expire.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::auth::snowflake_oauth::SnowflakeOAuthClient;
use crate::db::snowflake_sessions;
use crate::encryption::EncryptionProvider;

/// Snowflake token refresh controller
///
/// Runs periodically to refresh tokens that are expiring soon.
pub struct SnowflakeRefreshController {
    db_pool: PgPool,
    snowflake_client: Arc<SnowflakeOAuthClient>,
    encryption_provider: Arc<dyn EncryptionProvider>,
    /// How often to check for expiring tokens
    check_interval: Duration,
    /// Refresh tokens expiring within this window
    refresh_window_minutes: i64,
}

impl SnowflakeRefreshController {
    /// Create a new Snowflake refresh controller
    pub fn new(
        db_pool: PgPool,
        snowflake_client: Arc<SnowflakeOAuthClient>,
        encryption_provider: Arc<dyn EncryptionProvider>,
    ) -> Self {
        Self {
            db_pool,
            snowflake_client,
            encryption_provider,
            check_interval: Duration::from_secs(300), // 5 minutes
            refresh_window_minutes: 10,               // Refresh tokens expiring in 10 minutes
        }
    }

    /// Start the refresh controller loop
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.refresh_loop().await;
        });
    }

    /// Main refresh loop
    async fn refresh_loop(&self) {
        info!("Snowflake token refresh controller started");
        let mut ticker = interval(self.check_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.refresh_expiring_tokens().await {
                error!("Error refreshing Snowflake tokens: {}", e);
            }
        }
    }

    /// Find and refresh tokens that are expiring soon
    async fn refresh_expiring_tokens(&self) -> anyhow::Result<()> {
        let expires_before = Utc::now() + ChronoDuration::minutes(self.refresh_window_minutes);

        let expiring_tokens =
            snowflake_sessions::find_expiring_tokens(&self.db_pool, expires_before).await?;

        if expiring_tokens.is_empty() {
            debug!("No Snowflake tokens expiring soon");
            return Ok(());
        }

        info!(
            "Found {} Snowflake tokens expiring before {}",
            expiring_tokens.len(),
            expires_before
        );

        for token in expiring_tokens {
            if let Err(e) = self.refresh_token(&token).await {
                warn!(
                    "Failed to refresh token for session/project {}/{}: {}",
                    &token.session_id[..8.min(token.session_id.len())],
                    token.project_name,
                    e
                );
            }
        }

        Ok(())
    }

    /// Refresh a single token
    async fn refresh_token(
        &self,
        token: &crate::db::models::SnowflakeAppToken,
    ) -> anyhow::Result<()> {
        debug!(
            "Refreshing Snowflake token for session/project: {}/{}",
            &token.session_id[..8.min(token.session_id.len())],
            token.project_name
        );

        // Decrypt refresh token
        let refresh_token = self
            .encryption_provider
            .decrypt(&token.refresh_token_encrypted)
            .await?;

        // Call Snowflake to refresh
        let new_tokens = self.snowflake_client.refresh_token(&refresh_token).await?;

        // Encrypt new tokens
        let new_access_encrypted = self
            .encryption_provider
            .encrypt(&new_tokens.access_token)
            .await?;

        let new_refresh_encrypted = self
            .encryption_provider
            .encrypt(&new_tokens.refresh_token)
            .await?;

        let new_expires_at = Utc::now() + ChronoDuration::seconds(new_tokens.expires_in as i64);

        // Update in database
        snowflake_sessions::upsert_app_token(
            &self.db_pool,
            &token.session_id,
            &token.project_name,
            &new_access_encrypted,
            &new_refresh_encrypted,
            new_expires_at,
        )
        .await?;

        info!(
            "Successfully refreshed Snowflake token for project '{}', new expiry: {}",
            token.project_name, new_expires_at
        );

        Ok(())
    }
}
