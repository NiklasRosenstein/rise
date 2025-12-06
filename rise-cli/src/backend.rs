use anyhow::Result;
use rise_backend::settings::Settings;

use crate::dev_oidc_issuer;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum BackendCommands {
    /// Start the HTTP server
    Server,
    /// Start a controller
    #[command(subcommand)]
    Controller(ControllerCommands),
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

#[derive(Debug, Clone, clap::Subcommand)]
pub enum ControllerCommands {
    /// Start the deployment controller (Docker backend)
    DeploymentDocker,
    /// Start the project controller
    Project,
    /// Start the ECR controller (requires ECR registry configuration)
    Ecr,
}

pub async fn handle_backend_command(cmd: BackendCommands) -> Result<()> {
    match cmd {
        BackendCommands::DevOidcIssuer { port, token } => dev_oidc_issuer::run(port, token).await,
        _ => {
            // Other commands need settings
            let settings = Settings::new()?;
            match cmd {
                BackendCommands::Server => rise_backend::run_server(settings).await,
                BackendCommands::Controller(controller_cmd) => match controller_cmd {
                    ControllerCommands::DeploymentDocker => {
                        rise_backend::run_deployment_controller(settings).await
                    }
                    ControllerCommands::Project => {
                        rise_backend::run_project_controller(settings).await
                    }
                    ControllerCommands::Ecr => rise_backend::run_ecr_controller(settings).await,
                },
                BackendCommands::DevOidcIssuer { .. } => unreachable!(),
            }
        }
    }
}
