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
/// The DeploymentController runs two background loops:
/// 1. Reconciliation loop - processes deployments in Pushed or Deploying states
/// 2. Health check loop - monitors deployments in Completed state
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
            reconcile_interval: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
        })
    }

    /// Start both reconciliation and health check loops
    ///
    /// This spawns two tokio tasks that run in the background
    pub fn start(self: Arc<Self>) {
        // Spawn reconciliation loop
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.reconcile_loop().await;
        });

        // Spawn health check loop
        tokio::spawn(async move {
            self.health_check_loop().await;
        });
    }

    /// Reconciliation loop - processes non-terminal deployments
    ///
    /// Runs every 5 seconds and processes deployments in Pushed or Deploying states
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
        } else if new_status == DeploymentStatus::Completed {
            db_deployments::mark_completed(&self.state.db_pool, deployment.id).await?;
            projects::set_active_deployment(&self.state.db_pool, project.id, deployment.id).await?;
        }

        // Update project status
        projects::update_calculated_status(&self.state.db_pool, project.id).await?;

        Ok(())
    }

    /// Health check loop - monitors active deployments
    ///
    /// Runs every 30 seconds and checks health of all Completed deployments
    async fn health_check_loop(&self) {
        info!("Deployment health check loop started");
        let mut ticker = interval(self.health_check_interval);

        loop {
            if let Err(e) = self.check_deployment_health().await {
                error!("Error in health check loop: {}", e);
            }
            ticker.tick().await;
        }
    }

    /// Check health of all completed deployments
    ///
    /// If a deployment is unhealthy, marks it as Failed
    async fn check_deployment_health(&self) -> anyhow::Result<()> {
        // Find all Completed deployments
        let deployments = db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Completed).await?;

        info!("Checking health for {} deployments", deployments.len());
        for deployment in deployments {
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

                    // If unhealthy, mark deployment as Failed
                    if !health.healthy {
                        let msg = health.message.unwrap_or_else(|| "Health check failed".to_string());
                        warn!("Deployment {} failed health check: {}", deployment.deployment_id, msg);
                        db_deployments::mark_failed(&self.state.db_pool, deployment.id, &msg).await?;
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
}
