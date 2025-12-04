use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::config::Config;

/// Deployment model matching the API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Deployment {
    pub id: String,
    pub deployment_id: String,
    pub project: String,
    pub created_by: String,
    pub status: DeploymentStatus,
    pub error_message: Option<String>,
    pub completed_at: Option<String>,
    pub build_logs: Option<String>,
    pub controller_metadata: serde_json::Value,
    pub deployment_url: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum DeploymentStatus {
    Pending,
    Building,
    Pushing,
    Pushed,
    Deploying,
    Completed,
    Failed,
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Pushed => write!(f, "Pushed"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Completed => write!(f, "Completed"),
            DeploymentStatus::Failed => write!(f, "Failed"),
        }
    }
}

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
    let url = format!("{}/projects/{}/deployments/{}", backend_url, project, deployment_id);

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to fetch deployment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to fetch deployment ({}): {}", status, error_text);
    }

    let deployment: Deployment = response.json().await.context("Failed to parse deployment response")?;

    Ok(deployment)
}

/// List deployments for a project
pub async fn list_deployments(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    limit: usize,
) -> Result<()> {
    let token = config.token.as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

    info!("Listing deployments for project '{}'", project);

    let url = format!("{}/projects/{}/deployments", backend_url, project);

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to list deployments")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to list deployments ({}): {}", status, error_text);
    }

    let mut deployments: Vec<Deployment> = response.json().await.context("Failed to parse deployments")?;

    // Limit results
    deployments.truncate(limit);

    if deployments.is_empty() {
        println!("No deployments found for project '{}'", project);
        return Ok(());
    }

    // Print table header
    println!("{:<40} {:<15} {:<25} {:<50}", "DEPLOYMENT", "STATUS", "CREATED", "URL");
    println!("{}", "-".repeat(130));

    for deployment in deployments {
        let deployment_ref = format!("{}:{}", project, deployment.deployment_id);
        let url = deployment.deployment_url.as_deref().unwrap_or("-");

        // Format created time (just show date and time, not full RFC3339)
        let created = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&deployment.created) {
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            deployment.created.clone()
        };

        println!(
            "{:<40} {:<15} {:<25} {:<50}",
            deployment_ref,
            deployment.status.to_string(),
            created,
            url
        );

        // Show error message if failed
        if deployment.status == DeploymentStatus::Failed {
            if let Some(error) = deployment.error_message {
                println!("  Error: {}", error);
            }
        }
    }

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
    let token = config.token.as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

    let timeout = parse_duration(timeout_str)?;
    let start_time = Instant::now();

    debug!("Fetching deployment {}:{}", project, deployment_id);

    loop {
        let deployment = fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

        // Print deployment details
        print_deployment_details(&deployment, project);

        // Check if deployment is in terminal state
        let is_terminal = matches!(
            deployment.status,
            DeploymentStatus::Completed | DeploymentStatus::Failed
        );

        if !follow || is_terminal {
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
    if !deployment.controller_metadata.is_null() && deployment.controller_metadata != serde_json::json!({}) {
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
    let token = config.token.as_ref()
        .context("Not logged in. Please run 'rise login' first.")?;

    info!("Rolling back project '{}' to deployment '{}'", project, deployment_id);

    println!("Initiating rollback to {}:{}...", project, deployment_id);

    // Call the rollback endpoint
    let url = format!("{}/projects/{}/deployments/{}/rollback", backend_url, project, deployment_id);
    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send rollback request")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());

        match status {
            reqwest::StatusCode::NOT_FOUND => {
                bail!("Deployment '{}:{}' not found", project, deployment_id);
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                bail!("Authentication failed. Please run 'rise login' again.");
            }
            reqwest::StatusCode::FORBIDDEN => {
                bail!("You don't have permission to rollback project '{}'", project);
            }
            reqwest::StatusCode::BAD_REQUEST => {
                bail!("Cannot rollback: {}", error_text);
            }
            _ => {
                bail!("Rollback failed ({}): {}", status, error_text);
            }
        }
    }

    let rollback_response: RollbackResponse = response.json().await
        .context("Failed to parse rollback response")?;

    println!();
    println!("✓ Rollback initiated successfully!");
    println!("  New deployment ID: {}", rollback_response.new_deployment_id);
    println!("  Rolled back from:  {}", rollback_response.rolled_back_from);
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
    ).await?;

    Ok(())
}
