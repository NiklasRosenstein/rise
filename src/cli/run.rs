// Local development runner - builds and runs container images locally

use anyhow::{bail, Context, Result};
use reqwest::Client;
use std::process::{Command, Stdio};
use tracing::{info, warn};

use crate::build::{self, BuildOptions};
use crate::cli::env;
use crate::config::Config;

/// Options for running a container locally
pub struct RunOptions<'a> {
    pub project_name: Option<&'a str>,
    pub path: &'a str,
    pub http_port: u16,
    pub expose: u16,
    pub build_args: &'a build::BuildArgs,
}

/// Build and run a container image locally for development
pub async fn run_locally(
    http_client: &Client,
    config: &Config,
    options: RunOptions<'_>,
) -> Result<()> {
    let backend_url = config.get_backend_url();

    // Generate a local image tag
    let image_tag = format!(
        "rise-local-{}",
        options
            .project_name
            .unwrap_or("app")
            .replace(['/', ':'], "-")
    );

    info!("Building image locally: {}", image_tag);

    // Build the image using the existing build system
    let build_options = BuildOptions::from_build_args(
        config,
        image_tag.clone(),
        options.path.to_string(),
        options.build_args,
    )
    .with_push(false); // Never push local dev images

    build::build_image(build_options)?;

    // Resolve container CLI
    let container_cli = options
        .build_args
        .container_cli
        .as_deref()
        .unwrap_or("docker");

    info!("Starting container with {}...", container_cli);

    // Prepare docker run command
    let mut cmd = Command::new(container_cli);
    cmd.arg("run")
        .arg("--rm") // Remove container when it exits
        .arg("-it") // Interactive with TTY
        .arg("-p")
        .arg(format!("{}:{}", options.expose, options.http_port)); // Port mapping

    // Set PORT environment variable
    cmd.arg("-e").arg(format!("PORT={}", options.http_port));

    // Fetch and set project environment variables if project is specified
    if let Some(project_name) = options.project_name {
        if let Some(token) = config.get_token() {
            match env::fetch_non_secret_env_vars(http_client, &backend_url, &token, project_name)
                .await
            {
                Ok(env_vars) => {
                    if !env_vars.is_empty() {
                        info!(
                            "Loading {} non-secret environment variables from project '{}'",
                            env_vars.len(),
                            project_name
                        );
                        for (key, value) in env_vars {
                            cmd.arg("-e").arg(format!("{}={}", key, value));
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch environment variables from project '{}': {}",
                        project_name, e
                    );
                    warn!("Continuing without project environment variables");
                }
            }
        } else {
            warn!("Not logged in - skipping project environment variables");
            warn!("Run 'rise login' to load environment variables from the project");
        }
    }

    // Add the image tag
    cmd.arg(&image_tag);

    // Set up stdio to inherit from parent (allows interactive usage)
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    info!(
        "Running: {} run --rm -it -p {}:{} -e PORT={} {} [+ project env vars]",
        container_cli, options.expose, options.http_port, options.http_port, image_tag
    );
    info!(
        "Application will be available at http://localhost:{}",
        options.expose
    );
    info!("Press Ctrl+C to stop the container");

    // Execute the command and wait for completion
    let status = cmd.status().context("Failed to run container")?;

    if !status.success() {
        bail!("Container exited with status: {}", status);
    }

    Ok(())
}
