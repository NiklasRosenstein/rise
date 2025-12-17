// Build module - Container image building orchestration
//
// This module provides a clean API for building container images using various
// backends (Docker, Pack, Railpack) and handles related concerns like BuildKit
// daemon management, SSL certificate handling, and registry operations.

mod buildkit;
mod docker;
mod method;
mod pack;
mod proxy;
mod railpack;
mod registry;
mod ssl;

pub use method::BuildArgs;
pub(crate) use method::{BuildMethod, BuildOptions};
pub(crate) use registry::docker_login;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use buildkit::{check_ssl_cert_and_warn, ensure_managed_buildkit_daemon};
use docker::build_image_with_dockerfile;
use method::{requires_buildkit, select_build_method};
use pack::build_image_with_buildpacks;
use railpack::build_image_with_railpacks;

/// Main entry point for building container images
pub(crate) fn build_image(options: BuildOptions) -> Result<()> {
    // Resolve container CLI - use explicit value or default to "docker"
    let container_cli = options.container_cli.as_deref().unwrap_or("docker");

    debug!("Using container CLI: {}", container_cli);
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

    // Handle BuildKit daemon management
    let buildkit_host = if requires_buildkit(&build_method) && options.managed_buildkit {
        // Priority 1: Use existing BUILDKIT_HOST if set
        if let Ok(existing_host) = std::env::var("BUILDKIT_HOST") {
            info!("Using existing BUILDKIT_HOST: {}", existing_host);
            Some(existing_host)
        } else {
            // Priority 2: Create/manage our own buildkit daemon
            let ssl_cert_path = std::env::var("SSL_CERT_FILE").ok().map(PathBuf::from);

            Some(ensure_managed_buildkit_daemon(
                ssl_cert_path.as_deref(),
                container_cli,
            )?)
        }
    } else {
        // Managed buildkit not enabled, warn if SSL cert is set
        check_ssl_cert_and_warn(&build_method, options.managed_buildkit);
        None
    };

    // Execute build based on selected method
    match build_method {
        BuildMethod::Docker => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using docker build method");
            }
            if !options.buildpacks.is_empty() {
                warn!("--buildpack flags are ignored when using docker build method");
            }
            if options.railpack_embed_ssl_cert {
                warn!("--railpack-embed-ssl-cert flag is ignored when using docker build method");
            }

            build_image_with_dockerfile(
                &options.app_path,
                &options.image_tag,
                container_cli,
                false, // use_buildx: always false for docker backend (use railpack:buildx for buildx)
                options.push,
                buildkit_host.as_deref(),
            )?;
        }
        BuildMethod::Pack => {
            if options.container_cli.is_some() {
                warn!("--container-cli flag is ignored when using pack build method");
            }
            if options.managed_buildkit {
                warn!("--managed-buildkit flag is ignored when using pack build method");
            }
            if options.railpack_embed_ssl_cert {
                warn!("--railpack-embed-ssl-cert flag is ignored when using pack build method");
            }

            build_image_with_buildpacks(
                &options.app_path,
                &options.image_tag,
                options.builder.as_deref(),
                &options.buildpacks,
                &options.pack_env,
            )?;

            // Pack doesn't support push during build, so push separately if requested
            if options.push {
                registry::docker_push(container_cli, &options.image_tag)?;
            }
        }
        BuildMethod::Railpack { use_buildctl } => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using railpack build method");
            }
            if !options.buildpacks.is_empty() {
                warn!("--buildpack flags are ignored when using railpack build method");
            }
            if use_buildctl && options.container_cli.is_some() {
                warn!("--container-cli flag is ignored when using railpack:buildctl build method");
            }

            build_image_with_railpacks(
                &options.app_path,
                &options.image_tag,
                container_cli,
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
