use clap::{Parser, Subcommand};
use anyhow::Result;
use reqwest::Client;

mod config;
mod login;
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
        /// Email for password authentication
        #[arg(long)]
        email: Option<String>,
        /// Password for authentication (only used with --email)
        #[arg(long)]
        password: Option<String>,
    },
    /// Create a new project
    Create {
        name: String,
        #[arg(long)]
        visibility: Option<String>,
        #[arg(long)]
        owner: Option<String>,
    },
    /// List projects
    Ls {},
    /// Deploy a project
    Deploy {},
    /// Team management commands
    #[command(subcommand)]
    Team(TeamCommands),
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let http_client = Client::new();
    let mut config = config::Config::load()?;
    let backend_url = config.get_backend_url();

    match &cli.command {
        Commands::Login { email, password } => {
            match (email, password) {
                (Some(email), Some(pass)) => {
                    // Password flow: both email and password provided
                    login::handle_password_login(&http_client, &backend_url, email, pass, &mut config).await?;
                }
                (Some(email), None) => {
                    // Password flow: prompt for password
                    let pass = rpassword::prompt_password("Password: ")?;
                    login::handle_password_login(&http_client, &backend_url, email, &pass, &mut config).await?;
                }
                (None, _) => {
                    // Browser flow: no email provided, use device flow
                    login::handle_device_login(&http_client, &backend_url, &mut config).await?;
                }
            }
        }
        Commands::Create { name, visibility, owner } => {
            println!("Create command not yet implemented.");
            println!("Name: {}, Visibility: {:?}, Owner: {:?}", name, visibility, owner);
        }
        Commands::Ls {} => {
            println!("List command not yet implemented.");
        }
        Commands::Deploy {} => {
            println!("Deploy command not yet implemented.");
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
    }

    Ok(())
}
