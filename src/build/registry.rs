// Container registry operations (push and login)

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

/// Push image to container registry
pub(crate) fn docker_push(container_cli: &str, image_tag: &str) -> Result<()> {
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

/// Pull image from a registry
pub(crate) fn docker_pull(container_cli: &str, image: &str, platform: &str) -> Result<()> {
    info!("Pulling image: {} (platform: {})", image, platform);

    let mut cmd = Command::new(container_cli);
    cmd.arg("pull").arg("--platform").arg(platform).arg(image);

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} pull", container_cli))?;

    if !status.success() {
        bail!("{} pull failed with status: {}", container_cli, status);
    }

    Ok(())
}

/// Tag a container image
pub(crate) fn docker_tag(container_cli: &str, source: &str, target: &str) -> Result<()> {
    info!("Tagging image: {} -> {}", source, target);

    let mut cmd = Command::new(container_cli);
    cmd.arg("tag").arg(source).arg(target);

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} tag", container_cli))?;

    if !status.success() {
        bail!("{} tag failed with status: {}", container_cli, status);
    }

    Ok(())
}

/// Inject a bearer token directly into the container CLI's auth config file.
///
/// Writes `{"auths":{"<registry>":{"registrytoken":"<token>"}}}` into the appropriate
/// config file for Docker (`~/.docker/config.json`) or Podman
/// (`$REGISTRY_AUTH_FILE` / `$XDG_RUNTIME_DIR/containers/auth.json` /
/// `~/.config/containers/auth.json`).
///
/// This is used for GitLab scoped JWTs, which cannot be applied via `docker login`
/// because the login command makes its own auth handshake that rejects pre-obtained tokens.
pub(crate) fn inject_registry_auth(container_cli: &str, registry: &str, token: &str) -> Result<()> {
    let auth_file = resolve_auth_file_path(container_cli)?;
    debug!(
        "Injecting registry auth for {} into {}",
        registry,
        auth_file.display()
    );

    // Create parent directory if it doesn't exist
    if let Some(parent) = auth_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Read existing config or start fresh
    let mut config: serde_json::Value = if auth_file.exists() {
        let content = std::fs::read_to_string(&auth_file)
            .with_context(|| format!("Failed to read {}", auth_file.display()))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Set the registrytoken entry
    config["auths"][registry]["registrytoken"] = serde_json::Value::String(token.to_string());

    // Write atomically via a temp file in the same directory
    let tmp_path = auth_file.with_extension("tmp");
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&config)?)
        .with_context(|| format!("Failed to write {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &auth_file)
        .with_context(|| format!("Failed to update {}", auth_file.display()))?;

    Ok(())
}

/// Resolve the auth config file path for the given container CLI.
fn resolve_auth_file_path(container_cli: &str) -> Result<PathBuf> {
    if container_cli.contains("podman") {
        // Podman credential lookup order
        if let Ok(path) = std::env::var("REGISTRY_AUTH_FILE") {
            return Ok(PathBuf::from(path));
        }
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            return Ok(PathBuf::from(runtime_dir).join("containers/auth.json"));
        }
        let home = home_dir().context("Could not determine home directory for Podman auth file")?;
        Ok(home.join(".config/containers/auth.json"))
    } else {
        // Docker credential lookup order
        if let Ok(docker_config) = std::env::var("DOCKER_CONFIG") {
            return Ok(PathBuf::from(docker_config).join("config.json"));
        }
        let home = home_dir().context("Could not determine home directory for Docker config")?;
        Ok(home.join(".docker/config.json"))
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Login to container registry
pub(crate) fn docker_login(
    container_cli: &str,
    registry: &str,
    username: &str,
    password: &str,
) -> Result<()> {
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
