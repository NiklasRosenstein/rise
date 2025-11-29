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
        let home = dirs::home_dir()
            .context("Failed to get home directory")?;

        let config_dir = home.join(".config").join("rise");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .context("Failed to create config directory")?;
        }

        Ok(config_dir.join("config.json"))
    }

    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let contents = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;

        let config: Config = serde_json::from_str(&contents)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&config_path, json)
            .context("Failed to write config file")?;

        Ok(())
    }

    /// Set the authentication token
    pub fn set_token(&mut self, token: String) -> Result<()> {
        self.token = Some(token);
        self.save()
    }

    /// Get the authentication token
    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Get the backend URL (with default fallback)
    pub fn get_backend_url(&self) -> String {
        self.backend_url
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:3001".to_string())
    }
}
