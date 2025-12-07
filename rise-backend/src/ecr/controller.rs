use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::projects as db_projects;
use crate::ecr::{EcrRepoManager, ECR_FINALIZER};
use crate::state::ControllerState;

/// ECR Controller manages ECR repository lifecycle
///
/// Responsibilities:
/// 1. **Provision loop**: Creates ECR repos for projects that don't have them yet
/// 2. **Cleanup loop**: Handles ECR repo deletion/orphaning when projects are deleted
///
/// The controller uses the finalizer pattern to coordinate with project deletion:
/// - When a repo is created, the finalizer is added to the project
/// - When the project is marked for deletion, cleanup runs
/// - Only after cleanup completes is the finalizer removed
/// - The project controller waits for all finalizers to be removed before deleting
pub struct EcrController {
    state: Arc<ControllerState>,
    manager: Arc<EcrRepoManager>,
    provision_interval: Duration,
    cleanup_interval: Duration,
}

impl EcrController {
    /// Create a new ECR controller
    pub fn new(state: Arc<ControllerState>, manager: Arc<EcrRepoManager>) -> Self {
        Self {
            state,
            manager,
            provision_interval: Duration::from_secs(10),
            cleanup_interval: Duration::from_secs(5),
        }
    }

    /// Start both provision and cleanup loops
    pub fn start(self: Arc<Self>) {
        let provision_self = Arc::clone(&self);
        tokio::spawn(async move {
            provision_self.provision_loop().await;
        });

        let cleanup_self = Arc::clone(&self);
        tokio::spawn(async move {
            cleanup_self.cleanup_loop().await;
        });
    }

    /// Provision loop - creates ECR repos for active projects
    ///
    /// Runs every 10 seconds and:
    /// 1. Lists all active projects (not Deleting/Terminated)
    /// 2. For each project without the ECR finalizer, creates the repo
    /// 3. Adds the ECR finalizer to track that cleanup is needed
    async fn provision_loop(&self) {
        info!("ECR provision loop started");
        let mut ticker = interval(self.provision_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.provision_repositories().await {
                error!("Error in ECR provision loop: {}", e);
            }
        }
    }

    /// Process provisioning for all active projects
    async fn provision_repositories(&self) -> anyhow::Result<()> {
        // Get all active projects
        let projects = db_projects::list_active(&self.state.db_pool).await?;

        for project in projects {
            // Skip if project already has ECR finalizer (repo already managed)
            if project.finalizers.contains(&ECR_FINALIZER.to_string()) {
                continue;
            }

            debug!("Provisioning ECR repository for project: {}", project.name);

            // Try to create the repository
            match self.manager.create_repository(&project.name).await {
                Ok(created) => {
                    if created {
                        info!("Created ECR repository for project: {}", project.name);
                    } else {
                        debug!(
                            "ECR repository already exists for project: {}",
                            project.name
                        );
                    }

                    // Add finalizer to indicate ECR cleanup is needed on deletion
                    db_projects::add_finalizer(&self.state.db_pool, project.id, ECR_FINALIZER)
                        .await?;
                    debug!("Added ECR finalizer to project: {}", project.name);
                }
                Err(e) => {
                    warn!(
                        "Failed to create ECR repository for project {}: {}",
                        project.name, e
                    );
                    // Continue to next project, will retry on next loop
                }
            }
        }

        Ok(())
    }

    /// Cleanup loop - handles ECR repo cleanup for deleted projects
    ///
    /// Runs every 5 seconds and:
    /// 1. Finds projects in Deleting status with ECR finalizer
    /// 2. Deletes or tags the ECR repo based on auto_remove setting
    /// 3. Removes the ECR finalizer so project can be fully deleted
    async fn cleanup_loop(&self) {
        info!("ECR cleanup loop started");
        let mut ticker = interval(self.cleanup_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.cleanup_repositories().await {
                error!("Error in ECR cleanup loop: {}", e);
            }
        }
    }

    /// Process cleanup for all deleting projects with ECR finalizer
    async fn cleanup_repositories(&self) -> anyhow::Result<()> {
        // Find projects marked for deletion that still have ECR finalizer
        let projects =
            db_projects::find_deleting_with_finalizer(&self.state.db_pool, ECR_FINALIZER, 10)
                .await?;

        for project in projects {
            debug!("Cleaning up ECR repository for project: {}", project.name);

            // Perform cleanup based on auto_remove setting
            let cleanup_result = if self.manager.auto_remove() {
                // Delete the repository
                match self.manager.delete_repository(&project.name).await {
                    Ok(deleted) => {
                        if deleted {
                            info!("Deleted ECR repository for project: {}", project.name);
                        } else {
                            info!(
                                "ECR repository did not exist for project: {} (already deleted)",
                                project.name
                            );
                        }
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            } else {
                // Tag as orphaned instead of deleting
                match self.manager.tag_as_orphaned(&project.name).await {
                    Ok(tagged) => {
                        if tagged {
                            info!(
                                "Tagged ECR repository as orphaned for project: {}",
                                project.name
                            );
                        } else {
                            info!(
                                "ECR repository did not exist for project: {} (already deleted)",
                                project.name
                            );
                        }
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            };

            match cleanup_result {
                Ok(()) => {
                    // Remove finalizer so project can be deleted
                    db_projects::remove_finalizer(&self.state.db_pool, project.id, ECR_FINALIZER)
                        .await?;
                    info!(
                        "Removed ECR finalizer from project: {}, cleanup complete",
                        project.name
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to cleanup ECR repository for project {}: {}",
                        project.name, e
                    );
                    // Continue to next project, will retry on next loop
                }
            }
        }

        Ok(())
    }
}
