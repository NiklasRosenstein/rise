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
    pub use_project_env: bool,
    pub path: &'a str,
    pub http_port: u16,
    pub expose: u16,
    pub run_env: &'a [(String, String)],
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

    cmd.arg("--add-host=host.docker.internal:host-gateway");

    // Always try to resolve project name from rise.toml or explicit argument
    let project_name = if let Some(name) = options.project_name {
        // Explicit project name takes precedence
        Some(name.to_string())
    } else {
        // Try to load from rise.toml
        match build::config::load_full_project_config(options.path) {
            Ok(Some(config)) => {
                if let Some(project_config) = config.project {
                    Some(project_config.name)
                } else {
                    None
                }
            }
            Ok(None) => None,
            Err(e) => {
                warn!("Failed to load rise.toml: {}", e);
                None
            }
        }
    };

    // Load project environment variables if enabled and we have a project name
    if options.use_project_env {
        if let Some(project_name) = &project_name {
            if let Some(token) = config.get_token() {
                match env::fetch_env_vars_with_secret_list(
                    http_client,
                    &backend_url,
                    &token,
                    project_name,
                )
                .await
                {
                    Ok((env_vars, secret_keys)) => {
                        // Set non-secret environment variables
                        if !env_vars.is_empty() {
                            info!(
                                "Loading {} non-secret environment variable{} from project '{}'",
                                env_vars.len(),
                                if env_vars.len() == 1 { "" } else { "s" },
                                project_name
                            );
                            for (key, value) in env_vars {
                                cmd.arg("-e").arg(format!("{}={}", key, value));
                            }
                        }

                        // Warn about secret variables that cannot be loaded
                        if !secret_keys.is_empty() {
                            warn!(
                                "Project '{}' has {} secret environment variable{} that cannot be loaded automatically:",
                                project_name,
                                secret_keys.len(),
                                if secret_keys.len() == 1 { "" } else { "s" }
                            );
                            for key in &secret_keys {
                                warn!("  - {}", key);
                            }
                            warn!("Provide secret values manually using -e/--run-env if needed");
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
                warn!("Not logged in - cannot load project environment variables");
                warn!("Run 'rise login' to authenticate");
            }
        }
    }

    // Add user-specified runtime environment variables (these take precedence)
    if !options.run_env.is_empty() {
        info!(
            "Setting {} runtime environment variable{}",
            options.run_env.len(),
            if options.run_env.len() == 1 { "" } else { "s" }
        );
        for (key, value) in options.run_env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
    }

    // Add the image tag
    cmd.arg(&image_tag);

    // Set up stdio to inherit from parent (allows interactive usage)
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    info!(
        "Running container: {} (port {}:{}, PORT={})",
        image_tag, options.expose, options.http_port, options.http_port
    );
    if options.use_project_env && project_name.is_some() {
        info!("Project environment variables loaded (non-secret only)");
    }
    info!(
        "Application will be available at http://localhost:{}",
        options.expose
    );
    info!("Press Ctrl+C to stop the container");

    // Execute the command and wait for completion
    let status = cmd.status().context("Failed to run container")?;

    if !status.success() {
        if let Some(code) = status.code() {
            bail!("Container exited with status code: {}", code);
        } else {
            bail!("Container was terminated by a signal");
        }
    }

    Ok(())
}
