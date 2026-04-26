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

    /// Health check configuration (optional)
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,

    /// Per-environment configuration (optional)
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,
}

/// Per-environment configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct EnvironmentConfig {
    /// Plain-text environment variables scoped to this environment
    #[serde(default)]
    pub env: HashMap<String, String>,
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

fn default_probe_path() -> String {
    "/".to_string()
}

fn default_true() -> bool {
    true
}

fn default_initial_delay() -> i32 {
    10
}

fn default_period_seconds() -> i32 {
    10
}

fn default_timeout_seconds() -> i32 {
    5
}

fn default_failure_threshold() -> i32 {
    3
}

/// Health check (liveness/readiness probe) configuration for a deployment
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCheckConfig {
    /// Enable liveness probes (default: true)
    #[serde(default = "default_true")]
    pub liveness_enabled: bool,

    /// Enable readiness probes (default: true)
    #[serde(default = "default_true")]
    pub readiness_enabled: bool,

    /// HTTP path for health probes (default: "/")
    #[serde(default = "default_probe_path")]
    pub path: String,

    /// Initial delay in seconds before the first probe (default: 10)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_seconds: i32,

    /// How often to probe in seconds (default: 10)
    #[serde(default = "default_period_seconds")]
    pub period_seconds: i32,

    /// Probe timeout in seconds (default: 5)
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: i32,

    /// Number of failures before the probe is considered failed (default: 3)
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: i32,
}

/// Build configuration options for a project
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct BuildConfig {
    /// Build backend (docker, docker:build, docker:buildx, buildctl, pack, railpack[:buildx], railpack:buildctl)
    pub backend: Option<String>,

    /// Buildpack builder to use (only for pack backend)
    pub builder: Option<String>,

    /// Buildpack(s) to use (only for pack backend)
    pub buildpacks: Option<Vec<String>>,

    /// Build arguments to pass to the build
    /// Format: KEY=VALUE or KEY (to pass from environment)
    #[serde(alias = "env")]
    pub args: Option<Vec<String>>,

    /// Container CLI to use (docker or podman)
    pub container_cli: Option<String>,

    /// Enable managed BuildKit daemon with SSL certificate support
    pub managed_buildkit: Option<bool>,

    /// Path to Dockerfile (relative to rise.toml location). Defaults to "Dockerfile" or "Containerfile"
    pub dockerfile: Option<String>,

    /// Default build context (docker/podman only) - the context directory for the build
    /// This is the path argument to `docker build <path>`. Defaults to rise.toml location.
    /// Path is relative to the rise.toml file location.
    pub build_context: Option<String>,

    /// Build contexts (docker/podman only) - additional named contexts for multi-stage builds
    /// Format: { "name" = "path" } where path is relative to the rise.toml file location
    #[serde(default)]
    pub build_contexts: Option<HashMap<String, String>>,

    /// Disable build cache
    pub no_cache: Option<bool>,
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
        let deserializer = toml::Deserializer::parse(&content)
            .map_err(|e| anyhow::Error::new(e).context("Failed to parse TOML"))?;
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
    fn test_load_config_with_environments() {
        let temp_dir = tempfile::tempdir().unwrap();
        let rise_toml_path = temp_dir.path().join("rise.toml");

        std::fs::write(
            &rise_toml_path,
            r#"
[project]
name = "my-app"

[project.env]
LOG_LEVEL = "info"
DATABASE_URL = "postgres://localhost/mydb"

[environments.staging.env]
DATABASE_URL = "postgres://staging-db/mydb"
LOG_LEVEL = "debug"

[environments.production.env]
DATABASE_URL = "postgres://prod-db/mydb"
"#,
        )
        .unwrap();

        let result = load_full_project_config(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let config = result.unwrap().unwrap();

        // Check global env
        let project = config.project.as_ref().unwrap();
        assert_eq!(project.env.get("LOG_LEVEL").unwrap(), "info");
        assert_eq!(
            project.env.get("DATABASE_URL").unwrap(),
            "postgres://localhost/mydb"
        );

        // Check environments
        assert_eq!(config.environments.len(), 2);

        let staging = config.environments.get("staging").unwrap();
        assert_eq!(
            staging.env.get("DATABASE_URL").unwrap(),
            "postgres://staging-db/mydb"
        );
        assert_eq!(staging.env.get("LOG_LEVEL").unwrap(), "debug");

        let production = config.environments.get("production").unwrap();
        assert_eq!(
            production.env.get("DATABASE_URL").unwrap(),
            "postgres://prod-db/mydb"
        );
        assert!(!production.env.contains_key("LOG_LEVEL"));
    }

    #[test]
    fn test_load_config_without_environments() {
        let temp_dir = tempfile::tempdir().unwrap();
        let rise_toml_path = temp_dir.path().join("rise.toml");

        std::fs::write(
            &rise_toml_path,
            r#"
[project]
name = "no-envs"

[project.env]
FOO = "bar"
"#,
        )
        .unwrap();

        let result = load_full_project_config(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let config = result.unwrap().unwrap();
        assert!(config.environments.is_empty());
    }

    #[test]
    fn test_roundtrip_config_with_environments() {
        let config = ProjectBuildConfig {
            version: Some(1),
            project: Some(ProjectConfig {
                name: "roundtrip-app".to_string(),
                access_class: "public".to_string(),
                custom_domains: Vec::new(),
                env: HashMap::from([("GLOBAL".to_string(), "val".to_string())]),
            }),
            build: None,
            health_check: None,
            environments: HashMap::from([(
                "staging".to_string(),
                EnvironmentConfig {
                    env: HashMap::from([("STAGE_VAR".to_string(), "stage_val".to_string())]),
                },
            )]),
        };

        let temp_dir = tempfile::tempdir().unwrap();
        write_project_config(temp_dir.path().to_str().unwrap(), &config).unwrap();

        let loaded = load_full_project_config(temp_dir.path().to_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.version, Some(1));
        assert_eq!(loaded.project.as_ref().unwrap().name, "roundtrip-app");
        assert_eq!(
            loaded.project.as_ref().unwrap().env.get("GLOBAL").unwrap(),
            "val"
        );
        let staging = loaded.environments.get("staging").unwrap();
        assert_eq!(staging.env.get("STAGE_VAR").unwrap(), "stage_val");
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
builder = "heroku/builder:24"
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
