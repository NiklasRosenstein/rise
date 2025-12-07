use crate::config::Config;
use anyhow::{Context, Result};
use comfy_table::{Attribute, Cell, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct MeResponse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    email: String,
}

// Helper function to get current user info
async fn get_current_user(
    http_client: &Client,
    backend_url: &str,
    token: &str,
) -> Result<MeResponse> {
    let url = format!("{}/me", backend_url);

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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub enum ProjectVisibility {
    Public,
    Private,
}

impl std::str::FromStr for ProjectVisibility {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "public" => Ok(ProjectVisibility::Public),
            "private" => Ok(ProjectVisibility::Private),
            _ => Err(anyhow::anyhow!(
                "Invalid visibility: {}. Must be 'public' or 'private'",
                s
            )),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
enum ProjectStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
    Deleting,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectStatus::Running => write!(f, "Running"),
            ProjectStatus::Stopped => write!(f, "Stopped"),
            ProjectStatus::Deploying => write!(f, "Deploying"),
            ProjectStatus::Failed => write!(f, "Failed"),
            ProjectStatus::Deleting => write!(f, "Deleting"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Project {
    id: String,
    name: String,
    status: ProjectStatus,
    visibility: ProjectVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_user_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_deployment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_deployment_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct UserInfo {
    id: String,
    email: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TeamInfo {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
enum OwnerInfo {
    User(UserInfo),
    Team(TeamInfo),
}

#[derive(Debug, Deserialize)]
struct ProjectWithOwnerInfo {
    id: String,
    name: String,
    status: ProjectStatus,
    #[allow(dead_code)]
    visibility: ProjectVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment_url: Option<String>,
    #[serde(default)]
    finalizers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectErrorResponse {
    error: String,
    suggestions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateProjectResponse {
    project: Project,
}

#[derive(Debug, Deserialize)]
struct UpdateProjectResponse {
    project: Project,
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
        visibility,
        owner: owner_payload,
    };

    let url = format!("{}/projects", backend_url);
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

    let url = format!("{}/projects", backend_url);
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
                    Cell::new("OWNER").add_attribute(Attribute::Bold),
                    Cell::new("ACTIVE DEPLOYMENT").add_attribute(Attribute::Bold),
                    Cell::new("URL").add_attribute(Attribute::Bold),
                ]);

            for project in projects {
                let url = project
                    .project_url
                    .as_deref()
                    .or(project.deployment_url.as_deref())
                    .unwrap_or("(not deployed)");

                // Format active deployment with status
                let active_deployment = match (
                    project.active_deployment_id.as_deref(),
                    project.active_deployment_status.as_deref(),
                ) {
                    (Some(id), Some(status)) => format!("{} ({})", id, status),
                    (Some(id), None) => id.to_string(),
                    _ => "-".to_string(),
                };

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
    by_id: bool,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // Always request expanded data with owner info
    let url = format!(
        "{}/projects/{}?expand=owner&by_id={}",
        backend_url, project_identifier, by_id
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
        if let Some(url) = project.deployment_url {
            println!("URL: {}", url);
        } else {
            println!("URL: (not deployed)");
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

// Update project (name, visibility, status, or transfer ownership)
pub async fn update_project(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_identifier: &str,
    by_id: bool,
    name: Option<String>,
    visibility: Option<ProjectVisibility>,
    owner: Option<String>,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

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
        name,
        visibility,
        owner: owner_payload,
    };

    let url = format!(
        "{}/projects/{}?by_id={}",
        backend_url, project_identifier, by_id
    );
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
    by_id: bool,
) -> Result<()> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!(
        "{}/projects/{}?by_id={}",
        backend_url, project_identifier, by_id
    );
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
