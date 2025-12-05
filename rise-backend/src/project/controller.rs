use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::models::{DeploymentStatus, ProjectStatus};
use crate::db::{deployments as db_deployments, projects as db_projects};
use crate::deployment::state_machine;
use crate::state::AppState;

/// Project controller handles project lifecycle operations
///
/// Currently implements:
/// - Deletion loop: processes projects in Deleting status
pub struct ProjectController {
    state: Arc<AppState>,
    deletion_interval: Duration,
}

impl ProjectController {
    /// Create a new project controller
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            deletion_interval: Duration::from_secs(5),
        }
    }

    /// Start deletion loop
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.deletion_loop().await;
        });
    }

    /// Deletion loop - processes projects in Deleting status
    ///
    /// Runs every 5 seconds and:
    /// 1. Finds projects marked as Deleting
    /// 2. For each project, checks all deployments
    /// 3. Cancels pre-infrastructure deployments (Pending/Building/Pushing)
    /// 4. Terminates post-infrastructure deployments (Deploying/Healthy/Unhealthy)
    /// 5. Once all deployments are terminal, deletes the project
    async fn deletion_loop(&self) {
        info!("Project deletion loop started");
        let mut ticker = interval(self.deletion_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.process_deleting_projects().await {
                error!("Error in deletion loop: {}", e);
            }
        }
    }

    /// Process all projects in Deleting status
    async fn process_deleting_projects(&self) -> anyhow::Result<()> {
        let deleting = db_projects::find_deleting(&self.state.db_pool, 10).await?;

        for project in deleting {
            debug!("Processing deletion for project {}", project.name);

            // Find all deployments for this project
            let deployments = db_deployments::list_for_project(&self.state.db_pool, project.id).await?;

            // Check if any non-terminal deployments exist
            let mut has_non_terminal = false;

            for deployment in &deployments {
                if state_machine::is_terminal(&deployment.status) {
                    continue;
                }

                has_non_terminal = true;

                // Distinguish pre-infrastructure vs post-infrastructure
                let is_pre_infrastructure = matches!(
                    deployment.status,
                    DeploymentStatus::Pending | DeploymentStatus::Building | DeploymentStatus::Pushing
                );

                if is_pre_infrastructure {
                    // Cancel pre-infrastructure deployments
                    // These haven't provisioned resources yet
                    if deployment.status != DeploymentStatus::Cancelling {
                        info!(
                            "Cancelling pre-infrastructure deployment {} (status={:?})",
                            deployment.deployment_id, deployment.status
                        );
                        db_deployments::mark_cancelling(&self.state.db_pool, deployment.id).await?;
                    }
                } else {
                    // Terminate post-infrastructure deployments
                    // These have containers/resources that need cleanup
                    if deployment.status != DeploymentStatus::Terminating {
                        info!(
                            "Terminating post-infrastructure deployment {} (status={:?})",
                            deployment.deployment_id, deployment.status
                        );
                        db_deployments::mark_terminating(
                            &self.state.db_pool,
                            deployment.id,
                            crate::db::models::TerminationReason::UserStopped,
                        ).await?;
                    }
                }
            }

            // If all deployments are terminal, delete the project
            if !has_non_terminal {
                info!(
                    "All deployments for project {} are terminated, deleting project",
                    project.name
                );
                db_projects::delete(&self.state.db_pool, project.id).await?;
            } else {
                debug!(
                    "Project {} still has non-terminal deployments, waiting",
                    project.name
                );
            }
        }

        Ok(())
    }
}
