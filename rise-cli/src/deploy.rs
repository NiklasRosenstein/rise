use anyhow::{Result, Context, bail};
use reqwest::Client;
use serde::Deserialize;
use std::process::Command;
use std::path::Path;

use crate::config::Config;

#[derive(Debug, Deserialize)]
struct RegistryCredentials {
    registry_url: String,
    username: String,
    password: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GetRegistryCredsResponse {
    credentials: RegistryCredentials,
    repository: String,
}

pub async fn handle_deploy(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_name: &str,
    path: &str,
) -> Result<()> {
    println!("Deploying project '{}' from path '{}'...", project_name, path);

    // Verify path exists
    let app_path = Path::new(path);
    if !app_path.exists() {
        bail!("Path '{}' does not exist", path);
    }
    if !app_path.is_dir() {
        bail!("Path '{}' is not a directory", path);
    }

    // Get authentication token
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run 'rise login' first."))?;

    // Step 1: Get registry credentials from backend
    println!("Fetching registry credentials for project '{}'...", project_name);
    let registry_info = get_registry_credentials(http_client, backend_url, token, project_name).await?;

    println!("Registry URL: {}", registry_info.credentials.registry_url);
    println!("Repository: {}", registry_info.repository);

    // Step 2: Build image with buildpacks
    let image_tag = format!("{}/{}", registry_info.credentials.registry_url, registry_info.repository);
    println!("Building image with buildpacks: {}...", image_tag);
    build_image_with_buildpacks(path, &image_tag)?;

    // Step 3: Login to registry if credentials provided
    if !registry_info.credentials.username.is_empty() {
        println!("Logging into registry...");
        docker_login(
            &registry_info.credentials.registry_url,
            &registry_info.credentials.username,
            &registry_info.credentials.password,
        )?;
    } else {
        println!("Using existing Docker credentials for registry");
    }

    // Step 4: Push image
    println!("Pushing image to registry...");
    docker_push(&image_tag)?;

    println!("✓ Successfully deployed {} to {}", project_name, image_tag);

    Ok(())
}

async fn get_registry_credentials(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project_name: &str,
) -> Result<GetRegistryCredsResponse> {
    let url = format!("{}/registry/credentials?project={}", backend_url, project_name);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to fetch registry credentials")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        bail!("Failed to get registry credentials ({}): {}", status, error_text);
    }

    let registry_info: GetRegistryCredsResponse = response
        .json()
        .await
        .context("Failed to parse registry credentials response")?;

    Ok(registry_info)
}

fn build_image_with_buildpacks(app_path: &str, image_tag: &str) -> Result<()> {
    // Check if pack CLI is available
    let pack_check = Command::new("pack")
        .arg("version")
        .output();

    if pack_check.is_err() {
        bail!(
            "pack CLI not found. Please install it from https://buildpacks.io/docs/tools/pack/\n\
             On macOS: brew install buildpacks/tap/pack\n\
             On Linux: see https://buildpacks.io/docs/tools/pack/"
        );
    }

    println!("Running: pack build {} --path {}", image_tag, app_path);

    let status = Command::new("pack")
        .arg("build")
        .arg(image_tag)
        .arg("--path")
        .arg(app_path)
        .arg("--builder")
        .arg("paketobuildpacks/builder:base")
        .status()
        .context("Failed to execute pack build")?;

    if !status.success() {
        bail!("pack build failed with status: {}", status);
    }

    Ok(())
}

fn docker_login(registry: &str, username: &str, password: &str) -> Result<()> {
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

fn docker_push(image_tag: &str) -> Result<()> {
    println!("Running: docker push {}", image_tag);

    let status = Command::new("docker")
        .arg("push")
        .arg(image_tag)
        .status()
        .context("Failed to execute docker push")?;

    if !status.success() {
        bail!("docker push failed with status: {}", status);
    }

    Ok(())
}
