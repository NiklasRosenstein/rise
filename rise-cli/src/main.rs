use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::Client;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod backend;
mod config;
mod deployment;
mod dev_oidc_issuer;
mod login;
mod project;
mod service_account;
mod team;

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
        /// Use browser-based OAuth2 authorization code flow (default)
        #[arg(long, conflicts_with = "device")]
        browser: bool,
        /// Use device authorization flow
        #[arg(long, conflicts_with = "browser")]
        device: bool,
    },
    /// Backend server and controller commands
    #[command(subcommand)]
    Backend(backend::BackendCommands),
    /// Project management commands
    #[command(subcommand)]
    #[command(visible_alias = "p")]
    Project(ProjectCommands),
    /// Team management commands
    #[command(subcommand)]
    #[command(visible_alias = "t")]
    Team(TeamCommands),
    /// Deployment management commands
    #[command(subcommand)]
    #[command(visible_alias = "d")]
    Deployment(DeploymentCommands),
    /// Service account (workload identity) management commands
    #[command(subcommand)]
    #[command(visible_alias = "sa")]
    ServiceAccount(ServiceAccountCommands),
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
    },
    /// List all projects
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {},
    /// Show project details
    #[command(visible_alias = "s")]
    Show {
        /// Project name or ID
        project: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
    /// Update project
    #[command(visible_alias = "u")]
    #[command(visible_alias = "edit")]
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
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
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
        /// Team name or ID
        team: String,
        /// Force lookup by ID instead of name
        #[arg(long)]
        by_id: bool,
    },
    /// Update team
    #[command(visible_alias = "u")]
    #[command(visible_alias = "edit")]
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
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
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
    /// Create a new deployment
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        /// Project name to deploy to
        project: String,
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
    },
    /// List deployments for a project
    #[command(visible_alias = "ls")]
    #[command(visible_alias = "l")]
    List {
        /// Project name
        project: String,
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
        /// Project name
        project: String,
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
        /// Project name
        project: String,
        /// Deployment ID to rollback to
        deployment_id: String,
    },
    /// Stop all deployments in a group
    Stop {
        /// Project name
        project: String,
        /// Deployment group to stop
        #[arg(long, short)]
        group: String,
    },
}

#[derive(Subcommand, Debug)]
enum ServiceAccountCommands {
    /// Create a new service account for a project
    #[command(visible_alias = "c")]
    #[command(visible_alias = "new")]
    Create {
        /// Project name
        project: String,
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
        /// Project name
        project: String,
    },
    /// Show service account details
    #[command(visible_alias = "s")]
    #[command(visible_alias = "get")]
    Show {
        /// Project name
        project: String,
        /// Service account ID
        id: String,
    },
    /// Delete a service account
    #[command(visible_alias = "del")]
    #[command(visible_alias = "rm")]
    Delete {
        /// Project name
        project: String,
        /// Service account ID
        id: String,
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

    // Backend commands don't need CLI config (they use Settings from TOML/env vars)
    // Only client commands (login, project, team, deployment, service-account) need it
    match &cli.command {
        Commands::Backend(backend_cmd) => {
            return backend::handle_backend_command(backend_cmd.clone()).await;
        }
        _ => {}
    }

    // Load CLI config for client commands
    let http_client = Client::new();
    let mut config = config::Config::load()?;
    let backend_url = config.get_backend_url();

    match &cli.command {
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
            } => {
                let visibility_enum: project::ProjectVisibility =
                    visibility.parse().unwrap_or_else(|e| {
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
                )
                .await?;
            }
            ProjectCommands::List {} => {
                project::list_projects(&http_client, &backend_url, &config).await?;
            }
            ProjectCommands::Show { project, by_id } => {
                project::show_project(&http_client, &backend_url, &config, project, *by_id).await?;
            }
            ProjectCommands::Update {
                project,
                by_id,
                name,
                visibility,
                owner,
            } => {
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
                )
                .await?;
            }
            ProjectCommands::Delete { project, by_id } => {
                project::delete_project(&http_client, &backend_url, &config, project, *by_id)
                    .await?;
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
            TeamCommands::Show { team, by_id } => {
                team::show_team(&http_client, &backend_url, &config, team, *by_id).await?;
            }
            TeamCommands::Update {
                team,
                by_id,
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
                    *by_id,
                    name.clone(),
                    add_owners_vec,
                    remove_owners_vec,
                    add_members_vec,
                    remove_members_vec,
                )
                .await?;
            }
            TeamCommands::Delete { team, by_id } => {
                team::delete_team(&http_client, &backend_url, &config, team, *by_id).await?;
            }
        },
        Commands::Deployment(deployment_cmd) => match deployment_cmd {
            DeploymentCommands::Create {
                project,
                path,
                image,
                group,
                expire,
                http_port,
            } => {
                // Validate http_port requirements
                let port = match (image.as_ref(), http_port) {
                    // If using pre-built image, http_port is required
                    (Some(_), None) => {
                        eprintln!("Error: --http-port is required when using --image");
                        eprintln!(
                            "Example: rise deployment create {} --image {} --http-port 80",
                            project,
                            image.as_ref().unwrap()
                        );
                        std::process::exit(1);
                    }
                    // If using pre-built image with port specified, use it
                    (Some(_), Some(p)) => *p,
                    // If building from source without port specified, default to 8080 (Paketo buildpack default)
                    (None, None) => {
                        info!(
                            "No --http-port specified, defaulting to 8080 (Paketo buildpack default)"
                        );
                        8080
                    }
                    // If building from source with port specified, use it
                    (None, Some(p)) => *p,
                };

                deployment::create_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    path,
                    image.as_deref(),
                    group.as_deref(),
                    expire.as_deref(),
                    port,
                )
                .await?;
            }
            DeploymentCommands::List {
                project,
                group,
                limit,
            } => {
                deployment::list_deployments(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    group.as_deref(),
                    *limit,
                )
                .await?;
            }
            DeploymentCommands::Show {
                project,
                deployment_id,
                follow,
                timeout,
            } => {
                deployment::show_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    deployment_id,
                    *follow,
                    timeout,
                )
                .await?;
            }
            DeploymentCommands::Rollback {
                project,
                deployment_id,
            } => {
                deployment::rollback_deployment(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    deployment_id,
                )
                .await?;
            }
            DeploymentCommands::Stop { project, group } => {
                deployment::stop_deployments_by_group(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    group,
                )
                .await?;
            }
        },
        Commands::ServiceAccount(sa_cmd) => match sa_cmd {
            ServiceAccountCommands::Create {
                project,
                issuer,
                claims,
            } => {
                let claims_map: std::collections::HashMap<String, String> =
                    claims.iter().cloned().collect();

                // Validate aud claim requirement
                if !claims_map.contains_key("aud") {
                    eprintln!(
                        "Error: The 'aud' (audience) claim is required for service accounts."
                    );
                    eprintln!("       Recommended format: rise-project-{{project-name}}");
                    eprintln!("       Example: --claim aud=rise-project-{}", project);
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
                    project,
                    issuer,
                    claims_map,
                )
                .await?;
            }
            ServiceAccountCommands::List { project } => {
                service_account::list_service_accounts(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                )
                .await?;
            }
            ServiceAccountCommands::Show { project, id } => {
                service_account::show_service_account(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    id,
                )
                .await?;
            }
            ServiceAccountCommands::Delete { project, id } => {
                service_account::delete_service_account(
                    &http_client,
                    &backend_url,
                    &config,
                    project,
                    id,
                )
                .await?;
            }
        },
    }

    Ok(())
}
