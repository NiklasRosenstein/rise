#[cfg(feature = "backend")]
mod kubernetes;

#[cfg(feature = "backend")]
pub use kubernetes::{KubernetesController, KubernetesControllerConfig};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::models::{Deployment, DeploymentStatus, Project};
use crate::db::{deployments as db_deployments, projects};
use crate::server::deployment::state_machine;
use crate::server::state::ControllerState;

/// Result of a reconciliation operation
pub struct ReconcileResult {
    pub status: DeploymentStatus,
    pub controller_metadata: serde_json::Value,
    pub error_message: Option<String>,
}

/// Health status of a deployment
pub struct HealthStatus {
    pub healthy: bool,
    pub message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub pod_status: Option<serde_json::Value>,
}

/// URLs where a deployment can be accessed
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeploymentUrls {
    /// Default URL based on ingress template configuration
    pub default_url: String,
    /// Primary URL - the starred custom domain if one exists, otherwise the default URL
    pub primary_url: String,
    /// Additional URLs for custom domains
    pub custom_domain_urls: Vec<String>,
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
    async fn reconcile(
        &self,
        deployment: &Deployment,
        project: &Project,
    ) -> anyhow::Result<ReconcileResult>;

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

    /// Calculate URLs where this deployment can be accessed
    ///
    /// Returns the primary URL (from ingress templates) and any custom domain URLs.
    /// URLs are calculated dynamically based on current controller configuration.
    ///
    /// # Arguments
    /// * `deployment` - The deployment to get URLs for
    /// * `project` - The project this deployment belongs to
    ///
    /// # Returns
    /// A DeploymentUrls struct containing the primary URL and custom domain URLs
    async fn get_deployment_urls(
        &self,
        deployment: &Deployment,
        project: &Project,
    ) -> anyhow::Result<DeploymentUrls>;

    /// Calculate URLs where a project would be accessed for a given deployment group.
    ///
    /// Similar to `get_deployment_urls` but takes a group name string instead of a Deployment object.
    /// Used for preview endpoints where no deployment exists yet.
    async fn get_project_urls(
        &self,
        project: &Project,
        deployment_group: &str,
    ) -> anyhow::Result<DeploymentUrls>;

    /// Stream logs from a deployment
    ///
    /// Returns a stream of log bytes from the deployment's runtime (pod/container).
    /// Each backend implements this in its own way (Kubernetes pods, CloudWatch, etc.).
    ///
    /// # Arguments
    /// * `deployment` - The deployment to stream logs from
    /// * `follow` - Whether to follow the logs (stream continuously)
    /// * `tail_lines` - Optional number of lines to show from the end
    /// * `timestamps` - Whether to include timestamps in the output
    /// * `since_seconds` - Optional number of seconds to look back
    ///
    /// # Returns
    /// A boxed stream of log bytes, or an error if logs are not available
    async fn stream_logs(
        &self,
        deployment: &Deployment,
        follow: bool,
        tail_lines: Option<i64>,
        timestamps: bool,
        since_seconds: Option<i64>,
    ) -> anyhow::Result<futures::stream::BoxStream<'static, Result<bytes::Bytes, anyhow::Error>>>;
}

/// Main controller orchestrator
///
/// The DeploymentController runs background loops for:
/// 1. Reconciliation loop - processes deployments in Pushed, Deploying, Healthy, Unhealthy states
/// 2. Health check loop - monitors deployments in Healthy and Unhealthy states
/// 3. Termination loop - processes deployments in Terminating state
pub struct DeploymentController {
    state: Arc<ControllerState>,
    backend: Arc<dyn DeploymentBackend>,
    reconcile_interval: Duration,
    health_check_interval: Duration,
    termination_interval: Duration,
    cancellation_interval: Duration,
    expiration_interval: Duration,
}

impl DeploymentController {
    /// Create a new deployment controller
    ///
    /// # Arguments
    /// * `state` - Minimal controller state with database access
    /// * `backend` - The deployment backend implementation (e.g., KubernetesController)
    /// * `reconcile_interval` - How often to check for deployments to reconcile
    /// * `health_check_interval` - How often to perform health checks
    /// * `termination_interval` - How often to process terminating deployments
    /// * `cancellation_interval` - How often to process cancelling deployments
    /// * `expiration_interval` - How often to check for expired deployments
    pub fn new(
        state: Arc<ControllerState>,
        backend: Arc<dyn DeploymentBackend>,
        reconcile_interval: Duration,
        health_check_interval: Duration,
        termination_interval: Duration,
        cancellation_interval: Duration,
        expiration_interval: Duration,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            state,
            backend,
            reconcile_interval,
            health_check_interval,
            termination_interval,
            cancellation_interval,
            expiration_interval,
        })
    }

    /// Start reconciliation, health check, termination, and cancellation loops
    ///
    /// This spawns multiple tokio tasks that run in the background
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
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.termination_loop().await;
        });

        // Spawn cancellation loop
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.cancellation_loop().await;
        });

        // Spawn expiration loop
        tokio::spawn(async move {
            self.expiration_loop().await;
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

            // Reconcile active deployments
            if let Err(e) = self.reconcile_deployments().await {
                error!("Error in reconciliation loop: {}", e);
            }

            // Check for timed out deployments
            if let Err(e) = self.check_deployment_timeouts().await {
                error!("Error checking deployment timeouts: {}", e);
            }

            // Failed deployments can still have infrastructure (e.g. pod/runtime errors).
            // Queue them for termination so backend cleanup runs.
            if let Err(e) = self.queue_failed_deployments_for_cleanup().await {
                error!("Error queueing failed deployments for cleanup: {}", e);
            }
        }
    }

    /// Process all deployments in Pushed or Deploying states, and Healthy/Unhealthy deployments needing reconciliation
    async fn reconcile_deployments(&self) -> anyhow::Result<()> {
        // Find deployments that need reconciliation (Pushed or Deploying)
        let deployments = db_deployments::find_non_terminal(&self.state.db_pool, 10).await?;

        for deployment in deployments {
            let deployment_id = deployment.deployment_id.clone();
            if let Err(e) = self.reconcile_single_deployment(deployment).await {
                error!("Failed to reconcile deployment {}: {}", deployment_id, e);
            }
        }

        // Also find Healthy/Unhealthy deployments that need reconciliation
        // (due to config changes like custom domains or env vars)
        let flagged_deployments =
            db_deployments::find_needing_reconcile(&self.state.db_pool, 10).await?;

        if !flagged_deployments.is_empty() {
            info!(
                "Found {} deployment(s) with needs_reconcile flag set",
                flagged_deployments.len()
            );
        }

        for deployment in flagged_deployments {
            let deployment_id = deployment.deployment_id.clone();
            info!(
                "Reconciling deployment {} (status: {:?}) due to needs_reconcile flag",
                deployment_id, deployment.status
            );
            if let Err(e) = self.reconcile_single_deployment(deployment).await {
                error!("Failed to reconcile deployment {}: {}", deployment_id, e);
            } else {
                info!(
                    "Successfully reconciled deployment {} for config changes",
                    deployment_id
                );
            }
        }

        Ok(())
    }

    /// Reconcile a single deployment
    ///
    /// Calls the backend's reconcile method and updates the deployment in the database
    async fn reconcile_single_deployment(&self, deployment: Deployment) -> anyhow::Result<()> {
        // Skip reconciliation for deployments in cleanup states
        if matches!(
            deployment.status,
            DeploymentStatus::Terminating | DeploymentStatus::Cancelling
        ) {
            debug!(
                "Skipping reconciliation for deployment {} in {:?} state",
                deployment.deployment_id, deployment.status
            );
            return Ok(());
        }

        // Check for deployment timeout (5 minutes in Deploying state)
        if deployment.status == DeploymentStatus::Deploying {
            // Only check timeout if deploying_started_at is set
            // Deployments without this timestamp (created before this feature) won't be timed out
            if let Some(deploying_started_at) = deployment.deploying_started_at {
                let elapsed = Utc::now().signed_duration_since(deploying_started_at);
                let timeout_duration = chrono::Duration::minutes(5);

                if elapsed > timeout_duration {
                    warn!(
                        "Deployment {} timed out after {} seconds in Deploying state, marking as Terminating",
                        deployment.deployment_id, elapsed.num_seconds()
                    );

                    db_deployments::mark_terminating(
                        &self.state.db_pool,
                        deployment.id,
                        crate::db::models::TerminationReason::Failed,
                    )
                    .await?;

                    // Update project status after marking deployment as terminating
                    projects::update_calculated_status(&self.state.db_pool, deployment.project_id)
                        .await?;

                    return Ok(());
                }
            }
        }

        // Get project info
        let project = projects::find_by_id(&self.state.db_pool, deployment.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Call backend reconcile
        let result = self.backend.reconcile(&deployment, &project).await?;

        // Store status for later comparison
        let new_status = result.status.clone();

        // Update deployment with reconciliation result
        // Note: This will fail if deployment moved to Terminating/Cancelling, which is expected
        match db_deployments::update_status(&self.state.db_pool, deployment.id, result.status).await
        {
            Ok(_) => {}
            Err(e) => {
                // If update failed, deployment might have moved to cleanup state
                debug!(
                    "Failed to update deployment {} status: {}. Deployment may have moved to cleanup state.",
                    deployment.deployment_id, e
                );
                return Ok(());
            }
        }

        db_deployments::update_controller_metadata(
            &self.state.db_pool,
            deployment.id,
            &result.controller_metadata,
        )
        .await?;

        if let Some(error) = result.error_message {
            db_deployments::mark_failed(&self.state.db_pool, deployment.id, &error).await?;
        } else if new_status == DeploymentStatus::Healthy {
            // Find active deployment IN THIS GROUP *before* marking new as Healthy
            // This prevents a race condition where the query would return the new deployment
            let active_in_group = db_deployments::find_active_for_project_and_group(
                &self.state.db_pool,
                deployment.project_id,
                &deployment.deployment_group,
            )
            .await?;

            // Now mark the new deployment as healthy
            db_deployments::mark_healthy(&self.state.db_pool, deployment.id).await?;

            // Supersede old active deployment in this group
            if let Some(old_active) = active_in_group {
                if old_active.id != deployment.id && !state_machine::is_terminal(&old_active.status)
                {
                    info!(
                        "Deployment {} replacing {} in group '{}', marking old as Terminating",
                        deployment.deployment_id,
                        old_active.deployment_id,
                        deployment.deployment_group
                    );
                    db_deployments::mark_terminating(
                        &self.state.db_pool,
                        old_active.id,
                        crate::db::models::TerminationReason::Superseded,
                    )
                    .await?;
                }
            }

            // Clean up other ACTIVE (Healthy/Unhealthy) deployments in this group
            // Do NOT clean up deployments that are still being deployed (Pushed, Deploying, etc.)
            let other_in_group = db_deployments::find_non_terminal_for_project_and_group(
                &self.state.db_pool,
                project.id,
                &deployment.deployment_group,
            )
            .await?;

            for other in other_in_group {
                // Only clean up OTHER deployments that are in ACTIVE running states
                // Don't clean up deployments that are still being deployed (Pending, Building, Pushing, Pushed, Deploying)
                if other.id != deployment.id
                    && state_machine::is_active(&other.status)
                    && !state_machine::is_terminal(&other.status)
                {
                    info!(
                        "Cleaning up non-active deployment {} in group '{}', marking as Terminating",
                        other.deployment_id, deployment.deployment_group
                    );
                    db_deployments::mark_terminating(
                        &self.state.db_pool,
                        other.id,
                        crate::db::models::TerminationReason::Superseded,
                    )
                    .await?;
                }
            }

            // Mark deployment as active
            db_deployments::mark_as_active(
                &self.state.db_pool,
                deployment.id,
                project.id,
                &deployment.deployment_group,
            )
            .await?;
        }

        // Update project status
        projects::update_calculated_status(&self.state.db_pool, project.id).await?;

        // Clear needs_reconcile flag if it was set
        if deployment.needs_reconcile {
            db_deployments::clear_needs_reconcile(&self.state.db_pool, deployment.id).await?;
            debug!(
                "Cleared needs_reconcile flag for deployment {}",
                deployment.deployment_id
            );
        }

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
        let healthy_deployments =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Healthy).await?;

        for deployment in healthy_deployments {
            match self.backend.health_check(&deployment).await {
                Ok(health) => {
                    // Update health status in metadata
                    let mut metadata = deployment.controller_metadata.clone();
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert(
                            "health".to_string(),
                            serde_json::json!({
                                "healthy": health.healthy,
                                "message": health.message,
                                "last_check": health.last_check.to_rfc3339(),
                            }),
                        );

                        // Store pod_status if available
                        if let Some(pod_status) = health.pod_status {
                            obj.insert("pod_status".to_string(), pod_status);
                        }
                    }
                    db_deployments::update_controller_metadata(
                        &self.state.db_pool,
                        deployment.id,
                        &metadata,
                    )
                    .await?;

                    // If unhealthy, mark deployment as Unhealthy (not Failed - allow recovery)
                    if !health.healthy {
                        let msg = health
                            .message
                            .unwrap_or_else(|| "Health check failed".to_string());
                        warn!(
                            "Deployment {} is now unhealthy: {}",
                            deployment.deployment_id, msg
                        );
                        info!(
                            "Calling mark_unhealthy for deployment id: {}",
                            deployment.id
                        );
                        match db_deployments::mark_unhealthy(
                            &self.state.db_pool,
                            deployment.id,
                            msg.clone(),
                        )
                        .await
                        {
                            Ok(updated) => {
                                info!("Successfully marked deployment {} as Unhealthy. New status: {:?}", deployment.deployment_id, updated.status);
                            }
                            Err(e) => {
                                error!(
                                    "Failed to mark deployment {} as unhealthy: {}",
                                    deployment.deployment_id, e
                                );
                                return Err(e);
                            }
                        }
                        projects::update_calculated_status(
                            &self.state.db_pool,
                            deployment.project_id,
                        )
                        .await?;
                    }
                }
                Err(e) => {
                    warn!(
                        "Health check error for deployment {}: {}",
                        deployment.deployment_id, e
                    );
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
        let unhealthy_deployments =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Unhealthy)
                .await?;

        for deployment in unhealthy_deployments {
            debug!(
                "Checking unhealthy deployment {} for recovery",
                deployment.deployment_id
            );
            match self.backend.health_check(&deployment).await {
                Ok(health) => {
                    if health.healthy {
                        // Deployment has recovered!
                        info!(
                            "Deployment {} has recovered, marking as Healthy",
                            deployment.deployment_id
                        );
                        db_deployments::mark_healthy(&self.state.db_pool, deployment.id).await?;
                        projects::update_calculated_status(
                            &self.state.db_pool,
                            deployment.project_id,
                        )
                        .await?;
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
                    warn!(
                        "Health check error for unhealthy deployment {}: {}",
                        deployment.deployment_id, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Termination loop - processes deployments in Terminating state
    ///
    /// Terminates containers for deployments marked as Terminating
    async fn termination_loop(&self) {
        info!("Deployment termination loop started");
        let mut ticker = interval(self.termination_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.process_terminating_deployments().await {
                error!("Error in termination loop: {}", e);
            }
        }
    }

    /// Process all deployments in Terminating state
    async fn process_terminating_deployments(&self) -> anyhow::Result<()> {
        let terminating =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Terminating)
                .await?;

        for deployment in terminating {
            debug!("Terminating deployment {}", deployment.deployment_id);

            // Call backend to terminate (stop and remove container)
            match self.backend.terminate(&deployment).await {
                Ok(_) => {
                    info!(
                        "Successfully terminated deployment {}",
                        deployment.deployment_id
                    );

                    // Mark as terminal state based on termination reason
                    match deployment.termination_reason {
                        Some(crate::db::models::TerminationReason::Superseded) => {
                            db_deployments::mark_superseded(&self.state.db_pool, deployment.id)
                                .await?;
                        }
                        Some(crate::db::models::TerminationReason::UserStopped) => {
                            db_deployments::mark_stopped(&self.state.db_pool, deployment.id)
                                .await?;
                        }
                        Some(crate::db::models::TerminationReason::Failed) => {
                            // Preserve the original deployment failure reason if present.
                            let error_message = deployment
                                .error_message
                                .as_deref()
                                .unwrap_or("Deployment failed");
                            db_deployments::mark_failed(
                                &self.state.db_pool,
                                deployment.id,
                                error_message,
                            )
                            .await?;
                        }
                        Some(crate::db::models::TerminationReason::Expired) => {
                            db_deployments::mark_expired(&self.state.db_pool, deployment.id)
                                .await?;
                        }
                        Some(crate::db::models::TerminationReason::Cancelled) | None => {
                            // Cancelled or unknown reason - default to Stopped
                            db_deployments::mark_stopped(&self.state.db_pool, deployment.id)
                                .await?;
                        }
                    }

                    // Update project status
                    projects::update_calculated_status(&self.state.db_pool, deployment.project_id)
                        .await?;
                }
                Err(e) => {
                    error!(
                        "Failed to terminate deployment {}: {}",
                        deployment.deployment_id, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Queue terminal Failed deployments for termination so backend infrastructure gets cleaned up.
    ///
    /// Some failures happen after infrastructure is already created (e.g. pod runtime/readiness errors).
    /// These deployments are already terminal, so they won't be reconciled again unless explicitly queued.
    async fn queue_failed_deployments_for_cleanup(&self) -> anyhow::Result<()> {
        let failed =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Failed).await?;

        for deployment in failed {
            // Failed deployments that already came through termination have this reason set.
            if matches!(
                deployment.termination_reason,
                Some(crate::db::models::TerminationReason::Failed)
            ) {
                continue;
            }

            info!(
                "Queueing failed deployment {} for cleanup",
                deployment.deployment_id
            );

            db_deployments::mark_terminating(
                &self.state.db_pool,
                deployment.id,
                crate::db::models::TerminationReason::Failed,
            )
            .await?;
        }

        Ok(())
    }

    /// Cancellation loop - processes deployments in Cancelling state
    ///
    /// Cancels deployments that haven't provisioned infrastructure yet
    async fn cancellation_loop(&self) {
        info!("Deployment cancellation loop started");
        let mut ticker = interval(self.cancellation_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.process_cancelling_deployments().await {
                error!("Error in cancellation loop: {}", e);
            }
        }
    }

    /// Process all deployments in Cancelling state
    async fn process_cancelling_deployments(&self) -> anyhow::Result<()> {
        let cancelling =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Cancelling)
                .await?;

        for deployment in cancelling {
            debug!("Cancelling deployment {}", deployment.deployment_id);

            // Call backend to cancel (cleanup build artifacts, no infrastructure to remove)
            match self.backend.cancel(&deployment).await {
                Ok(_) => {
                    info!(
                        "Successfully cancelled deployment {}",
                        deployment.deployment_id
                    );

                    // Mark as Cancelled
                    db_deployments::mark_cancelled(&self.state.db_pool, deployment.id).await?;

                    // Update project status
                    projects::update_calculated_status(&self.state.db_pool, deployment.project_id)
                        .await?;
                }
                Err(e) => {
                    error!(
                        "Failed to cancel deployment {}: {}",
                        deployment.deployment_id, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Expiration loop - monitors and cleans up expired deployments
    ///
    /// Checks for deployments past their expires_at timestamp
    async fn expiration_loop(&self) {
        info!("Deployment expiration loop started");
        let mut ticker = interval(self.expiration_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.cleanup_expired_deployments().await {
                error!("Error in expiration loop: {}", e);
            }
        }
    }

    /// Find and terminate expired deployments
    async fn cleanup_expired_deployments(&self) -> anyhow::Result<()> {
        let expired = db_deployments::find_expired(&self.state.db_pool, 50).await?;

        for deployment in &expired {
            info!(
                "Deployment {} in group '{}' has expired, marking as Terminating",
                deployment.deployment_id, deployment.deployment_group
            );

            // Mark as terminating with Expired reason
            db_deployments::mark_terminating(
                &self.state.db_pool,
                deployment.id,
                crate::db::models::TerminationReason::Expired,
            )
            .await?;

            // Update project status
            projects::update_calculated_status(&self.state.db_pool, deployment.project_id).await?;
        }

        if !expired.is_empty() {
            info!("Cleaned up {} expired deployments", expired.len());
        }

        Ok(())
    }

    /// Check for deployments stuck in pre-Pushed states and mark them as Failed
    ///
    /// Deployments stuck in Pending, Building, or Pushing for >10 minutes are timed out.
    /// This handles cases where the CLI is interrupted before pushing the image.
    async fn check_deployment_timeouts(&self) -> anyhow::Result<()> {
        // Find deployments stuck in pre-Pushed states for >10 minutes
        let timeout_threshold = Utc::now() - chrono::Duration::minutes(10);

        let stuck_deployments = db_deployments::find_stuck_pre_pushed_before(
            &self.state.db_pool,
            timeout_threshold,
            50,
        )
        .await?;

        for deployment in stuck_deployments {
            warn!(
                "Deployment {} stuck in {} state for >10 minutes, marking as Failed",
                deployment.deployment_id, deployment.status
            );

            let error_msg = format!(
                "Deployment timed out after 10 minutes in {} state. \
                 This usually indicates the CLI was interrupted during build/push.",
                deployment.status
            );

            if let Err(e) =
                db_deployments::mark_failed(&self.state.db_pool, deployment.id, &error_msg).await
            {
                error!(
                    "Failed to mark deployment {} as failed: {}",
                    deployment.deployment_id, e
                );
            } else {
                // Update project status after marking as failed
                if let Err(e) =
                    projects::update_calculated_status(&self.state.db_pool, deployment.project_id)
                        .await
                {
                    error!(
                        "Failed to update project status for deployment {}: {}",
                        deployment.deployment_id, e
                    );
                }
            }
        }

        Ok(())
    }
}
