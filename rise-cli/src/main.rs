use clap::{Parser, Subcommand};
use anyhow::Result;
use reqwest::Client;

mod config;
mod login;

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
        /// Username/email for password authentication
        #[arg(long)]
        username: Option<String>,
        /// Password for authentication (only used with --username)
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let http_client = Client::new();
    let mut config = config::Config::load()?;
    let backend_url = config.get_backend_url();

    match &cli.command {
        Commands::Login { username, password } => {
            match (username, password) {
                (Some(user), Some(pass)) => {
                    // Password flow: both username and password provided
                    login::handle_password_login(&http_client, &backend_url, user, pass, &mut config).await?;
                }
                (Some(user), None) => {
                    // Password flow: prompt for password
                    let pass = rpassword::prompt_password("Password: ")?;
                    login::handle_password_login(&http_client, &backend_url, user, &pass, &mut config).await?;
                }
                (None, _) => {
                    // Browser flow: no username provided
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
    }

    Ok(())
}
