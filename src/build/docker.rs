// Docker/Dockerfile builds

use anyhow::{bail, Context, Result};
use std::process::Command;
use tracing::{debug, info};

use super::registry::docker_push;

/// Build image using Docker or Podman with a Dockerfile
pub(crate) fn build_image_with_dockerfile(
    app_path: &str,
    image_tag: &str,
    container_cli: &str,
    use_buildx: bool,
    push: bool,
    buildkit_host: Option<&str>,
) -> Result<()> {
    // Check if container CLI is available
    let cli_check = Command::new(container_cli).arg("--version").output();
    if cli_check.is_err() {
        bail!(
            "{} CLI not found. Please install Docker or Podman.",
            container_cli
        );
    }

    let mut cmd = Command::new(container_cli);

    // Only buildx supports --push during build
    // Regular docker build and podman build don't support --push
    let supports_push_flag = use_buildx;

    if use_buildx {
        // Check buildx availability
        let buildx_check = Command::new(container_cli)
            .args(["buildx", "version"])
            .output();
        if buildx_check.is_err() {
            bail!(
                "{} buildx not available. Install it or omit --use-buildx flag.",
                container_cli
            );
        }

        cmd.arg("buildx");
        info!(
            "Building image with {} buildx: {}",
            container_cli, image_tag
        );
    } else {
        info!("Building image with {}: {}", container_cli, image_tag);
    }

    cmd.arg("build").arg("-t").arg(image_tag);

    // Add platform flag for consistent architecture
    cmd.arg("--platform").arg("linux/amd64");

    // Add proxy build arguments
    let proxy_vars = super::proxy::read_and_transform_proxy_vars();
    if !proxy_vars.is_empty() {
        info!("Injecting proxy variables for docker build");
        for (key, value) in &proxy_vars {
            cmd.arg("--build-arg").arg(format!("{}={}", key, value));
        }
    }

    cmd.arg(app_path);

    // Set BUILDKIT_HOST if provided and using buildx
    if use_buildx {
        if let Some(host) = buildkit_host {
            cmd.env("BUILDKIT_HOST", host);
        }
    }

    if push && supports_push_flag {
        // Only use --push with buildx
        cmd.arg("--push");
    } else if use_buildx && !push {
        // For buildx without push, we need --load to get image into local daemon
        cmd.arg("--load");
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} build", container_cli))?;

    if !status.success() {
        bail!("{} build failed with status: {}", container_cli, status);
    }

    // If push was requested but --push flag wasn't supported, need separate push
    if push && !supports_push_flag {
        docker_push(container_cli, image_tag)?;
    }

    Ok(())
}
