use anyhow::Result;
use rise_backend::settings::Settings;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum BackendCommands {
    /// Start the HTTP server
    Server,
    /// Start a controller
    #[command(subcommand)]
    Controller(ControllerCommands),
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum ControllerCommands {
    /// Start the deployment controller (Docker backend)
    DeploymentDocker,
    /// Start the project controller
    Project,
}

pub async fn handle_backend_command(cmd: BackendCommands) -> Result<()> {
    let settings = Settings::new()?;

    match cmd {
        BackendCommands::Server => rise_backend::run_server(settings).await,
        BackendCommands::Controller(controller_cmd) => match controller_cmd {
            ControllerCommands::DeploymentDocker => {
                rise_backend::run_deployment_controller(settings).await
            }
            ControllerCommands::Project => rise_backend::run_project_controller(settings).await,
        },
    }
}
