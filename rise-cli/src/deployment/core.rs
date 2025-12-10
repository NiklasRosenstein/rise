use anyhow::{bail, Context, Result};
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Color, Table,
};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::Config;

// Re-export models from backend to ensure consistency
pub use rise_backend::deployment::models::{Deployment, DeploymentStatus};

/// Build method for container images
#[derive(Debug, Clone, Copy, PartialEq)]
enum BuildMethod {
    Docker,
    Pack,
    Railpack { use_buildctl: bool },
}

impl BuildMethod {
    /// Parse backend string into BuildMethod
    fn from_backend_str(backend: &str) -> Result<Self> {
        match backend {
            "docker" => Ok(BuildMethod::Docker),
            "pack" => Ok(BuildMethod::Pack),
            "railpack" | "railpack:buildx" => Ok(BuildMethod::Railpack {
                use_buildctl: false,
            }),
            "railpack:buildctl" => Ok(BuildMethod::Railpack { use_buildctl: true }),
            _ => bail!(
                "Invalid build backend '{}'. Supported: docker, pack, railpack, railpack:buildctl",
                backend
            ),
        }
    }
}

/// Select build method based on explicit backend or auto-detection
/// Returns BuildMethod based on backend string or directory contents
fn select_build_method(app_path: &str, backend: Option<&str>) -> Result<BuildMethod> {
    if let Some(backend_str) = backend {
        // Explicit backend specified
        BuildMethod::from_backend_str(backend_str)
    } else {
        // Auto-detect
        let dockerfile_path = Path::new(app_path).join("Dockerfile");
        if dockerfile_path.exists() && dockerfile_path.is_file() {
            info!("Detected Dockerfile, using docker backend");
            Ok(BuildMethod::Docker)
        } else {
            info!("No Dockerfile found, using pack backend");
            Ok(BuildMethod::Pack)
        }
    }
}

/// Check if a build method requires BuildKit
fn requires_buildkit(method: &BuildMethod) -> bool {
    matches!(method, BuildMethod::Docker | BuildMethod::Railpack { .. })
}

/// Compute SHA256 hash of a file
fn compute_file_hash(path: &Path) -> Result<String> {
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
fn create_buildkit_daemon(
    container_cli: &str,
    daemon_name: &str,
    ssl_cert_file: &Path,
) -> Result<()> {
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

    // Set BUILDKIT_HOST environment variable for subsequent commands
    std::env::set_var(
        "BUILDKIT_HOST",
        format!("docker-container://{}", daemon_name),
    );

    info!("BuildKit daemon '{}' created successfully", daemon_name);

    Ok(())
}

/// Ensure managed BuildKit daemon is running with correct SSL certificate
fn ensure_managed_buildkit_daemon(ssl_cert_file: &Path, container_cli: &str) -> Result<()> {
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
                    // Set BUILDKIT_HOST for this session
                    std::env::set_var(
                        "BUILDKIT_HOST",
                        format!("docker-container://{}", daemon_name),
                    );
                    return Ok(());
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

    // Create new daemon with certificate
    create_buildkit_daemon(container_cli, daemon_name, ssl_cert_file)?;

    Ok(())
}

/// Warn user about SSL certificate issues when managed BuildKit is disabled
fn check_ssl_cert_and_warn(method: &BuildMethod, managed_buildkit: bool) {
    if let Ok(_ssl_cert) = std::env::var("SSL_CERT_FILE") {
        if requires_buildkit(method) && !managed_buildkit {
            eprintln!("\nWarning: SSL_CERT_FILE is set but managed BuildKit daemon is disabled.");
            eprintln!();
            eprintln!(
                "Railpack builds may fail with SSL certificate errors in corporate environments."
            );
            eprintln!();
            eprintln!("To enable automatic BuildKit daemon management:");
            eprintln!("  rise build --managed-buildkit ...");
            eprintln!();
            eprintln!("Or set environment variable:");
            eprintln!("  export RISE_MANAGED_BUILDKIT=true");
            eprintln!();
            eprintln!("For manual setup, see: https://github.com/NiklasRosenstein/rise/issues/18");
            eprintln!();
        }
    }
}

#[derive(Debug, Deserialize)]
struct RollbackResponse {
    new_deployment_id: String,
    rolled_back_from: String,
    image_tag: String,
}

/// Parse duration string (e.g., "5m", "30s", "1h")
pub(super) fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Duration string is empty");
    }

    let (num_str, unit) = if let Some(num_str) = s.strip_suffix("ms") {
        (num_str, "ms")
    } else {
        let num_str = &s[..s.len() - 1];
        let unit = &s[s.len() - 1..];
        (num_str, unit)
    };

    let num: u64 = num_str.parse().context("Invalid duration number")?;

    let duration = match unit {
        "ms" => Duration::from_millis(num),
        "s" => Duration::from_secs(num),
        "m" => Duration::from_secs(num * 60),
        "h" => Duration::from_secs(num * 3600),
        _ => bail!("Invalid duration unit '{}'. Use ms, s, m, or h", unit),
    };

    Ok(duration)
}

/// Fetch deployment by project name and deployment_id
pub(super) async fn fetch_deployment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    deployment_id: &str,
) -> Result<Deployment> {
    let url = format!(
        "{}/projects/{}/deployments/{}",
        backend_url, project, deployment_id
    );

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to fetch deployment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to fetch deployment ({}): {}", status, error_text);
    }

    let deployment: Deployment = response
        .json()
        .await
        .context("Failed to parse deployment response")?;

    Ok(deployment)
}

/// List deployments for a project
pub async fn list_deployments(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    group: Option<&str>,
    limit: usize,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    if let Some(g) = group {
        info!(
            "Listing deployments for project '{}' in group '{}'",
            project, g
        );
    } else {
        info!("Listing deployments for project '{}'", project);
    }

    let mut url = format!("{}/projects/{}/deployments", backend_url, project);

    // Add group query parameter if provided
    if let Some(g) = group {
        url = format!("{}?group={}", url, urlencoding::encode(g));
    }

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to list deployments")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to list deployments ({}): {}", status, error_text);
    }

    let mut deployments: Vec<Deployment> = response
        .json()
        .await
        .context("Failed to parse deployments")?;

    // Limit results
    deployments.truncate(limit);

    if deployments.is_empty() {
        println!("No deployments found for project '{}'", project);
        return Ok(());
    }

    // Group deployments by deployment_group to find active (Healthy) ones
    let mut active_per_group = std::collections::HashMap::new();
    for deployment in &deployments {
        if deployment.status == DeploymentStatus::Healthy {
            active_per_group.insert(
                deployment.deployment_group.clone(),
                deployment.deployment_id.clone(),
            );
        }
    }

    // Find the active deployment in the default group (this is the project's active deployment)
    let default_active = active_per_group.get("default");

    // Create table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("DEPLOYMENT").add_attribute(Attribute::Bold),
            Cell::new("STATUS").add_attribute(Attribute::Bold),
            Cell::new("CREATED BY").add_attribute(Attribute::Bold),
            Cell::new("IMAGE").add_attribute(Attribute::Bold),
            Cell::new("GROUP").add_attribute(Attribute::Bold),
            Cell::new("EXPIRY").add_attribute(Attribute::Bold),
            Cell::new("CREATED").add_attribute(Attribute::Bold),
            Cell::new("URL").add_attribute(Attribute::Bold),
            Cell::new("ERROR").add_attribute(Attribute::Bold),
        ]);

    for deployment in deployments {
        // Just use deployment_id in the table, project is already in context
        let deployment_display = deployment.deployment_id.clone();
        let url = deployment.deployment_url.as_deref().unwrap_or("-");

        // Format created time (just show date and time, not full RFC3339)
        let created = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&deployment.created) {
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            deployment.created.clone()
        };

        // Format expiry time
        let expiry = if let Some(ref expires_at) = deployment.expires_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                expires_at.clone()
            }
        } else {
            "-".to_string()
        };

        // Determine if this deployment is active in its group
        let is_active_in_group =
            active_per_group.get(&deployment.deployment_group) == Some(&deployment.deployment_id);

        // Determine if this is the default group's active deployment (bold)
        let is_default_active = default_active == Some(&deployment.deployment_id);

        // Format image (show the image tag or "-" if not set)
        let image_display = deployment.image.as_deref().unwrap_or("-");

        // Create cells with appropriate styling
        let mut deployment_cell = Cell::new(&deployment_display);
        let mut status_cell = Cell::new(deployment.status.to_string());
        let mut created_by_cell = Cell::new(&deployment.created_by_email);
        let mut image_cell = Cell::new(image_display);
        let mut group_cell = Cell::new(&deployment.deployment_group);
        let mut expiry_cell = Cell::new(&expiry);
        let mut created_cell = Cell::new(&created);
        let mut url_cell = Cell::new(url);

        // Apply bold to the entire row if this is the default group's active deployment
        if is_default_active {
            deployment_cell = deployment_cell.add_attribute(Attribute::Bold);
            status_cell = status_cell.add_attribute(Attribute::Bold);
            created_by_cell = created_by_cell.add_attribute(Attribute::Bold);
            image_cell = image_cell.add_attribute(Attribute::Bold);
            group_cell = group_cell.add_attribute(Attribute::Bold);
            expiry_cell = expiry_cell.add_attribute(Attribute::Bold);
            created_cell = created_cell.add_attribute(Attribute::Bold);
            url_cell = url_cell.add_attribute(Attribute::Bold);
        }

        // Apply green color if this is active in its group
        if is_active_in_group {
            deployment_cell = deployment_cell.fg(Color::Green);
            status_cell = status_cell.fg(Color::Green);
        }

        // Create error cell with truncated message
        let error_cell = if let Some(ref error) = deployment.error_message {
            let truncated = if error.len() > 40 {
                format!("{}...", &error[..37])
            } else {
                error.clone()
            };
            Cell::new(truncated).fg(Color::Red)
        } else {
            Cell::new("-")
        };

        table.add_row(vec![
            deployment_cell,
            status_cell,
            created_by_cell,
            image_cell,
            group_cell,
            expiry_cell,
            created_cell,
            url_cell,
            error_cell,
        ]);
    }

    println!("{}", table);

    Ok(())
}

/// Show deployment details and optionally follow until terminal state
pub async fn show_deployment(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    deployment_id: &str,
    follow: bool,
    timeout_str: &str,
) -> Result<()> {
    if follow {
        // Use new enhanced UI for follow mode
        let deployment = super::follow_ui::follow_deployment_with_ui(
            http_client,
            backend_url,
            config,
            project,
            deployment_id,
            timeout_str,
        )
        .await?;

        // Exit with error if deployment failed
        if deployment.status == DeploymentStatus::Failed {
            if let Some(error) = deployment.error_message {
                bail!("Deployment failed: {}", error);
            } else {
                bail!("Deployment failed");
            }
        }

        Ok(())
    } else {
        // One-shot display (no follow)
        let token = config
            .token
            .as_ref()
            .context("Not logged in. Please run 'rise login' first.")?;

        let deployment =
            fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

        // Use the same UI as follow mode
        super::follow_ui::print_deployment_snapshot(&deployment);

        // Exit with error if deployment failed
        if deployment.status == DeploymentStatus::Failed {
            if let Some(error) = deployment.error_message {
                bail!("Deployment failed: {}", error);
            } else {
                bail!("Deployment failed");
            }
        }

        Ok(())
    }
}

/// Rollback to a previous deployment
///
/// Creates a new deployment with the same image as the reference deployment
pub async fn rollback_deployment(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    deployment_id: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    info!(
        "Rolling back project '{}' to deployment '{}'",
        project, deployment_id
    );

    println!(
        "Initiating rollback for project '{}' to deployment '{}'...",
        project, deployment_id
    );

    // Call the rollback endpoint
    let url = format!(
        "{}/projects/{}/deployments/{}/rollback",
        backend_url, project, deployment_id
    );
    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send rollback request")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        match status {
            reqwest::StatusCode::NOT_FOUND => {
                bail!(
                    "Deployment '{}' not found for project '{}'",
                    deployment_id,
                    project
                );
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                bail!("Authentication failed. Please run 'rise login' again.");
            }
            reqwest::StatusCode::FORBIDDEN => {
                bail!(
                    "You don't have permission to rollback project '{}'",
                    project
                );
            }
            reqwest::StatusCode::BAD_REQUEST => {
                bail!("Cannot rollback: {}", error_text);
            }
            _ => {
                bail!("Rollback failed ({}): {}", status, error_text);
            }
        }
    }

    let rollback_response: RollbackResponse = response
        .json()
        .await
        .context("Failed to parse rollback response")?;

    println!();
    println!("✓ Rollback initiated successfully!");
    println!(
        "  New deployment ID: {}",
        rollback_response.new_deployment_id
    );
    println!(
        "  Rolled back from:  {}",
        rollback_response.rolled_back_from
    );
    println!("  Using image:       {}", rollback_response.image_tag);
    println!();

    // Follow the new deployment to completion
    show_deployment(
        http_client,
        backend_url,
        config,
        project,
        &rollback_response.new_deployment_id,
        true,  // follow
        "10m", // timeout
    )
    .await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct StopDeploymentsResponse {
    stopped_count: usize,
    deployment_ids: Vec<String>,
}

/// Stop all deployments in a group for a project
pub async fn stop_deployments_by_group(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    group: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    info!(
        "Stopping deployments in group '{}' for project '{}'",
        group, project
    );

    let url = format!(
        "{}/projects/{}/deployments/stop?group={}",
        backend_url,
        project,
        urlencoding::encode(group)
    );

    let response = http_client
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to stop deployments")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to stop deployments ({}): {}", status, error_text);
    }

    let stop_response: StopDeploymentsResponse = response
        .json()
        .await
        .context("Failed to parse stop response")?;

    if stop_response.stopped_count == 0 {
        println!("No running deployments found in group '{}'", group);
    } else {
        println!(
            "✓ Stopped {} deployment(s) in group '{}':",
            stop_response.stopped_count, group
        );
        for deployment_id in &stop_response.deployment_ids {
            println!("  - {}", deployment_id);
        }
    }

    Ok(())
}

// ============================================================================
// Deployment Creation (merged from deploy.rs)
// ============================================================================

/// Build a container image locally without pushing
///
/// This function selects the build method based on explicit backend or auto-detection
/// and builds the image locally. For railpack buildx builds, the --load flag is
/// automatically added to ensure the image is available in the local daemon.
pub fn build_image(
    config: &Config,
    tag: &str,
    path: &str,
    backend: Option<&str>,
    builder: Option<&str>,
    container_cli: Option<&str>,
    managed_buildkit: Option<bool>,
) -> Result<()> {
    // Resolve container CLI
    let container_cli = container_cli
        .map(String::from)
        .unwrap_or_else(|| config.get_container_cli());

    debug!("Using container CLI: {}", container_cli);
    info!("Building image '{}' from path '{}'", tag, path);

    // Verify path exists
    let app_path = Path::new(path);
    if !app_path.exists() {
        bail!("Path '{}' does not exist", path);
    }
    if !app_path.is_dir() {
        bail!("Path '{}' is not a directory", path);
    }

    // Select build method
    let build_method = select_build_method(path, backend)?;

    // Check if managed BuildKit daemon should be used
    let use_managed_buildkit = managed_buildkit.unwrap_or_else(|| config.get_managed_buildkit());

    // Handle SSL certificate and BuildKit daemon management
    if let Ok(ssl_cert_file) = std::env::var("SSL_CERT_FILE") {
        if requires_buildkit(&build_method) {
            if use_managed_buildkit {
                let cert_path = Path::new(&ssl_cert_file);
                ensure_managed_buildkit_daemon(cert_path, &container_cli)?;
            } else {
                check_ssl_cert_and_warn(&build_method, use_managed_buildkit);
            }
        }
    }

    match build_method {
        BuildMethod::Docker => {
            if builder.is_some() {
                warn!("--builder flag is ignored when using docker build method");
            }

            build_image_with_dockerfile(
                path,
                tag,
                &container_cli,
                false, // use_buildx: always false for docker backend (use railpack:buildx for buildx)
                false, // push=false for local build
            )?;
        }
        BuildMethod::Pack => {
            build_image_with_buildpacks(path, tag, builder)?;
        }
        BuildMethod::Railpack { use_buildctl } => {
            if builder.is_some() {
                warn!("--builder flag is ignored when using railpack build method");
            }

            build_image_with_railpacks(
                path,
                tag,
                &container_cli,
                use_buildctl,
                false, // push=false for local build
            )?;
        }
    }

    info!("✓ Successfully built image '{}'", tag);
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RegistryCredentials {
    registry_url: String,
    username: String,
    password: String,
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CreateDeploymentResponse {
    deployment_id: String,
    image_tag: String,
    credentials: RegistryCredentials,
}
#[allow(clippy::too_many_arguments)]
pub async fn create_deployment(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_name: &str,
    path: &str,
    image: Option<&str>,
    group: Option<&str>,
    expires_in: Option<&str>,
    http_port: u16,
    backend: Option<&str>,
    builder: Option<&str>,
    container_cli: Option<&str>,
    managed_buildkit: Option<bool>,
) -> Result<()> {
    // Resolve which container CLI to use
    let container_cli = container_cli
        .map(String::from)
        .unwrap_or_else(|| config.get_container_cli());

    debug!("Using container CLI: {}", container_cli);
    if let Some(image_ref) = image {
        info!(
            "Deploying project '{}' with pre-built image '{}'",
            project_name, image_ref
        );
    } else {
        info!("Deploying project '{}' from path '{}'", project_name, path);

        // Verify path exists only when building from source
        let app_path = Path::new(path);
        if !app_path.exists() {
            bail!("Path '{}' does not exist", path);
        }
        if !app_path.is_dir() {
            bail!("Path '{}' is not a directory", path);
        }
    }

    // Check if managed BuildKit daemon should be used
    let use_managed_buildkit = managed_buildkit.unwrap_or_else(|| config.get_managed_buildkit());

    // Handle SSL certificate and BuildKit daemon management (only when building, not for pre-built images)
    if image.is_none() {
        if let Ok(ssl_cert_file) = std::env::var("SSL_CERT_FILE") {
            let build_method = select_build_method(path, backend)?;
            if requires_buildkit(&build_method) {
                if use_managed_buildkit {
                    let cert_path = Path::new(&ssl_cert_file);
                    ensure_managed_buildkit_daemon(cert_path, &container_cli)?;
                } else {
                    check_ssl_cert_and_warn(&build_method, use_managed_buildkit);
                }
            }
        }
    }

    // Get authentication token
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run 'rise login' first."))?;

    // Step 1: Create deployment and get deployment ID + credentials
    info!("Creating deployment for project '{}'", project_name);
    let deployment_info = call_create_deployment_api(
        http_client,
        backend_url,
        &token,
        project_name,
        image,
        group,
        expires_in,
        http_port,
    )
    .await?;

    info!("Deployment ID: {}", deployment_info.deployment_id);
    info!("Image tag: {}", deployment_info.image_tag);

    // Set up Ctrl+C handler to gracefully cancel deployment
    let deployment_id_clone = deployment_info.deployment_id.clone();
    let backend_url_clone = backend_url.to_string();
    let http_client_clone = http_client.clone();
    let token_clone = token.to_string();

    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            eprintln!("\n⚠️  Caught Ctrl+C, cancelling deployment...");

            // Cancel the deployment
            if let Err(e) = cancel_deployment(
                &http_client_clone,
                &backend_url_clone,
                &token_clone,
                &deployment_id_clone,
            )
            .await
            {
                eprintln!("Failed to cancel deployment: {}", e);
            } else {
                eprintln!("✓ Deployment cancelled");
            }

            std::process::exit(130); // Standard exit code for SIGINT
        }
    });

    if image.is_some() {
        // Pre-built image path: Skip build/push, backend already marked as Pushed
        info!("✓ Pre-built image deployment created");
    } else {
        // Build from source path: Execute build and push
        // Step 2: Login to registry if credentials provided (needed for pack --publish)
        if !deployment_info.credentials.username.is_empty() {
            info!("Logging into registry");
            if let Err(e) = docker_login(
                &container_cli,
                &deployment_info.credentials.registry_url,
                &deployment_info.credentials.username,
                &deployment_info.credentials.password,
            ) {
                update_deployment_status(
                    http_client,
                    backend_url,
                    &token,
                    &deployment_info.deployment_id,
                    "Failed",
                    Some(&e.to_string()),
                )
                .await?;
                return Err(e);
            }
        }

        // Step 3: Update status to 'building'
        update_deployment_status(
            http_client,
            backend_url,
            &token,
            &deployment_info.deployment_id,
            "Building",
            None,
        )
        .await?;

        // Step 4: Select build method and build image
        let build_method = select_build_method(path, backend)?;

        match build_method {
            BuildMethod::Docker => {
                // Build and push (buildx uses --push, regular docker/podman uses separate push)
                info!("Building image with docker: {}", deployment_info.image_tag);

                if let Err(e) = build_image_with_dockerfile(
                    path,
                    &deployment_info.image_tag,
                    &container_cli,
                    false, // use_buildx: could be a CLI flag in future
                    true,  // push: true for deployment
                ) {
                    update_deployment_status(
                        http_client,
                        backend_url,
                        &token,
                        &deployment_info.deployment_id,
                        "Failed",
                        Some(&e.to_string()),
                    )
                    .await?;
                    return Err(e);
                }

                // Image pushed (either via --push for buildx or separate push for docker/podman)
            }
            BuildMethod::Pack => {
                // Build with pack (buildpacks)
                info!("Building image with pack: {}", deployment_info.image_tag);
                if let Err(e) =
                    build_image_with_buildpacks(path, &deployment_info.image_tag, builder)
                {
                    update_deployment_status(
                        http_client,
                        backend_url,
                        &token,
                        &deployment_info.deployment_id,
                        "Failed",
                        Some(&e.to_string()),
                    )
                    .await?;
                    return Err(e);
                }

                // Step 5: Mark as pushing
                update_deployment_status(
                    http_client,
                    backend_url,
                    &token,
                    &deployment_info.deployment_id,
                    "Pushing",
                    None,
                )
                .await?;

                // Step 5a: Push image to registry
                info!("Pushing image to registry: {}", deployment_info.image_tag);
                if let Err(e) = docker_push(&container_cli, &deployment_info.image_tag) {
                    update_deployment_status(
                        http_client,
                        backend_url,
                        &token,
                        &deployment_info.deployment_id,
                        "Failed",
                        Some(&e.to_string()),
                    )
                    .await?;
                    return Err(e);
                }
            }
            BuildMethod::Railpack { use_buildctl } => {
                // Build and push in one step with Railpack
                info!(
                    "Building image with railpack: {}",
                    deployment_info.image_tag
                );

                if let Err(e) = build_image_with_railpacks(
                    path,
                    &deployment_info.image_tag,
                    &container_cli,
                    use_buildctl,
                    true, // push: true for deployment
                ) {
                    update_deployment_status(
                        http_client,
                        backend_url,
                        &token,
                        &deployment_info.deployment_id,
                        "Failed",
                        Some(&e.to_string()),
                    )
                    .await?;
                    return Err(e);
                }

                // Image already pushed via BuildKit output, skip separate push step
            }
        }

        // Step 6: Mark as pushed (controller will take over deployment)
        update_deployment_status(
            http_client,
            backend_url,
            &token,
            &deployment_info.deployment_id,
            "Pushed",
            None,
        )
        .await?;

        info!(
            "✓ Successfully pushed {} to {}",
            project_name, deployment_info.image_tag
        );
    }
    info!("  Deployment ID: {}", deployment_info.deployment_id);
    println!();

    // Step 7: Follow deployment until completion
    show_deployment(
        http_client,
        backend_url,
        config,
        project_name,
        &deployment_info.deployment_id,
        true,  // follow
        "10m", // timeout
    )
    .await?;

    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn call_create_deployment_api(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project_name: &str,
    image: Option<&str>,
    group: Option<&str>,
    expires_in: Option<&str>,
    http_port: u16,
) -> Result<CreateDeploymentResponse> {
    let url = format!("{}/deployments", backend_url);
    let mut payload = serde_json::json!({
        "project": project_name,
        "http_port": http_port,
    });

    // Add image field if provided
    if let Some(image_ref) = image {
        payload["image"] = serde_json::json!(image_ref);
    }

    // Add group field if provided (defaults to "default" on backend)
    if let Some(group_name) = group {
        payload["group"] = serde_json::json!(group_name);
    }

    // Add expires_in field if provided
    if let Some(expiration) = expires_in {
        payload["expires_in"] = serde_json::json!(expiration);
    }

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .context("Failed to create deployment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to create deployment ({}): {}", status, error_text);
    }

    let deployment_info: CreateDeploymentResponse = response
        .json()
        .await
        .context("Failed to parse deployment response")?;

    Ok(deployment_info)
}

/// Cancel a deployment by marking it as Cancelling
async fn cancel_deployment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    deployment_id: &str,
) -> Result<()> {
    let url = format!("{}/deployments/{}/status", backend_url, deployment_id);

    let payload = serde_json::json!({
        "status": "Cancelling"
    });

    let response = http_client
        .patch(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .context("Failed to cancel deployment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to cancel deployment ({}): {}", status, error_text);
    }

    Ok(())
}

async fn update_deployment_status(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    deployment_id: &str,
    status: &str,
    error_message: Option<&str>,
) -> Result<()> {
    let url = format!("{}/deployments/{}/status", backend_url, deployment_id);
    let mut payload = serde_json::json!({
        "status": status,
    });

    if let Some(error) = error_message {
        payload["error_message"] = serde_json::json!(error);
    }

    debug!("Updating deployment {} status to {}", deployment_id, status);

    let response = http_client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await;

    // Don't fail deployment if status update fails, just log it
    match response {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => {
            let status = resp.status();
            let error = resp.text().await.unwrap_or_else(|_| "Unknown".to_string());
            warn!("Failed to update deployment status: {} - {}", status, error);
            Ok(())
        }
        Err(e) => {
            warn!("Failed to update deployment status: {}", e);
            Ok(())
        }
    }
}

fn build_image_with_buildpacks(
    app_path: &str,
    image_tag: &str,
    builder: Option<&str>,
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

    // Default to paketobuildpacks/builder:base if no builder specified
    let builder_image = builder.unwrap_or("paketobuildpacks/builder:base");
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

fn build_image_with_dockerfile(
    app_path: &str,
    image_tag: &str,
    container_cli: &str,
    use_buildx: bool,
    push: bool,
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

    cmd.arg("build").arg("-t").arg(image_tag).arg(app_path);

    // Add platform flag for consistent architecture
    cmd.arg("--platform").arg("linux/amd64");

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

/// RAII guard for cleaning up temp files
struct CleanupGuard {
    path: std::path::PathBuf,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
            debug!("Cleaned up temp file: {}", self.path.display());
        }
    }
}

/// Build image with Railpacks
fn build_image_with_railpacks(
    app_path: &str,
    image_tag: &str,
    container_cli: &str,
    use_buildctl: bool,
    push: bool,
) -> Result<()> {
    // Check railpack CLI availability
    let railpack_check = Command::new("railpack").arg("--version").output();
    if railpack_check.is_err() {
        bail!(
            "railpack CLI not found. Ensure the railpack CLI is installed and available in PATH.\n\
             In production, this should be available in the rise-builder image."
        );
    }

    // Generate temp file paths with UUID
    let uuid = uuid::Uuid::new_v4();
    let temp_dir = std::env::temp_dir();
    let plan_file = temp_dir.join(format!("railpack-plan-{}.json", uuid));
    let info_file = temp_dir.join(format!("railpack-info-{}.json", uuid));

    // Set up cleanup guards
    let _plan_guard = CleanupGuard {
        path: plan_file.clone(),
    };
    let _info_guard = CleanupGuard {
        path: info_file.clone(),
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

    // Build with buildx or buildctl
    if use_buildctl {
        build_with_buildctl(app_path, &plan_file, image_tag, push)?;
    } else {
        build_with_buildx(app_path, &plan_file, image_tag, container_cli, push)?;
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

/// Build with buildctl
fn build_with_buildctl(
    app_path: &str,
    plan_file: &Path,
    image_tag: &str,
    push: bool,
) -> Result<()> {
    // Check buildctl availability
    let buildctl_check = Command::new("buildctl").arg("--version").output();
    if buildctl_check.is_err() {
        bail!("buildctl not found. Install buildctl or use railpack:buildx backend instead.");
    }

    info!("Building image with buildctl: {}", image_tag);

    println!(
        "Plafile contents: {}",
        std::fs::read_to_string(plan_file).unwrap_or_default()
    );

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

fn docker_push(container_cli: &str, image_tag: &str) -> Result<()> {
    info!("Pushing image to registry: {}", image_tag);

    let mut cmd = Command::new(container_cli);
    cmd.arg("push").arg(image_tag);

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} push", container_cli))?;

    if !status.success() {
        bail!("{} push failed with status: {}", container_cli, status);
    }

    Ok(())
}

fn docker_login(container_cli: &str, registry: &str, username: &str, password: &str) -> Result<()> {
    debug!(
        "Executing: {} login {} --username {} --password-stdin",
        container_cli, registry, username
    );

    let status = Command::new(container_cli)
        .arg("login")
        .arg(registry)
        .arg("--username")
        .arg(username)
        .arg("--password-stdin")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(password.as_bytes())?;
            }
            child.wait()
        })
        .with_context(|| format!("Failed to execute {} login", container_cli))?;

    if !status.success() {
        bail!("{} login failed with status: {}", container_cli, status);
    }

    Ok(())
}
