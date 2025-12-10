use anyhow::Result;
use rise_backend::settings::Settings;

use crate::dev_oidc_issuer;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum BackendCommands {
    /// Start the HTTP server with all controllers
    Server,
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
        BackendCommands::Server => {
            let settings = Settings::new()?;
            rise_backend::run_server(settings).await
        }
        BackendCommands::DevOidcIssuer { port, token } => dev_oidc_issuer::run(port, token).await,
    }
}
