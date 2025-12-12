// BuildKit daemon lifecycle management

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use super::method::{requires_buildkit, BuildMethod};

/// Compute SHA256 hash of a file
pub(crate) fn compute_file_hash(path: &Path) -> Result<String> {
    let contents = fs::read(path).context("Failed to read certificate file")?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Represents the SSL certificate state of a BuildKit daemon
#[derive(Debug, PartialEq)]
enum DaemonState {
    /// Daemon has an SSL certificate with the given hash
    HasCert(String),
    /// Daemon was intentionally created without an SSL certificate
    NoCert,
    /// Daemon does not exist
    NotFound,
}

/// Get the current state of the BuildKit daemon
fn get_daemon_state(container_cli: &str, daemon_name: &str) -> DaemonState {
    // Check if daemon exists
    let inspect_status = Command::new(container_cli)
        .args(["inspect", daemon_name])
        .output();

    let Ok(output) = inspect_status else {
        return DaemonState::NotFound;
    };

    if !output.status.success() {
        return DaemonState::NotFound;
    }

    // Daemon exists, check for no_ssl_cert label
    let no_cert_output = Command::new(container_cli)
        .args([
            "inspect",
            "--format",
            "{{index .Config.Labels \"rise.no_ssl_cert\"}}",
            daemon_name,
        ])
        .output();

    if let Ok(output) = no_cert_output {
        if output.status.success() {
            let label_value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if label_value == "true" {
                return DaemonState::NoCert;
            }
        }
    }

    // Check for SSL cert hash label
    let cert_hash_output = Command::new(container_cli)
        .args([
            "inspect",
            "--format",
            "{{index .Config.Labels \"rise.ssl_cert_hash\"}}",
            daemon_name,
        ])
        .output();

    if let Ok(output) = cert_hash_output {
        if output.status.success() {
            let cert_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !cert_hash.is_empty() {
                return DaemonState::HasCert(cert_hash);
            }
        }
    }

    // Daemon exists but has no labels (old daemon or created externally)
    // Treat as NoCert to avoid assuming anything
    DaemonState::NoCert
}

/// Stop BuildKit daemon
fn stop_buildkit_daemon(container_cli: &str, daemon_name: &str) -> Result<()> {
    info!("Stopping existing BuildKit daemon '{}'", daemon_name);

    let status = Command::new(container_cli)
        .args(["stop", daemon_name])
        .status()
        .context("Failed to stop BuildKit daemon")?;

    if !status.success() {
        bail!("Failed to stop BuildKit daemon");
    }

    Ok(())
}

/// Create BuildKit daemon with optional SSL certificate mounted
/// Returns the BUILDKIT_HOST value to be used with this daemon
fn create_buildkit_daemon(
    container_cli: &str,
    daemon_name: &str,
    ssl_cert_file: Option<&Path>,
) -> Result<String> {
    if let Some(cert_path) = ssl_cert_file {
        info!(
            "Creating managed BuildKit daemon '{}' with SSL certificate: {}",
            daemon_name,
            cert_path.display()
        );
    } else {
        info!(
            "Creating managed BuildKit daemon '{}' without SSL certificate",
            daemon_name
        );
    }

    let mut cmd = Command::new(container_cli);
    cmd.args([
        "run",
        "--platform",
        "linux/amd64",
        "--privileged",
        "--name",
        daemon_name,
        "--rm",
        "-d",
    ]);

    // Add labels and volume mount based on SSL cert presence
    if let Some(cert_path) = ssl_cert_file {
        // Resolve certificate path to absolute path
        let cert_path_abs = if cert_path.is_absolute() {
            cert_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(cert_path)
        };

        let cert_path_abs = cert_path_abs
            .canonicalize()
            .context("Failed to resolve SSL certificate path")?;

        let cert_str = cert_path_abs
            .to_str()
            .context("SSL certificate path contains invalid UTF-8")?;

        // Compute hash of certificate file
        let cert_hash = compute_file_hash(&cert_path_abs)?;

        cmd.arg("--label")
            .arg(format!("rise.ssl_cert_file={}", cert_str))
            .arg("--label")
            .arg(format!("rise.ssl_cert_hash={}", cert_hash))
            .arg("--volume")
            .arg(format!(
                "{}:/etc/ssl/certs/ca-certificates.crt:ro",
                cert_str
            ));
    } else {
        // No SSL cert, add label to track this state
        cmd.arg("--label").arg("rise.no_ssl_cert=true");
    }

    cmd.arg("moby/buildkit");

    let status = cmd.status().context("Failed to start BuildKit daemon")?;

    if !status.success() {
        bail!("Failed to create BuildKit daemon");
    }

    info!("BuildKit daemon '{}' created successfully", daemon_name);

    // Return BUILDKIT_HOST value for this daemon
    Ok(format!("docker-container://{}", daemon_name))
}

/// Ensure managed BuildKit daemon is running with correct SSL certificate
/// Returns the BUILDKIT_HOST value to be used with this daemon
pub(crate) fn ensure_managed_buildkit_daemon(
    ssl_cert_file: Option<&Path>,
    container_cli: &str,
) -> Result<String> {
    let daemon_name = "rise-buildkit";

    // Get current daemon state
    let current_state = get_daemon_state(container_cli, daemon_name);

    match (ssl_cert_file, &current_state) {
        // Certificate provided, daemon has matching cert
        (Some(cert_path), DaemonState::HasCert(current_hash)) => {
            // Verify hash matches
            let cert_path_abs = cert_path
                .canonicalize()
                .context("Failed to resolve SSL certificate path")?;
            let expected_hash = compute_file_hash(&cert_path_abs)?;

            if current_hash == &expected_hash {
                debug!("BuildKit daemon is up-to-date with current SSL_CERT_FILE");
                return Ok(format!("docker-container://{}", daemon_name));
            }

            info!("SSL certificate has changed (hash mismatch), recreating daemon");
            stop_buildkit_daemon(container_cli, daemon_name)?;
        }

        // Certificate provided, but daemon has no cert label
        (Some(_), DaemonState::NoCert) => {
            info!("SSL certificate now available, recreating daemon with certificate");
            stop_buildkit_daemon(container_cli, daemon_name)?;
        }

        // No certificate, daemon has no cert label (matches)
        (None, DaemonState::NoCert) => {
            debug!("BuildKit daemon is up-to-date (no SSL certificate)");
            return Ok(format!("docker-container://{}", daemon_name));
        }

        // No certificate, but daemon has cert (mismatch)
        (None, DaemonState::HasCert(_)) => {
            info!("SSL certificate removed, recreating daemon without certificate");
            stop_buildkit_daemon(container_cli, daemon_name)?;
        }

        // Daemon doesn't exist
        (_, DaemonState::NotFound) => {
            // Will create new daemon below
        }
    }

    // Create new daemon with or without certificate and return its BUILDKIT_HOST
    create_buildkit_daemon(container_cli, daemon_name, ssl_cert_file)
}

/// Ensure buildx builder exists for the given BuildKit daemon
/// Returns the builder name to use
pub(crate) fn ensure_buildx_builder(container_cli: &str, buildkit_host: &str) -> Result<String> {
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

/// Warn user about SSL certificate issues when managed BuildKit is disabled
pub(crate) fn check_ssl_cert_and_warn(method: &BuildMethod, managed_buildkit: bool) {
    if let Ok(_ssl_cert) = std::env::var("SSL_CERT_FILE") {
        if requires_buildkit(method) && !managed_buildkit {
            warn!(
                "SSL_CERT_FILE is set but managed BuildKit daemon is disabled. \
                 Railpack builds may fail with SSL certificate errors in corporate environments."
            );
            warn!("To enable automatic BuildKit daemon management:");
            warn!("  rise build --managed-buildkit ...");
            warn!("Or set environment variable:");
            warn!("  export RISE_MANAGED_BUILDKIT=true");
            warn!("For manual setup, see: https://github.com/NiklasRosenstein/rise/issues/18");
        }
    }
}
