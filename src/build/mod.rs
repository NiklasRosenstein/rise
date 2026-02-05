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

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use buildkit::{check_ssl_cert_and_warn, ensure_managed_buildkit_daemon};
use docker::{build_image_with_dockerfile, DockerBuildOptions};
use method::{requires_buildkit, select_build_method};
use pack::build_image_with_buildpacks;
use railpack::build_image_with_railpacks;

/// Read an environment variable, treating empty strings as if the variable is not set.
///
/// This helper ensures that empty environment variables (e.g., `SSL_CERT_FILE=""`) are
/// handled the same as unset variables, avoiding errors when code attempts to use
/// empty paths or values.
pub(crate) fn env_var_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    })
}

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
        container_cli,
    )?;

    // Determine if we should use managed buildkit
    let managed_buildkit = match options.managed_buildkit {
        Some(value) => {
            // Explicitly set by user (CLI flag, config, or env var)
            value
        }
        None => {
            // Auto-detect: enable if all conditions met:
            // 1. Backend requires BuildKit
            // 2. SSL_CERT_FILE is set (needs injection)
            // 3. BUILDKIT_HOST is NOT set (user not managing their own)
            env_var_non_empty("BUILDKIT_HOST").is_none()
                && requires_buildkit(&build_method)
                && env_var_non_empty("SSL_CERT_FILE").is_some()
        }
    };

    // Handle BuildKit daemon management
    let buildkit_host = if requires_buildkit(&build_method) && managed_buildkit {
        // Check if user already has BUILDKIT_HOST (even if managed_buildkit=true)
        if let Some(existing_host) = env_var_non_empty("BUILDKIT_HOST") {
            info!("Using existing BUILDKIT_HOST: {}", existing_host);
            Some(existing_host)
        } else {
            // Create/manage our own buildkit daemon
            let ssl_cert_path = env_var_non_empty("SSL_CERT_FILE").map(PathBuf::from);
            Some(ensure_managed_buildkit_daemon(
                ssl_cert_path.as_deref(),
                container_cli,
            )?)
        }
    } else {
        // Check for SSL cert warnings if managed buildkit disabled
        check_ssl_cert_and_warn(&build_method, managed_buildkit);
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
            if options.managed_buildkit.is_some() {
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
            let ssl_cert_file = env_var_non_empty("SSL_CERT_FILE");
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
            let (_temp_dir, effective_dockerfile) = if ssl_cert_path.is_some() {
                if original_dockerfile_path.exists() {
                    info!("SSL_CERT_FILE detected, preprocessing Dockerfile for bind mounts");
                    let (temp_dir, processed_path) =
                        dockerfile_ssl::preprocess_dockerfile_for_ssl(&original_dockerfile_path)?;
                    (Some(temp_dir), processed_path)
                } else {
                    (None, original_dockerfile_path)
                }
            } else {
                (None, original_dockerfile_path)
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

            // Add SSL cert using named build context (bind mount)
            // RAII cleanup via SslCertContext drop
            let mut local_contexts = HashMap::new();
            let _ssl_cert_context: Option<dockerfile_ssl::SslCertContext> =
                if let Some(ref cert_path) = ssl_cert_path {
                    // Create temp directory with cert for bind mount
                    // Using a separate local context keeps the cert separate from the main context
                    // and reduces risk of accidental inclusion via generic COPY commands
                    let context = dockerfile_ssl::SslCertContext::new(cert_path)?;

                    // Add to local_contexts map for buildctl --local argument
                    local_contexts.insert(
                        dockerfile_ssl::SSL_CERT_BUILD_CONTEXT.to_string(),
                        context.context_path.to_string_lossy().to_string(),
                    );

                    Some(context)
                } else {
                    None
                };

            build_with_buildctl(
                &options.app_path,
                &effective_dockerfile,
                &options.image_tag,
                options.push,
                buildkit_host.as_deref(),
                &secrets,
                &local_contexts,
                BuildctlFrontend::Dockerfile,
            )?;

            // Note: SslCertContext cleanup is automatic via RAII when it goes out of scope
        }
    }

    info!("✓ Successfully built image '{}'", options.image_tag);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_non_empty_with_empty_string() {
        // Test that empty string is treated as unset
        std::env::set_var("TEST_EMPTY_VAR", "");
        assert_eq!(env_var_non_empty("TEST_EMPTY_VAR"), None);
        std::env::remove_var("TEST_EMPTY_VAR");
    }

    #[test]
    fn test_env_var_non_empty_with_value() {
        // Test that non-empty value is returned
        std::env::set_var("TEST_VALUE_VAR", "some_value");
        assert_eq!(
            env_var_non_empty("TEST_VALUE_VAR"),
            Some("some_value".to_string())
        );
        std::env::remove_var("TEST_VALUE_VAR");
    }

    #[test]
    fn test_env_var_non_empty_with_unset() {
        // Test that unset variable returns None
        std::env::remove_var("TEST_UNSET_VAR");
        assert_eq!(env_var_non_empty("TEST_UNSET_VAR"), None);
    }

    #[test]
    fn test_env_var_non_empty_with_whitespace() {
        // Test that whitespace-only string is NOT treated as empty
        // (only fully empty strings are treated as unset)
        std::env::set_var("TEST_WHITESPACE_VAR", "   ");
        assert_eq!(
            env_var_non_empty("TEST_WHITESPACE_VAR"),
            Some("   ".to_string())
        );
        std::env::remove_var("TEST_WHITESPACE_VAR");
    }
}
