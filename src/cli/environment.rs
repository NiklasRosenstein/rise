use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Deserialize)]
struct EnvironmentResponse {
    name: String,
    primary_deployment_group: Option<String>,
    is_default: bool,
    is_production: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct CreateEnvironmentRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_deployment_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_production: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UpdateEnvironmentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_deployment_group: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_production: Option<bool>,
}

pub async fn handle_environment_command(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    cmd: &crate::EnvironmentCommands,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Please run 'rise login' first"))?;

    match cmd {
        crate::EnvironmentCommands::Create {
            name,
            project,
            path,
            group,
            default,
            production,
        } => {
            let project_name = crate::resolve_project_name(project.clone(), path)?;
            create_environment(
                http_client,
                backend_url,
                &token,
                &project_name,
                name,
                group.as_deref(),
                *default,
                *production,
            )
            .await
        }
        crate::EnvironmentCommands::List { project, path } => {
            let project_name = crate::resolve_project_name(project.clone(), path)?;
            list_environments(http_client, backend_url, &token, &project_name).await
        }
        crate::EnvironmentCommands::Show {
            name,
            project,
            path,
        } => {
            let project_name = crate::resolve_project_name(project.clone(), path)?;
            show_environment(http_client, backend_url, &token, &project_name, name).await
        }
        crate::EnvironmentCommands::Update {
            name,
            project,
            path,
            rename,
            group,
            default,
            production,
        } => {
            let project_name = crate::resolve_project_name(project.clone(), path)?;
            update_environment(
                http_client,
                backend_url,
                &token,
                &project_name,
                name,
                rename.as_deref(),
                group.as_deref(),
                *default,
                *production,
            )
            .await
        }
        crate::EnvironmentCommands::Delete {
            name,
            project,
            path,
        } => {
            let project_name = crate::resolve_project_name(project.clone(), path)?;
            delete_environment(http_client, backend_url, &token, &project_name, name).await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_environment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    name: &str,
    group: Option<&str>,
    is_default: bool,
    is_production: bool,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/environments", backend_url, project);

    let payload = CreateEnvironmentRequest {
        name: name.to_string(),
        primary_deployment_group: group.map(|g| g.to_string()),
        is_default: if is_default { Some(true) } else { None },
        is_production: if is_production { Some(true) } else { None },
    };

    let response = http_client
        .post(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .context("Failed to create environment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to create environment ({}): {}", status, error_text);
    }

    let env: EnvironmentResponse = response
        .json()
        .await
        .context("Failed to parse environment response")?;

    println!(
        "Created environment '{}' for project '{}'",
        env.name, project
    );
    if let Some(ref g) = env.primary_deployment_group {
        println!("  Primary group: {}", g);
    }
    if env.is_default {
        println!("  Default: yes");
    }
    if env.is_production {
        println!("  Production: yes");
    }

    Ok(())
}

async fn list_environments(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/environments", backend_url, project);

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to list environments")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to list environments ({}): {}", status, error_text);
    }

    let envs: Vec<EnvironmentResponse> = response
        .json()
        .await
        .context("Failed to parse environments response")?;

    if envs.is_empty() {
        println!("No environments configured for project '{}'", project);
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("PRIMARY GROUP").add_attribute(Attribute::Bold),
            Cell::new("DEFAULT").add_attribute(Attribute::Bold),
            Cell::new("PRODUCTION").add_attribute(Attribute::Bold),
        ]);

    for env in envs {
        table.add_row(vec![
            Cell::new(&env.name),
            Cell::new(env.primary_deployment_group.as_deref().unwrap_or("-")),
            Cell::new(if env.is_default { "yes" } else { "-" }),
            Cell::new(if env.is_production { "yes" } else { "-" }),
        ]);
    }

    println!("{}", table);

    Ok(())
}

async fn show_environment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    name: &str,
) -> Result<()> {
    let url = format!(
        "{}/api/v1/projects/{}/environments/{}",
        backend_url, project, name
    );

    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to get environment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to get environment ({}): {}", status, error_text);
    }

    let env: EnvironmentResponse = response
        .json()
        .await
        .context("Failed to parse environment response")?;

    println!("Name:           {}", env.name);
    println!(
        "Primary group:  {}",
        env.primary_deployment_group.as_deref().unwrap_or("-")
    );
    println!(
        "Default:        {}",
        if env.is_default { "yes" } else { "no" }
    );
    println!(
        "Production:     {}",
        if env.is_production { "yes" } else { "no" }
    );
    println!("Created:        {}", env.created_at);
    println!("Updated:        {}", env.updated_at);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_environment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    name: &str,
    rename: Option<&str>,
    group: Option<&str>,
    is_default: Option<bool>,
    is_production: Option<bool>,
) -> Result<()> {
    let url = format!(
        "{}/api/v1/projects/{}/environments/{}",
        backend_url, project, name
    );

    let primary_deployment_group = group.map(|g| {
        if g.is_empty() {
            None
        } else {
            Some(g.to_string())
        }
    });

    let payload = UpdateEnvironmentRequest {
        name: rename.map(|n| n.to_string()),
        primary_deployment_group,
        is_default,
        is_production,
    };

    let response = http_client
        .patch(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .context("Failed to update environment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to update environment ({}): {}", status, error_text);
    }

    let env: EnvironmentResponse = response
        .json()
        .await
        .context("Failed to parse environment response")?;

    println!(
        "Updated environment '{}' in project '{}'",
        env.name, project
    );

    Ok(())
}

async fn delete_environment(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    name: &str,
) -> Result<()> {
    let url = format!(
        "{}/api/v1/projects/{}/environments/{}",
        backend_url, project, name
    );

    let response = http_client
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to delete environment")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to delete environment ({}): {}", status, error_text);
    }

    println!("Deleted environment '{}' from project '{}'", name, project);

    Ok(())
}
