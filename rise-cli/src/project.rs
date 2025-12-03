use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Debug, Deserialize)]
struct MeResponse {
    id: String,
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
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to get current user (status {}): {}", status, error_text);
    }

    let me_response: MeResponse = response.json().await
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
            _ => Err(anyhow::anyhow!("Invalid visibility: {}. Must be 'public' or 'private'", s)),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
enum ProjectStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectStatus::Running => write!(f, "Running"),
            ProjectStatus::Stopped => write!(f, "Stopped"),
            ProjectStatus::Deploying => write!(f, "Deploying"),
            ProjectStatus::Failed => write!(f, "Failed"),
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
    visibility: ProjectVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerInfo>,
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
    let token = config.get_token()
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
        let current_user = get_current_user(http_client, backend_url, token).await?;
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
        let create_response: CreateProjectResponse = response.json().await
            .context("Failed to parse create project response")?;

        println!("✓ Project '{}' created successfully!", create_response.project.name);
        println!("  ID: {}", create_response.project.id);
        println!("  Status: {}", create_response.project.status);
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to create project (status {}): {}", status, error_text);
    }

    Ok(())
}

// List all projects
pub async fn list_projects(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/projects", backend_url);
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send list projects request")?;

    if response.status().is_success() {
        let projects: Vec<Project> = response.json().await
            .context("Failed to parse list projects response")?;

        if projects.is_empty() {
            println!("No projects found.");
        } else {
            println!("Projects:");
            println!("{:<25} {:<15} {:<40}", "NAME", "STATUS", "URL");
            println!("{}", "-".repeat(85));
            for project in projects {
                let url = format!("https://{}.rise.net", project.name);
                println!("{:<25} {:<15} {:<40}",
                    project.name,
                    format!("{}", project.status),
                    url
                );
            }
        }
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to list projects (status {}): {}", status, error_text);
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
    let token = config.get_token()
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
        let project: ProjectWithOwnerInfo = response.json().await
            .context("Failed to parse get project response")?;

        println!("Project: {}", project.name);
        println!("ID: {}", project.id);
        println!("Status: {}", project.status);
        println!("URL: https://{}.rise.net", project.name);

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
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        // Handle 404 with potential fuzzy match suggestions
        let error: ProjectErrorResponse = response.json().await
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
        let error_text = response.text().await
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
    let token = config.get_token()
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

    let url = format!("{}/projects/{}?by_id={}", backend_url, project_identifier, by_id);
    let response = http_client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to send update project request")?;

    if response.status().is_success() {
        let update_response: UpdateProjectResponse = response.json().await
            .context("Failed to parse update project response")?;

        println!("✓ Project '{}' updated successfully!", update_response.project.name);
        println!("  Status: {}", update_response.project.status);
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        let error: ProjectErrorResponse = response.json().await
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
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to update project (status {}): {}", status, error_text);
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
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/projects/{}?by_id={}", backend_url, project_identifier, by_id);
    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send delete project request")?;

    if response.status().is_success() {
        println!("✓ Project deleted successfully!");
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        let error: ProjectErrorResponse = response.json().await
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
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to delete project (status {}): {}", status, error_text);
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum DeploymentStatus {
    Pending,
    Building,
    Pushing,
    Deploying,
    Completed,
    Failed,
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Completed => write!(f, "Completed"),
            DeploymentStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Deployment {
    id: String,
    deployment_id: String,
    project: String,
    created_by: String,
    status: DeploymentStatus,
    error_message: Option<String>,
    completed_at: Option<String>,
    build_logs: Option<String>,
    created: String,
    updated: String,
}

// List deployments for a project
pub async fn list_deployments(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project_name: &str,
    limit: usize,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/projects/{}/deployments", backend_url, project_name);
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send list deployments request")?;

    if response.status().is_success() {
        let mut deployments: Vec<Deployment> = response.json().await
            .context("Failed to parse list deployments response")?;

        // Apply limit
        deployments.truncate(limit);

        if deployments.is_empty() {
            println!("No deployments found for project '{}'.", project_name);
        } else {
            println!("Deployments for '{}':", project_name);
            println!("{:<20} {:<15} {:<25} {:<25}", "DEPLOYMENT ID", "STATUS", "CREATED", "COMPLETED");
            println!("{}", "-".repeat(90));
            for deployment in deployments {
                // Parse and format created timestamp
                let created = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&deployment.created) {
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    deployment.created
                };

                // Parse and format completed timestamp
                let completed = deployment.completed_at
                    .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "-".to_string());

                println!("{:<20} {:<15} {:<25} {:<25}",
                    deployment.deployment_id,
                    format!("{}", deployment.status),
                    created,
                    completed
                );

                // Show error message if failed
                if let Some(error_msg) = deployment.error_message {
                    println!("  └─ Error: {}", error_msg);
                }
            }
        }
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        let error_text = response.text().await
            .unwrap_or_else(|_| "Project not found".to_string());
        anyhow::bail!("{}", error_text);
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to list deployments (status {}): {}", status, error_text);
    }

    Ok(())
}
