// Build method selection and configuration

use anyhow::{bail, Result};
use clap::Args;
use std::path::Path;
use tracing::info;

use crate::config::Config;

/// Build method for container images
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BuildMethod {
    Docker,
    Pack,
    Railpack { use_buildctl: bool },
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

    /// Container CLI to use (docker or podman)
    #[arg(long)]
    pub container_cli: Option<String>,

    /// Enable managed BuildKit daemon with SSL certificate support
    #[arg(long)]
    pub managed_buildkit: bool,

    /// Embed SSL certificate into Railpack build plan for build-time RUN command support
    #[arg(long)]
    pub railpack_embed_ssl_cert: bool,
}

/// Options for building container images
#[derive(Debug, Clone)]
pub(crate) struct BuildOptions {
    pub image_tag: String,
    pub app_path: String,
    pub backend: Option<String>,
    pub builder: Option<String>,
    pub buildpacks: Vec<String>,
    pub container_cli: Option<String>,
    pub managed_buildkit: bool,
    pub railpack_embed_ssl_cert: bool,
    pub push: bool,
}

impl BuildOptions {
    /// Create BuildOptions from BuildArgs and Config
    pub(crate) fn from_build_args(
        _config: &Config,
        image_tag: String,
        app_path: String,
        build_args: &BuildArgs,
    ) -> Self {
        Self {
            image_tag,
            app_path,
            backend: build_args.backend.clone(),
            builder: build_args.builder.clone(),
            buildpacks: build_args.buildpacks.clone(),
            container_cli: build_args.container_cli.clone(),
            managed_buildkit: build_args.managed_buildkit,
            railpack_embed_ssl_cert: build_args.railpack_embed_ssl_cert,
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
            "docker" => Ok(BuildMethod::Docker),
            "pack" => Ok(BuildMethod::Pack),
            "railpack" | "railpack:buildx" => Ok(BuildMethod::Railpack {
                use_buildctl: false,
            }),
            "railpack:buildctl" => Ok(BuildMethod::Railpack { use_buildctl: true }),
            _ => bail!(
                "Invalid build backend '{}'. Supported: docker, pack, railpack, railpack:buildctl",
                backend
            ),
        }
    }
}

/// Select build method based on explicit backend or auto-detection
/// Returns BuildMethod based on backend string or directory contents
pub(crate) fn select_build_method(app_path: &str, backend: Option<&str>) -> Result<BuildMethod> {
    if let Some(backend_str) = backend {
        // Explicit backend specified
        BuildMethod::from_backend_str(backend_str)
    } else {
        // Auto-detect
        let dockerfile_path = Path::new(app_path).join("Dockerfile");
        if dockerfile_path.exists() && dockerfile_path.is_file() {
            info!("Detected Dockerfile, using docker backend");
            Ok(BuildMethod::Docker)
        } else {
            info!("No Dockerfile found, using pack backend");
            Ok(BuildMethod::Pack)
        }
    }
}

/// Check if a build method requires BuildKit
pub(crate) fn requires_buildkit(method: &BuildMethod) -> bool {
    matches!(method, BuildMethod::Docker | BuildMethod::Railpack { .. })
}
