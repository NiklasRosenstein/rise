// Docker/Dockerfile builds

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

use super::dockerfile_ssl::{preprocess_dockerfile_for_ssl, SslMountStrategy};
use super::registry::docker_push;

/// Options for building with Docker/Podman
pub(crate) struct DockerBuildOptions<'a> {
    pub app_path: &'a str,
    pub dockerfile: Option<&'a str>,
    pub image_tag: &'a str,
    pub container_cli: &'a str,
    pub use_buildx: bool,
    pub push: bool,
    pub buildkit_host: Option<&'a str>,
    pub env: &'a [String],
    pub build_context: Option<&'a str>,
    pub build_contexts: &'a std::collections::HashMap<String, String>,
}

/// Build image using Docker or Podman with a Dockerfile
pub(crate) fn build_image_with_dockerfile(options: DockerBuildOptions) -> Result<()> {
    // Check if container CLI is available
    let cli_check = Command::new(options.container_cli)
        .arg("--version")
        .output();
    if cli_check.is_err() {
        bail!(
            "{} CLI not found. Please install Docker or Podman.",
            options.container_cli
        );
    }

    // Check for SSL certificate and determine if preprocessing is needed
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

    // Warn if SSL cert is set but buildx is not being used
    if ssl_cert_path.is_some() && !options.use_buildx {
        warn!(
            "SSL_CERT_FILE is set but docker:build does not support BuildKit secrets. \
             Use 'docker:buildx' backend for SSL certificate support during builds."
        );
    }

    // Preprocess Dockerfile for SSL if using buildx and SSL cert is available
    let (_temp_dir, effective_dockerfile, ssl_strategy) =
        if options.use_buildx && ssl_cert_path.is_some() {
            let original_dockerfile = options
                .dockerfile
                .map(|df| Path::new(options.app_path).join(df))
                .unwrap_or_else(|| Path::new(options.app_path).join("Dockerfile"));

            if original_dockerfile.exists() {
                info!("SSL_CERT_FILE detected, preprocessing Dockerfile for secret mounts");
                let (temp_dir, processed_path, strategy) = preprocess_dockerfile_for_ssl(
                    &original_dockerfile,
                    ssl_cert_path.as_ref().unwrap(),
                )?;
                (Some(temp_dir), Some(processed_path), Some(strategy))
            } else {
                (
                    None,
                    options
                        .dockerfile
                        .map(|df| Path::new(options.app_path).join(df)),
                    None,
                )
            }
        } else {
            (
                None,
                options
                    .dockerfile
                    .map(|df| Path::new(options.app_path).join(df)),
                None,
            )
        };

    let mut cmd = Command::new(options.container_cli);

    // Only buildx supports --push during build
    // Regular docker build and podman build don't support --push
    let supports_push_flag = options.use_buildx;

    if options.use_buildx {
        // Check buildx availability
        let buildx_check = Command::new(options.container_cli)
            .args(["buildx", "version"])
            .output();
        if buildx_check.is_err() {
            bail!(
                "{} buildx not available. Install it or use docker:build backend instead.",
                options.container_cli
            );
        }

        cmd.arg("buildx");
        info!(
            "Building image with {} buildx: {}",
            options.container_cli, options.image_tag
        );
    } else {
        info!(
            "Building image with {}: {}",
            options.container_cli, options.image_tag
        );
    }

    cmd.arg("build").arg("-t").arg(options.image_tag);

    // Add dockerfile path if specified or preprocessed
    if let Some(ref df) = effective_dockerfile {
        cmd.arg("-f").arg(df);
    }

    // Add platform flag for consistent architecture
    cmd.arg("--platform").arg("linux/amd64");

    // Add SSL certificate based on mount strategy
    // Track cert file for cleanup if we copy it to build context
    let mut cert_cleanup_path: Option<PathBuf> = None;

    if options.use_buildx {
        if let (Some(ref cert_path), Some(strategy)) = (&ssl_cert_path, ssl_strategy) {
            match strategy {
                SslMountStrategy::Secret => {
                    // Use secret mount (default for certs ≤ 500KiB)
                    cmd.arg("--secret")
                        .arg(format!("id=SSL_CERT_FILE,src={}", cert_path.display()));
                }
                SslMountStrategy::Bind => {
                    // Copy cert to build context for bind mount (certs > 500KiB)
                    let build_context_path =
                        Path::new(options.build_context.unwrap_or(options.app_path));
                    let cert_dest = build_context_path.join(".rise-ssl-cert.crt");
                    std::fs::copy(cert_path, &cert_dest).with_context(|| {
                        format!(
                            "Failed to copy SSL certificate to build context: {}",
                            cert_dest.display()
                        )
                    })?;
                    debug!("Copied SSL certificate to build context for bind mount");
                    cert_cleanup_path = Some(cert_dest);
                }
            }
        }
    }

    // Add proxy build arguments
    let proxy_vars = super::proxy::read_and_transform_proxy_vars();
    if !proxy_vars.is_empty() {
        info!("Injecting proxy variables for docker build");
        for (key, value) in &proxy_vars {
            cmd.arg("--build-arg").arg(format!("{}={}", key, value));
        }
    }

    cmd.arg("--add-host")
        .arg("host.docker.internal:host-gateway");

    // Add user-specified build arguments
    for build_arg in options.env {
        cmd.arg("--build-arg").arg(build_arg);
    }

    // Add build contexts (additional named contexts for multi-stage builds)
    if !options.build_contexts.is_empty() {
        info!("Using {} build context(s)", options.build_contexts.len());
        for (name, path) in options.build_contexts {
            cmd.arg("--build-context").arg(format!("{}={}", name, path));
            debug!("Build context: {}={}", name, path);
        }
    }

    // Use custom build context or default to app_path
    let context_path = options.build_context.unwrap_or(options.app_path);
    cmd.arg(context_path);

    // Set BUILDKIT_HOST if provided and using buildx
    if options.use_buildx {
        if let Some(host) = options.buildkit_host {
            cmd.env("BUILDKIT_HOST", host);
        }
    }

    if options.push && supports_push_flag {
        // Only use --push with buildx
        cmd.arg("--push");
    } else if options.use_buildx && !options.push {
        // For buildx without push, we need --load to get image into local daemon
        cmd.arg("--load");
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} build", options.container_cli))?;

    // Clean up temporary SSL certificate file if we created one
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

    if !status.success() {
        bail!(
            "{} build failed with status: {}",
            options.container_cli,
            status
        );
    }

    // If push was requested but --push flag wasn't supported, need separate push
    if options.push && !supports_push_flag {
        docker_push(options.container_cli, options.image_tag)?;
    }

    Ok(())
}
