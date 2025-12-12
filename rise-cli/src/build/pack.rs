// Buildpacks implementation (pack CLI)

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info};

/// Build image using Cloud Native Buildpacks (pack CLI)
pub(crate) fn build_image_with_buildpacks(
    app_path: &str,
    image_tag: &str,
    builder: Option<&str>,
    buildpacks: &[String],
) -> Result<()> {
    // Check if pack CLI is available
    let pack_check = Command::new("pack").arg("version").output();

    if pack_check.is_err() {
        bail!(
            "pack CLI not found. Please install it from https://buildpacks.io/docs/tools/pack/\n\
             On macOS: brew install buildpacks/tap/pack\n\
             On Linux: see https://buildpacks.io/docs/tools/pack/"
        );
    }

    let builder_image = builder.unwrap_or("paketobuildpacks/builder-jammy-base");
    info!("Using builder: {}", builder_image);

    let mut cmd = Command::new("pack");
    cmd.arg("build")
        .arg(image_tag)
        .arg("--path")
        .arg(app_path)
        .arg("--docker-host")
        .arg("inherit")
        .arg("--network")
        .arg("host")
        .arg("--builder")
        .arg(builder_image)
        .arg("--platform")
        .arg("linux/amd64")
        .env("DOCKER_API_VERSION", "1.44");

    // Add buildpacks if specified
    if !buildpacks.is_empty() {
        info!("Using buildpacks: {:?}", buildpacks);
        for buildpack in buildpacks {
            cmd.arg("--buildpack").arg(buildpack);
        }
    }

    // Never use --publish - always build locally and push separately
    // This avoids code bifurcation and allows CA certificate injection

    // If SSL_CERT_FILE is set, inject CA certificate into lifecycle container
    if let Ok(ca_cert_path) = std::env::var("SSL_CERT_FILE") {
        let cert_path = Path::new(&ca_cert_path);

        // Validate the file exists
        if !cert_path.exists() {
            bail!("CA certificate file not found: {}", ca_cert_path);
        }

        // Convert to absolute path if relative
        let absolute_path = if cert_path.is_absolute() {
            cert_path.to_path_buf()
        } else {
            std::env::current_dir()
                .context("Failed to get current directory")?
                .join(cert_path)
        };

        // Resolve symlinks to get the actual file path
        let resolved_path = absolute_path.canonicalize().with_context(|| {
            format!(
                "Failed to resolve certificate path: {}",
                absolute_path.display()
            )
        })?;

        let resolved_path_str = resolved_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Certificate path contains invalid UTF-8"))?;

        // Mount the CA certificate into the lifecycle container
        cmd.arg("--volume").arg(format!(
            "{}:/etc/ssl/certs/ca-certificates.crt:ro",
            resolved_path_str
        ));

        // Tell the lifecycle container where to find the certificate
        cmd.arg("--env")
            .arg("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt");

        info!(
            "Injecting CA certificate from: {} (resolved from: {})",
            resolved_path_str, ca_cert_path
        );
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute pack build")?;

    if !status.success() {
        bail!("pack build failed with status: {}", status);
    }

    Ok(())
}
