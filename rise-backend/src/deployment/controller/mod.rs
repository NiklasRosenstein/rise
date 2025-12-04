mod docker;

pub use docker::DockerController;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::models::{Deployment, Project, DeploymentStatus, TerminationReason};
use crate::db::{deployments as db_deployments, projects};
use crate::deployment::state_machine;
use crate::state::AppState;

/// Hint for when to reconcile a deployment next
#[derive(Debug, Clone)]
pub enum ReconcileHint {
    /// Reconcile again immediately (status changed)
    Immediate,
    /// Reconcile after specific duration (e.g., retry after 30s, poll after 10s)
    After(Duration),
    /// Use default reconciliation interval
    Default,
}

/// Result of a reconciliation operation
pub struct ReconcileResult {
    pub status: DeploymentStatus,
    pub deployment_url: Option<String>,
    pub controller_metadata: serde_json::Value,
    pub error_message: Option<String>,
    pub next_reconcile: ReconcileHint,
}

/// Health status of a deployment
pub struct HealthStatus {
    pub healthy: bool,
    pub message: Option<String>,
    pub last_check: DateTime<Utc>,
}

/// Trait that all deployment backends must implement
///
/// This trait allows for multiple controller implementations (Docker, Kubernetes, etc.)
/// Each controller manages deployments in its own way and stores controller-specific
/// metadata in the deployment's controller_metadata JSONB field.
#[async_trait]
pub trait DeploymentBackend: Send + Sync {
    /// Reconcile a deployment - bring it to desired state
    ///
    /// This method is called repeatedly until the deployment reaches a terminal state.
    /// It should be idempotent and able to handle being called multiple times.
    /// The controller can use the deployment's controller_metadata field to track
    /// reconciliation progress across calls.
    ///
    /// # Arguments
    /// * `deployment` - The deployment to reconcile
    /// * `project` - The project this deployment belongs to
    ///
    /// # Returns
    /// A ReconcileResult containing the new status, metadata, and optional error message
    async fn reconcile(&self, deployment: &Deployment, project: &Project) -> anyhow::Result<ReconcileResult>;

    /// Check health of a running deployment
    ///
    /// Used by the backend to monitor active deployments. Should check if the
    /// deployment's container/pod is still running and healthy.
    ///
    /// # Arguments
    /// * `deployment` - The deployment to check
    ///
    /// # Returns
    /// A HealthStatus indicating if the deployment is healthy
    async fn health_check(&self, deployment: &Deployment) -> anyhow::Result<HealthStatus>;

    /// Stop a running deployment
    ///
    /// Called when a deployment needs to be stopped (e.g., during cleanup or rollback)
    ///
    /// # Arguments
    /// * `deployment` - The deployment to stop
    async fn stop(&self, deployment: &Deployment) -> anyhow::Result<()>;

    /// Cancel a deployment that hasn't provisioned infrastructure yet
    ///
    /// Called when deployment is in Cancelling state (pre-infrastructure).
    /// Should cleanup build artifacts but no infrastructure to deprovision.
    ///
    /// # Arguments
    /// * `deployment` - The deployment to cancel
    async fn cancel(&self, deployment: &Deployment) -> anyhow::Result<()>;

    /// Terminate a running deployment gracefully
    ///
    /// Called when deployment is in Terminating state (post-infrastructure).
    /// Should deprovision infrastructure (stop and remove containers/pods).
    ///
    /// # Arguments
    /// * `deployment` - The deployment to terminate
    async fn terminate(&self, deployment: &Deployment) -> anyhow::Result<()>;
}

/// Main controller orchestrator
///
/// The DeploymentController runs three background loops:
/// 1. Reconciliation loop - processes deployments in Pushed, Deploying, Healthy, Unhealthy states
/// 2. Health check loop - monitors deployments in Healthy and Unhealthy states
/// 3. Termination loop - processes deployments in Terminating state
pub struct DeploymentController {
    state: Arc<AppState>,
    backend: Arc<dyn DeploymentBackend>,
    reconcile_interval: Duration,
    health_check_interval: Duration,
}

impl DeploymentController {
    /// Create a new deployment controller
    ///
    /// # Arguments
    /// * `state` - The application state (contains DB pool, settings, etc.)
    pub fn new(state: Arc<AppState>) -> anyhow::Result<Self> {
        // For MVP, hardcode Docker backend
        // Future: read from config to select backend type
        let backend: Arc<dyn DeploymentBackend> = Arc::new(DockerController::new()?);

        Ok(Self {
            state,
            backend,
            reconcile_interval: Duration::from_secs(15),
            health_check_interval: Duration::from_secs(5),
        })
    }

    /// Start reconciliation, health check, and termination loops
    ///
    /// This spawns three tokio tasks that run in the background
    pub fn start(self: Arc<Self>) {
        // Spawn reconciliation loop
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.reconcile_loop().await;
        });

        // Spawn health check loop
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.health_check_loop().await;
        });

        // Spawn termination loop
        tokio::spawn(async move {
            self.termination_loop().await;
        });
    }

    /// Reconciliation loop - processes non-terminal deployments
    ///
    /// Runs every 15 seconds and processes deployments in Pushed or Deploying states
    async fn reconcile_loop(&self) {
        info!("Deployment reconciliation loop started");
        let mut ticker = interval(self.reconcile_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.reconcile_deployments().await {
                error!("Error in reconciliation loop: {}", e);
            }
        }
    }

    /// Process all deployments in Pushed or Deploying states
    async fn reconcile_deployments(&self) -> anyhow::Result<()> {
        // Find deployments that need reconciliation (Pushed or Deploying)
        let deployments = db_deployments::find_non_terminal(&self.state.db_pool, 10).await?;

        for deployment in deployments {
            let deployment_id = deployment.deployment_id.clone();
            if let Err(e) = self.reconcile_single_deployment(deployment).await {
                error!("Failed to reconcile deployment {}: {}", deployment_id, e);
            }
        }

        Ok(())
    }

    /// Reconcile a single deployment
    ///
    /// Calls the backend's reconcile method and updates the deployment in the database
    async fn reconcile_single_deployment(&self, deployment: Deployment) -> anyhow::Result<()> {
        // Get project info
        let project = projects::find_by_id(&self.state.db_pool, deployment.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Call backend reconcile
        let result = self.backend.reconcile(&deployment, &project).await?;

        // Store status for later comparison
        let new_status = result.status.clone();

        // Update deployment with reconciliation result
        db_deployments::update_status(&self.state.db_pool, deployment.id, result.status).await?;

        if let Some(url) = result.deployment_url {
            db_deployments::update_deployment_url(&self.state.db_pool, deployment.id, &url).await?;
        }

        db_deployments::update_controller_metadata(
            &self.state.db_pool,
            deployment.id,
            &result.controller_metadata
        ).await?;

        if let Some(error) = result.error_message {
            db_deployments::mark_failed(&self.state.db_pool, deployment.id, &error).await?;
        } else if new_status == DeploymentStatus::Healthy {
            // New deployment just became healthy - set as active
            db_deployments::mark_healthy(&self.state.db_pool, deployment.id).await?;

            // Handle supersession: if there's an old active deployment, mark it for termination
            if let Some(old_active_id) = project.active_deployment_id {
                if old_active_id != deployment.id {
                    // Get the old active deployment
                    if let Ok(Some(old_deployment)) = db_deployments::find_by_id(&self.state.db_pool, old_active_id).await {
                        // Supersede old deployment if it's not already terminal
                        if !state_machine::is_terminal(&old_deployment.status) {
                            info!(
                                "Deployment {} is replacing {} (status={:?}), marking old as Terminating",
                                deployment.deployment_id, old_deployment.deployment_id, old_deployment.status
                            );
                            db_deployments::mark_terminating(
                                &self.state.db_pool,
                                old_active_id,
                                crate::db::models::TerminationReason::Superseded
                            ).await?;
                        }
                    }
                }
            }

            // Set new deployment as active
            projects::set_active_deployment(&self.state.db_pool, project.id, deployment.id).await?;

            // Also clean up any other non-terminal deployments for this project
            // (deployments that started but never became active)
            let other_deployments = db_deployments::find_non_terminal_for_project(
                &self.state.db_pool,
                project.id
            ).await?;

            for other_deployment in other_deployments {
                // Skip the new active deployment
                if other_deployment.id == deployment.id {
                    continue;
                }

                // Skip if already terminal
                if state_machine::is_terminal(&other_deployment.status) {
                    continue;
                }

                info!(
                    "Cleaning up non-active deployment {} (status={:?}) for project {}, marking as Terminating",
                    other_deployment.deployment_id, other_deployment.status, project.name
                );

                db_deployments::mark_terminating(
                    &self.state.db_pool,
                    other_deployment.id,
                    crate::db::models::TerminationReason::Superseded
                ).await?;
            }
        }

        // Update project status
        projects::update_calculated_status(&self.state.db_pool, project.id).await?;

        Ok(())
    }

    /// Health check loop - monitors active deployments
    ///
    /// Runs every 5 seconds and checks health of all Healthy/Unhealthy deployments
    async fn health_check_loop(&self) {
        info!("Deployment health check loop started");
        let mut ticker = interval(self.health_check_interval);

        loop {
            // Check Healthy deployments (may transition to Unhealthy)
            if let Err(e) = self.check_deployment_health().await {
                error!("Error checking deployment health: {}", e);
            }

            // Monitor Unhealthy deployments (may recover to Healthy or timeout to Failed)
            if let Err(e) = self.monitor_unhealthy_deployments().await {
                error!("Error monitoring unhealthy deployments: {}", e);
            }

            ticker.tick().await;
        }
    }

    /// Check health of all Healthy deployments
    ///
    /// If a deployment is unhealthy, marks it as Unhealthy (not Failed)
    async fn check_deployment_health(&self) -> anyhow::Result<()> {
        // Find all Healthy deployments
        let healthy_deployments = db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Healthy).await?;

        info!("Checking health for {} healthy deployments", healthy_deployments.len());
        for deployment in healthy_deployments {
            info!("Checking health for deployment {}", deployment.deployment_id);
            match self.backend.health_check(&deployment).await {
                Ok(health) => {
                    // Update health status in metadata
                    let mut metadata = deployment.controller_metadata.clone();
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert("health".to_string(), serde_json::json!({
                            "healthy": health.healthy,
                            "message": health.message,
                            "last_check": health.last_check.to_rfc3339(),
                        }));
                    }
                    db_deployments::update_controller_metadata(&self.state.db_pool, deployment.id, &metadata).await?;

                    // If unhealthy, mark deployment as Unhealthy (not Failed - allow recovery)
                    if !health.healthy {
                        let msg = health.message.unwrap_or_else(|| "Health check failed".to_string());
                        warn!("Deployment {} is now unhealthy: {}", deployment.deployment_id, msg);
                        info!("Calling mark_unhealthy for deployment id: {}", deployment.id);
                        match db_deployments::mark_unhealthy(&self.state.db_pool, deployment.id, msg.clone()).await {
                            Ok(updated) => {
                                info!("Successfully marked deployment {} as Unhealthy. New status: {:?}", deployment.deployment_id, updated.status);
                            }
                            Err(e) => {
                                error!("Failed to mark deployment {} as unhealthy: {}", deployment.deployment_id, e);
                                return Err(e);
                            }
                        }
                        projects::update_calculated_status(&self.state.db_pool, deployment.project_id).await?;
                    }
                }
                Err(e) => {
                    warn!("Health check error for deployment {}: {}", deployment.deployment_id, e);
                }
            }
        }

        Ok(())
    }

    /// Monitor Unhealthy deployments for recovery
    ///
    /// Checks all Unhealthy deployments to see if they've recovered (mark as Healthy).
    /// Unhealthy deployments stay Unhealthy indefinitely until they recover or get terminated.
    async fn monitor_unhealthy_deployments(&self) -> anyhow::Result<()> {
        // Find all Unhealthy deployments
        let unhealthy_deployments = db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Unhealthy).await?;

        info!("Monitoring {} unhealthy deployments for recovery", unhealthy_deployments.len());
        for deployment in unhealthy_deployments {
            info!("Checking unhealthy deployment {} for recovery", deployment.deployment_id);
            match self.backend.health_check(&deployment).await {
                Ok(health) => {
                    if health.healthy {
                        // Deployment has recovered!
                        info!("Deployment {} has recovered, marking as Healthy", deployment.deployment_id);
                        db_deployments::mark_healthy(&self.state.db_pool, deployment.id).await?;
                        projects::update_calculated_status(&self.state.db_pool, deployment.project_id).await?;
                    } else {
                        // Still unhealthy - keep waiting for recovery or explicit termination
                        let unhealthy_duration = chrono::Utc::now() - deployment.updated_at;
                        info!(
                            "Deployment {} still unhealthy ({} minutes), waiting for recovery or termination",
                            deployment.deployment_id,
                            unhealthy_duration.num_minutes()
                        );
                    }
                }
                Err(e) => {
                    warn!("Health check error for unhealthy deployment {}: {}", deployment.deployment_id, e);
                }
            }
        }

        Ok(())
    }

    /// Termination loop - processes deployments in Terminating state
    ///
    /// Runs every 5 seconds and terminates containers for deployments marked as Terminating
    async fn termination_loop(&self) {
        info!("Deployment termination loop started");
        let mut ticker = interval(Duration::from_secs(5));

        loop {
            ticker.tick().await;
            if let Err(e) = self.process_terminating_deployments().await {
                error!("Error in termination loop: {}", e);
            }
        }
    }

    /// Process all deployments in Terminating state
    async fn process_terminating_deployments(&self) -> anyhow::Result<()> {
        let terminating = db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Terminating).await?;

        info!("Processing {} terminating deployments", terminating.len());
        for deployment in terminating {
            info!("Terminating deployment {}", deployment.deployment_id);

            // Call backend to terminate (stop and remove container)
            match self.backend.terminate(&deployment).await {
                Ok(_) => {
                    info!("Successfully terminated deployment {}", deployment.deployment_id);

                    // Mark as terminal state based on termination reason
                    match deployment.termination_reason {
                        Some(crate::db::models::TerminationReason::Superseded) => {
                            db_deployments::mark_superseded(&self.state.db_pool, deployment.id).await?;
                        }
                        Some(crate::db::models::TerminationReason::UserStopped) => {
                            db_deployments::mark_stopped(&self.state.db_pool, deployment.id).await?;
                        }
                        _ => {
                            // Default to Stopped
                            db_deployments::mark_stopped(&self.state.db_pool, deployment.id).await?;
                        }
                    }

                    // Update project status
                    projects::update_calculated_status(&self.state.db_pool, deployment.project_id).await?;
                }
                Err(e) => {
                    error!("Failed to terminate deployment {}: {}", deployment.deployment_id, e);
                }
            }
        }

        Ok(())
    }
}
