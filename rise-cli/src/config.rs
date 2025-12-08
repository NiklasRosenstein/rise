use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// TODO: Use keyring crate for secure token storage instead of plain JSON
// This would store tokens in the system's secure credential storage:
// - macOS: Keychain
// - Linux: Secret Service API / libsecret
// - Windows: Credential Manager

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub token: Option<String>,
    pub backend_url: Option<String>,
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;

        let config_dir = home.join(".config").join("rise");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).context("Failed to create config directory")?;
        }

        Ok(config_dir.join("config.json"))
    }

    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let contents = fs::read_to_string(&config_path).context("Failed to read config file")?;

        let config: Config =
            serde_json::from_str(&contents).context("Failed to parse config file")?;

        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let json = serde_json::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&config_path, json).context("Failed to write config file")?;

        Ok(())
    }

    /// Set the authentication token
    pub fn set_token(&mut self, token: String) -> Result<()> {
        self.token = Some(token);
        self.save()
    }

    /// Get the authentication token
    /// Checks RISE_TOKEN environment variable first, then falls back to config file
    pub fn get_token(&self) -> Option<String> {
        // Check environment variable first
        if let Ok(token) = std::env::var("RISE_TOKEN") {
            return Some(token);
        }
        // Fall back to config file
        self.token.clone()
    }

    /// Set the backend URL
    pub fn set_backend_url(&mut self, url: String) -> Result<()> {
        self.backend_url = Some(url);
        self.save()
    }

    /// Get the backend URL (with default fallback)
    /// Checks RISE_URL environment variable first, then falls back to config file, then to default
    pub fn get_backend_url(&self) -> String {
        // Check environment variable first
        if let Ok(url) = std::env::var("RISE_URL") {
            return url;
        }
        // Fall back to config file, then to default
        self.backend_url
            .clone()
            .unwrap_or_else(|| "http://localhost:3000".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_url_precedence() {
        // Test 1: Default when nothing is set
        let config = Config {
            token: None,
            backend_url: None,
        };
        assert_eq!(config.get_backend_url(), "http://localhost:3000");

        // Test 2: Config file value used when env var not set
        let config = Config {
            token: None,
            backend_url: Some("https://api.example.com".to_string()),
        };
        assert_eq!(config.get_backend_url(), "https://api.example.com");

        // Test 3: Environment variable takes precedence (would need to be tested with actual env var)
        // This test would require setting RISE_URL in the environment, which we skip in unit tests
        // but document the expected behavior
    }

    #[test]
    fn test_token_precedence() {
        // Test config file token when env var not set
        let config = Config {
            token: Some("config-token".to_string()),
            backend_url: None,
        };
        // When RISE_TOKEN env var is not set, should use config file token
        if std::env::var("RISE_TOKEN").is_err() {
            assert_eq!(config.get_token(), Some("config-token".to_string()));
        }
    }
}
