use anyhow::Result;

use crate::dev_oidc_issuer;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum BackendCommands {
    /// Start the HTTP server with all controllers
    #[cfg(feature = "server")]
    Server,
    /// Check backend configuration for errors and unused options
    #[cfg(feature = "server")]
    CheckConfig,
    /// Run a local OIDC issuer for testing service accounts
    DevOidcIssuer {
        /// Port to listen on
        #[arg(long, short, default_value = "5678")]
        port: u16,
        /// Generate and print a token at startup (format: 'key=value,key=value')
        #[arg(long)]
        token: Option<String>,
    },
}

pub async fn handle_backend_command(cmd: BackendCommands) -> Result<()> {
    match cmd {
        #[cfg(feature = "server")]
        BackendCommands::Server => {
            let settings = crate::server::settings::Settings::new()?;
            crate::server::run_server(settings).await
        }
        #[cfg(feature = "server")]
        BackendCommands::CheckConfig => {
            println!("Checking backend configuration...");
            match crate::server::settings::Settings::new() {
                Ok(_) => {
                    println!("✓ Configuration is valid");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("✗ Configuration error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BackendCommands::DevOidcIssuer { port, token } => dev_oidc_issuer::run(port, token).await,
    }
}
