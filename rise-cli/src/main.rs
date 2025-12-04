use clap::{Parser, Subcommand};
use anyhow::Result;
use reqwest::Client;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod login;
mod team;
mod project;
mod deploy;
mod deployment;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate with the Rise backend
    Login {
        /// Backend URL to authenticate with
        #[arg(long)]
        url: Option<String>,
        /// Email for password authentication
        #[arg(long)]
        email: Option<String>,
        /// Password for authentication (only used with --email)
        #[arg(long)]
        password: Option<String>,
    },
    /// Project management commands
    #[command(subcommand)]
    Project(ProjectCommands),
    /// Deploy a project
    Deploy {
        /// Project name to deploy to
        #[arg(long, short)]
        project: String,
        /// Path to the directory containing the application (defaults to current directory)
        #[arg(default_value = ".")]
        path: String,
    },
    /// Team management commands
    #[command(subcommand)]
    Team(TeamCommands),
    /// Deployment management commands
    #[command(subcommand)]
    Deployment(DeploymentCommands),
}

#[derive(Subcommand, Debug)]
enum ProjectCommands {
    /// Create a new project
    Create {
        /// Project name
        name: String,
        /// Visibility (public or private)
        #[arg(long, default_value = "private")]
        visibility: String,
        /// Owner (format: "user:email" or "team:name", defaults to current user)
        #[arg(long)]
        owner: Option<String>,
    },
    /// List all projects
    List {},
    /// Show project details
    Show {
        /// Project name or ID
        project: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
    /// Update project
    Update {
        /// Project name or ID
        project: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
        /// New project name
        #[arg(long)]
        name: Option<String>,
        /// New visibility (public or private)
        #[arg(long)]
        visibility: Option<String>,
        /// Transfer ownership (format: "user:email" or "team:name")
        #[arg(long)]
        owner: Option<String>,
    },
    /// Delete a project
    Delete {
        /// Project name or ID
        project: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TeamCommands {
    /// Create a new team
    Create {
        /// Team name
        name: String,
        /// Owner emails (comma-separated, defaults to current user)
        #[arg(long)]
        owners: Option<String>,
        /// Member emails (comma-separated, optional)
        #[arg(long, default_value = "")]
        members: String,
    },
    /// List all teams
    List {},
    /// Show team details
    Show {
        /// Team name or ID
        team: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
    /// Update team
    Update {
        /// Team name or ID
        team: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
        /// New team name
        #[arg(long)]
        name: Option<String>,
        /// Add owners (comma-separated email addresses)
        #[arg(long)]
        add_owners: Option<String>,
        /// Remove owners (comma-separated email addresses)
        #[arg(long)]
        remove_owners: Option<String>,
        /// Add members (comma-separated email addresses)
        #[arg(long)]
        add_members: Option<String>,
        /// Remove members (comma-separated email addresses)
        #[arg(long)]
        remove_members: Option<String>,
    },
    /// Delete a team
    Delete {
        /// Team name or ID
        team: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
}

#[derive(Subcommand, Debug)]
enum DeploymentCommands {
    /// List deployments for a project
    List {
        /// Project name
        project: String,
        /// Limit number of deployments to show
        #[arg(long, short, default_value = "10")]
        limit: usize,
    },
    /// Show deployment details (format: project:deployment_id)
    Show {
        /// Deployment reference (format: project:deployment_id)
        deployment: String,
        /// Follow deployment until completion
        #[arg(long, short)]
        follow: bool,
        /// Timeout for following deployment
        #[arg(long, default_value = "5m")]
        timeout: String,
    },
    /// Rollback to a previous deployment
    Rollback {
        /// Deployment reference (format: project:deployment_id)
        deployment: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let http_client = Client::new();
    let mut config = config::Config::load()?;
    let backend_url = config.get_backend_url();

    match &cli.command {
        Commands::Login { url, email, password } => {
            // Use provided URL or fall back to config default
            let login_url = url.as_deref().unwrap_or(&backend_url);

            match (email, password) {
                (Some(email), Some(pass)) => {
                    // Password flow: both email and password provided
                    login::handle_password_login(&http_client, login_url, email, pass, &mut config, url.as_deref()).await?;
                }
                (Some(email), None) => {
                    // Password flow: prompt for password
                    let pass = rpassword::prompt_password("Password: ")?;
                    login::handle_password_login(&http_client, login_url, email, &pass, &mut config, url.as_deref()).await?;
                }
                (None, _) => {
                    // Browser flow: no email provided, use device flow
                    login::handle_device_login(&http_client, login_url, &mut config, url.as_deref()).await?;
                }
            }
        }
        Commands::Deploy { project, path } => {
            deploy::handle_deploy(&http_client, &backend_url, &config, project, path).await?;
        }
        Commands::Project(project_cmd) => {
            match project_cmd {
                ProjectCommands::Create { name, visibility, owner } => {
                    let visibility_enum: project::ProjectVisibility = visibility.parse()
                        .unwrap_or_else(|e| {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        });

                    project::create_project(&http_client, &backend_url, &config, name, visibility_enum, owner.clone()).await?;
                }
                ProjectCommands::List {} => {
                    project::list_projects(&http_client, &backend_url, &config).await?;
                }
                ProjectCommands::Show { project, by_id } => {
                    project::show_project(&http_client, &backend_url, &config, project, *by_id).await?;
                }
                ProjectCommands::Update { project, by_id, name, visibility, owner } => {
                    let visibility_enum = if let Some(v) = visibility {
                        Some(v.parse().unwrap_or_else(|e: anyhow::Error| {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }))
                    } else {
                        None
                    };

                    project::update_project(
                        &http_client,
                        &backend_url,
                        &config,
                        project,
                        *by_id,
                        name.clone(),
                        visibility_enum,
                        owner.clone(),
                    ).await?;
                }
                ProjectCommands::Delete { project, by_id } => {
                    project::delete_project(&http_client, &backend_url, &config, project, *by_id).await?;
                }
            }
        }
        Commands::Team(team_cmd) => {
            match team_cmd {
                TeamCommands::Create { name, owners, members } => {
                    let owners_vec: Option<Vec<String>> = owners.as_ref().map(|o| {
                        o.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    });
                    let members_vec: Vec<String> = members.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    team::create_team(&http_client, &backend_url, &config, name, owners_vec, members_vec).await?;
                }
                TeamCommands::List {} => {
                    team::list_teams(&http_client, &backend_url, &config).await?;
                }
                TeamCommands::Show { team, by_id } => {
                    team::show_team(&http_client, &backend_url, &config, team, *by_id).await?;
                }
                TeamCommands::Update { team, by_id, name, add_owners, remove_owners, add_members, remove_members } => {
                    let add_owners_vec: Vec<String> = add_owners.as_ref()
                        .map(|s| s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                        .unwrap_or_default();
                    let remove_owners_vec: Vec<String> = remove_owners.as_ref()
                        .map(|s| s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                        .unwrap_or_default();
                    let add_members_vec: Vec<String> = add_members.as_ref()
                        .map(|s| s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                        .unwrap_or_default();
                    let remove_members_vec: Vec<String> = remove_members.as_ref()
                        .map(|s| s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                        .unwrap_or_default();

                    team::update_team(
                        &http_client,
                        &backend_url,
                        &config,
                        team,
                        *by_id,
                        name.clone(),
                        add_owners_vec,
                        remove_owners_vec,
                        add_members_vec,
                        remove_members_vec,
                    ).await?;
                }
                TeamCommands::Delete { team, by_id } => {
                    team::delete_team(&http_client, &backend_url, &config, team, *by_id).await?;
                }
            }
        }
        Commands::Deployment(deployment_cmd) => {
            match deployment_cmd {
                DeploymentCommands::List { project, limit } => {
                    deployment::list_deployments(&http_client, &backend_url, &config, project, *limit).await?;
                }
                DeploymentCommands::Show { deployment, follow, timeout } => {
                    let (project, deployment_id) = deployment::parse_deployment_ref(deployment)?;
                    deployment::show_deployment(&http_client, &backend_url, &config, &project, &deployment_id, *follow, timeout).await?;
                }
                DeploymentCommands::Rollback { deployment } => {
                    let (project, deployment_id) = deployment::parse_deployment_ref(deployment)?;
                    deployment::rollback_deployment(&http_client, &backend_url, &config, &project, &deployment_id).await?;
                }
            }
        }
    }

    Ok(())
}
