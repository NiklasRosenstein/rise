pub mod providers;

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::settings::EncryptionSettings;

/// Encryption provider trait for encrypting/decrypting secrets
#[async_trait]
pub trait EncryptionProvider: Send + Sync {
    /// Encrypt plaintext and return base64-encoded ciphertext
    async fn encrypt(&self, plaintext: &str) -> Result<String>;

    /// Decrypt base64-encoded ciphertext and return plaintext
    async fn decrypt(&self, ciphertext: &str) -> Result<String>;

    /// Get provider name for logging
    fn provider_name(&self) -> &str;
}

/// Initialize encryption provider from settings
pub async fn init_provider(
    encryption_settings: Option<&EncryptionSettings>,
) -> Result<Option<Arc<dyn EncryptionProvider>>> {
    if let Some(encryption_config) = encryption_settings {
        match encryption_config {
            EncryptionSettings::Local { key } => {
                let provider = providers::local::LocalEncryptionProvider::new(key)
                    .context("Failed to initialize local encryption provider")?;
                Ok(Some(Arc::new(provider)))
            }
            EncryptionSettings::AwsKms {
                region,
                key_id,
                access_key_id,
                secret_access_key,
            } => {
                let provider = providers::aws_kms::AwsKmsEncryptionProvider::new(
                    region,
                    key_id.clone(),
                    access_key_id.clone(),
                    secret_access_key.clone(),
                )
                .await
                .context("Failed to initialize AWS KMS encryption provider")?;
                Ok(Some(Arc::new(provider)))
            }
        }
    } else {
        Ok(None)
    }
}
