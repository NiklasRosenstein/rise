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

/// Get the SSL_CERT_FILE hash from daemon labels
fn get_daemon_cert_hash(container_cli: &str, daemon_name: &str) -> Result<String> {
    let output = Command::new(container_cli)
        .args([
            "inspect",
            "--format",
            "{{index .Config.Labels \"rise.ssl_cert_hash\"}}",
            daemon_name,
        ])
        .output()
        .context("Failed to inspect BuildKit daemon")?;

    if !output.status.success() {
        bail!("Failed to get daemon certificate hash label");
    }

    let cert_hash = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in daemon label")?
        .trim()
        .to_string();

    Ok(cert_hash)
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

/// Create BuildKit daemon with SSL certificate mounted
/// Returns the BUILDKIT_HOST value to be used with this daemon
fn create_buildkit_daemon(
    container_cli: &str,
    daemon_name: &str,
    ssl_cert_file: &Path,
) -> Result<String> {
    info!(
        "Creating managed BuildKit daemon '{}' with SSL certificate: {}",
        daemon_name,
        ssl_cert_file.display()
    );

    // Resolve certificate path to absolute path
    let cert_path = if ssl_cert_file.is_absolute() {
        ssl_cert_file.to_path_buf()
    } else {
        std::env::current_dir()?.join(ssl_cert_file)
    };

    let cert_path = cert_path
        .canonicalize()
        .context("Failed to resolve SSL certificate path")?;

    let cert_str = cert_path
        .to_str()
        .context("SSL certificate path contains invalid UTF-8")?;

    // Compute hash of certificate file
    let cert_hash = compute_file_hash(&cert_path)?;

    let status = Command::new(container_cli)
        .args([
            "run",
            "--platform",
            "linux/amd64",
            "--privileged",
            "--name",
            daemon_name,
            "--rm",
            "-d",
            "--label",
            &format!("rise.ssl_cert_file={}", cert_str),
            "--label",
            &format!("rise.ssl_cert_hash={}", cert_hash),
            "--volume",
            &format!("{}:/etc/ssl/certs/ca-certificates.crt:ro", cert_str),
            "moby/buildkit",
        ])
        .status()
        .context("Failed to start BuildKit daemon")?;

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
    ssl_cert_file: &Path,
    container_cli: &str,
) -> Result<String> {
    let daemon_name = "rise-buildkit";

    // Check if daemon exists
    let status = Command::new(container_cli)
        .args(["inspect", daemon_name])
        .output()
        .context("Failed to check for existing BuildKit daemon")?;

    if status.status.success() {
        // Daemon exists, verify SSL_CERT_FILE hash hasn't changed
        match get_daemon_cert_hash(container_cli, daemon_name) {
            Ok(current_hash) => {
                // Resolve and compute hash of current certificate file
                let cert_path = ssl_cert_file
                    .canonicalize()
                    .context("Failed to resolve SSL certificate path")?;
                let expected_hash = compute_file_hash(&cert_path)?;

                if current_hash == expected_hash {
                    debug!("BuildKit daemon is up-to-date with current SSL_CERT_FILE");
                    // Return BUILDKIT_HOST for this daemon
                    return Ok(format!("docker-container://{}", daemon_name));
                }

                info!("SSL certificate has changed (hash mismatch), recreating daemon");
                stop_buildkit_daemon(container_cli, daemon_name)?;
            }
            Err(e) => {
                warn!(
                    "Failed to get daemon certificate hash: {}, recreating daemon",
                    e
                );
                stop_buildkit_daemon(container_cli, daemon_name)?;
            }
        }
    }

    // Create new daemon with certificate and return its BUILDKIT_HOST
    create_buildkit_daemon(container_cli, daemon_name, ssl_cert_file)
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
