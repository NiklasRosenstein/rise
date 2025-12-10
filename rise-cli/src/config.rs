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
    pub container_cli: Option<String>,
    pub managed_buildkit: Option<bool>,
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

    /// Set the container CLI
    #[allow(dead_code)]
    pub fn set_container_cli(&mut self, cli: String) -> Result<()> {
        self.container_cli = Some(cli);
        self.save()
    }

    /// Get the container CLI to use (docker or podman)
    /// Checks RISE_CONTAINER_CLI environment variable first, then falls back to config file,
    /// then to auto-detection (podman if available, docker otherwise)
    pub fn get_container_cli(&self) -> String {
        // Check environment variable first
        if let Ok(cli) = std::env::var("RISE_CONTAINER_CLI") {
            return cli;
        }
        // Fall back to config file
        if let Some(ref cli) = self.container_cli {
            return cli.clone();
        }
        // Auto-detect: prefer podman if docker is not available
        detect_container_cli()
    }

    /// Get whether to use managed BuildKit daemon
    /// Checks RISE_MANAGED_BUILDKIT environment variable first, then falls back to config file
    /// Returns false by default (opt-in feature)
    pub fn get_managed_buildkit(&self) -> bool {
        // Check environment variable first
        if let Ok(val) = std::env::var("RISE_MANAGED_BUILDKIT") {
            return val.to_lowercase() == "true" || val == "1";
        }
        // Fall back to config file, default to false
        self.managed_buildkit.unwrap_or(false)
    }

    /// Set whether to use managed BuildKit daemon
    #[allow(dead_code)]
    pub fn set_managed_buildkit(&mut self, enabled: bool) -> Result<()> {
        self.managed_buildkit = Some(enabled);
        self.save()
    }
}

/// Auto-detect which container CLI is available
/// Returns "podman" if docker is not available and podman is, otherwise "docker"
fn detect_container_cli() -> String {
    use std::process::Command;

    // Check if docker is available
    if Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return "docker".to_string();
    }

    // Check if podman is available
    if Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return "podman".to_string();
    }

    // Default to docker if neither is detected
    "docker".to_string()
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
            container_cli: None,
            managed_buildkit: None,
        };
        assert_eq!(config.get_backend_url(), "http://localhost:3000");

        // Test 2: Config file value used when env var not set
        let config = Config {
            token: None,
            backend_url: Some("https://api.example.com".to_string()),
            container_cli: None,
            managed_buildkit: None,
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
            container_cli: None,
            managed_buildkit: None,
        };
        // When RISE_TOKEN env var is not set, should use config file token
        if std::env::var("RISE_TOKEN").is_err() {
            assert_eq!(config.get_token(), Some("config-token".to_string()));
        }
    }

    #[test]
    fn test_managed_buildkit_default() {
        // Test default (should be false)
        let config = Config {
            token: None,
            backend_url: None,
            container_cli: None,
            managed_buildkit: None,
        };
        assert!(!config.get_managed_buildkit());

        // Test config file value used when env var not set
        let config = Config {
            token: None,
            backend_url: None,
            container_cli: None,
            managed_buildkit: Some(true),
        };
        assert!(config.get_managed_buildkit());
    }
}
