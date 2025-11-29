use clap::{Parser, Subcommand};
use anyhow::Result;
use reqwest::Client;

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
    Login {},
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
    // TODO: Load backend_url from configuration
    let backend_url = "http://127.0.0.1:3000";

    match &cli.command {
        Commands::Login {} => {
            login::handle_login(&http_client, backend_url).await?;
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
