use anyhow::{Context, Result, bail};
use comfy_table::{
    Attribute, Cell, Color, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL,
};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::config::Config;

// Re-export models from backend to ensure consistency
pub use rise_backend::deployment::models::{Deployment, DeploymentStatus};

#[derive(Debug, Deserialize)]
struct RollbackResponse {
    new_deployment_id: String,
    rolled_back_from: String,
    image_tag: String,
}

/// Parse deployment reference in project:deployment_id format
///
/// # Arguments
/// * `ref_str` - Reference string (e.g., "my-app:20241124-1542")
///
/// # Returns
/// Tuple of (project_name, deployment_id)
pub fn parse_deployment_ref(ref_str: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = ref_str.split(':').collect();
    if parts.len() != 2 {
        bail!(
            "Invalid deployment reference '{}'. Expected format: project:deployment_id (e.g., my-app:20241124-1542)",
            ref_str
        );
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse duration string (e.g., "5m", "30s", "1h")
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Duration string is empty");
    }

    let (num_str, unit) = if s.ends_with("ms") {
        (&s[..s.len() - 2], "ms")
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
async fn fetch_deployment(
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
        .token
        .as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

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
            Cell::new("GROUP").add_attribute(Attribute::Bold),
            Cell::new("EXPIRY").add_attribute(Attribute::Bold),
            Cell::new("CREATED").add_attribute(Attribute::Bold),
            Cell::new("URL").add_attribute(Attribute::Bold),
            Cell::new("ERROR").add_attribute(Attribute::Bold),
        ]);

    for deployment in deployments {
        let deployment_ref = format!("{}:{}", project, deployment.deployment_id);
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

        // Create cells with appropriate styling
        let mut deployment_cell = Cell::new(&deployment_ref);
        let mut status_cell = Cell::new(deployment.status.to_string());
        let mut group_cell = Cell::new(&deployment.deployment_group);
        let mut expiry_cell = Cell::new(&expiry);
        let mut created_cell = Cell::new(&created);
        let mut url_cell = Cell::new(url);

        // Apply bold to the entire row if this is the default group's active deployment
        if is_default_active {
            deployment_cell = deployment_cell.add_attribute(Attribute::Bold);
            status_cell = status_cell.add_attribute(Attribute::Bold);
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
    let token = config
        .token
        .as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

    let timeout = parse_duration(timeout_str)?;
    let start_time = Instant::now();

    debug!("Fetching deployment {}:{}", project, deployment_id);

    loop {
        let deployment =
            fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

        // Print deployment details
        print_deployment_details(&deployment, project);

        // Check if deployment has reached a final state (terminal or healthy)
        let is_done = matches!(
            deployment.status,
            DeploymentStatus::Healthy
                | DeploymentStatus::Cancelled
                | DeploymentStatus::Stopped
                | DeploymentStatus::Superseded
                | DeploymentStatus::Failed
        );

        if !follow || is_done {
            // Exit with error if deployment failed
            if deployment.status == DeploymentStatus::Failed {
                if let Some(error) = deployment.error_message {
                    bail!("Deployment failed: {}", error);
                } else {
                    bail!("Deployment failed");
                }
            }
            return Ok(());
        }

        // Check timeout
        if start_time.elapsed() >= timeout {
            bail!(
                "Timeout waiting for deployment to complete (status: {})",
                deployment.status
            );
        }

        // Wait before polling again
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Print deployment details
fn print_deployment_details(deployment: &Deployment, project: &str) {
    println!("\nDeployment: {}:{}", project, deployment.deployment_id);
    println!("Status:     {}", deployment.status);
    println!("Created:    {}", deployment.created);
    println!("Updated:    {}", deployment.updated);

    if let Some(url) = &deployment.deployment_url {
        println!("URL:        {}", url);
    }

    if let Some(completed) = &deployment.completed_at {
        println!("Completed:  {}", completed);
    }

    if let Some(error) = &deployment.error_message {
        println!("Error:      {}", error);
    }

    // Show controller metadata if not empty
    if !deployment.controller_metadata.is_null()
        && deployment.controller_metadata != serde_json::json!({})
    {
        if let Ok(metadata_str) = serde_json::to_string_pretty(&deployment.controller_metadata) {
            println!("\nController Metadata:");
            println!("{}", metadata_str);
        }
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
        .token
        .as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

    info!(
        "Rolling back project '{}' to deployment '{}'",
        project, deployment_id
    );

    println!("Initiating rollback to {}:{}...", project, deployment_id);

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
                bail!("Deployment '{}:{}' not found", project, deployment_id);
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
    println!("Following deployment progress...");
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
        .token
        .as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

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

#[derive(Debug, Deserialize)]
struct RegistryCredentials {
    registry_url: String,
    username: String,
    password: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CreateDeploymentResponse {
    deployment_id: String,
    image_tag: String,
    credentials: RegistryCredentials,
}

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
) -> Result<()> {
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

    // Get authentication token
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run 'rise login' first."))?;

    // Step 1: Create deployment and get deployment ID + credentials
    info!("Creating deployment for project '{}'", project_name);
    let deployment_info = call_create_deployment_api(
        http_client,
        backend_url,
        token,
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
                &deployment_info.credentials.registry_url,
                &deployment_info.credentials.username,
                &deployment_info.credentials.password,
            ) {
                update_deployment_status(
                    http_client,
                    backend_url,
                    token,
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
            token,
            &deployment_info.deployment_id,
            "Building",
            None,
        )
        .await?;

        // Step 4: Build and push image with buildpacks (--publish handles both)
        info!(
            "Building and publishing image with buildpacks: {}",
            deployment_info.image_tag
        );
        if let Err(e) = build_image_with_buildpacks(path, &deployment_info.image_tag) {
            update_deployment_status(
                http_client,
                backend_url,
                token,
                &deployment_info.deployment_id,
                "Failed",
                Some(&e.to_string()),
            )
            .await?;
            return Err(e);
        }

        // Step 5: Mark as pushed (controller will take over deployment)
        update_deployment_status(
            http_client,
            backend_url,
            token,
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
    println!("Following deployment progress...");
    println!();

    // Step 6: Follow deployment until completion
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

fn build_image_with_buildpacks(app_path: &str, image_tag: &str) -> Result<()> {
    // Check if pack CLI is available
    let pack_check = Command::new("pack").arg("version").output();

    if pack_check.is_err() {
        bail!(
            "pack CLI not found. Please install it from https://buildpacks.io/docs/tools/pack/\n\
             On macOS: brew install buildpacks/tap/pack\n\
             On Linux: see https://buildpacks.io/docs/tools/pack/"
        );
    }

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
        .arg("paketobuildpacks/builder:base")
        .arg("--publish")
        .env("DOCKER_API_VERSION", "1.44");

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute pack build")?;

    if !status.success() {
        bail!("pack build failed with status: {}", status);
    }

    Ok(())
}

fn docker_login(registry: &str, username: &str, password: &str) -> Result<()> {
    debug!(
        "Executing: docker login {} --username {} --password-stdin",
        registry, username
    );

    let status = Command::new("docker")
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
        .context("Failed to execute docker login")?;

    if !status.success() {
        bail!("docker login failed with status: {}", status);
    }

    Ok(())
}
