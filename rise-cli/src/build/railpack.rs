// Railpack builds (buildx & buildctl variants)

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use super::ssl::embed_ssl_cert_in_plan;

/// RAII guard for cleaning up temp files and directories
struct CleanupGuard {
    path: std::path::PathBuf,
    is_directory: bool,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            if self.is_directory {
                let _ = std::fs::remove_dir_all(&self.path);
                debug!("Cleaned up temp directory: {}", self.path.display());
            } else {
                let _ = std::fs::remove_file(&self.path);
                debug!("Cleaned up temp file: {}", self.path.display());
            }
        }
    }
}

/// Build image with Railpacks
pub(crate) fn build_image_with_railpacks(
    app_path: &str,
    image_tag: &str,
    container_cli: &str,
    use_buildctl: bool,
    push: bool,
    buildkit_host: Option<&str>,
    embed_ssl_cert: bool,
) -> Result<()> {
    // Check railpack CLI availability
    let railpack_check = Command::new("railpack").arg("--version").output();
    if railpack_check.is_err() {
        bail!(
            "railpack CLI not found. Ensure the railpack CLI is installed and available in PATH.\n\
             In production, this should be available in the rise-builder image."
        );
    }

    // Create .railpack-build directory in app_path
    let build_dir = Path::new(app_path).join(".railpack-build");
    let dir_existed = build_dir.exists();

    if !dir_existed {
        fs::create_dir(&build_dir).with_context(|| {
            format!("Failed to create build directory: {}", build_dir.display())
        })?;
    }

    let plan_file = build_dir.join("plan.json");
    let info_file = build_dir.join("info.json");

    // Set up cleanup guards
    // If we created the directory, clean up the entire directory
    // Otherwise, just clean up the individual files
    let _cleanup_guard = if !dir_existed {
        CleanupGuard {
            path: build_dir,
            is_directory: true,
        }
    } else {
        // When directory existed, we'll clean up files individually
        // Store the first file in the guard, we'll use a separate guard for the second
        CleanupGuard {
            path: plan_file.clone(),
            is_directory: false,
        }
    };

    let _info_guard = if dir_existed {
        Some(CleanupGuard {
            path: info_file.clone(),
            is_directory: false,
        })
    } else {
        None
    };

    info!("Running railpack prepare for: {}", app_path);

    // Run railpack prepare
    let mut cmd = Command::new("railpack");
    cmd.arg("prepare")
        .arg(app_path)
        .arg("--plan-out")
        .arg(&plan_file)
        .arg("--info-out")
        .arg(&info_file);

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute railpack prepare")?;

    if !status.success() {
        bail!("railpack prepare failed with status: {}", status);
    }

    // Verify plan file was created
    if !plan_file.exists() {
        bail!(
            "railpack prepare did not create plan file at {}",
            plan_file.display()
        );
    }

    info!("✓ Railpack prepare completed");

    // Embed SSL certificate if requested
    if embed_ssl_cert {
        if let Ok(ssl_cert_file) = std::env::var("SSL_CERT_FILE") {
            let cert_path = Path::new(&ssl_cert_file);
            if cert_path.exists() {
                embed_ssl_cert_in_plan(&plan_file, cert_path)?;
            } else {
                warn!(
                    "SSL_CERT_FILE set to '{}' but file not found",
                    ssl_cert_file
                );
            }
        } else {
            warn!(
                "--railpack-embed-ssl-cert specified but SSL_CERT_FILE environment variable not set"
            );
        }
    } else if std::env::var("SSL_CERT_FILE").is_ok() {
        // SSL_CERT_FILE is set but flag not used - warn user
        warn!(
            "SSL_CERT_FILE is set but --railpack-embed-ssl-cert not specified. \
             Build-time RUN commands may fail with SSL errors. \
             Use --railpack-embed-ssl-cert to embed certificate into build plan."
        );
    }

    // Debug log plan contents
    if let Ok(plan_contents) = fs::read_to_string(&plan_file) {
        debug!("Railpack plan.json contents:\n{}", plan_contents);
    }

    // Build with buildx or buildctl
    if use_buildctl {
        build_with_buildctl(app_path, &plan_file, image_tag, push, buildkit_host)?;
    } else {
        build_with_buildx(
            app_path,
            &plan_file,
            image_tag,
            container_cli,
            push,
            buildkit_host,
        )?;
    }

    Ok(())
}

/// Build with docker buildx
fn build_with_buildx(
    app_path: &str,
    plan_file: &Path,
    image_tag: &str,
    container_cli: &str,
    push: bool,
    buildkit_host: Option<&str>,
) -> Result<()> {
    // Check buildx availability
    let buildx_check = Command::new(container_cli)
        .args(["buildx", "version"])
        .output();
    if buildx_check.is_err() {
        bail!(
            "{} buildx not available. Install buildx or use railpack:buildctl backend instead.",
            container_cli
        );
    }

    info!(
        "Building image with {} buildx: {}",
        container_cli, image_tag
    );

    // If buildkit_host is provided, we need to create/use a builder pointing to it
    let builder_name = if let Some(host) = buildkit_host {
        Some(ensure_buildx_builder(container_cli, host)?)
    } else {
        None
    };

    let mut cmd = Command::new(container_cli);
    cmd.arg("buildx")
        .arg("build")
        .arg("--build-arg")
        .arg("BUILDKIT_SYNTAX=ghcr.io/railwayapp/railpack-frontend")
        .arg("-f")
        .arg(plan_file)
        .arg("-t")
        .arg(image_tag)
        .arg("--platform")
        .arg("linux/amd64");

    // Use the managed builder if available
    if let Some(builder) = builder_name {
        cmd.arg("--builder").arg(builder);
    }

    if push {
        cmd.arg("--push");
    } else {
        // For local builds, use --load to ensure image is available in local daemon
        cmd.arg("--load");
    }

    cmd.arg(app_path);

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} buildx build", container_cli))?;

    if !status.success() {
        bail!(
            "{} buildx build failed with status: {}",
            container_cli,
            status
        );
    }

    Ok(())
}

/// Ensure buildx builder exists for the given BuildKit daemon
/// Returns the builder name to use
fn ensure_buildx_builder(container_cli: &str, buildkit_host: &str) -> Result<String> {
    let builder_name = "rise-buildkit";

    // Check if builder already exists
    let inspect_status = Command::new(container_cli)
        .args(["buildx", "inspect", builder_name])
        .output();

    match inspect_status {
        Ok(output) if output.status.success() => {
            // Builder exists, check if it's pointing to the correct endpoint
            let inspect_output = String::from_utf8_lossy(&output.stdout);

            // Check if the buildkit_host appears in the inspect output
            // The output contains lines like "Endpoint: docker-container://rise-buildkit"
            if inspect_output.contains(buildkit_host) {
                debug!(
                    "Buildx builder '{}' already exists with correct endpoint",
                    builder_name
                );
                return Ok(builder_name.to_string());
            }

            // Builder exists but points to wrong endpoint, remove and recreate
            info!(
                "Buildx builder '{}' exists but points to different endpoint, recreating",
                builder_name
            );
            let _ = Command::new(container_cli)
                .args(["buildx", "rm", builder_name])
                .status();
        }
        _ => {
            info!(
                "Creating buildx builder '{}' for BuildKit daemon: {}",
                builder_name, buildkit_host
            );
        }
    }

    // Create new builder pointing to the BuildKit daemon
    let status = Command::new(container_cli)
        .args(["buildx", "create", "--name", builder_name, buildkit_host])
        .status()
        .context("Failed to create buildx builder")?;

    if !status.success() {
        bail!("Failed to create buildx builder '{}'", builder_name);
    }

    info!("Buildx builder '{}' created successfully", builder_name);
    Ok(builder_name.to_string())
}

/// Build with buildctl
fn build_with_buildctl(
    app_path: &str,
    plan_file: &Path,
    image_tag: &str,
    push: bool,
    buildkit_host: Option<&str>,
) -> Result<()> {
    // Check buildctl availability
    let buildctl_check = Command::new("buildctl").arg("--version").output();
    if buildctl_check.is_err() {
        bail!("buildctl not found. Install buildctl or use railpack:buildx backend instead.");
    }

    info!("Building image with buildctl: {}", image_tag);

    let mut cmd = Command::new("buildctl");
    cmd.arg("build")
        .arg("--local")
        .arg(format!("context={}", app_path))
        .arg("--local")
        .arg(format!("dockerfile={}", plan_file.display()))
        .arg("--frontend=gateway.v0")
        .arg("--opt")
        .arg("source=ghcr.io/railwayapp/railpack-frontend")
        .arg("--output");

    // Set BUILDKIT_HOST if provided
    if let Some(host) = buildkit_host {
        cmd.env("BUILDKIT_HOST", host);
    }

    if push {
        cmd.arg(format!(
            "type=image,name={},push=true,platform=linux/amd64",
            image_tag
        ));
    } else {
        cmd.arg(format!(
            "type=image,name={},platform=linux/amd64",
            image_tag
        ));
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute buildctl build")?;

    if !status.success() {
        bail!("buildctl build failed with status: {}", status);
    }

    Ok(())
}
