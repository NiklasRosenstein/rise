// Build module - Container image building orchestration
//
// This module provides a clean API for building container images using various
// backends (Docker, Pack, Railpack) and handles related concerns like BuildKit
// daemon management, SSL certificate handling, and registry operations.

mod buildkit;
pub mod config;
mod docker;
mod dockerfile_ssl;
mod method;
mod pack;
mod proxy;
mod railpack;
mod registry;
mod ssl;

pub use method::BuildArgs;
pub(crate) use method::{BuildMethod, BuildOptions};
pub(crate) use railpack::{build_with_buildctl, BuildctlFrontend, RailpackBuildOptions};
pub(crate) use registry::docker_login;

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use buildkit::{check_ssl_cert_and_warn, ensure_managed_buildkit_daemon};
use docker::{build_image_with_dockerfile, DockerBuildOptions};
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
    let (build_method, dockerfile) = select_build_method(
        &options.app_path,
        options.backend.as_deref(),
        options.dockerfile.as_deref(),
    )?;

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

    // Resolve build_context relative to app_path
    let resolved_build_context = options.build_context.as_ref().map(|ctx| {
        let resolved = app_path.join(ctx);
        resolved.to_string_lossy().to_string()
    });

    // Resolve build_contexts paths relative to app_path
    let resolved_build_contexts: std::collections::HashMap<String, String> = options
        .build_contexts
        .iter()
        .map(|(name, path)| {
            let resolved = app_path.join(path);
            (name.clone(), resolved.to_string_lossy().to_string())
        })
        .collect();

    // Execute build based on selected method
    match build_method {
        BuildMethod::Docker { use_buildx } => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using docker build method");
            }
            if !options.buildpacks.is_empty() {
                warn!("--buildpack flags are ignored when using docker build method");
            }
            if options.railpack_embed_ssl_cert {
                warn!("--railpack-embed-ssl-cert flag is ignored when using docker build method");
            }

            build_image_with_dockerfile(DockerBuildOptions {
                app_path: &options.app_path,
                dockerfile: dockerfile.as_deref(),
                image_tag: &options.image_tag,
                container_cli,
                use_buildx,
                push: options.push,
                buildkit_host: buildkit_host.as_deref(),
                env: &options.env,
                build_context: resolved_build_context.as_deref(),
                build_contexts: &resolved_build_contexts,
            })?;
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
                &options.env,
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

            build_image_with_railpacks(RailpackBuildOptions {
                app_path: &options.app_path,
                image_tag: &options.image_tag,
                container_cli,
                use_buildctl,
                push: options.push,
                buildkit_host: buildkit_host.as_deref(),
                embed_ssl_cert: options.railpack_embed_ssl_cert,
                env: &options.env,
            })?;
        }
        BuildMethod::Buildctl => {
            if options.builder.is_some() {
                warn!("--builder flag is ignored when using buildctl build method");
            }
            if !options.buildpacks.is_empty() {
                warn!("--buildpack flags are ignored when using buildctl build method");
            }
            if options.container_cli.is_some() {
                warn!("--container-cli flag is ignored when using buildctl build method");
            }
            if options.railpack_embed_ssl_cert {
                warn!("--railpack-embed-ssl-cert flag is ignored when using buildctl build method");
            }

            // Check for SSL certificate
            let ssl_cert_file = std::env::var("SSL_CERT_FILE").ok();
            let ssl_cert_path = ssl_cert_file.as_ref().and_then(|p| {
                let path = Path::new(p);
                if path.exists() {
                    Some(path.to_path_buf())
                } else {
                    warn!("SSL_CERT_FILE set to '{}' but file not found", p);
                    None
                }
            });

            // Construct dockerfile path
            let original_dockerfile_path = dockerfile
                .as_ref()
                .map(|df| Path::new(&options.app_path).join(df))
                .unwrap_or_else(|| Path::new(&options.app_path).join("Dockerfile"));

            // Preprocess Dockerfile for SSL if cert is available
            let (_temp_dir, effective_dockerfile, ssl_strategy) = if ssl_cert_path.is_some() {
                if original_dockerfile_path.exists() {
                    info!("SSL_CERT_FILE detected, preprocessing Dockerfile for secret mounts");
                    let (temp_dir, processed_path, strategy) =
                        dockerfile_ssl::preprocess_dockerfile_for_ssl(
                            &original_dockerfile_path,
                            ssl_cert_path.as_ref().unwrap(),
                        )?;
                    (Some(temp_dir), processed_path, Some(strategy))
                } else {
                    (None, original_dockerfile_path, None)
                }
            } else {
                (None, original_dockerfile_path, None)
            };

            // Parse env vars into HashMap for secrets
            let mut secrets = proxy::read_and_transform_proxy_vars();
            for env_var in &options.env {
                if let Some((key, value)) = env_var.split_once('=') {
                    secrets.insert(key.to_string(), value.to_string());
                } else if let Ok(value) = std::env::var(env_var) {
                    secrets.insert(env_var.to_string(), value);
                }
            }

            // Add SSL cert based on mount strategy
            // Track cert file and dockerignore for cleanup if we copy cert to build context
            let mut cert_cleanup_path: Option<PathBuf> = None;
            let mut dockerignore_cleanup: Option<(PathBuf, Option<String>)> = None;

            if let (Some(ref cert_path), Some(strategy)) = (&ssl_cert_path, ssl_strategy) {
                match strategy {
                    dockerfile_ssl::SslMountStrategy::Secret => {
                        // Use secret mount (default for certs ≤ 500KiB)
                        secrets.insert(
                            "SSL_CERT_FILE".to_string(),
                            cert_path.to_string_lossy().to_string(),
                        );
                    }
                    dockerfile_ssl::SslMountStrategy::Bind => {
                        // Copy cert to build context for bind mount (certs > 500KiB)
                        let cert_dest = Path::new(&options.app_path).join(".rise-ssl-cert.crt");
                        std::fs::copy(cert_path, &cert_dest).context(format!(
                            "Failed to copy SSL certificate to build context: {}",
                            cert_dest.display()
                        ))?;
                        debug!("Copied SSL certificate to build context for bind mount");
                        cert_cleanup_path = Some(cert_dest);

                        // Add .rise-ssl-cert.crt to .dockerignore to prevent it from being copied into the image
                        let dockerignore_path = Path::new(&options.app_path).join(".dockerignore");
                        let original_content = if dockerignore_path.exists() {
                            Some(std::fs::read_to_string(&dockerignore_path).context(format!(
                                "Failed to read .dockerignore: {}",
                                dockerignore_path.display()
                            ))?)
                        } else {
                            None
                        };

                        // Check if .rise-ssl-cert.crt is already ignored
                        let needs_entry = original_content
                            .as_ref()
                            .map(|content| {
                                !content.lines().any(|line| {
                                    let trimmed = line.trim();
                                    trimmed == ".rise-ssl-cert.crt"
                                        || trimmed == ".rise-*"
                                        || trimmed == ".rise*"
                                })
                            })
                            .unwrap_or(true);

                        if needs_entry {
                            let new_content = if let Some(ref content) = original_content {
                                format!("{}\n.rise-ssl-cert.crt\n", content.trim_end())
                            } else {
                                ".rise-ssl-cert.crt\n".to_string()
                            };

                            std::fs::write(&dockerignore_path, &new_content).context(format!(
                                "Failed to write .dockerignore: {}",
                                dockerignore_path.display()
                            ))?;

                            debug!("Added .rise-ssl-cert.crt to .dockerignore");
                            dockerignore_cleanup = Some((dockerignore_path, original_content));
                        }
                    }
                }
            }

            let build_result = build_with_buildctl(
                &options.app_path,
                &effective_dockerfile,
                &options.image_tag,
                options.push,
                buildkit_host.as_deref(),
                &secrets,
                BuildctlFrontend::Dockerfile,
            );

            // Clean up temporary SSL certificate file and .dockerignore if we created/modified them
            if let Some(cert_path) = cert_cleanup_path {
                if cert_path.exists() {
                    if let Err(e) = std::fs::remove_file(&cert_path) {
                        warn!(
                            "Failed to clean up temporary SSL certificate file {}: {}",
                            cert_path.display(),
                            e
                        );
                    } else {
                        debug!(
                            "Cleaned up temporary SSL certificate file: {}",
                            cert_path.display()
                        );
                    }
                }
            }

            // Restore .dockerignore to original state
            if let Some((dockerignore_path, original_content)) = dockerignore_cleanup {
                if let Some(content) = original_content {
                    // Restore original .dockerignore
                    if let Err(e) = std::fs::write(&dockerignore_path, content) {
                        warn!(
                            "Failed to restore .dockerignore {}: {}",
                            dockerignore_path.display(),
                            e
                        );
                    } else {
                        debug!("Restored .dockerignore to original state");
                    }
                } else {
                    // Remove .dockerignore if it didn't exist before
                    if dockerignore_path.exists() {
                        if let Err(e) = std::fs::remove_file(&dockerignore_path) {
                            warn!(
                                "Failed to remove temporary .dockerignore {}: {}",
                                dockerignore_path.display(),
                                e
                            );
                        } else {
                            debug!("Removed temporary .dockerignore");
                        }
                    }
                }
            }

            build_result?;
        }
    }

    info!("✓ Successfully built image '{}'", options.image_tag);
    Ok(())
}
