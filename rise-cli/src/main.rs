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
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
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
            login::handle_login(&http_client, &backend_url, username, password, &mut config).await?;
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
