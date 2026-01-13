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

    /// Access class (e.g., public, private)
    #[serde(default = "default_access_class", alias = "visibility")]
    pub access_class: String,

    /// Custom domains
    #[serde(default)]
    pub custom_domains: Vec<String>,

    /// Plain-text environment variables (non-secret)
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_access_class() -> String {
    "public".to_string()
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

        // Deserialize and collect any unused fields
        let mut unused_fields = Vec::new();
        let deserializer = toml::Deserializer::new(&content);
        let config: ProjectBuildConfig = serde_ignored::deserialize(deserializer, |path| {
            unused_fields.push(path.to_string());
        })?;

        // Warn about unused fields
        for field in &unused_fields {
            warn!(
                "Unknown configuration field in {}: {}",
                path.display(),
                field
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_with_unused_fields() {
        // Create a temporary directory for test
        let temp_dir = tempfile::tempdir().unwrap();
        let rise_toml_path = temp_dir.path().join("rise.toml");

        // Write a config with some unknown fields
        std::fs::write(
            &rise_toml_path,
            r#"
version = 1

[project]
name = "test-project"
access_class = "private"

[build]
backend = "docker"

# Unknown fields that should trigger warnings
unknown_field = "test"
another_unknown = 123

[unknown_section]
foo = "bar"
"#,
        )
        .unwrap();

        // Load the config - it should succeed despite unknown fields
        let result = load_full_project_config(temp_dir.path().to_str().unwrap());

        assert!(result.is_ok(), "Config should load despite unknown fields");
        let config = result.unwrap();
        assert!(config.is_some(), "Config should be present");

        let config = config.unwrap();
        assert_eq!(config.version, Some(1));
        assert!(config.project.is_some());
        assert_eq!(config.project.unwrap().name, "test-project");
    }

    #[test]
    fn test_load_config_without_unknown_fields() {
        // Create a temporary directory for test
        let temp_dir = tempfile::tempdir().unwrap();
        let rise_toml_path = temp_dir.path().join("rise.toml");

        // Write a clean config
        std::fs::write(
            &rise_toml_path,
            r#"
version = 1

[project]
name = "clean-project"
access_class = "public"

[build]
backend = "pack"
builder = "paketobuildpacks/builder-jammy-base"
"#,
        )
        .unwrap();

        // Load the config - should work fine
        let result = load_full_project_config(temp_dir.path().to_str().unwrap());

        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.version, Some(1));
        assert!(config.project.is_some());
        assert_eq!(config.project.as_ref().unwrap().name, "clean-project");
        assert_eq!(config.project.as_ref().unwrap().access_class, "public");
    }
}
