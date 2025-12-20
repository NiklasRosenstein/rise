// Project-level build configuration (rise.toml / .rise.toml)

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use tracing::{info, warn};

/// Root structure for rise.toml / .rise.toml configuration file
#[derive(Debug, Deserialize, Default)]
pub struct ProjectBuildConfig {
    /// Build configuration section
    #[serde(default)]
    pub build: BuildConfig,
}

/// Build configuration options for a project
#[derive(Debug, Deserialize, Default)]
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

/// Load project-level build configuration from rise.toml or .rise.toml
///
/// Searches for rise.toml first, then .rise.toml in the given directory.
/// Returns Ok(None) if no config file is found.
/// Returns Err if file exists but cannot be read or parsed.
pub(crate) fn load_project_config(app_path: &str) -> Result<Option<BuildConfig>> {
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
        Ok(Some(config.build))
    } else {
        Ok(None)
    }
}
