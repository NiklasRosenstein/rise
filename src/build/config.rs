// Project-level build configuration (rise.toml / .rise.toml)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Root structure for rise.toml / .rise.toml configuration file
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ProjectBuildConfig {
    /// Optional version (must be 1 if present)
    pub version: Option<u32>,

    /// Project metadata (optional)
    #[serde(default)]
    pub project: Option<ProjectConfig>,

    /// Build configuration (optional)
    #[serde(default)]
    pub build: Option<BuildConfig>,
}

/// Project metadata configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    /// Project name
    pub name: String,

    /// Visibility (Public or Private)
    #[serde(default = "default_visibility")]
    pub visibility: String,

    /// Custom domains
    #[serde(default)]
    pub custom_domains: Vec<String>,

    /// Plain-text environment variables (non-secret)
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_visibility() -> String {
    "private".to_string()
}

/// Build configuration options for a project
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct BuildConfig {
    /// Build backend (docker, pack, railpack[:buildx], railpack:buildctl)
    pub backend: Option<String>,

    /// Buildpack builder to use (only for pack backend)
    pub builder: Option<String>,

    /// Buildpack(s) to use (only for pack backend)
    pub buildpacks: Option<Vec<String>>,

    /// Environment variables to pass to the build
    /// Format: KEY=VALUE or KEY (to pass from environment)
    pub env: Option<Vec<String>>,

    /// Container CLI to use (docker or podman)
    pub container_cli: Option<String>,

    /// Enable managed BuildKit daemon with SSL certificate support
    pub managed_buildkit: Option<bool>,

    /// Embed SSL certificate into Railpack build plan
    pub railpack_embed_ssl_cert: Option<bool>,
}

/// Load full project configuration from rise.toml or .rise.toml
///
/// Searches for rise.toml first, then .rise.toml in the given directory.
/// Returns Ok(None) if no config file is found.
/// Returns Err if file exists but cannot be read or parsed, or if version is unsupported.
pub fn load_full_project_config(app_path: &str) -> Result<Option<ProjectBuildConfig>> {
    let rise_toml = Path::new(app_path).join("rise.toml");
    let dot_rise_toml = Path::new(app_path).join(".rise.toml");

    // Warn if both files exist
    if rise_toml.exists() && dot_rise_toml.exists() {
        warn!("Both rise.toml and .rise.toml found. Using rise.toml.");
    }

    // Determine which config file to use
    let config_path = if rise_toml.exists() {
        Some(rise_toml)
    } else if dot_rise_toml.exists() {
        Some(dot_rise_toml)
    } else {
        None
    };

    // Parse if found
    if let Some(path) = config_path {
        info!("Loading project config from {}", path.display());
        let content = std::fs::read_to_string(&path)?;
        let config: ProjectBuildConfig = toml::from_str(&content)?;

        // Validate version
        if let Some(version) = config.version {
            if version != 1 {
                anyhow::bail!(
                    "Unsupported rise.toml version: {}. This CLI supports version 1.",
                    version
                );
            }
        } else {
            debug!("No version specified in rise.toml, using latest");
        }

        Ok(Some(config))
    } else {
        Ok(None)
    }
}

/// Write project configuration to rise.toml
///
/// Creates or overwrites rise.toml in the specified directory.
pub fn write_project_config(app_path: &str, config: &ProjectBuildConfig) -> Result<()> {
    let rise_toml_path = Path::new(app_path).join("rise.toml");
    let toml_string = toml::to_string_pretty(config)?;
    std::fs::write(&rise_toml_path, toml_string)?;
    info!("Wrote project config to {}", rise_toml_path.display());
    Ok(())
}
