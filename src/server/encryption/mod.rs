pub mod providers;

use anyhow::Result;
use async_trait::async_trait;

/// Encryption provider trait for encrypting/decrypting secrets
#[async_trait]
pub trait EncryptionProvider: Send + Sync {
    /// Encrypt plaintext and return base64-encoded ciphertext
    async fn encrypt(&self, plaintext: &str) -> Result<String>;

    /// Decrypt base64-encoded ciphertext and return plaintext
    async fn decrypt(&self, ciphertext: &str) -> Result<String>;
}
