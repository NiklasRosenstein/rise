use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::deployment;

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

pub async fn handle_deploy(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_name: &str,
    path: &str,
    image: Option<&str>,
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
    let deployment_info =
        create_deployment(http_client, backend_url, token, project_name, image).await?;

    info!("Deployment ID: {}", deployment_info.deployment_id);
    info!("Image tag: {}", deployment_info.image_tag);

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
    deployment::show_deployment(
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

async fn create_deployment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project_name: &str,
    image: Option<&str>,
) -> Result<CreateDeploymentResponse> {
    let url = format!("{}/deployments", backend_url);
    let mut payload = serde_json::json!({
        "project": project_name,
    });

    // Add image field if provided
    if let Some(image_ref) = image {
        payload["image"] = serde_json::json!(image_ref);
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
