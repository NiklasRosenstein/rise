use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::build::config::{
    load_full_project_config, write_project_config, ProjectBuildConfig, ProjectConfig,
};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CustomDomainResponse {
    id: String,
    domain: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct CustomDomainsResponse {
    domains: Vec<CustomDomainResponse>,
}

#[derive(Debug, Serialize)]
struct AddCustomDomainRequest {
    domain: String,
}

/// Add a custom domain to a project
pub async fn add_domain(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    domain: &str,
    app_path: Option<&str>,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/domains", backend_url, project);

    let payload = AddCustomDomainRequest {
        domain: domain.to_string(),
    };

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to add custom domain (status {}): {}",
            status,
            error_text
        );
    }

    println!(
        "✓ Added custom domain '{}' to project '{}'",
        domain, project
    );

    // Update rise.toml to persist the domain if app_path is provided
    if let Some(path) = app_path {
        update_rise_toml_add_domain(path, project, domain)?;
    }

    Ok(())
}

/// Helper function to add domain to rise.toml
fn update_rise_toml_add_domain(app_path: &str, project: &str, domain: &str) -> Result<()> {
    // Load existing config or create new one
    let mut config = load_full_project_config(app_path)?.unwrap_or(ProjectBuildConfig {
        version: Some(1),
        project: None,
        build: None,
    });

    // Ensure project section exists
    if config.project.is_none() {
        config.project = Some(ProjectConfig {
            name: project.to_string(),
            visibility: "private".to_string(),
            custom_domains: Vec::new(),
            env: std::collections::HashMap::new(),
        });
    }

    // Add domain if not already present
    if let Some(ref mut project_config) = config.project {
        if !project_config.custom_domains.contains(&domain.to_string()) {
            project_config.custom_domains.push(domain.to_string());
            write_project_config(app_path, &config)?;
            println!("✓ Updated rise.toml with custom domain '{}'", domain);
        }
    }

    Ok(())
}

/// List custom domains for a project
pub async fn list_domains(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<()> {
    let url = format!("{}/api/v1/projects/{}/domains", backend_url, project);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to list custom domains (status {}): {}",
            status,
            error_text
        );
    }

    let domains_response: CustomDomainsResponse = response
        .json()
        .await
        .context("Failed to parse domains response")?;

    if domains_response.domains.is_empty() {
        println!("No custom domains configured for project '{}'", project);
        return Ok(());
    }

    // Create a table to display the domains
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![Cell::new("DOMAIN"), Cell::new("CREATED AT")]);

    for domain in &domains_response.domains {
        table.add_row(vec![
            Cell::new(&domain.domain),
            Cell::new(&domain.created_at),
        ]);
    }

    println!("{}", table);

    Ok(())
}

/// Remove a custom domain from a project
pub async fn remove_domain(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    domain: &str,
    app_path: Option<&str>,
) -> Result<()> {
    let url = format!(
        "{}/api/v1/projects/{}/domains/{}",
        backend_url, project, domain
    );

    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to remove custom domain (status {}): {}",
            status,
            error_text
        );
    }

    println!(
        "✓ Removed custom domain '{}' from project '{}'",
        domain, project
    );

    // Update rise.toml to remove the domain if app_path is provided
    if let Some(path) = app_path {
        update_rise_toml_remove_domain(path, domain)?;
    }

    Ok(())
}

/// Helper function to remove domain from rise.toml
fn update_rise_toml_remove_domain(app_path: &str, domain: &str) -> Result<()> {
    // Load existing config
    if let Some(mut config) = load_full_project_config(app_path)? {
        // Remove domain if present
        if let Some(ref mut project_config) = config.project {
            if let Some(pos) = project_config
                .custom_domains
                .iter()
                .position(|d| d == domain)
            {
                project_config.custom_domains.remove(pos);
                write_project_config(app_path, &config)?;
                println!("✓ Removed custom domain '{}' from rise.toml", domain);
            }
        }
    }

    Ok(())
}
