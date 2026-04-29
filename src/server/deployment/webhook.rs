//! Metacontroller sync and finalize webhook handlers.
//!
//! Metacontroller calls the sync webhook periodically (every N seconds) and whenever
//! the `RiseProject` CRD changes. The webhook inspects the database for the project's
//! current state, performs health checks using observed K8s resources, updates DB
//! statuses, and returns the desired set of child K8s resources.
//!
//! Metacontroller then reconciles: it creates/updates resources that are returned
//! and deletes resources that are NOT returned (garbage collection).

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment as K8sDeployment;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tracing::{debug, error, info, warn};

use crate::db::models::{Deployment, DeploymentStatus, Project, TerminationReason};
use crate::db::{
    deployments as db_deployments, environments as db_environments, projects as db_projects,
};
use crate::server::deployment::crd;
use crate::server::deployment::resource_builder::{
    ResourceBuilder, ANNOTATION_LAST_REFRESH, IMAGE_PULL_SECRET_NAME,
    IRRECOVERABLE_CONTAINER_REASONS, LABEL_DEPLOYMENT_ID,
};
use crate::server::deployment::state_machine;
use crate::server::state::AppState;

// ── Metacontroller webhook protocol types ──────────────────────────────

/// Metacontroller sync request
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub parent: serde_json::Value,
    #[serde(default)]
    pub children: ObservedChildren,
}

/// Observed children keyed by "Kind.apiVersion" → name → resource JSON
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct ObservedChildren {
    #[serde(rename = "Namespace.v1", default)]
    pub namespaces: HashMap<String, serde_json::Value>,
    #[serde(rename = "Secret.v1", default)]
    pub secrets: HashMap<String, serde_json::Value>,
    #[serde(rename = "ServiceAccount.v1", default)]
    pub service_accounts: HashMap<String, serde_json::Value>,
    #[serde(rename = "Deployment.apps/v1", default)]
    pub deployments: HashMap<String, serde_json::Value>,
    #[serde(rename = "Service.v1", default)]
    pub services: HashMap<String, serde_json::Value>,
    #[serde(rename = "Ingress.networking.k8s.io/v1", default)]
    pub ingresses: HashMap<String, serde_json::Value>,
    #[serde(rename = "NetworkPolicy.networking.k8s.io/v1", default)]
    pub network_policies: HashMap<String, serde_json::Value>,
}

/// Metacontroller sync response
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub status: serde_json::Value,
    pub children: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "resyncAfterSeconds")]
    pub resync_after_seconds: Option<f64>,
}

/// Metacontroller finalize request (same shape as sync)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct FinalizeRequest {
    pub parent: serde_json::Value,
    #[serde(default)]
    pub children: ObservedChildren,
}

/// Metacontroller finalize response
#[derive(Debug, Serialize)]
pub struct FinalizeResponse {
    pub status: serde_json::Value,
    pub children: Vec<serde_json::Value>,
    pub finalized: bool,
}

// ── Deployment timeout ─────────────────────────────────────────────────

/// Duration a deployment can be in Deploying state before timing out
const DEPLOYING_TIMEOUT_MINUTES: i64 = 5;
/// Duration a deployment can be in pre-Pushed states before timing out
const PRE_PUSHED_TIMEOUT_MINUTES: i64 = 10;
/// Duration after which image pull secret is refreshed (6 hours)
const SECRET_REFRESH_HOURS: i64 = 6;

// ── Webhook authentication ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WebhookQuery {
    token: Option<String>,
}

fn validate_webhook_token(
    state: &AppState,
    provided: &Option<String>,
) -> Result<(), (StatusCode, &'static str)> {
    let Some(expected) = &state.metacontroller_webhook_token else {
        return Ok(());
    };
    let Some(provided) = provided else {
        return Err((StatusCode::FORBIDDEN, "Missing webhook token"));
    };
    if expected.as_bytes().ct_eq(provided.as_bytes()).into() {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Invalid webhook token"))
    }
}

// ── Sync webhook handler ───────────────────────────────────────────────

pub async fn handle_sync(
    State(state): State<AppState>,
    Query(query): Query<WebhookQuery>,
    Json(request): Json<SyncRequest>,
) -> Response {
    if let Err((status, msg)) = validate_webhook_token(&state, &query.token) {
        return (status, msg).into_response();
    }
    let project_name = match request
        .parent
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
    {
        Some(name) => name.to_string(),
        None => {
            error!("Sync request missing parent.metadata.name");
            return (
                StatusCode::BAD_REQUEST,
                Json(SyncResponse {
                    status: serde_json::json!({"error": "missing parent name"}),
                    children: vec![],
                    resync_after_seconds: Some(300.0),
                }),
            )
                .into_response();
        }
    };

    match process_sync(&state, &project_name, &request.children).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!(project = %project_name, "Sync webhook error: {:?}", e);
            // Return 500 so Metacontroller treats this as a failed sync and does NOT
            // apply the (empty) children list, which would garbage-collect all resources.
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("{:#}", e),
                    "lastSyncTime": Utc::now().to_rfc3339(),
                })),
            )
                .into_response()
        }
    }
}

async fn process_sync(
    state: &AppState,
    project_name: &str,
    observed: &ObservedChildren,
) -> anyhow::Result<SyncResponse> {
    // 1. Load project from DB
    let project = match db_projects::find_by_name(&state.db_pool, project_name).await? {
        Some(p) => p,
        None => {
            warn!(project = %project_name, "Project not found in DB, deleting orphaned RiseProject CRD");
            // Auto-delete the orphaned CRD so Metacontroller stops syncing it
            if let Some(ref kube_client) = state.kube_client {
                if let Err(e) = crd::delete_rise_project(kube_client, project_name).await {
                    error!(project = %project_name, "Failed to delete orphaned RiseProject CRD: {:?}", e);
                }
            }
            return Ok(SyncResponse {
                status: serde_json::json!({
                    "error": "project not found",
                    "lastSyncTime": Utc::now().to_rfc3339(),
                }),
                children: vec![],
                resync_after_seconds: Some(300.0),
            });
        }
    };

    // 2. Load all deployments for this project
    let all_deployments = db_deployments::list_for_project(&state.db_pool, project.id).await?;

    // Split into non-terminal and terminal
    let non_terminal: Vec<&Deployment> = all_deployments
        .iter()
        .filter(|d| !state_machine::is_terminal(&d.status))
        .collect();

    // 3. Perform status transitions based on observed K8s state
    perform_status_transitions(state, &project, &non_terminal, observed).await?;

    // 4. Re-load deployments since statuses may have changed
    let all_deployments = db_deployments::list_for_project(&state.db_pool, project.id).await?;
    let project = match db_projects::find_by_id(&state.db_pool, project.id).await? {
        Some(p) => p,
        None => {
            // Project was deleted between initial lookup and now — return empty children
            return Ok(SyncResponse {
                status: serde_json::json!({
                    "lastSyncTime": Utc::now().to_rfc3339(),
                }),
                children: vec![],
                resync_after_seconds: Some(300.0),
            });
        }
    };

    // 5. Get ResourceBuilder
    let resource_builder = match &state.resource_builder {
        Some(rb) => rb,
        None => {
            return Ok(SyncResponse {
                status: serde_json::json!({
                    "error": "no resource builder configured",
                    "lastSyncTime": Utc::now().to_rfc3339(),
                }),
                children: vec![],
                resync_after_seconds: Some(300.0),
            });
        }
    };

    // 6. Compute desired children
    let children = compute_desired_children(
        state,
        resource_builder,
        &project,
        &all_deployments,
        observed,
    )
    .await?;

    Ok(SyncResponse {
        status: serde_json::json!({
            "lastSyncTime": Utc::now().to_rfc3339(),
        }),
        children,
        resync_after_seconds: None,
    })
}

/// Perform status transitions based on observed Kubernetes state.
///
/// This replaces the reconcile loop, health check loop, expiration loop,
/// and timeout checks from the old controller.
async fn perform_status_transitions(
    state: &AppState,
    project: &Project,
    non_terminal: &[&Deployment],
    observed: &ObservedChildren,
) -> anyhow::Result<()> {
    for deployment in non_terminal {
        // Skip pre-infrastructure deployments — the CLI drives those transitions
        if matches!(
            deployment.status,
            DeploymentStatus::Pending | DeploymentStatus::Building | DeploymentStatus::Pushing
        ) {
            // Check for pre-pushed timeout
            check_pre_pushed_timeout(state, deployment).await?;
            continue;
        }

        // Handle Cancelling — mark as Cancelled immediately (no infra to clean up)
        if deployment.status == DeploymentStatus::Cancelling {
            info!(
                deployment_id = %deployment.deployment_id,
                "Cancelling deployment — marking as Cancelled"
            );
            db_deployments::mark_cancelled(&state.db_pool, deployment.id).await?;
            db_projects::update_calculated_status(&state.db_pool, project.id).await?;
            continue;
        }

        // Handle Terminating — mark as terminal based on reason
        // (Metacontroller will delete the K8s Deployment since we won't return it)
        if deployment.status == DeploymentStatus::Terminating {
            complete_termination(state, deployment, project).await?;
            continue;
        }

        // For Pushed/Deploying/Healthy/Unhealthy — check observed K8s Deployment
        match deployment.status {
            DeploymentStatus::Pushed => {
                // Transition Pushed → Deploying
                info!(
                    deployment_id = %deployment.deployment_id,
                    "Deployment image pushed, transitioning to Deploying"
                );
                db_deployments::update_status(
                    &state.db_pool,
                    deployment.id,
                    DeploymentStatus::Deploying,
                )
                .await?;
                db_projects::update_calculated_status(&state.db_pool, project.id).await?;
            }

            DeploymentStatus::Deploying => {
                check_deploying_timeout(state, deployment, project).await?;
                check_deployment_health_from_observed(state, deployment, project, observed).await?;
            }

            DeploymentStatus::Healthy | DeploymentStatus::Unhealthy => {
                check_deployment_health_from_observed(state, deployment, project, observed).await?;
            }

            _ => {}
        }
    }

    // Check for expired deployments
    check_expirations(state, non_terminal, project).await?;

    // Failed deployments don't need explicit cleanup: `should_have_infrastructure` returns
    // false for Failed status, so Metacontroller garbage-collects their K8s resources.

    Ok(())
}

/// Check if a pre-pushed deployment has timed out
async fn check_pre_pushed_timeout(state: &AppState, deployment: &Deployment) -> anyhow::Result<()> {
    let elapsed = Utc::now().signed_duration_since(deployment.created_at);
    if elapsed > chrono::Duration::minutes(PRE_PUSHED_TIMEOUT_MINUTES) {
        warn!(
            deployment_id = %deployment.deployment_id,
            "Deployment stuck in {} state for >{} minutes, marking as Failed",
            deployment.status,
            PRE_PUSHED_TIMEOUT_MINUTES
        );
        let error_msg = format!(
            "Deployment timed out after {} minutes in {} state. \
             This usually indicates the CLI was interrupted during build/push.",
            PRE_PUSHED_TIMEOUT_MINUTES, deployment.status
        );
        db_deployments::mark_failed(&state.db_pool, deployment.id, &error_msg).await?;
        db_projects::update_calculated_status(&state.db_pool, deployment.project_id).await?;
    }
    Ok(())
}

/// Check if a deploying deployment has timed out
async fn check_deploying_timeout(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
) -> anyhow::Result<()> {
    if let Some(deploying_started_at) = deployment.deploying_started_at {
        let elapsed = Utc::now().signed_duration_since(deploying_started_at);
        if elapsed > chrono::Duration::minutes(DEPLOYING_TIMEOUT_MINUTES) {
            let error_msg = format!(
                "Deployment timed out after {} seconds in Deploying state",
                elapsed.num_seconds()
            );
            warn!(
                deployment_id = %deployment.deployment_id,
                "{}", error_msg
            );
            db_deployments::mark_failed(&state.db_pool, deployment.id, &error_msg).await?;
            db_projects::update_calculated_status(&state.db_pool, project.id).await?;
        }
    }
    Ok(())
}

/// Complete termination: move from Terminating to the appropriate terminal state.
async fn complete_termination(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
) -> anyhow::Result<()> {
    match deployment.termination_reason {
        Some(TerminationReason::Superseded) => {
            db_deployments::mark_superseded(&state.db_pool, deployment.id).await?;
        }
        Some(TerminationReason::UserStopped) => {
            db_deployments::mark_stopped(&state.db_pool, deployment.id).await?;
        }
        Some(TerminationReason::Expired) => {
            db_deployments::mark_expired(&state.db_pool, deployment.id).await?;
        }
        Some(TerminationReason::Failed) | Some(TerminationReason::Cancelled) | None => {
            // Failed/Cancelled termination reasons and missing reasons all resolve to Stopped
            db_deployments::mark_stopped(&state.db_pool, deployment.id).await?;
        }
    }
    db_projects::update_calculated_status(&state.db_pool, project.id).await?;
    Ok(())
}

/// Check deployment health from observed K8s Deployment status.
/// Handles transitions: Deploying → Healthy/Failed, Healthy → Unhealthy, Unhealthy → Healthy.
async fn check_deployment_health_from_observed(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
    observed: &ObservedChildren,
) -> anyhow::Result<()> {
    // Metacontroller keys namespaced children of cluster-scoped parents as "namespace/name"
    let resource_builder = match &state.resource_builder {
        Some(rb) => rb,
        None => return Ok(()),
    };
    let namespace = resource_builder.namespace_name(project);
    let k8s_deploy_name = format!(
        "{}/{}-{}",
        namespace, project.name, deployment.deployment_id
    );

    let observed_deploy = match observed.deployments.get(&k8s_deploy_name) {
        Some(d) => d,
        None => {
            // K8s Deployment doesn't exist yet — nothing to check
            debug!(
                deployment_id = %deployment.deployment_id,
                "No observed K8s Deployment yet for {}", k8s_deploy_name
            );
            return Ok(());
        }
    };

    // Parse the observed deployment to check readiness
    let observed_k8s: K8sDeployment = serde_json::from_value(observed_deploy.clone())?;

    let desired_replicas = observed_k8s
        .spec
        .as_ref()
        .and_then(|s| s.replicas)
        .unwrap_or(1);
    let ready_replicas = observed_k8s
        .status
        .as_ref()
        .and_then(|s| s.ready_replicas)
        .unwrap_or(0);

    // Check for pod-level errors and collect full pod status via kube-rs
    // (Metacontroller only gives us the Deployment, not individual pods)
    let pod_check =
        check_pod_errors_via_kube(state, project, deployment, desired_replicas, ready_replicas)
            .await;

    // Update controller_metadata with pod status
    if let Some(ref pod_status) = pod_check.pod_status {
        let is_healthy = deployment.status == DeploymentStatus::Healthy
            || (deployment.status == DeploymentStatus::Deploying
                && !pod_check.has_error
                && ready_replicas >= desired_replicas
                && desired_replicas > 0);

        let metadata = serde_json::json!({
            "pod_status": pod_status,
            "health": {
                "last_check": Utc::now().to_rfc3339(),
                "healthy": is_healthy,
            },
        });
        if let Err(e) =
            db_deployments::update_controller_metadata(&state.db_pool, deployment.id, &metadata)
                .await
        {
            warn!(
                deployment_id = %deployment.deployment_id,
                "Failed to update controller metadata: {:?}", e
            );
        }
    }

    let is_ready = ready_replicas >= desired_replicas && desired_replicas > 0;

    match deployment.status {
        DeploymentStatus::Deploying => {
            if pod_check.has_error {
                let error_msg = pod_check
                    .error_message
                    .unwrap_or_else(|| "Pod error".to_string());
                warn!(
                    deployment_id = %deployment.deployment_id,
                    "Deployment has irrecoverable pod error: {}", error_msg
                );
                // If previously healthy, preserve as Unhealthy
                if deployment.first_healthy_at.is_some()
                    && state_machine::is_valid_transition(
                        &deployment.status,
                        &DeploymentStatus::Unhealthy,
                    )
                {
                    db_deployments::mark_unhealthy(&state.db_pool, deployment.id, error_msg)
                        .await?;
                } else {
                    db_deployments::mark_failed(&state.db_pool, deployment.id, &error_msg).await?;
                }
                db_projects::update_calculated_status(&state.db_pool, project.id).await?;
            } else if is_ready {
                info!(
                    deployment_id = %deployment.deployment_id,
                    "Deployment is ready ({}/{} replicas), marking as Healthy",
                    ready_replicas,
                    desired_replicas
                );
                handle_deployment_became_healthy(state, deployment, project).await?;
            }
        }

        DeploymentStatus::Healthy if pod_check.has_error || !is_ready => {
            let msg = pod_check.error_message.unwrap_or_else(|| {
                format!(
                    "Deployment unhealthy: {}/{} replicas ready",
                    ready_replicas, desired_replicas
                )
            });
            warn!(
                deployment_id = %deployment.deployment_id,
                "Healthy deployment is now unhealthy: {}", msg
            );
            db_deployments::mark_unhealthy(&state.db_pool, deployment.id, msg).await?;
            db_projects::update_calculated_status(&state.db_pool, project.id).await?;
        }

        DeploymentStatus::Unhealthy if !pod_check.has_error && is_ready => {
            info!(
                deployment_id = %deployment.deployment_id,
                "Unhealthy deployment has recovered, marking as Healthy"
            );
            db_deployments::mark_healthy(&state.db_pool, deployment.id).await?;
            db_projects::update_calculated_status(&state.db_pool, project.id).await?;
        }

        _ => {}
    }

    Ok(())
}

/// Result of checking pod status via kube-rs API.
struct PodCheckResult {
    /// Whether any pod has an irrecoverable error
    has_error: bool,
    /// Error message if has_error is true
    error_message: Option<String>,
    /// Full pod status for storing in controller_metadata
    pod_status: Option<serde_json::Value>,
}

/// Check pods for errors via direct kube-rs API call and collect full pod status.
async fn check_pod_errors_via_kube(
    state: &AppState,
    project: &Project,
    deployment: &Deployment,
    desired_replicas: i32,
    ready_replicas: i32,
) -> PodCheckResult {
    let kube_client = match &state.kube_client {
        Some(client) => client,
        None => {
            return PodCheckResult {
                has_error: false,
                error_message: None,
                pod_status: None,
            }
        }
    };

    let namespace = match &state.resource_builder {
        Some(rb) => rb.namespace_name(project),
        None => {
            return PodCheckResult {
                has_error: false,
                error_message: None,
                pod_status: None,
            }
        }
    };
    let pod_api: kube::Api<k8s_openapi::api::core::v1::Pod> =
        kube::Api::namespaced(kube_client.clone(), &namespace);

    let pods = match pod_api
        .list(&kube::api::ListParams::default().labels(&format!(
            "{}={}",
            LABEL_DEPLOYMENT_ID, deployment.deployment_id
        )))
        .await
    {
        Ok(pods) => pods,
        Err(e) => {
            debug!(
                deployment_id = %deployment.deployment_id,
                "Failed to list pods for health check: {:?}", e
            );
            return PodCheckResult {
                has_error: false,
                error_message: None,
                pod_status: None,
            };
        }
    };

    let mut has_error = false;
    let mut error_message: Option<String> = None;
    let mut pod_infos: Vec<serde_json::Value> = Vec::new();
    let mut current_replicas: i32 = 0;

    for pod in &pods.items {
        current_replicas += 1;
        let pod_name = pod
            .metadata
            .name
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let pod_phase = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_deref())
            .unwrap_or("Unknown")
            .to_string();

        // Collect pod conditions
        let conditions: Vec<serde_json::Value> = pod
            .status
            .as_ref()
            .and_then(|s| s.conditions.as_ref())
            .map(|conds| {
                conds
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "type": c.type_,
                            "status": c.status,
                            "reason": c.reason,
                            "message": c.message,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Collect container statuses
        let mut container_infos: Vec<serde_json::Value> = Vec::new();
        if let Some(container_statuses) = pod
            .status
            .as_ref()
            .and_then(|s| s.container_statuses.as_ref())
        {
            for cs in container_statuses {
                let state_info = if let Some(state) = &cs.state {
                    if let Some(waiting) = &state.waiting {
                        let reason = waiting.reason.as_deref().unwrap_or("");
                        // Check for irrecoverable errors
                        if !has_error && IRRECOVERABLE_CONTAINER_REASONS.contains(&reason) {
                            has_error = true;
                            let message = waiting.message.as_deref().unwrap_or(reason);
                            error_message = Some(format!("{}: {}", reason, message));
                        }
                        Some(serde_json::json!({
                            "state_type": "waiting",
                            "reason": waiting.reason,
                            "message": waiting.message,
                        }))
                    } else if let Some(running) = &state.running {
                        Some(serde_json::json!({
                            "state_type": "running",
                            "reason": running.started_at.as_ref().map(|t| t.0.to_string()),
                        }))
                    } else if let Some(terminated) = &state.terminated {
                        // Check terminated with too many restarts
                        if !has_error && terminated.exit_code != 0 && cs.restart_count >= 3 {
                            has_error = true;
                            let reason = terminated.reason.as_deref().unwrap_or("ContainerFailed");
                            let default_msg = format!("Exit code: {}", terminated.exit_code);
                            let message = terminated.message.as_deref().unwrap_or(&default_msg);
                            error_message = Some(format!(
                                "{}: {} (restarts: {})",
                                reason, message, cs.restart_count
                            ));
                        }
                        Some(serde_json::json!({
                            "state_type": "terminated",
                            "reason": terminated.reason,
                            "message": terminated.message,
                            "exit_code": terminated.exit_code,
                        }))
                    } else {
                        None
                    }
                } else {
                    None
                };

                container_infos.push(serde_json::json!({
                    "name": cs.name,
                    "ready": cs.ready,
                    "restart_count": cs.restart_count,
                    "state": state_info,
                }));
            }
        }

        pod_infos.push(serde_json::json!({
            "name": pod_name,
            "phase": pod_phase,
            "conditions": conditions,
            "containers": container_infos,
        }));
    }

    let pod_status = serde_json::json!({
        "desired_replicas": desired_replicas,
        "ready_replicas": ready_replicas,
        "current_replicas": current_replicas,
        "pods": pod_infos,
        "last_checked": Utc::now().to_rfc3339(),
    });

    PodCheckResult {
        has_error,
        error_message,
        pod_status: Some(pod_status),
    }
}

/// Handle a deployment becoming Healthy: mark active, supersede old deployments.
async fn handle_deployment_became_healthy(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
) -> anyhow::Result<()> {
    // Find currently active deployment in this group BEFORE marking new as Healthy
    let active_in_group = db_deployments::find_active_for_project_and_group(
        &state.db_pool,
        project.id,
        &deployment.deployment_group,
    )
    .await?;

    // Mark the new deployment as healthy
    db_deployments::mark_healthy(&state.db_pool, deployment.id).await?;

    // Supersede the old active deployment
    if let Some(old_active) = active_in_group {
        if old_active.id != deployment.id && !state_machine::is_terminal(&old_active.status) {
            info!(
                "Deployment {} replacing {} in group '{}', marking old as Terminating",
                deployment.deployment_id, old_active.deployment_id, deployment.deployment_group
            );
            db_deployments::mark_terminating(
                &state.db_pool,
                old_active.id,
                TerminationReason::Superseded,
            )
            .await?;
        }
    }

    // Clean up other active (Healthy/Unhealthy) deployments in the group
    let others = db_deployments::find_non_terminal_for_project_and_group(
        &state.db_pool,
        project.id,
        &deployment.deployment_group,
    )
    .await?;

    for other in others {
        if other.id != deployment.id
            && state_machine::is_active(&other.status)
            && !state_machine::is_terminal(&other.status)
        {
            info!(
                "Cleaning up non-active deployment {} in group '{}', marking as Terminating",
                other.deployment_id, deployment.deployment_group
            );
            db_deployments::mark_terminating(
                &state.db_pool,
                other.id,
                TerminationReason::Superseded,
            )
            .await?;
        }
    }

    // Mark deployment as active
    db_deployments::mark_as_active(
        &state.db_pool,
        deployment.id,
        project.id,
        &deployment.deployment_group,
    )
    .await?;

    // Clear needs_reconcile if set
    if deployment.needs_reconcile {
        db_deployments::clear_needs_reconcile(&state.db_pool, deployment.id).await?;
    }

    db_projects::update_calculated_status(&state.db_pool, project.id).await?;

    Ok(())
}

/// Check for expired deployments
async fn check_expirations(
    state: &AppState,
    non_terminal: &[&Deployment],
    project: &Project,
) -> anyhow::Result<()> {
    let now = Utc::now();
    for deployment in non_terminal {
        if let Some(expires_at) = deployment.expires_at {
            if now > expires_at
                && !matches!(
                    deployment.status,
                    DeploymentStatus::Terminating | DeploymentStatus::Cancelling
                )
            {
                info!(
                    deployment_id = %deployment.deployment_id,
                    "Deployment has expired, marking as Terminating"
                );
                db_deployments::mark_terminating(
                    &state.db_pool,
                    deployment.id,
                    TerminationReason::Expired,
                )
                .await?;
                db_projects::update_calculated_status(&state.db_pool, project.id).await?;
            }
        }
    }
    Ok(())
}

// ── Compute desired children ───────────────────────────────────────────

/// Compute the desired set of Kubernetes child resources for a project.
///
/// Resources NOT returned will be deleted by Metacontroller (garbage collection).
async fn compute_desired_children(
    state: &AppState,
    resource_builder: &ResourceBuilder,
    project: &Project,
    all_deployments: &[Deployment],
    observed: &ObservedChildren,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut children: Vec<serde_json::Value> = Vec::new();
    let namespace = resource_builder.namespace_name(project);

    // 1. Namespace (always)
    let ns = resource_builder.create_namespace(project);
    children.push(serde_json::to_value(&ns)?);

    // 2. Image pull secret (if needed)
    if resource_builder.image_pull_secret_name.is_none()
        && resource_builder.registry_provider.requires_pull_secret()
    {
        if let Some(secret) =
            build_image_pull_secret(resource_builder, project, &namespace, observed).await?
        {
            children.push(serde_json::to_value(&secret)?);
        }
    }

    // 3. Backend service + endpoints (if configured)
    if let Some(ref backend_address) = resource_builder.backend_address {
        add_backend_resources(
            state,
            &mut children,
            resource_builder,
            project,
            &namespace,
            backend_address,
        )
        .await?;
    }

    // Collect deployments that should have K8s infrastructure
    let infra_deployments: Vec<&Deployment> = all_deployments
        .iter()
        .filter(|d| should_have_infrastructure(d))
        .collect();

    // 4. Per-environment ServiceAccounts
    let mut seen_environments: std::collections::HashSet<String> = std::collections::HashSet::new();
    for deployment in &infra_deployments {
        if let Some(env_id) = deployment.environment_id {
            if let Some(environment) = db_environments::find_by_id(&state.db_pool, env_id).await? {
                if seen_environments.insert(environment.name.clone()) {
                    // Check if we should skip SA creation for production
                    if resource_builder.use_default_service_account_for_production
                        && environment.is_production
                    {
                        continue;
                    }
                    let sa = resource_builder.create_service_account(
                        project,
                        &environment.name,
                        &namespace,
                    );
                    children.push(serde_json::to_value(&sa)?);
                }
            }
        }
    }

    // 5. K8s Deployments, Services, Ingresses, NetworkPolicies per deployment/group
    // Track which deployment groups have active deployments (for Service selector)
    let mut active_by_group: HashMap<String, &Deployment> = HashMap::new();
    for deployment in &infra_deployments {
        if deployment.is_active {
            active_by_group.insert(deployment.deployment_group.clone(), deployment);
        }
    }

    // K8s Deployments — one per non-terminal, infrastructure-bearing deployment
    for deployment in &infra_deployments {
        let env_name = resolve_environment_name(state, deployment).await?;
        let env_vars = load_env_vars(state, project, deployment).await?;

        // Resolve image
        let source_deployment_id =
            if let Some(source_id) = deployment.rolled_back_from_deployment_id {
                db_deployments::find_by_id(&state.db_pool, source_id)
                    .await?
                    .map(|d| d.deployment_id)
            } else {
                None
            };
        let image =
            resource_builder.resolve_image(project, deployment, source_deployment_id.as_deref());

        // Resolve service account name
        let sa_name = resolve_service_account_name(state, resource_builder, deployment).await?;

        let k8s_deploy = resource_builder.create_k8s_deployment(
            project,
            deployment,
            &namespace,
            &image,
            deployment.http_port as u16,
            env_vars,
            sa_name,
            env_name.as_deref(),
        );
        children.push(serde_json::to_value(&k8s_deploy)?);
    }

    // Services, Ingresses, NetworkPolicies — one per group with an active deployment
    let custom_domains =
        crate::db::custom_domains::list_project_custom_domains(&state.db_pool, project.id).await?;

    for (group, active_deployment) in &active_by_group {
        let env_name = resolve_environment_name(state, active_deployment).await?;

        // Service (selector points to the active deployment)
        let service = resource_builder.create_service(
            project,
            active_deployment,
            &namespace,
            active_deployment.http_port as u16,
            env_name.as_deref(),
        );
        children.push(serde_json::to_value(&service)?);

        // Primary Ingress
        let ingress = resource_builder.create_primary_ingress(
            project,
            active_deployment,
            &namespace,
            env_name.as_deref(),
        )?;
        children.push(serde_json::to_value(&ingress)?);

        // Custom domain Ingress (only for production primary group)
        let environment = if let Some(env_id) = active_deployment.environment_id {
            db_environments::find_by_id(&state.db_pool, env_id).await?
        } else {
            None
        };

        let is_production_primary = environment
            .as_ref()
            .map(|env| {
                env.is_production && env.primary_deployment_group.as_deref() == Some(group.as_str())
            })
            .unwrap_or(*group == crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP);

        if is_production_primary {
            let valid_domains =
                resource_builder.filter_valid_custom_domains(custom_domains.clone());
            if !valid_domains.is_empty() {
                let custom_ingress = resource_builder.create_custom_domain_ingress(
                    project,
                    active_deployment,
                    &namespace,
                    &valid_domains,
                    env_name.as_deref(),
                )?;
                children.push(serde_json::to_value(&custom_ingress)?);
            }
        }

        // NetworkPolicy
        let np = resource_builder.create_network_policy(
            project,
            active_deployment,
            &namespace,
            env_name.as_deref(),
        );
        children.push(serde_json::to_value(&np)?);
    }

    Ok(children)
}

/// Returns true if this deployment should have K8s infrastructure (K8s Deployment resource).
fn should_have_infrastructure(deployment: &Deployment) -> bool {
    matches!(
        deployment.status,
        DeploymentStatus::Pushed
            | DeploymentStatus::Deploying
            | DeploymentStatus::Healthy
            | DeploymentStatus::Unhealthy
    )
}

/// Build image pull secret, refreshing if stale.
async fn build_image_pull_secret(
    resource_builder: &ResourceBuilder,
    project: &Project,
    namespace: &str,
    observed: &ObservedChildren,
) -> anyhow::Result<Option<Secret>> {
    // Metacontroller keys namespaced children of cluster-scoped parents as "namespace/name"
    let secret_key = format!("{}/{}", namespace, IMAGE_PULL_SECRET_NAME);

    // Check if existing secret is fresh enough
    let needs_refresh = match observed.secrets.get(&secret_key) {
        Some(secret_json) => {
            let last_refresh = secret_json
                .get("metadata")
                .and_then(|m| m.get("annotations"))
                .and_then(|a| a.get(ANNOTATION_LAST_REFRESH))
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());

            match last_refresh {
                Some(ts) => {
                    let age = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
                    age > chrono::Duration::hours(SECRET_REFRESH_HOURS)
                }
                None => true, // No annotation → refresh
            }
        }
        None => true, // Not observed → create
    };

    if !needs_refresh {
        // Return a clean secret with only the fields we control, using the existing
        // data and last-refresh annotation from the observed secret. Echoing back
        // the full observed JSON would include server-set fields (managedFields,
        // resourceVersion, etc.) that change on every update, creating a perpetual
        // diff loop with Metacontroller's last-applied-configuration.
        if let Some(secret_json) = observed.secrets.get(&secret_key) {
            let existing: Secret = serde_json::from_value(secret_json.clone())?;
            let last_refresh = existing
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(ANNOTATION_LAST_REFRESH))
                .cloned()
                .unwrap_or_default();
            let mut annotations = std::collections::BTreeMap::new();
            annotations.insert(ANNOTATION_LAST_REFRESH.to_string(), last_refresh);
            return Ok(Some(Secret {
                metadata: ObjectMeta {
                    name: Some(IMAGE_PULL_SECRET_NAME.to_string()),
                    namespace: Some(namespace.to_string()),
                    annotations: Some(annotations),
                    ..Default::default()
                },
                type_: existing.type_,
                data: existing.data,
                ..Default::default()
            }));
        }
    }

    // Fetch fresh credentials
    let credentials = resource_builder
        .registry_provider
        .get_credentials(&project.name)
        .await?;
    let registry_host = resource_builder.registry_provider.registry_host();

    let secret = resource_builder.create_dockerconfigjson_secret(
        IMAGE_PULL_SECRET_NAME,
        namespace,
        registry_host,
        &credentials,
    )?;

    Ok(Some(secret))
}

/// Add backend service + endpoints resources.
///
/// The Service is returned as a Metacontroller child. For IP-based backends,
/// the Endpoints are applied directly via kube-rs because Endpoints cannot be
/// a Metacontroller child resource type — Kubernetes auto-creates Endpoints
/// for Services with selectors (e.g. the deployment `default` Service), and
/// Metacontroller would thrash deleting/adopting those in an infinite loop.
/// Since child resource types are all-or-nothing, we manage the `rise-backend`
/// Endpoints outside of Metacontroller as well.
async fn add_backend_resources(
    state: &AppState,
    children: &mut Vec<serde_json::Value>,
    resource_builder: &ResourceBuilder,
    project: &Project,
    namespace: &str,
    backend_address: &crate::server::settings::BackendAddress,
) -> anyhow::Result<()> {
    let is_ip = backend_address.host.parse::<std::net::IpAddr>().is_ok();

    if is_ip {
        // IP address → ClusterIP Service (as child) + Endpoints (applied directly)
        let svc = resource_builder.create_backend_service_clusterip(
            project,
            namespace,
            backend_address.port,
        );
        children.push(serde_json::to_value(&svc)?);

        let endpoints = resource_builder.create_backend_endpoints(
            project,
            namespace,
            &backend_address.host,
            backend_address.port,
        );
        apply_backend_endpoints(state, &endpoints, namespace).await;
    } else {
        // DNS name → ExternalName
        let svc = resource_builder.create_backend_service_externalname(
            project,
            namespace,
            &backend_address.host,
        );
        children.push(serde_json::to_value(&svc)?);
    }

    Ok(())
}

/// Apply backend Endpoints directly via kube-rs server-side apply.
async fn apply_backend_endpoints(
    state: &AppState,
    endpoints: &k8s_openapi::api::core::v1::Endpoints,
    namespace: &str,
) {
    let Some(ref kube_client) = state.kube_client else {
        return;
    };
    let api: kube::Api<k8s_openapi::api::core::v1::Endpoints> =
        kube::Api::namespaced(kube_client.clone(), namespace);
    let name = endpoints.metadata.name.as_deref().unwrap_or("rise-backend");
    let params = kube::api::PatchParams::apply("rise-controller").force();
    if let Err(e) = api
        .patch(name, &params, &kube::api::Patch::Apply(endpoints))
        .await
    {
        warn!(
            namespace = %namespace,
            "Failed to apply backend Endpoints: {:?}", e
        );
    }
}

/// Resolve the environment name for a deployment
async fn resolve_environment_name(
    state: &AppState,
    deployment: &Deployment,
) -> anyhow::Result<Option<String>> {
    match deployment.environment_id {
        Some(env_id) => match db_environments::find_by_id(&state.db_pool, env_id).await? {
            Some(env) => Ok(Some(env.name)),
            None => {
                warn!(
                    deployment_id = %deployment.deployment_id,
                    environment_id = %env_id,
                    "Environment not found for label resolution"
                );
                Ok(None)
            }
        },
        None => Ok(None),
    }
}

/// Resolve service account name for a deployment
async fn resolve_service_account_name(
    state: &AppState,
    resource_builder: &ResourceBuilder,
    deployment: &Deployment,
) -> anyhow::Result<Option<String>> {
    let env_id = match deployment.environment_id {
        Some(id) => id,
        None => return Ok(None),
    };

    let environment = match db_environments::find_by_id(&state.db_pool, env_id).await? {
        Some(env) => env,
        None => return Ok(None),
    };

    if resource_builder.use_default_service_account_for_production && environment.is_production {
        return Ok(None);
    }

    Ok(Some(ResourceBuilder::environment_service_account_name(
        &environment.name,
    )))
}

/// Load and decrypt environment variables for a deployment
async fn load_env_vars(
    state: &AppState,
    _project: &Project,
    deployment: &Deployment,
) -> anyhow::Result<Vec<k8s_openapi::api::core::v1::EnvVar>> {
    let env_vars = crate::db::env_vars::load_deployment_env_vars_decrypted(
        &state.db_pool,
        deployment.id,
        state.encryption_provider.as_deref(),
    )
    .await?;

    Ok(env_vars
        .into_iter()
        .map(|(key, value)| k8s_openapi::api::core::v1::EnvVar {
            name: key,
            value: Some(value),
            ..Default::default()
        })
        .collect())
}

// ── Finalize webhook handler ───────────────────────────────────────────

pub async fn handle_finalize(
    State(state): State<AppState>,
    Query(query): Query<WebhookQuery>,
    Json(request): Json<FinalizeRequest>,
) -> Response {
    if let Err((status, msg)) = validate_webhook_token(&state, &query.token) {
        return (status, msg).into_response();
    }

    let project_name = match request
        .parent
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
    {
        Some(name) => name.to_string(),
        None => {
            error!("Finalize request missing parent.metadata.name");
            return (
                StatusCode::BAD_REQUEST,
                Json(FinalizeResponse {
                    status: serde_json::json!({"error": "missing parent name"}),
                    children: vec![],
                    finalized: true,
                }),
            )
                .into_response();
        }
    };

    match process_finalize(&state, &project_name).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!(project = %project_name, "Finalize webhook error: {:?}", e);
            // On error, still finalize to avoid blocking deletion
            (
                StatusCode::OK,
                Json(FinalizeResponse {
                    status: serde_json::json!({
                        "error": format!("{:#}", e),
                    }),
                    children: vec![],
                    finalized: true,
                }),
            )
                .into_response()
        }
    }
}

async fn process_finalize(
    state: &AppState,
    project_name: &str,
) -> anyhow::Result<FinalizeResponse> {
    info!(project = %project_name, "Processing finalize webhook — marking all deployments as stopped");

    if let Some(project) = db_projects::find_by_name(&state.db_pool, project_name).await? {
        // Mark all non-terminal deployments as Stopped
        let deployments = db_deployments::list_for_project(&state.db_pool, project.id).await?;
        for deployment in deployments {
            if !state_machine::is_terminal(&deployment.status) {
                info!(
                    deployment_id = %deployment.deployment_id,
                    "Finalize: marking deployment as Stopped"
                );
                // Try the most appropriate terminal transition
                if state_machine::is_valid_transition(
                    &deployment.status,
                    &DeploymentStatus::Cancelling,
                ) {
                    db_deployments::mark_cancelling(&state.db_pool, deployment.id).await?;
                    db_deployments::mark_cancelled(&state.db_pool, deployment.id).await?;
                } else {
                    db_deployments::mark_terminating(
                        &state.db_pool,
                        deployment.id,
                        TerminationReason::UserStopped,
                    )
                    .await?;
                    db_deployments::mark_stopped(&state.db_pool, deployment.id).await?;
                }
            }
        }
        db_projects::update_calculated_status(&state.db_pool, project.id).await?;
    }

    Ok(FinalizeResponse {
        status: serde_json::json!({}),
        children: vec![],
        finalized: true,
    })
}
