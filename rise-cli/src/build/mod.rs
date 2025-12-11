// Build module - Container image building orchestration
//
// This module provides a clean API for building container images using various
// backends (Docker, Pack, Railpack) and handles related concerns like BuildKit
// daemon management, SSL certificate handling, and registry operations.

mod buildkit;
mod docker;
mod method;
mod pack;
mod railpack;
mod registry;
mod ssl;

pub use method::BuildArgs;
pub(crate) use method::{BuildMethod, BuildOptions};

use anyhow::{bail, Result};
use std::path::Path;
use tracing::{debug, info, warn};

use buildkit::{check_ssl_cert_and_warn, ensure_managed_buildkit_daemon};
use docker::build_image_with_dockerfile;
use method::{requires_buildkit, select_build_method};
use pack::build_image_with_buildpacks;
use railpack::build_image_with_railpacks;

/// Main entry point for building container images
pub(crate) fn build_image(options: BuildOptions) -> Result<()> {
    debug!("Using container CLI: {}", options.container_cli);
    info!(
        "Building image '{}' from path '{}'",
        options.image_tag, options.app_path
    );

    // Verify path exists
    let app_path = Path::new(&options.app_path);
    if !app_path.exists() {
        bail!("Path '{}' does not exist", options.app_path);
    }
    if !app_path.is_dir() {
        bail!("Path '{}' is not a directory", options.app_path);
    }

    // Select build method
    let build_method = select_build_method(&options.app_path, options.backend.as_deref())?;

    // Handle SSL certificate and BuildKit daemon management
    let buildkit_host = if let Ok(ssl_cert_file) = std::env::var("SSL_CERT_FILE") {
        if requires_buildkit(&build_method) {
            if options.managed_buildkit {
                let cert_path = Path::new(&ssl_cert_file);
                Some(ensure_managed_buildkit_daemon(
                    cert_path,
                    &options.container_cli,
                )?)
            } else {
                check_ssl_cert_and_warn(&build_method, options.managed_buildkit);
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Execute build based on selected method
    match build_method {
        BuildMethod::Docker => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using docker build method");
            }

            build_image_with_dockerfile(
                &options.app_path,
                &options.image_tag,
                &options.container_cli,
                false, // use_buildx: always false for docker backend (use railpack:buildx for buildx)
                options.push,
                buildkit_host.as_deref(),
            )?;
        }
        BuildMethod::Pack => {
            build_image_with_buildpacks(
                &options.app_path,
                &options.image_tag,
                options.builder.as_deref(),
            )?;

            // Pack doesn't support push during build, so push separately if requested
            if options.push {
                registry::docker_push(&options.container_cli, &options.image_tag)?;
            }
        }
        BuildMethod::Railpack { use_buildctl } => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using railpack build method");
            }

            build_image_with_railpacks(
                &options.app_path,
                &options.image_tag,
                &options.container_cli,
                use_buildctl,
                options.push,
                buildkit_host.as_deref(),
                options.railpack_embed_ssl_cert,
            )?;
        }
    }

    info!("✓ Successfully built image '{}'", options.image_tag);
    Ok(())
}

/// Login to a registry (for use by deployment module)
pub(crate) fn login_to_registry(
    container_cli: &str,
    registry: &str,
    username: &str,
    password: &str,
) -> Result<()> {
    registry::docker_login(container_cli, registry, username, password)
}
