use crate::api::project::{
    CreateProjectResponse, DomainsResponse, EnvVarsResponse, MeResponse, OwnerInfo, Project,
    ProjectErrorResponse, ProjectStatus, ProjectWithOwnerInfo, UpdateProjectResponse,
};

// Re-export for backwards compatibility with main.rs
pub use crate::api::project::ProjectVisibility;
use crate::config::Config;
use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Table};
use reqwest::Client;
use serde::Serialize;

// Helper function to get current user info
async fn get_current_user(
    http_client: &Client,
    backend_url: &str,
    token: &str,
) -> Result<MeResponse> {
    let url = format!("{}/api/v1/users/me", backend_url);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to get current user")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to get current user (status {}): {}",
            status,
            error_text
        );
    }

    let me_response: MeResponse = response
        .json()
        .await
        .context("Failed to parse me response")?;

    Ok(me_response)
}

// Parse owner string (format: "user:email" or "team:name")
fn parse_owner(owner: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = owner.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid owner format. Use 'user:email' or 'team:name'");
    }

    let owner_type = parts[0].to_lowercase();
    let owner_value = parts[1].to_string();

    if owner_type != "user" && owner_type != "team" {
        anyhow::bail!("Owner type must be 'user' or 'team'");
    }

    Ok((owner_type, owner_value))
}

// Create a new project
pub async fn create_project(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    name: &str,
    visibility: ProjectVisibility,
    owner: Option<String>,
    path: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // Determine owner
    let (owner_type, owner_id) = if let Some(owner_str) = owner {
        let (otype, ovalue) = parse_owner(&owner_str)?;

        // For user owner, we need to look up the ID from email
        // For team owner, we can use the team name directly (backend will resolve)
        if otype == "user" {
            // TODO: Add user lookup endpoint or use email directly
            (otype, ovalue)
        } else {
            (otype, ovalue)
        }
    } else {
        // Default to current user
        let current_user = get_current_user(http_client, backend_url, &token).await?;
        ("user".to_string(), current_user.id)
    };

    #[derive(Serialize)]
    #[serde(rename_all = "snake_case")]
    enum OwnerType {
        User(String),
        Team(String),
    }

    let owner_payload = if owner_type == "user" {
        OwnerType::User(owner_id)
    } else {
        OwnerType::Team(owner_id)
    };

    #[derive(Serialize)]
    struct CreateRequest {
        name: String,
        visibility: ProjectVisibility,
        owner: OwnerType,
    }

    let request = CreateRequest {
        name: name.to_string(),
        visibility: visibility.clone(),
        owner: owner_payload,
    };

    let url = format!("{}/api/v1/projects", backend_url);
    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to send create project request")?;

    if response.status().is_success() {
        let create_response: CreateProjectResponse = response
            .json()
            .await
            .context("Failed to parse create project response")?;

        println!(
            "✓ Project '{}' created successfully!",
            create_response.project.name
        );
        println!("  ID: {}", create_response.project.id);
        println!("  Status: {}", create_response.project.status);

        // Generate rise.toml
        use crate::build::config::{write_project_config, ProjectBuildConfig, ProjectConfig};
        use std::collections::HashMap;

        let project_config = ProjectConfig {
            name: name.to_string(),
            visibility: visibility.to_string().to_lowercase(),
            custom_domains: Vec::new(),
            env: HashMap::new(),
        };

        let config_to_write = ProjectBuildConfig {
            version: Some(1),
            project: Some(project_config),
            build: None,
        };

        write_project_config(path, &config_to_write)?;
        println!("  Created rise.toml at {}/rise.toml", path);
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to create project (status {}): {}",
            status,
            error_text
        );
    }

    Ok(())
}

// List all projects
pub async fn list_projects(http_client: &Client, backend_url: &str, config: &Config) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/api/v1/projects", backend_url);
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send list projects request")?;

    if response.status().is_success() {
        let projects: Vec<Project> = response
            .json()
            .await
            .context("Failed to parse list projects response")?;

        if projects.is_empty() {
            println!("No projects found.");
        } else {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    Cell::new("NAME").add_attribute(Attribute::Bold),
                    Cell::new("STATUS").add_attribute(Attribute::Bold),
                    Cell::new("VISIBILITY").add_attribute(Attribute::Bold),
                    Cell::new("OWNER").add_attribute(Attribute::Bold),
                    Cell::new("ACTIVE DEPLOYMENT").add_attribute(Attribute::Bold),
                    Cell::new("URL").add_attribute(Attribute::Bold),
                ]);

            for project in projects {
                let url = project.primary_url.as_deref().unwrap_or("(not deployed)");

                // Format active deployment status
                let active_deployment = project
                    .active_deployment_status
                    .as_deref()
                    .unwrap_or("-")
                    .to_string();

                // Format owner
                let owner = if let Some(user_email) = &project.owner_user_email {
                    format!("user:{}", user_email)
                } else if let Some(team_name) = &project.owner_team_name {
                    format!("team:{}", team_name)
                } else {
                    "-".to_string()
                };

                table.add_row(vec![
                    Cell::new(&project.name),
                    Cell::new(format!("{}", project.status)),
                    Cell::new(format!("{}", project.visibility)),
                    Cell::new(&owner),
                    Cell::new(&active_deployment),
                    Cell::new(url),
                ]);
            }

            println!("{}", table);
        }
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to list projects (status {}): {}",
            status,
            error_text
        );
    }

    Ok(())
}

// Show project details
pub async fn show_project(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_identifier: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // Always request expanded data with owner info
    let url = format!(
        "{}/api/v1/projects/{}?expand=owner",
        backend_url, project_identifier
    );
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send get project request")?;

    if response.status().is_success() {
        let project: ProjectWithOwnerInfo = response
            .json()
            .await
            .context("Failed to parse get project response")?;

        println!("Project: {}", project.name);
        println!("ID: {}", project.id);
        println!("Status: {}", project.status);
        println!("Visibility: {}", project.visibility);
        if let Some(url) = project.primary_url {
            println!("Primary URL: {}", url);
        } else {
            println!("Primary URL: (not deployed)");
        }
        if !project.custom_domain_urls.is_empty() {
            println!("Custom Domains:");
            for domain_url in &project.custom_domain_urls {
                println!("  - {}", domain_url);
            }
        }

        println!("\nOwner:");
        if let Some(owner) = project.owner {
            match owner {
                OwnerInfo::User(user) => {
                    println!("  Type: User");
                    println!("  Email: {}", user.email);
                }
                OwnerInfo::Team(team) => {
                    println!("  Type: Team");
                    println!("  Name: {}", team.name);
                }
            }
        } else {
            println!("  (none)");
        }

        // Display deployment groups if any exist
        if let Some(groups) = &project.deployment_groups {
            if !groups.is_empty() {
                println!("\nDeployment Groups:");
                for group in groups {
                    println!("  - {}", group);
                }
            }
        }

        // Display finalizers if any exist
        if !project.finalizers.is_empty() {
            println!("\nFinalizers:");
            for finalizer in &project.finalizers {
                println!("  - {}", finalizer);
            }
        } else if project.status == ProjectStatus::Deleting {
            println!("\nFinalizers: (none - ready for deletion)");
        }
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        // Handle 404 with potential fuzzy match suggestions
        let error: ProjectErrorResponse = response
            .json()
            .await
            .context("Failed to parse error response")?;

        eprintln!("{}", error.error);
        if let Some(suggestions) = error.suggestions {
            eprintln!("\nDid you mean one of these?");
            for suggestion in suggestions {
                eprintln!("  - {}", suggestion);
            }
        }
        std::process::exit(1);
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to get project (status {}): {}", status, error_text);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_project(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_identifier: &str,
    name: Option<String>,
    visibility: Option<ProjectVisibility>,
    owner: Option<String>,
    sync: bool,
    path: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // Sync mode: Load rise.toml and push everything to backend
    if sync {
        use crate::build::config::load_full_project_config;
        use tracing::info;

        let full_config = load_full_project_config(path)?
            .ok_or_else(|| anyhow::anyhow!("No rise.toml found at {}", path))?;

        let project_config = full_config
            .project
            .ok_or_else(|| anyhow::anyhow!("No [project] section found in rise.toml"))?;

        // Parse visibility
        let visibility_enum: ProjectVisibility = project_config.visibility.parse()?;

        info!("Syncing project metadata from rise.toml to backend...");

        // Update project name and visibility
        #[derive(Serialize)]
        struct SyncUpdateRequest {
            name: String,
            visibility: ProjectVisibility,
        }

        let request = SyncUpdateRequest {
            name: project_config.name.clone(),
            visibility: visibility_enum,
        };

        let url = format!("{}/api/v1/projects/{}", backend_url, project_identifier);
        let response = http_client
            .put(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .send()
            .await
            .context("Failed to send update project request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!(
                "Failed to update project (status {}): {}",
                status,
                error_text
            );
        }

        let update_response: UpdateProjectResponse = response
            .json()
            .await
            .context("Failed to parse update project response")?;

        println!(
            "✓ Project '{}' updated successfully!",
            update_response.project.name
        );

        // Sync custom domains
        if !project_config.custom_domains.is_empty() {
            sync_custom_domains(
                http_client,
                backend_url,
                &token,
                &update_response.project.name,
                &project_config.custom_domains,
            )
            .await?;
        }

        // Sync environment variables
        if !project_config.env.is_empty() {
            sync_env_vars(
                http_client,
                backend_url,
                &token,
                &update_response.project.name,
                &project_config.env,
            )
            .await?;
        }

        return Ok(());
    }

    #[derive(Serialize)]
    #[serde(rename_all = "snake_case")]
    enum OwnerType {
        User(String),
        Team(String),
    }

    let owner_payload = if let Some(owner_str) = owner {
        let (owner_type, owner_id) = parse_owner(&owner_str)?;
        Some(if owner_type == "user" {
            OwnerType::User(owner_id)
        } else {
            OwnerType::Team(owner_id)
        })
    } else {
        None
    };

    #[derive(Serialize)]
    struct UpdateRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        visibility: Option<ProjectVisibility>,
        #[serde(skip_serializing_if = "Option::is_none")]
        owner: Option<OwnerType>,
    }

    let request = UpdateRequest {
        name: name.clone(),
        visibility: visibility.clone(),
        owner: owner_payload,
    };

    let url = format!("{}/api/v1/projects/{}", backend_url, project_identifier);
    let response = http_client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to send update project request")?;

    if response.status().is_success() {
        let update_response: UpdateProjectResponse = response
            .json()
            .await
            .context("Failed to parse update project response")?;

        println!(
            "✓ Project '{}' updated successfully!",
            update_response.project.name
        );
        println!("  Status: {}", update_response.project.status);

        // Update local rise.toml if it exists
        use crate::build::config::{load_full_project_config, write_project_config};
        if let Some(mut full_config) = load_full_project_config(path)? {
            if let Some(ref mut project_config) = full_config.project {
                let mut updated = false;

                // Update name in rise.toml if provided
                if let Some(ref new_name) = name {
                    project_config.name = new_name.clone();
                    updated = true;
                }

                // Update visibility in rise.toml if provided
                if let Some(ref new_visibility) = visibility {
                    project_config.visibility = new_visibility.to_string().to_lowercase();
                    updated = true;
                }

                if updated {
                    write_project_config(path, &full_config)?;
                    println!("  Updated rise.toml");
                }
            }
        }
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        let error: ProjectErrorResponse = response
            .json()
            .await
            .context("Failed to parse error response")?;

        eprintln!("{}", error.error);
        if let Some(suggestions) = error.suggestions {
            eprintln!("\nDid you mean one of these?");
            for suggestion in suggestions {
                eprintln!("  - {}", suggestion);
            }
        }
        std::process::exit(1);
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to update project (status {}): {}",
            status,
            error_text
        );
    }

    Ok(())
}

// Delete a project
pub async fn delete_project(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_identifier: &str,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/api/v1/projects/{}", backend_url, project_identifier);
    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send delete project request")?;

    if response.status() == reqwest::StatusCode::ACCEPTED {
        println!("✓ Project is being deleted (deployments are being cleaned up)");
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        let error: ProjectErrorResponse = response
            .json()
            .await
            .context("Failed to parse error response")?;

        eprintln!("{}", error.error);
        if let Some(suggestions) = error.suggestions {
            eprintln!("\nDid you mean one of these?");
            for suggestion in suggestions {
                eprintln!("  - {}", suggestion);
            }
        }
        std::process::exit(1);
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!(
            "Failed to delete project (status {}): {}",
            status,
            error_text
        );
    }

    Ok(())
}

/// Sync custom domains from rise.toml to backend
pub async fn sync_custom_domains(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    desired_domains: &[String],
) -> Result<()> {
    use crate::cli::domain;
    use tracing::warn;

    // Fetch current domains from backend
    let url = format!("{}/api/v1/projects/{}/domains", backend_url, project);
    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to fetch current domains")?;

    let current_domains_response: DomainsResponse = if response.status().is_success() {
        response.json().await.context("Failed to parse domains")?
    } else {
        DomainsResponse {
            domains: Vec::new(),
        }
    };

    let current_domains: Vec<String> = current_domains_response
        .domains
        .into_iter()
        .map(|d| d.domain)
        .collect();

    // Add missing domains
    for domain in desired_domains {
        if !current_domains.contains(domain) {
            println!("Adding domain '{}' from rise.toml", domain);
            domain::add_domain(http_client, backend_url, token, project, domain).await?;
        }
    }

    // Warn about unmanaged domains
    for domain in &current_domains {
        if !desired_domains.contains(domain) {
            warn!(
                "Domain '{}' exists in backend but not in rise.toml. \
                 This domain is not managed by rise.toml. \
                 Run 'rise domain remove {} {}' to remove it.",
                domain, project, domain
            );
        }
    }

    Ok(())
}

/// Sync environment variables from rise.toml to backend
pub async fn sync_env_vars(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    desired_env: &std::collections::HashMap<String, String>,
) -> Result<()> {
    use crate::cli::env;
    use tracing::warn;

    // Fetch current env vars from backend
    let url = format!("{}/api/v1/projects/{}/env", backend_url, project);
    let response = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to fetch current environment variables")?;

    let current_env_response: EnvVarsResponse = if response.status().is_success() {
        response.json().await.context("Failed to parse env vars")?
    } else {
        EnvVarsResponse {
            env_vars: Vec::new(),
        }
    };

    // Filter to only non-secret vars (rise.toml only manages plain-text vars)
    let current_non_secret_vars: Vec<String> = current_env_response
        .env_vars
        .into_iter()
        .filter(|v| !v.is_secret)
        .map(|v| v.key)
        .collect();

    // Set/update vars from rise.toml (always non-secret)
    for (key, value) in desired_env {
        println!("Setting env var '{}' from rise.toml", key);
        env::set_env(http_client, backend_url, token, project, key, value, false).await?;
    }

    // Warn about unmanaged non-secret vars
    for key in &current_non_secret_vars {
        if !desired_env.contains_key(key) {
            warn!(
                "Env var '{}' exists in backend but not in rise.toml. \
                 This variable is not managed by rise.toml. \
                 Run 'rise env delete {} {}' to remove it.",
                key, project, key
            );
        }
    }

    Ok(())
}
