use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct EnvVarResponse {
    key: String,
    value: String, // Will be masked ("••••••••") for secrets
    is_secret: bool,
}

#[derive(Debug, Deserialize)]
struct EnvVarsResponse {
    env_vars: Vec<EnvVarResponse>,
}

#[derive(Debug, Serialize)]
struct SetEnvVarRequest {
    value: String,
    #[serde(default)]
    is_secret: bool,
}

/// Fetch environment variables from a project (internal helper)
async fn fetch_env_vars_response(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<EnvVarsResponse> {
    let url = format!("{}/api/v1/projects/{}/env", backend_url, project);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to fetch environment variables")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to fetch environment variables (status {}): {}",
            status,
            error_text
        );
    }

    let env_vars_response: EnvVarsResponse = response
        .json()
        .await
        .context("Failed to parse environment variables response")?;

    Ok(env_vars_response)
}

/// Fetch non-secret environment variables from a project (for use by other modules)
pub async fn fetch_non_secret_env_vars(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<Vec<(String, String)>> {
    let env_response = fetch_env_vars_response(http_client, backend_url, token, project).await?;

    // Filter out secret variables (they'll have masked values)
    let env_vars: Vec<(String, String)> = env_response
        .env_vars
        .into_iter()
        .filter_map(|var| {
            if var.is_secret {
                // Skip secrets as we cannot retrieve their actual values
                None
            } else {
                Some((var.key, var.value))
            }
        })
        .collect();

    Ok(env_vars)
}

/// Set an environment variable for a project
pub async fn set_env(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    key: &str,
    value: &str,
    is_secret: bool,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/env/{}", backend_url, project, key);

    let payload = SetEnvVarRequest {
        value: value.to_string(),
        is_secret,
    };

    let response = http_client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .context("Failed to set environment variable")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to set environment variable (status {}): {}",
            status,
            error_text
        );
    }

    let var_type = if is_secret { "secret" } else { "plain text" };
    println!(
        "✓ Set {} variable '{}' for project '{}'",
        var_type, key, project
    );

    Ok(())
}

/// List environment variables for a project
pub async fn list_env(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<()> {
    let env_vars_response =
        fetch_env_vars_response(http_client, backend_url, token, project).await?;

    if env_vars_response.env_vars.is_empty() {
        println!(
            "No environment variables configured for project '{}'",
            project
        );
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("KEY").add_attribute(Attribute::Bold),
            Cell::new("VALUE").add_attribute(Attribute::Bold),
            Cell::new("TYPE").add_attribute(Attribute::Bold),
        ]);

    for var in env_vars_response.env_vars {
        let var_type = if var.is_secret { "secret" } else { "plain" };
        table.add_row(vec![
            Cell::new(&var.key),
            Cell::new(&var.value),
            Cell::new(var_type),
        ]);
    }

    println!("{}", table);
    println!("\nProject: {}", project);
    println!("Note: Secret values are always masked for security");

    Ok(())
}

/// Delete an environment variable from a project
pub async fn unset_env(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    key: &str,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/env/{}", backend_url, project, key);

    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to delete environment variable")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to delete environment variable (status {}): {}",
            status,
            error_text
        );
    }

    println!("✓ Deleted variable '{}' from project '{}'", key, project);

    Ok(())
}

/// Import environment variables from a file
///
/// File format:
/// - Lines starting with # are comments
/// - Empty lines are ignored
/// - Format: KEY=value (plain text) or KEY=secret:value (secret)
/// - Example:
///   ```
///   # Database configuration
///   DB_HOST=localhost
///   DB_PASSWORD=secret:my-secret-password
///   ```
pub async fn import_env(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    file_path: &PathBuf,
) -> Result<()> {
    let contents = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let mut success_count = 0;
    let mut error_count = 0;

    for (line_num, line) in contents.lines().enumerate() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=value
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            eprintln!(
                "Warning: Line {} has invalid format (expected KEY=value): {}",
                line_num + 1,
                line
            );
            error_count += 1;
            continue;
        }

        let key = parts[0].trim();
        let value_part = parts[1];

        // Check if value is secret
        let (value, is_secret) = if let Some(stripped) = value_part.strip_prefix("secret:") {
            (stripped, true)
        } else {
            (value_part, false)
        };

        // Validate key name (alphanumeric and underscore only)
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            eprintln!(
                "Warning: Line {} has invalid key name '{}' (must be alphanumeric with underscores)",
                line_num + 1,
                key
            );
            error_count += 1;
            continue;
        }

        // Set the variable
        match set_env(
            http_client,
            backend_url,
            token,
            project,
            key,
            value,
            is_secret,
        )
        .await
        {
            Ok(_) => success_count += 1,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to set variable '{}' from line {}: {}",
                    key,
                    line_num + 1,
                    e
                );
                error_count += 1;
            }
        }
    }

    println!(
        "\n✓ Import complete: {} variables set, {} errors",
        success_count, error_count
    );

    if error_count > 0 {
        anyhow::bail!("Import completed with {} errors", error_count);
    }

    Ok(())
}

/// List environment variables for a deployment (read-only)
pub async fn list_deployment_env(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    deployment_id: &str,
) -> Result<()> {
    let url = format!(
        "{}/api/v1/projects/{}/deployments/{}/env",
        backend_url, project, deployment_id
    );

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to list deployment environment variables")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to list deployment environment variables (status {}): {}",
            status,
            error_text
        );
    }

    let env_vars_response: EnvVarsResponse = response
        .json()
        .await
        .context("Failed to parse environment variables response")?;

    if env_vars_response.env_vars.is_empty() {
        println!(
            "No environment variables configured for deployment '{}' in project '{}'",
            deployment_id, project
        );
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("KEY").add_attribute(Attribute::Bold),
            Cell::new("VALUE").add_attribute(Attribute::Bold),
            Cell::new("TYPE").add_attribute(Attribute::Bold),
        ]);

    for var in env_vars_response.env_vars {
        let var_type = if var.is_secret { "secret" } else { "plain" };
        table.add_row(vec![
            Cell::new(&var.key),
            Cell::new(&var.value),
            Cell::new(var_type),
        ]);
    }

    println!("{}", table);
    println!("\nProject: {}", project);
    println!("Deployment: {}", deployment_id);
    println!("Note: Secret values are always masked for security");
    println!("Note: Deployment environment variables are read-only snapshots");

    Ok(())
}
