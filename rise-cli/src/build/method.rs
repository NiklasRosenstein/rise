// Build method selection and configuration

use anyhow::{bail, Result};
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

/// Options for building container images
#[derive(Debug, Clone)]
pub(crate) struct BuildOptions {
    pub image_tag: String,
    pub app_path: String,
    pub backend: Option<String>,
    pub builder: Option<String>,
    pub container_cli: String,
    pub managed_buildkit: bool,
    pub railpack_embed_ssl_cert: bool,
    pub push: bool,
}

impl BuildOptions {
    /// Create BuildOptions from Config with defaults
    pub(crate) fn from_config(config: &Config, image_tag: String, app_path: String) -> Self {
        Self {
            image_tag,
            app_path,
            backend: None,
            builder: None,
            container_cli: config.get_container_cli(),
            managed_buildkit: config.get_managed_buildkit(),
            railpack_embed_ssl_cert: config.get_railpack_embed_ssl_cert(),
            push: false,
        }
    }

    /// Builder method to set push flag
    pub(crate) fn with_push(mut self, push: bool) -> Self {
        self.push = push;
        self
    }

    /// Builder method to set backend
    pub(crate) fn with_backend(mut self, backend: Option<String>) -> Self {
        self.backend = backend;
        self
    }

    /// Builder method to set builder
    pub(crate) fn with_builder(mut self, builder: Option<String>) -> Self {
        self.builder = builder;
        self
    }

    /// Builder method to set container CLI
    pub(crate) fn with_container_cli(mut self, container_cli: String) -> Self {
        self.container_cli = container_cli;
        self
    }

    /// Builder method to set managed buildkit
    pub(crate) fn with_managed_buildkit(mut self, managed_buildkit: bool) -> Self {
        self.managed_buildkit = managed_buildkit;
        self
    }

    /// Builder method to set railpack embed SSL cert
    pub(crate) fn with_railpack_embed_ssl_cert(mut self, railpack_embed_ssl_cert: bool) -> Self {
        self.railpack_embed_ssl_cert = railpack_embed_ssl_cert;
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
