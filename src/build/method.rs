// Build method selection and configuration

use anyhow::{bail, Result};
use clap::Args;
use std::path::Path;
use tracing::info;

use crate::config::Config;

/// Build method for container images
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BuildMethod {
    Docker {
        use_buildx: bool,
    },
    Pack,
    Railpack {
        use_buildctl: bool,
    },
    /// Plain buildctl with Dockerfile (without railpack)
    Buildctl,
}

/// Build-related CLI arguments that can be flattened into command structs
#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    /// Build backend (docker, pack, railpack[:buildx], railpack:buildctl)
    #[arg(long)]
    pub backend: Option<String>,

    /// Buildpack builder to use (only for pack backend)
    #[arg(long)]
    pub builder: Option<String>,

    /// Buildpack(s) to use (only for pack backend). Can be specified multiple times.
    #[arg(long = "buildpack", short = 'b')]
    pub buildpacks: Vec<String>,

    /// Environment variables to pass to the build. Can be specified multiple times.
    /// Format: KEY=VALUE or KEY (to pass from environment)
    #[arg(long = "env", short = 'e')]
    pub env: Vec<String>,

    /// Container CLI to use (docker or podman)
    #[arg(long)]
    pub container_cli: Option<String>,

    /// Enable managed BuildKit daemon with SSL certificate support
    #[arg(long, value_parser = clap::value_parser!(bool), default_missing_value = "true", num_args = 0..=1)]
    pub managed_buildkit: Option<bool>,

    /// Embed SSL certificate into Railpack build plan for build-time RUN command support
    #[arg(long, value_parser = clap::value_parser!(bool), default_missing_value = "true", num_args = 0..=1)]
    pub railpack_embed_ssl_cert: Option<bool>,

    /// Path to Dockerfile (relative to app path / rise.toml location). Defaults to "Dockerfile" or "Containerfile"
    #[arg(long)]
    pub dockerfile: Option<String>,

    /// Default build context (docker/podman only) - the context directory for the build.
    /// This is the path argument to `docker build <path>`. Defaults to app path.
    /// Path is relative to the app path / rise.toml location.
    #[arg(long = "context")]
    pub build_context: Option<String>,

    /// Build contexts for multi-stage builds (docker/podman only). Can be specified multiple times.
    /// Format: name=path where path is relative to app path / rise.toml location
    #[arg(long = "build-context")]
    pub build_contexts: Vec<String>,
}

/// Options for building container images
#[derive(Debug, Clone)]
pub(crate) struct BuildOptions {
    pub image_tag: String,
    pub app_path: String,
    pub backend: Option<String>,
    pub builder: Option<String>,
    pub buildpacks: Vec<String>,
    pub env: Vec<String>,
    pub container_cli: Option<String>,
    pub managed_buildkit: bool,
    pub railpack_embed_ssl_cert: bool,
    pub push: bool,
    /// Path to Dockerfile (relative to app_path / rise.toml location)
    pub dockerfile: Option<String>,
    /// Default build context (relative to app_path / rise.toml location)
    /// This is the path argument to `docker build <path>`. Defaults to app_path if None.
    /// Note: This value is resolved to an absolute path in build_image() before use.
    pub build_context: Option<String>,
    /// Build contexts for multi-stage builds (docker/podman only)
    /// Format: name -> path (relative to app_path / rise.toml location)
    /// Note: These paths are resolved to absolute paths in build_image() before use.
    pub build_contexts: std::collections::HashMap<String, String>,
}

impl BuildOptions {
    /// Create BuildOptions from BuildArgs and Config
    ///
    /// Configuration precedence (highest to lowest):
    /// 1. CLI flags (BuildArgs)
    /// 2. Project config file (rise.toml / .rise.toml)
    /// 3. Environment variables (via Config getters)
    /// 4. Global config file (via Config getters)
    /// 5. Auto-detection/defaults (via Config getters)
    pub(crate) fn from_build_args(
        config: &Config,
        image_tag: String,
        app_path: String,
        build_args: &BuildArgs,
    ) -> Self {
        use tracing::warn;

        // Load project-level build config from app_path with error handling
        let project_config = match crate::build::config::load_full_project_config(&app_path) {
            Ok(cfg) => cfg.and_then(|c| c.build),
            Err(e) => {
                warn!(
                    "Failed to load project config: {:#}. Continuing without it.",
                    e
                );
                None
            }
        };

        // Merge: CLI > Project > Environment (via Config) > Global (via Config) > Defaults
        Self {
            image_tag,
            app_path,

            // String options - use first non-None value
            backend: build_args
                .backend
                .clone()
                .or_else(|| project_config.as_ref().and_then(|c| c.backend.clone())),

            builder: build_args
                .builder
                .clone()
                .or_else(|| project_config.as_ref().and_then(|c| c.builder.clone())),

            container_cli: build_args
                .container_cli
                .clone()
                .or_else(|| {
                    project_config
                        .as_ref()
                        .and_then(|c| c.container_cli.clone())
                })
                .or_else(|| Some(config.get_container_cli())),

            // Vector options - all vectors merge config + CLI values (append)
            buildpacks: {
                let mut packs = project_config
                    .as_ref()
                    .and_then(|c| c.buildpacks.clone())
                    .unwrap_or_default();
                packs.extend(build_args.buildpacks.clone());
                packs
            },

            env: {
                let mut env = project_config
                    .as_ref()
                    .and_then(|c| c.env.clone())
                    .unwrap_or_default();
                env.extend(build_args.env.clone());
                env
            },

            // Boolean options - use Option chaining with Config getters as final fallback
            managed_buildkit: build_args
                .managed_buildkit
                .or_else(|| project_config.as_ref().and_then(|c| c.managed_buildkit))
                .unwrap_or_else(|| config.get_managed_buildkit()),

            railpack_embed_ssl_cert: build_args
                .railpack_embed_ssl_cert
                .or_else(|| {
                    project_config
                        .as_ref()
                        .and_then(|c| c.railpack_embed_ssl_cert)
                })
                .unwrap_or_else(|| config.get_railpack_embed_ssl_cert()),

            dockerfile: build_args
                .dockerfile
                .clone()
                .or_else(|| project_config.as_ref().and_then(|c| c.dockerfile.clone())),

            build_context: build_args.build_context.clone().or_else(|| {
                project_config
                    .as_ref()
                    .and_then(|c| c.build_context.clone())
            }),

            // Build contexts - merge config + CLI values (CLI overrides config for same name)
            build_contexts: {
                let mut contexts = project_config
                    .as_ref()
                    .and_then(|c| c.build_contexts.clone())
                    .unwrap_or_default();

                // Parse and merge CLI build contexts (format: "name=path")
                for ctx in &build_args.build_contexts {
                    if let Some((name, path)) = ctx.split_once('=') {
                        contexts.insert(name.to_string(), path.to_string());
                    } else {
                        warn!(
                            "Invalid build context format '{}'. Expected 'name=path'. Ignoring.",
                            ctx
                        );
                    }
                }

                contexts
            },

            push: false,
        }
    }

    /// Builder method to set push flag
    pub(crate) fn with_push(mut self, push: bool) -> Self {
        self.push = push;
        self
    }
}

impl BuildMethod {
    /// Parse backend string into BuildMethod
    pub(crate) fn from_backend_str(backend: &str) -> Result<Self> {
        match backend {
            "docker" | "docker:build" => Ok(BuildMethod::Docker { use_buildx: false }),
            "docker:buildx" => Ok(BuildMethod::Docker { use_buildx: true }),
            "buildctl" => Ok(BuildMethod::Buildctl),
            "pack" => Ok(BuildMethod::Pack),
            "railpack" | "railpack:buildx" => Ok(BuildMethod::Railpack {
                use_buildctl: false,
            }),
            "railpack:buildctl" => Ok(BuildMethod::Railpack { use_buildctl: true }),
            _ => bail!(
                "Invalid build backend '{}'. Supported: docker, docker:build, docker:buildx, buildctl, pack, railpack, railpack:buildctl",
                backend
            ),
        }
    }
}

/// Select build method based on explicit backend or auto-detection
/// Returns (BuildMethod, Option<dockerfile_path>)
pub(crate) fn select_build_method(
    app_path: &str,
    backend: Option<&str>,
    dockerfile: Option<&str>,
) -> Result<(BuildMethod, Option<String>)> {
    // Determine dockerfile path
    let (dockerfile_path, dockerfile_relative) = if let Some(df) = dockerfile {
        let path = Path::new(app_path).join(df);
        (path, Some(df.to_string()))
    } else {
        // Auto-detect: Dockerfile first, then Containerfile
        let dockerfile = Path::new(app_path).join("Dockerfile");
        let containerfile = Path::new(app_path).join("Containerfile");
        if dockerfile.exists() && dockerfile.is_file() {
            (dockerfile, Some("Dockerfile".to_string()))
        } else if containerfile.exists() && containerfile.is_file() {
            info!("Detected Containerfile");
            (containerfile, Some("Containerfile".to_string()))
        } else {
            // No dockerfile found
            (Path::new(app_path).join("Dockerfile"), None)
        }
    };

    if let Some(backend_str) = backend {
        // Explicit backend specified
        let method = BuildMethod::from_backend_str(backend_str)?;
        Ok((method, dockerfile_relative))
    } else {
        // Auto-detect based on dockerfile presence
        if dockerfile_path.exists() && dockerfile_path.is_file() {
            info!(
                "Detected {}, using docker backend",
                dockerfile_relative.as_deref().unwrap_or("Dockerfile")
            );
            Ok((
                BuildMethod::Docker { use_buildx: false },
                dockerfile_relative,
            ))
        } else {
            info!("No Dockerfile found, using pack backend");
            Ok((BuildMethod::Pack, None))
        }
    }
}

/// Check if a build method requires BuildKit
pub(crate) fn requires_buildkit(method: &BuildMethod) -> bool {
    matches!(
        method,
        BuildMethod::Docker { use_buildx: true }
            | BuildMethod::Railpack { .. }
            | BuildMethod::Buildctl
    )
}
