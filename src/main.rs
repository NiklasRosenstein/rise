use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Module declarations with feature gates
#[cfg(feature = "cli")]
mod api;
#[cfg(feature = "cli")]
mod build;
#[cfg(feature = "cli")]
mod cli;

#[cfg(feature = "cli")]
use api::project::ProjectVisibility;

#[cfg(feature = "server")]
mod db;
#[cfg(feature = "server")]
mod server;

// Re-export for convenience (CLI modules)
#[cfg(feature = "cli")]
use cli::*;

/// Resolve project name from explicit argument or rise.toml fallback
#[cfg(feature = "cli")]
fn resolve_project_name(explicit_project: Option<String>, path: &str) -> Result<String> {
    if let Some(project) = explicit_project {
        Ok(project)
    } else if let Some(config) = build::config::load_full_project_config(path)? {
        if let Some(project_config) = config.project {
            Ok(project_config.name)
        } else {
            anyhow::bail!("No project name specified and rise.toml has no [project] section")
        }
    } else {
        anyhow::bail!("No project name specified and no rise.toml found")
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Shared arguments for deployment creation
#[derive(Debug, Clone, clap::Args)]
struct DeployArgs {
    /// Project name (optional if rise.toml contains [project] section)
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// Path to the directory containing the application (defaults to current directory)
    #[arg(default_value = ".")]
    path: String,
    /// Pre-built image to deploy (e.g., nginx:latest). Skips build if provided.
    #[arg(long, short)]
    image: Option<String>,
    /// Deployment group (e.g., 'default', 'mr/27'). Defaults to 'default' if not specified.
    #[arg(long, short)]
    group: Option<String>,
    /// Expiration duration (e.g., '7d', '2h', '30m'). Deployment will be automatically cleaned up after this period.
    #[arg(long)]
    expire: Option<String>,
    /// HTTP port the application listens on (e.g., 3000, 8080, 5000).
    /// Required when using --image. Defaults to 8080 for buildpack builds.
    #[arg(long)]
    http_port: Option<u16>,
    #[command(flatten)]
    build_args: build::BuildArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Backend server and controller commands
    #[command(subcommand)]
    Backend(backend::BackendCommands),
    /// Build a container image locally without deploying
    Build {
        /// Tag for the built image (e.g., myapp:latest, registry.io/org/app:v1.0)
        tag: String,
        /// Path to the directory containing the application
        #[arg(default_value = ".")]
        path: String,
        /// Push image to registry after building
        #[arg(long)]
        push: bool,
        #[command(flatten)]
        build_args: build::BuildArgs,
    },
    /// Deploy an application (shortcut for 'deployment create')
    Deploy {
        #[command(flatten)]
        args: DeployArgs,
    },
    /// Deployment management commands
    #[command(subcommand)]
    #[command(visible_alias = "d")]
    Deployment(DeploymentCommands),
    /// Custom domain management commands
    #[command(subcommand)]
    #[command(visible_alias = "dom")]
    Domain(DomainCommands),
    /// Environment variable management commands
    #[command(subcommand)]
    #[command(visible_alias = "e")]
    Env(EnvCommands),
    /// Extension management commands
    #[command(subcommand)]
    #[command(visible_alias = "ext")]
    Extension(ExtensionCommands),
    /// Authenticate with the Rise backend
    Login {
        /// Backend URL to authenticate with
        #[arg(long)]
        url: Option<String>,
        /// Use browser-based OAuth2 authorization code flow (default)
        #[arg(long, conflicts_with = "device")]
        browser: bool,
        /// Use device authorization flow
        #[arg(long, conflicts_with = "browser")]
        device: bool,
    },
    /// Project management commands
    #[command(subcommand)]
    #[command(visible_alias = "p")]
    Project(ProjectCommands),
    /// Build and run a container locally for development
    Run {
        /// Project name (optional, used to load environment variables)
        #[arg(long, short)]
        project: Option<String>,
        /// Path to the directory containing the application
        #[arg(default_value = ".")]
        path: String,
        /// HTTP port the application listens on (also sets PORT env var)
        #[arg(long, default_value = "8080")]
        http_port: u16,
        /// Port to expose on the host (defaults to same as http-port)
        #[arg(long)]
        expose: Option<u16>,
        /// Runtime environment variables (format: KEY=VALUE, can be specified multiple times)
        #[arg(long = "run-env", short, value_parser = parse_key_val::<String, String>)]
        run_env: Vec<(String, String)>,
        #[command(flatten)]
        build_args: build::BuildArgs,
    },
    /// Service account (workload identity) management commands
    #[command(subcommand)]
    #[command(visible_alias = "sa")]
    ServiceAccount(ServiceAccountCommands),
    /// Team management commands
    #[command(subcommand)]
    #[command(visible_alias = "t")]
    Team(TeamCommands),
}

#[derive(Subcommand, Debug)]
enum ProjectCommands {
    /// Create a new project
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        /// Project name
        name: String,
        /// Visibility (public or private)
        #[arg(long, default_value = "private")]
        visibility: String,
        /// Owner (format: "user:email" or "team:name", defaults to current user)
        #[arg(long)]
        owner: Option<String>,
        /// Path where to create rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// List all projects
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {},
    /// Show project details
    #[command(visible_alias = "s")]
    Show {
        /// Project name
        project: String,
    },
    /// Update project
    #[command(visible_alias = "u")]
    #[command(visible_alias = "edit")]
    Update {
        /// Project name
        project: String,
        /// New project name
        #[arg(long)]
        name: Option<String>,
        /// New visibility (public or private)
        #[arg(long)]
        visibility: Option<String>,
        /// Transfer ownership (format: "user:email" or "team:name")
        #[arg(long)]
        owner: Option<String>,
        /// Sync from rise.toml to backend (ignores other flags)
        #[arg(long)]
        sync: bool,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Delete a project
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
    Delete {
        /// Project name
        project: String,
    },
}

#[derive(Subcommand, Debug)]
enum TeamCommands {
    /// Create a new team
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
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
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {},
    /// Show team details
    #[command(visible_alias = "s")]
    Show {
        /// Team name
        team: String,
    },
    /// Update team
    #[command(visible_alias = "u")]
    #[command(visible_alias = "edit")]
    Update {
        /// Team name
        team: String,
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
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
    Delete {
        /// Team name
        team: String,
    },
}

#[derive(Subcommand, Debug)]
enum DeploymentCommands {
    /// Create a new deployment
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        #[command(flatten)]
        args: DeployArgs,
    },
    /// List deployments for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Filter by deployment group
        #[arg(long, short)]
        group: Option<String>,
        /// Limit number of deployments to show
        #[arg(long, short, default_value = "10")]
        limit: usize,
    },
    /// Show deployment details
    #[command(visible_alias = "s")]
    Show {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Deployment ID
        deployment_id: String,
        /// Follow deployment until completion
        #[arg(long, short)]
        follow: bool,
        /// Timeout for following deployment
        #[arg(long, default_value = "5m")]
        timeout: String,
    },
    /// Rollback to a previous deployment
    Rollback {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Deployment ID to rollback to
        deployment_id: String,
    },
    /// Stop all deployments in a group
    Stop {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Deployment group to stop
        #[arg(long, short)]
        group: String,
    },
    /// Show logs from a deployment
    Logs {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Deployment ID (YYYYMMDD-HHMMSS format)
        deployment_id: String,
        /// Follow log output (stream continuously)
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show from end of logs
        #[arg(long)]
        tail: Option<usize>,
        /// Show timestamps in log output
        #[arg(long)]
        timestamps: bool,
        /// Show logs since duration (e.g., "5m", "1h")
        #[arg(long)]
        since: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ServiceAccountCommands {
    /// Create a new service account for a project
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// OIDC issuer URL (e.g., https://gitlab.com)
        #[arg(long)]
        issuer: String,
        /// Claims to match (format: key=value, can be specified multiple times)
        #[arg(long = "claim", value_parser = parse_key_val::<String, String>)]
        claims: Vec<(String, String)>,
    },
    /// List all service accounts for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Show service account details
    #[command(visible_alias = "s")]
    #[command(visible_alias = "get")]
    Show {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Service account ID
        id: String,
    },
    /// Delete a service account
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
    Delete {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Service account ID
        id: String,
    },
}

#[derive(Subcommand, Debug)]
enum EnvCommands {
    /// Set an environment variable for a project
    #[command(visible_alias = "s")]
    Set {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Variable name (e.g., DATABASE_URL)
        key: String,
        /// Variable value
        value: String,
        /// Mark as secret (encrypted at rest)
        #[arg(long)]
        secret: bool,
    },
    /// List environment variables for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Delete an environment variable from a project
    #[command(visible_alias = "unset")]
    #[command(visible_alias = "rm")]
    #[command(visible_alias = "del")]
    Delete {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Variable name
        key: String,
    },
    /// Import environment variables from a file
    #[command(visible_alias = "i")]
    Import {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Path to file containing environment variables
        /// Format: KEY=value or KEY=secret:value (for secrets)
        /// Lines starting with # are comments
        file: std::path::PathBuf,
    },
    /// Show environment variables for a deployment (read-only)
    ShowDeployment {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Deployment ID
        deployment_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum DomainCommands {
    /// Add a custom domain to a project
    #[command(visible_alias = "a")]
    Add {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Domain name (e.g., example.com)
        domain: String,
    },
    /// List custom domains for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Remove a custom domain from a project
    #[command(visible_alias = "rm")]
    #[command(visible_alias = "del")]
    Remove {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Domain name
        domain: String,
    },
}

#[derive(Subcommand, Debug)]
enum ExtensionCommands {
    /// Create or update an extension for a project
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Extension name
        extension: String,
        /// Extension type (handler identifier, e.g., "aws-rds-provisioner", "oauth")
        #[arg(long)]
        r#type: String,
        /// Extension spec as JSON string
        #[arg(long)]
        spec: String,
    },
    /// Update an extension (full replace)
    #[command(visible_alias = "u")]
    Update {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Extension name
        extension: String,
        /// Extension spec as JSON string
        #[arg(long)]
        spec: String,
    },
    /// Patch an extension (partial update, null values unset fields)
    #[command(visible_alias = "p")]
    Patch {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Extension name
        extension: String,
        /// Extension spec patch as JSON string
        #[arg(long)]
        spec: String,
    },
    /// List all extensions for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Show extension details
    #[command(visible_alias = "s")]
    Show {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Extension name
        extension: String,
    },
    /// Delete an extension from a project
    #[command(visible_alias = "rm")]
    #[command(visible_alias = "del")]
    Delete {
        /// Project name (optional if rise.toml contains [project] section)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Path to rise.toml (defaults to current directory)
        #[arg(long, default_value = ".")]
        path: String,
        /// Extension name
        extension: String,
    },
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(
    s: &str,
) -> Result<(T, U), Box<dyn std::error::Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: std::error::Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for all commands
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    // Transform Deploy command to Deployment::Create for zero-duplication aliasing
    let cli_command = match cli.command {
        Commands::Deploy { args } => Commands::Deployment(DeploymentCommands::Create { args }),
        other => other,
    };

    // Backend commands don't need CLI config (they use Settings from TOML/env vars)
    // Only client commands (login, project, team, deployment, service-account) need it
    if let Commands::Backend(backend_cmd) = &cli_command {
        return backend::handle_backend_command(backend_cmd.clone()).await;
    }

    // Load CLI config for client commands
    let http_client = Client::new();
    let mut config = config::Config::load()?;
    let backend_url = config.get_backend_url();

    // Check version compatibility for all commands except Backend
    // (Backend commands don't use the HTTP API)
    if !matches!(&cli_command, Commands::Backend(_)) {
        // Non-fatal version check - just warns user
        let _ = version::check_version_compatibility(&http_client, &backend_url).await;
    }

    match &cli_command {
        Commands::Login {
            url,
            browser: _,
            device,
        } => {
            // Use provided URL or fall back to config default
            let login_url = url.as_deref().unwrap_or(&backend_url);

            if *device {
                // Device flow (explicit)
                login::handle_device_flow(&http_client, login_url, &mut config, url.as_deref())
                    .await?;
            } else {
                // Authorization code flow with PKCE (default)
                login::handle_authorization_code_flow(
                    &http_client,
                    login_url,
                    &mut config,
                    url.as_deref(),
                )
                .await?;
            }
        }
        Commands::Backend(_) => {
            // Already handled above before config loading
            unreachable!("Backend commands should have been handled earlier")
        }
        Commands::Project(project_cmd) => match project_cmd {
            ProjectCommands::Create {
                name,
                visibility,
                owner,
                path,
            } => {
                let visibility_enum: ProjectVisibility = visibility.parse().unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });

                project::create_project(
                    &http_client,
                    &backend_url,
                    &config,
                    name,
                    visibility_enum,
                    owner.clone(),
                    path,
                )
                .await?;
            }
            ProjectCommands::List {} => {
                project::list_projects(&http_client, &backend_url, &config).await?;
            }
            ProjectCommands::Show { project } => {
                project::show_project(&http_client, &backend_url, &config, project).await?;
            }
            ProjectCommands::Update {
                project,
                name,
                visibility,
                owner,
                sync,
                path,
            } => {
                let visibility_enum = visibility.as_ref().map(|v| {
                    v.parse().unwrap_or_else(|e: anyhow::Error| {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    })
                });

                project::update_project(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    name.clone(),
                    visibility_enum,
                    owner.clone(),
                    *sync,
                    path,
                )
                .await?;
            }
            ProjectCommands::Delete { project } => {
                project::delete_project(&http_client, &backend_url, &config, project).await?;
            }
        },
        Commands::Team(team_cmd) => match team_cmd {
            TeamCommands::Create {
                name,
                owners,
                members,
            } => {
                let owners_vec: Option<Vec<String>> = owners.as_ref().map(|o| {
                    o.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                });
                let members_vec: Vec<String> = members
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                team::create_team(
                    &http_client,
                    &backend_url,
                    &config,
                    name,
                    owners_vec,
                    members_vec,
                )
                .await?;
            }
            TeamCommands::List {} => {
                team::list_teams(&http_client, &backend_url, &config).await?;
            }
            TeamCommands::Show { team } => {
                team::show_team(&http_client, &backend_url, &config, team).await?;
            }
            TeamCommands::Update {
                team,
                name,
                add_owners,
                remove_owners,
                add_members,
                remove_members,
            } => {
                let add_owners_vec: Vec<String> = add_owners
                    .as_ref()
                    .map(|s| {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let remove_owners_vec: Vec<String> = remove_owners
                    .as_ref()
                    .map(|s| {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let add_members_vec: Vec<String> = add_members
                    .as_ref()
                    .map(|s| {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let remove_members_vec: Vec<String> = remove_members
                    .as_ref()
                    .map(|s| {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();

                team::update_team(
                    &http_client,
                    &backend_url,
                    &config,
                    team,
                    name.clone(),
                    add_owners_vec,
                    remove_owners_vec,
                    add_members_vec,
                    remove_members_vec,
                )
                .await?;
            }
            TeamCommands::Delete { team } => {
                team::delete_team(&http_client, &backend_url, &config, team).await?;
            }
        },
        Commands::Deployment(deployment_cmd) => match deployment_cmd {
            DeploymentCommands::Create { args } => {
                let project_name = resolve_project_name(args.project.clone(), &args.path)?;
                // Validate http_port requirements
                let port = match (args.image.as_ref(), args.http_port) {
                    // If using pre-built image, http_port is required
                    (Some(_), None) => {
                        eprintln!("Error: --http-port is required when using --image");
                        eprintln!(
                            "Example: rise deployment create {} --image {} --http-port 80",
                            project_name,
                            args.image.as_ref().unwrap()
                        );
                        std::process::exit(1);
                    }
                    // If using pre-built image with port specified, use it
                    (Some(_), Some(p)) => p,
                    // If building from source without port specified, default to 8080 (Paketo buildpack default)
                    (None, None) => {
                        info!(
                            "No --http-port specified, defaulting to 8080 (Paketo buildpack default)"
                        );
                        8080
                    }
                    // If building from source with port specified, use it
                    (None, Some(p)) => p,
                };

                deployment::create_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    deployment::DeploymentOptions {
                        project_name: &project_name,
                        path: &args.path,
                        image: args.image.as_deref(),
                        group: args.group.as_deref(),
                        expires_in: args.expire.as_deref(),
                        http_port: port,
                        build_args: &args.build_args,
                    },
                )
                .await?;
            }
            DeploymentCommands::List {
                project,
                path,
                group,
                limit,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                deployment::list_deployments(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    group.as_deref(),
                    *limit,
                )
                .await?;
            }
            DeploymentCommands::Show {
                project,
                path,
                deployment_id,
                follow,
                timeout,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                deployment::show_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    deployment_id,
                    *follow,
                    timeout,
                )
                .await?;
            }
            DeploymentCommands::Rollback {
                project,
                path,
                deployment_id,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                deployment::rollback_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    deployment_id,
                )
                .await?;
            }
            DeploymentCommands::Stop {
                project,
                path,
                group,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                deployment::stop_deployments_by_group(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    group,
                )
                .await?;
            }
            DeploymentCommands::Logs {
                project,
                path,
                deployment_id,
                follow,
                tail,
                timestamps,
                since,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                let token = config.get_token().ok_or_else(|| {
                    anyhow::anyhow!("Not logged in. Please run 'rise login' first.")
                })?;
                deployment::get_logs(
                    &http_client,
                    &backend_url,
                    &token,
                    deployment::GetLogsParams {
                        project: &project_name,
                        deployment_id,
                        follow: *follow,
                        tail: *tail,
                        timestamps: *timestamps,
                        since: since.as_deref(),
                    },
                )
                .await?;
            }
        },
        Commands::ServiceAccount(sa_cmd) => match sa_cmd {
            ServiceAccountCommands::Create {
                project,
                path,
                issuer,
                claims,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                let claims_map: std::collections::HashMap<String, String> =
                    claims.iter().cloned().collect();

                // Validate aud claim requirement
                if !claims_map.contains_key("aud") {
                    eprintln!(
                        "Error: The 'aud' (audience) claim is required for service accounts."
                    );
                    eprintln!("       Recommended format: rise-project-{{project-name}}");
                    eprintln!("       Example: --claim aud=rise-project-{}", project_name);
                    std::process::exit(1);
                }

                // Validate at least one additional claim
                if claims_map.len() < 2 {
                    eprintln!("Error: At least one claim in addition to 'aud' is required.");
                    eprintln!("       Example: --claim aud=... --claim project_path=myorg/myrepo");
                    std::process::exit(1);
                }

                service_account::create_service_account(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    issuer,
                    claims_map,
                )
                .await?;
            }
            ServiceAccountCommands::List { project, path } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                service_account::list_service_accounts(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                )
                .await?;
            }
            ServiceAccountCommands::Show { project, path, id } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                service_account::show_service_account(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    id,
                )
                .await?;
            }
            ServiceAccountCommands::Delete { project, path, id } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                service_account::delete_service_account(
                    &http_client,
                    &backend_url,
                    &config,
                    &project_name,
                    id,
                )
                .await?;
            }
        },
        Commands::Env(env_cmd) => {
            let token = config.get_token().ok_or_else(|| {
                anyhow::anyhow!("Not authenticated. Please run 'rise login' first")
            })?;
            match env_cmd {
                EnvCommands::Set {
                    project,
                    path,
                    key,
                    value,
                    secret,
                } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    env::set_env(
                        &http_client,
                        &backend_url,
                        &token,
                        &project_name,
                        key,
                        value,
                        *secret,
                    )
                    .await?;
                }
                EnvCommands::List { project, path } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    env::list_env(&http_client, &backend_url, &token, &project_name).await?;
                }
                EnvCommands::Delete { project, path, key } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    env::unset_env(&http_client, &backend_url, &token, &project_name, key).await?;
                }
                EnvCommands::Import {
                    project,
                    path,
                    file,
                } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    env::import_env(&http_client, &backend_url, &token, &project_name, file)
                        .await?;
                }
                EnvCommands::ShowDeployment {
                    project,
                    path,
                    deployment_id,
                } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    env::list_deployment_env(
                        &http_client,
                        &backend_url,
                        &token,
                        &project_name,
                        deployment_id,
                    )
                    .await?;
                }
            }
        }
        Commands::Domain(domain_cmd) => {
            let token = config.get_token().ok_or_else(|| {
                anyhow::anyhow!("Not authenticated. Please run 'rise login' first")
            })?;
            match domain_cmd {
                DomainCommands::Add {
                    project,
                    path,
                    domain,
                } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    domain::add_domain(
                        &http_client,
                        &backend_url,
                        &token,
                        &project_name,
                        domain,
                        Some(path),
                    )
                    .await?;
                }
                DomainCommands::List { project, path } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    domain::list_domains(&http_client, &backend_url, &token, &project_name).await?;
                }
                DomainCommands::Remove {
                    project,
                    path,
                    domain,
                } => {
                    let project_name = resolve_project_name(project.clone(), path)?;
                    domain::remove_domain(
                        &http_client,
                        &backend_url,
                        &token,
                        &project_name,
                        domain,
                        Some(path),
                    )
                    .await?;
                }
            }
        }
        Commands::Extension(extension_cmd) => match extension_cmd {
            ExtensionCommands::Create {
                project,
                path,
                extension,
                r#type,
                spec,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                let spec: serde_json::Value =
                    serde_json::from_str(spec).context("Failed to parse spec as JSON")?;
                extension::create_extension(&project_name, extension, r#type, spec).await?;
            }
            ExtensionCommands::Update {
                project,
                path,
                extension,
                spec,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                let spec: serde_json::Value =
                    serde_json::from_str(spec).context("Failed to parse spec as JSON")?;
                extension::update_extension(&project_name, extension, spec).await?;
            }
            ExtensionCommands::Patch {
                project,
                path,
                extension,
                spec,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                let spec: serde_json::Value =
                    serde_json::from_str(spec).context("Failed to parse spec as JSON")?;
                extension::patch_extension(&project_name, extension, spec).await?;
            }
            ExtensionCommands::List { project, path } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                extension::list_extensions(&project_name).await?;
            }
            ExtensionCommands::Show {
                project,
                path,
                extension,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                extension::show_extension(&project_name, extension).await?;
            }
            ExtensionCommands::Delete {
                project,
                path,
                extension,
            } => {
                let project_name = resolve_project_name(project.clone(), path)?;
                extension::delete_extension(&project_name, extension).await?;
            }
        },
        Commands::Build {
            tag,
            path,
            push,
            build_args,
        } => {
            let options = build::BuildOptions::from_build_args(
                &config,
                tag.clone(),
                path.clone(),
                build_args,
            )
            .with_push(*push);

            build::build_image(options)?;
        }
        Commands::Run {
            project,
            path,
            http_port,
            expose,
            run_env,
            build_args,
        } => {
            let expose_port = expose.unwrap_or(*http_port);

            cli::run::run_locally(
                &http_client,
                &config,
                cli::run::RunOptions {
                    project_name: project.as_deref(),
                    path,
                    http_port: *http_port,
                    expose: expose_port,
                    run_env,
                    build_args,
                },
            )
            .await?;
        }
        Commands::Deploy { .. } => {
            // Already transformed to Deployment(Create) above
            unreachable!("Deploy command should have been transformed to Deployment(Create)")
        }
    }

    Ok(())
}
