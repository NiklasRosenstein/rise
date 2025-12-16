use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    #[allow(dead_code)]
    pub repository: String,
}

/// Check backend version and warn if it doesn't match CLI version
pub async fn check_version_compatibility(http_client: &Client, backend_url: &str) -> Result<()> {
    let cli_version = env!("CARGO_PKG_VERSION");

    // Fetch backend version
    let url = format!("{}/api/v1/version", backend_url);
    let response = http_client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch backend version")?;

    if !response.status().is_success() {
        // Don't fail the command if version check fails, just warn
        warn!("Could not check backend version compatibility");
        return Ok(());
    }

    let backend_info: VersionInfo = response
        .json()
        .await
        .context("Failed to parse version response")?;

    let backend_version = &backend_info.version;

    // Parse versions for comparison
    let cli_parts: Vec<&str> = cli_version.split('.').collect();
    let backend_parts: Vec<&str> = backend_version.split('.').collect();

    if cli_parts.len() < 2 || backend_parts.len() < 2 {
        warn!(
            cli_version = cli_version,
            backend_version = backend_version,
            "Version mismatch: invalid version format"
        );
        return Ok(());
    }

    let cli_major = cli_parts[0];
    let cli_minor = cli_parts[1];
    let backend_major = backend_parts[0];
    let backend_minor = backend_parts[1];

    // Compare versions
    if cli_version == backend_version {
        // Versions match exactly
        info!(
            cli_version = cli_version,
            backend_version = backend_version,
            "CLI and backend versions match"
        );
    } else if cli_major != backend_major {
        // Different major versions
        warn!(
            cli_version = cli_version,
            backend_version = backend_version,
            cli_major = cli_major,
            backend_major = backend_major,
            "Version mismatch: major versions differ"
        );
    } else if cli_minor != backend_minor {
        // Same major, different minor
        warn!(
            cli_version = cli_version,
            backend_version = backend_version,
            cli_minor = cli_minor,
            backend_minor = backend_minor,
            "Version mismatch: minor versions differ"
        );
    } else {
        // Same major and minor, just patch difference
        info!(
            cli_version = cli_version,
            backend_version = backend_version,
            "CLI and backend patch versions differ"
        );
    }

    Ok(())
}
