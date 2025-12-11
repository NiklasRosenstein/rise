// Container registry operations (push and login)

use anyhow::{bail, Context, Result};
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
