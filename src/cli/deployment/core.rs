use anyhow::{bail, Context, Result};
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Color, Table,
};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::build::{self, BuildOptions};
use crate::config::Config;

// Re-export models from API module (always available)
pub use crate::api::models::{Deployment, DeploymentStatus};

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
        "{}/api/v1/projects/{}/deployments/{}",
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

    let mut url = format!("{}/api/v1/projects/{}/deployments", backend_url, project);

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
        // Only show URL for Healthy deployments (inactive deployments can't be connected to)
        let url = if deployment.status == DeploymentStatus::Healthy {
            deployment.primary_url.as_deref().unwrap_or("-")
        } else {
            "-"
        };

        // Format created time (just show date and time, not full RFC3339)
        let created = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&deployment.created) {
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            deployment.created.clone()
        };

        // Format expiry time
        let expiry = if let Some(expires_at) = &deployment.expires_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                expires_at.to_string()
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
            let truncated: String = if error.len() > 40 {
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
        "{}/api/v1/projects/{}/deployments/{}/rollback",
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
        "{}/api/v1/projects/{}/deployments/stop?group={}",
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
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CreateDeploymentResponse {
    deployment_id: String,
    image_tag: String,
    credentials: RegistryCredentials,
}

/// Options for creating a deployment
pub struct DeploymentOptions<'a> {
    pub project_name: &'a str,
    pub path: &'a str,
    pub image: Option<&'a str>,
    pub group: Option<&'a str>,
    pub expires_in: Option<&'a str>,
    pub http_port: u16,
    pub build_args: &'a build::BuildArgs,
    pub from_deployment: Option<&'a str>,
    pub use_source_env_vars: bool,
}

pub async fn create_deployment(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    deploy_opts: DeploymentOptions<'_>,
) -> Result<()> {
    if let Some(from_deployment_id) = deploy_opts.from_deployment {
        info!(
            "Creating deployment for project '{}' from deployment '{}' with {} environment variables",
            deploy_opts.project_name,
            from_deployment_id,
            if deploy_opts.use_source_env_vars { "source" } else { "current project" }
        );
    } else if let Some(image_ref) = deploy_opts.image {
        info!(
            "Deploying project '{}' with pre-built image '{}'",
            deploy_opts.project_name, image_ref
        );
    } else {
        info!(
            "Deploying project '{}' from path '{}'",
            deploy_opts.project_name, deploy_opts.path
        );
    }

    // Get authentication token
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run 'rise login' first."))?;

    // Step 1: Create deployment and get deployment ID + credentials
    info!(
        "Creating deployment for project '{}'",
        deploy_opts.project_name
    );
    let deployment_info = call_create_deployment_api(
        http_client,
        backend_url,
        &token,
        deploy_opts.project_name,
        deploy_opts.image,
        deploy_opts.group,
        deploy_opts.expires_in,
        deploy_opts.http_port,
        deploy_opts.from_deployment,
        deploy_opts.use_source_env_vars,
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

    if deploy_opts.image.is_some() || deploy_opts.from_deployment.is_some() {
        // Pre-built image path or redeploy from existing deployment: Skip build/push, backend already marked as Pushed
        if deploy_opts.from_deployment.is_some() {
            info!("✓ Deployment created from existing deployment '{}' with {} environment variables",
                deploy_opts.from_deployment.unwrap(),
                if deploy_opts.use_source_env_vars { "source" } else { "current project" });
        } else {
            info!("✓ Pre-built image deployment created");
        }
    } else {
        // Build from source path: Execute build and push
        // Step 2: Login to registry if credentials provided
        if !deployment_info.credentials.username.is_empty() {
            info!("Logging into registry");
            if let Err(e) = build::docker_login(
                &deploy_opts
                    .build_args
                    .container_cli
                    .clone()
                    .unwrap_or_else(|| config.get_container_cli()),
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

        // Step 4: Build and push image using build module
        let options = BuildOptions::from_build_args(
            config,
            deployment_info.image_tag.clone(),
            deploy_opts.path.to_string(),
            deploy_opts.build_args,
        )
        .with_push(true);

        if let Err(e) = build::build_image(options) {
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

        // Step 5: Mark as pushed (controller will take over deployment)
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
            deploy_opts.project_name, deployment_info.image_tag
        );
    }
    info!("  Deployment ID: {}", deployment_info.deployment_id);
    println!();

    // Step 7: Follow deployment until completion
    show_deployment(
        http_client,
        backend_url,
        config,
        deploy_opts.project_name,
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
    from_deployment: Option<&str>,
    use_source_env_vars: bool,
) -> Result<CreateDeploymentResponse> {
    let url = format!("{}/api/v1/deployments", backend_url);
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

    // Add from_deployment field if provided
    if let Some(source_deployment_id) = from_deployment {
        payload["from_deployment"] = serde_json::json!(source_deployment_id);
        payload["use_source_env_vars"] = serde_json::json!(use_source_env_vars);
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
    let url = format!(
        "{}/api/v1/deployments/{}/status",
        backend_url, deployment_id
    );

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
    let url = format!(
        "{}/api/v1/deployments/{}/status",
        backend_url, deployment_id
    );
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

/// Parameters for get_logs function
pub struct GetLogsParams<'a> {
    pub project: &'a str,
    pub deployment_id: &'a str,
    pub follow: bool,
    pub tail: Option<usize>,
    pub timestamps: bool,
    pub since: Option<&'a str>,
}

/// Get logs from a deployment
pub async fn get_logs(
    http_client: &reqwest::Client,
    backend_url: &str,
    token: &str,
    params: GetLogsParams<'_>,
) -> anyhow::Result<()> {
    use futures::StreamExt;

    // Build URL with query parameters
    let mut url = format!(
        "{}/api/v1/projects/{}/deployments/{}/logs",
        backend_url, params.project, params.deployment_id
    );

    let mut query_params = vec![];
    let tail_param;
    let since_param;

    if params.follow {
        query_params.push("follow=true");
    }
    if let Some(t) = params.tail {
        tail_param = format!("tail={}", t);
        query_params.push(&tail_param);
    }
    if params.timestamps {
        query_params.push("timestamps=true");
    }
    if let Some(s) = params.since {
        // Parse duration like "5m", "1h" into seconds
        let seconds = parse_duration_to_seconds(s)?;
        since_param = format!("since={}", seconds);
        query_params.push(&since_param);
    }

    if !query_params.is_empty() {
        url.push('?');
        url.push_str(&query_params.join("&"));
    }

    debug!("Fetching logs from: {}", url);

    // Send request
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    // Check status
    if !response.status().is_success() {
        let status = response.status();
        let error = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown".to_string());
        return Err(anyhow::anyhow!(
            "Failed to get logs ({}): {}",
            status,
            error
        ));
    }

    // Setup Ctrl+C handler for graceful shutdown
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    // Stream response bytes
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    loop {
        tokio::select! {
            // Handle Ctrl+C
            _ = &mut ctrl_c => {
                debug!("Received Ctrl+C, stopping log stream");
                break;
            }
            // Process stream chunks
            chunk_result = stream.next() => {
                match chunk_result {
                    Some(Ok(chunk)) => {
                        let text = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&text);

                        // Process complete lines from buffer
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer.drain(..=newline_pos).collect::<String>();
                            let line = line.trim_end();

                            // Parse SSE format: lines starting with "data: "
                            if let Some(data) = line.strip_prefix("data: ") {
                                // Only print non-empty data lines
                                if !data.is_empty() {
                                    println!("{}", data);
                                }
                            } else if !line.is_empty() && !line.starts_with(':') {
                                // SSE comments start with ':', skip them
                                // Print other non-empty lines (in case format changes)
                                println!("{}", line);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("Stream error: {}", e));
                    }
                    None => {
                        // Stream ended
                        debug!("Log stream ended");
                        break;
                    }
                }
            }
        }
    }

    // Print any remaining buffered content
    if !buffer.is_empty() {
        let line = buffer.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            // Only print non-empty data
            if !data.is_empty() {
                println!("{}", data);
            }
        } else if !line.is_empty() && !line.starts_with(':') {
            println!("{}", line);
        }
    }

    Ok(())
}

/// Parse duration string (e.g., "5m", "1h", "30s") into seconds
fn parse_duration_to_seconds(duration: &str) -> anyhow::Result<i64> {
    let duration = duration.trim();

    if let Some(num_str) = duration.strip_suffix('s') {
        let num: i64 = num_str.parse()?;
        Ok(num)
    } else if let Some(num_str) = duration.strip_suffix('m') {
        let num: i64 = num_str.parse()?;
        Ok(num * 60)
    } else if let Some(num_str) = duration.strip_suffix('h') {
        let num: i64 = num_str.parse()?;
        Ok(num * 3600)
    } else if let Some(num_str) = duration.strip_suffix('d') {
        let num: i64 = num_str.parse()?;
        Ok(num * 86400)
    } else {
        Err(anyhow::anyhow!(
            "Invalid duration format '{}'. Use format like '5m', '1h', '30s', '7d'",
            duration
        ))
    }
}
