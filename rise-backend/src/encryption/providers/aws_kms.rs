use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_kms::primitives::Blob;
use aws_sdk_kms::Client as KmsClient;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::encryption::EncryptionProvider;

/// AWS KMS encryption provider
/// Uses AWS KMS for encryption/decryption operations
pub struct AwsKmsEncryptionProvider {
    client: KmsClient,
    key_id: String,
}

impl AwsKmsEncryptionProvider {
    /// Create a new AWS KMS encryption provider
    pub async fn new(
        region: &str,
        key_id: String,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
    ) -> Result<Self> {
        let config = if let (Some(access_key), Some(secret_key)) =
            (access_key_id, secret_access_key)
        {
            // Use static credentials (development only)
            let creds =
                aws_sdk_kms::config::Credentials::new(access_key, secret_key, None, None, "static");
            aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(region.to_string()))
                .credentials_provider(creds)
                .load()
                .await
        } else {
            // Use default credential chain (IRSA, instance profile, env vars, etc.)
            aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(region.to_string()))
                .load()
                .await
        };

        let client = KmsClient::new(&config);

        Ok(Self { client, key_id })
    }
}

#[async_trait]
impl EncryptionProvider for AwsKmsEncryptionProvider {
    async fn encrypt(&self, plaintext: &str) -> Result<String> {
        let response = self
            .client
            .encrypt()
            .key_id(&self.key_id)
            .plaintext(Blob::new(plaintext.as_bytes()))
            .send()
            .await
            .context("KMS encryption failed")?;

        let ciphertext_blob = response
            .ciphertext_blob()
            .context("No ciphertext blob in KMS response")?;

        // Encode to base64 for storage in database
        Ok(BASE64.encode(ciphertext_blob.as_ref()))
    }

    async fn decrypt(&self, ciphertext_base64: &str) -> Result<String> {
        // Decode from base64
        let ciphertext_bytes = BASE64
            .decode(ciphertext_base64)
            .context("Failed to decode ciphertext from base64")?;

        let response = self
            .client
            .decrypt()
            .ciphertext_blob(Blob::new(ciphertext_bytes))
            .send()
            .await
            .context("KMS decryption failed")?;

        let plaintext_blob = response
            .plaintext()
            .context("No plaintext in KMS response")?;

        // Convert to UTF-8 string
        let plaintext = String::from_utf8(plaintext_blob.clone().into_inner())
            .context("Decrypted data is not valid UTF-8")?;

        Ok(plaintext)
    }

    fn provider_name(&self) -> &str {
        "aws-kms"
    }
}
