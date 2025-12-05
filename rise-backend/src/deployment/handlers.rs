use anyhow::Context;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use regex::Regex;
use tracing::{debug, error, info};

use super::models::*;
use super::utils::generate_deployment_id;
use crate::db::models::{DeploymentStatus as DbDeploymentStatus, User};
use crate::db::{deployments as db_deployments, projects, teams as db_teams};
use crate::state::AppState;
use uuid::Uuid;

/// Check if user has permission to deploy to the project
async fn check_deploy_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user_id: Uuid,
) -> Result<(), String> {
    // If project is owned by the user directly, allow
    if let Some(owner_user_id) = project.owner_user_id {
        if owner_user_id == user_id {
            return Ok(());
        }
    }

    // If project is owned by a team, check if user is a member of that team
    if let Some(team_id) = project.owner_team_id {
        let is_member = db_teams::is_member(&state.db_pool, team_id, user_id)
            .await
            .map_err(|e| format!("Failed to check team membership: {}", e))?;

        if is_member {
            return Ok(());
        }

        let team = db_teams::find_by_id(&state.db_pool, team_id)
            .await
            .map_err(|e| format!("Failed to fetch team: {}", e))?
            .ok_or_else(|| "Team not found".to_string())?;

        return Err(format!(
            "You must be a member of team '{}' to deploy to this project",
            team.name
        ));
    }

    Err("You do not have permission to deploy to this project".to_string())
}

/// Validate group name format: must be 'default' or match [a-z0-9][a-z0-9/-]*[a-z0-9]
fn is_valid_group_name(name: &str) -> bool {
    if name == "default" {
        return true;
    }

    if name.len() > 100 {
        return false;
    }

    Regex::new(r"^[a-z0-9][a-z0-9/-]*[a-z0-9]$")
        .unwrap()
        .is_match(name)
}

/// Parse expiration duration string (e.g., "7d", "2h", "30m") to DateTime
fn parse_expiration(expires_in: &str) -> Result<DateTime<Utc>, String> {
    let s = expires_in.trim();
    let (num_str, unit) = if s.ends_with('d') {
        (&s[..s.len() - 1], "d")
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], "h")
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], "m")
    } else {
        return Err("Duration must end with d, h, or m".to_string());
    };

    let num: i64 = num_str
        .parse()
        .map_err(|_| "Invalid number in duration".to_string())?;

    if num <= 0 {
        return Err("Duration must be positive".to_string());
    }

    let duration = match unit {
        "d" => chrono::Duration::days(num),
        "h" => chrono::Duration::hours(num),
        "m" => chrono::Duration::minutes(num),
        _ => return Err("Invalid duration unit".to_string()),
    };

    Ok(Utc::now() + duration)
}

/// Normalize image reference by adding registry hostname and namespace if missing
///
/// # Examples
/// - `nginx` → `docker.io/library/nginx`
/// - `nginx:latest` → `docker.io/library/nginx:latest`
/// - `myorg/app:v1` → `docker.io/myorg/app:v1`
/// - `quay.io/nginx:latest` → `quay.io/nginx:latest` (unchanged)
fn normalize_image_reference(image: &str) -> String {
    // Check if image already has a registry hostname (contains '.' or ':' before first '/')
    let has_registry = image
        .split('/')
        .next()
        .map(|first_part| first_part.contains('.') || first_part.contains(':'))
        .unwrap_or(false);

    if has_registry {
        // Already has registry, return as-is
        return image.to_string();
    }

    // No registry specified, default to docker.io
    // Check if image has a namespace (contains '/')
    if image.contains('/') {
        // Has namespace: myorg/app:v1 → docker.io/myorg/app:v1
        format!("docker.io/{}", image)
    } else {
        // No namespace: nginx → docker.io/library/nginx
        format!("docker.io/library/{}", image)
    }
}

/// Resolve image tag to digest by contacting OCI registry directly
///
/// This function uses the OCI Distribution API to fetch the image manifest
/// (without pulling the entire image) and returns the digest-pinned reference.
///
/// # Arguments
/// * `oci_client` - OCI client for registry interaction
/// * `normalized_image` - Normalized image reference (e.g., "docker.io/library/nginx:latest")
///
/// # Returns
/// Fully-qualified digest reference (e.g., "docker.io/library/nginx@sha256:abc123...")
///
/// # Errors
/// Returns error if image doesn't exist, requires authentication, or registry is unreachable
async fn resolve_image_digest(
    oci_client: &crate::oci::OciClient,
    normalized_image: &str,
) -> anyhow::Result<String> {
    let digest_ref = oci_client
        .resolve_image_digest(normalized_image)
        .await
        .context(format!("Failed to resolve image '{}'", normalized_image))?;

    info!("Resolved '{}' to digest '{}'", normalized_image, digest_ref);
    Ok(digest_ref)
}

/// Convert API DeploymentStatus to DB DeploymentStatus
fn convert_status_to_db(status: DeploymentStatus) -> DbDeploymentStatus {
    match status {
        DeploymentStatus::Pending => DbDeploymentStatus::Pending,
        DeploymentStatus::Building => DbDeploymentStatus::Building,
        DeploymentStatus::Pushing => DbDeploymentStatus::Pushing,
        DeploymentStatus::Pushed => DbDeploymentStatus::Pushed,
        DeploymentStatus::Deploying => DbDeploymentStatus::Deploying,
        DeploymentStatus::Healthy => DbDeploymentStatus::Healthy,
        DeploymentStatus::Unhealthy => DbDeploymentStatus::Unhealthy,
        DeploymentStatus::Cancelling => DbDeploymentStatus::Cancelling,
        DeploymentStatus::Cancelled => DbDeploymentStatus::Cancelled,
        DeploymentStatus::Terminating => DbDeploymentStatus::Terminating,
        DeploymentStatus::Stopped => DbDeploymentStatus::Stopped,
        DeploymentStatus::Superseded => DbDeploymentStatus::Superseded,
        DeploymentStatus::Failed => DbDeploymentStatus::Failed,
        DeploymentStatus::Expired => DbDeploymentStatus::Expired,
    }
}

/// Convert DB DeploymentStatus to API DeploymentStatus
fn convert_status_from_db(status: DbDeploymentStatus) -> DeploymentStatus {
    match status {
        DbDeploymentStatus::Pending => DeploymentStatus::Pending,
        DbDeploymentStatus::Building => DeploymentStatus::Building,
        DbDeploymentStatus::Pushing => DeploymentStatus::Pushing,
        DbDeploymentStatus::Pushed => DeploymentStatus::Pushed,
        DbDeploymentStatus::Deploying => DeploymentStatus::Deploying,
        DbDeploymentStatus::Healthy => DeploymentStatus::Healthy,
        DbDeploymentStatus::Unhealthy => DeploymentStatus::Unhealthy,
        DbDeploymentStatus::Cancelling => DeploymentStatus::Cancelling,
        DbDeploymentStatus::Cancelled => DeploymentStatus::Cancelled,
        DbDeploymentStatus::Terminating => DeploymentStatus::Terminating,
        DbDeploymentStatus::Stopped => DeploymentStatus::Stopped,
        DbDeploymentStatus::Superseded => DeploymentStatus::Superseded,
        DbDeploymentStatus::Failed => DeploymentStatus::Failed,
        DbDeploymentStatus::Expired => DeploymentStatus::Expired,
    }
}

/// Convert DB Deployment to API Deployment
fn convert_deployment(deployment: crate::db::models::Deployment) -> Deployment {
    Deployment {
        id: deployment.id.to_string(),
        deployment_id: deployment.deployment_id,
        project: deployment.project_id.to_string(),
        created_by: deployment.created_by_id.to_string(),
        status: convert_status_from_db(deployment.status),
        deployment_group: deployment.deployment_group,
        expires_at: deployment.expires_at.map(|dt| dt.to_rfc3339()),
        error_message: deployment.error_message,
        completed_at: deployment.completed_at.map(|dt| dt.to_rfc3339()),
        build_logs: deployment.build_logs,
        controller_metadata: deployment.controller_metadata,
        deployment_url: deployment.deployment_url,
        image: deployment.image,
        image_digest: deployment.image_digest,
        created: deployment.created_at.to_rfc3339(),
        updated: deployment.updated_at.to_rfc3339(),
    }
}

/// POST /deployments - Create a new deployment
pub async fn create_deployment(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateDeploymentRequest>,
) -> Result<Json<CreateDeploymentResponse>, (StatusCode, String)> {
    info!("Creating deployment for project '{}'", payload.project);

    // Validate deployment group name
    if !is_valid_group_name(&payload.group) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9] (max 100 chars)",
                payload.group
            ),
        ));
    }

    // Parse expiration duration if provided
    let expires_at = if let Some(ref expires_in) = payload.expires_in {
        Some(parse_expiration(expires_in).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid expiration duration '{}': {}", expires_in, e),
            )
        })?)
    } else {
        None
    };

    // Query project by name
    let project = projects::find_by_name(&state.db_pool, &payload.project)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", payload.project),
            )
        })?;

    // Prevent deployments on projects in deletion lifecycle
    // Projects in Deleting or Terminated status should not accept new deployments
    if matches!(
        project.status,
        crate::db::models::ProjectStatus::Deleting | crate::db::models::ProjectStatus::Terminated
    ) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot create deployment for project in {:?} state",
                project.status
            ),
        ));
    }

    // Check deployment permissions
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    debug!("Generated deployment ID: {}", deployment_id);

    // Branch based on whether user provided a pre-built image
    if let Some(ref user_image) = payload.image {
        // Path 1: Pre-built image deployment
        info!("Creating deployment with pre-built image: {}", user_image);

        // Normalize image reference (add registry and namespace if missing)
        let normalized_image = normalize_image_reference(user_image);
        debug!("Normalized image: {} -> {}", user_image, normalized_image);

        // Resolve image to digest
        let image_digest = resolve_image_digest(&state.oci_client, &normalized_image)
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to resolve image '{}': {}", user_image, e),
                )
            })?;

        info!("Resolved image digest: {}", image_digest);

        // Create deployment record with image fields set
        let _deployment = db_deployments::create(
            &state.db_pool,
            &deployment_id,
            project.id,
            user.id,
            DbDeploymentStatus::Pushed, // Pre-built images skip build/push, go straight to Pushed
            Some(user_image),           // Store original user input
            Some(&image_digest),        // Store resolved digest
            &payload.group,             // deployment_group
            expires_at,                 // expires_at
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create deployment: {}", e),
            )
        })?;

        info!(
            "Created pre-built image deployment {} for project {}",
            deployment_id, payload.project
        );

        // Return response with digest as image_tag and empty credentials
        Ok(Json(CreateDeploymentResponse {
            deployment_id,
            image_tag: image_digest, // Return digest for consistency
            credentials: crate::registry::models::RegistryCredentials {
                registry_url: String::new(),
                username: String::new(),
                password: String::new(),
                expires_in: None,
            },
        }))
    } else {
        // Path 2: Build from source (current behavior)
        // Get registry credentials
        let registry_provider = state.registry_provider.as_ref().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "No registry configured".to_string(),
        ))?;

        let credentials = registry_provider
            .get_credentials(&payload.project)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get credentials: {}", e),
                )
            })?;

        // Construct image tag
        // Note: For Docker registry, credentials.registry_url already includes namespace
        let image_tag = format!(
            "{}:{}",
            format!(
                "{}/{}",
                credentials.registry_url.trim_end_matches('/'),
                payload.project
            ),
            deployment_id
        );

        debug!("Image tag: {}", image_tag);

        // Create deployment record in database (image fields are NULL)
        let _deployment = db_deployments::create(
            &state.db_pool,
            &deployment_id,
            project.id,
            user.id,
            DbDeploymentStatus::Pending,
            None,           // image - NULL for build-from-source
            None,           // image_digest - NULL for build-from-source
            &payload.group, // deployment_group
            expires_at,     // expires_at
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create deployment: {}", e),
            )
        })?;

        info!(
            "Created build-from-source deployment {} for project {}",
            deployment_id, payload.project
        );

        // Return response
        Ok(Json(CreateDeploymentResponse {
            deployment_id,
            image_tag,
            credentials,
        }))
    }
}

/// PATCH /deployments/{deployment_id}/status - Update deployment status
pub async fn update_deployment_status(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(deployment_id): Path<String>,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!(
        "Updating deployment {} status to {:?}",
        deployment_id, payload.status
    );

    // Find all deployments with this deployment_id (there should only be one)
    // We need to find the project first to check the deployment_id
    // For now, let's query by deployment_id across all projects
    // We'll need to add a function to find by deployment_id only

    // Query all projects to find the one with this deployment
    let all_projects = projects::list(&state.db_pool, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list projects: {}", e),
        )
    })?;

    let mut found_deployment: Option<crate::db::models::Deployment> = None;
    let mut found_project: Option<crate::db::models::Project> = None;

    for project in all_projects {
        if let Ok(Some(deployment)) =
            db_deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id).await
        {
            found_deployment = Some(deployment);
            found_project = Some(project);
            break;
        }
    }

    let deployment = found_deployment.ok_or((
        StatusCode::NOT_FOUND,
        format!("Deployment '{}' not found", deployment_id),
    ))?;

    let project = found_project.ok_or((
        StatusCode::NOT_FOUND,
        format!("Project for deployment '{}' not found", deployment_id),
    ))?;

    // Check if user has permission (owns the project)
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Update status in database
    let status_copy = payload.status.clone();
    let updated_deployment = match payload.status {
        DeploymentStatus::Failed => {
            let error_msg = payload.error_message.as_deref().unwrap_or("Unknown error");
            let deployment = db_deployments::mark_failed(&state.db_pool, deployment.id, error_msg)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update deployment: {}", e),
                    )
                })?;

            // Update project status to Failed
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update project status: {}", e),
                    )
                })?;

            deployment
        }
        _ => {
            let db_status = convert_status_to_db(payload.status);
            let deployment =
                db_deployments::update_status(&state.db_pool, deployment.id, db_status)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to update deployment: {}", e),
                        )
                    })?;

            // Update project status (e.g., to Deploying)
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update project status: {}", e),
                    )
                })?;

            deployment
        }
    };

    info!(
        "Updated deployment {} to status {:?}",
        deployment_id, status_copy
    );

    Ok(Json(convert_deployment(updated_deployment)))
}

/// Query parameters for listing deployments
#[derive(Debug, serde::Deserialize)]
pub struct ListDeploymentsQuery {
    #[serde(rename = "group")]
    pub deployment_group: Option<String>,
}

/// List deployments for a project
pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
    Query(query): Query<ListDeploymentsQuery>,
) -> Result<Json<Vec<Deployment>>, (StatusCode, String)> {
    debug!(
        "Listing deployments for project: {} (group: {:?})",
        project_name, query.deployment_group
    );

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to view deployments (owns the project or is team member)
    let has_permission = if let Some(owner_user_id) = project.owner_user_id {
        owner_user_id == user.id
    } else if let Some(team_id) = project.owner_team_id {
        db_teams::is_member(&state.db_pool, team_id, user.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to check team membership: {}", e),
                )
            })?
    } else {
        false
    };

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to view deployments for this project".to_string(),
        ));
    }

    // Get deployments from database (optionally filtered by group)
    let db_deployments = db_deployments::list_for_project_and_group(
        &state.db_pool,
        project.id,
        query.deployment_group.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list deployments: {}", e),
        )
    })?;

    // Convert to API models
    let deployments: Vec<Deployment> = db_deployments.into_iter().map(convert_deployment).collect();

    Ok(Json(deployments))
}

/// Query parameters for stopping deployments
#[derive(Debug, serde::Deserialize)]
pub struct StopDeploymentsQuery {
    pub group: String,
}

/// Response for stopping deployments
#[derive(Debug, serde::Serialize)]
pub struct StopDeploymentsResponse {
    pub stopped_count: usize,
    pub deployment_ids: Vec<String>,
}

/// POST /projects/{project_name}/deployments/stop - Stop all deployments in a group
pub async fn stop_deployments_by_group(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
    Query(query): Query<StopDeploymentsQuery>,
) -> Result<Json<StopDeploymentsResponse>, (StatusCode, String)> {
    info!(
        "Stopping all deployments in group '{}' for project '{}'",
        query.group, project_name
    );

    // Validate group name
    if !is_valid_group_name(&query.group) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9]",
                query.group
            ),
        ));
    }

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to stop deployments (owns the project)
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Find all non-terminal deployments in this group
    let deployments = db_deployments::find_non_terminal_for_project_and_group(
        &state.db_pool,
        project.id,
        &query.group,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to find deployments: {}", e),
        )
    })?;

    let mut stopped_ids = Vec::new();

    // Mark each deployment as Terminating with UserStopped reason
    for deployment in deployments {
        match db_deployments::mark_terminating(
            &state.db_pool,
            deployment.id,
            crate::db::models::TerminationReason::UserStopped,
        )
        .await
        {
            Ok(_) => {
                info!(
                    "Marked deployment {} as Terminating",
                    deployment.deployment_id
                );
                stopped_ids.push(deployment.deployment_id);
            }
            Err(e) => {
                error!(
                    "Failed to mark deployment {} as Terminating: {}",
                    deployment.deployment_id, e
                );
            }
        }
    }

    // Update project status
    projects::update_calculated_status(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update project status: {}", e),
            )
        })?;

    info!(
        "Stopped {} deployments in group '{}' for project '{}'",
        stopped_ids.len(),
        query.group,
        project_name
    );

    Ok(Json(StopDeploymentsResponse {
        stopped_count: stopped_ids.len(),
        deployment_ids: stopped_ids,
    }))
}

/// GET /projects/{project_name}/deployments/{deployment_id} - Get a specific deployment
pub async fn get_deployment_by_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    debug!(
        "Getting deployment {} for project {}",
        deployment_id, project_name
    );

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to view deployments (owns the project or is team member)
    let has_permission = if let Some(owner_user_id) = project.owner_user_id {
        owner_user_id == user.id
    } else if let Some(team_id) = project.owner_team_id {
        db_teams::is_member(&state.db_pool, team_id, user.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to check team membership: {}", e),
                )
            })?
    } else {
        false
    };

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to view deployments for this project".to_string(),
        ));
    }

    // Find deployment by project_id and deployment_id
    let deployment = db_deployments::find_by_project_and_deployment_id(
        &state.db_pool,
        project.id,
        &deployment_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to find deployment: {}", e),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "Deployment '{}' not found for project '{}'",
                deployment_id, project_name
            ),
        )
    })?;

    Ok(Json(convert_deployment(deployment)))
}

/// POST /projects/{project_name}/deployments/{deployment_id}/rollback - Rollback to a previous deployment
pub async fn rollback_deployment(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, source_deployment_id)): Path<(String, String)>,
) -> Result<Json<RollbackDeploymentResponse>, (StatusCode, String)> {
    info!(
        "Rolling back project '{}' to deployment '{}'",
        project_name, source_deployment_id
    );

    // Find project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check deployment permissions
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Find the source deployment (the one we're rolling back to)
    let source_deployment = db_deployments::find_by_project_and_deployment_id(
        &state.db_pool,
        project.id,
        &source_deployment_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to find source deployment: {}", e),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "Source deployment '{}' not found for project '{}'",
                source_deployment_id, project_name
            ),
        )
    })?;

    // Verify source deployment is in a terminal successful state
    if source_deployment.status != DbDeploymentStatus::Healthy {
        return Err((StatusCode::BAD_REQUEST, format!("Cannot rollback to deployment '{}' with status '{:?}'. Only Healthy deployments can be used for rollback.", source_deployment_id, source_deployment.status)));
    }

    // Determine image tag for rollback
    let image_tag = if let Some(ref digest) = source_deployment.image_digest {
        // Pre-built image deployment - use the pinned digest
        info!("Rolling back to pre-built image: {}", digest);
        digest.clone()
    } else {
        // Build-from-source deployment - construct image tag from deployment_id
        // Get registry configuration to construct the image tag
        let registry_provider = state.registry_provider.as_ref().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "No registry configured".to_string(),
        ))?;

        let credentials = registry_provider
            .get_credentials(&project_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get credentials: {}", e),
                )
            })?;

        let constructed_tag = format!(
            "{}:{}",
            format!(
                "{}/{}",
                credentials.registry_url.trim_end_matches('/'),
                project_name
            ),
            source_deployment_id
        );
        info!("Rolling back to built image: {}", constructed_tag);
        constructed_tag
    };

    // Generate new deployment ID
    let new_deployment_id = generate_deployment_id();
    debug!(
        "Generated new deployment ID for rollback: {}",
        new_deployment_id
    );

    // Create new deployment with Pushed status
    // Copy image and image_digest from source (will be NULL for built images)
    // Copy group from source deployment to maintain consistency
    let new_deployment = db_deployments::create(
        &state.db_pool,
        &new_deployment_id,
        project.id,
        user.id,
        DbDeploymentStatus::Pushed, // Start in Pushed state so controller picks it up
        source_deployment.image.as_deref(), // Copy image from source if present
        source_deployment.image_digest.as_deref(), // Copy digest from source if present
        &source_deployment.deployment_group, // Copy group from source
        None,                       // expires_at - rollbacks don't inherit expiration
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create rollback deployment: {}", e),
        )
    })?;

    // Store image_tag in controller metadata for visibility
    let controller_metadata = serde_json::json!({
        "image_tag": image_tag,
        "internal_port": 8080,  // Default port
        "reconcile_phase": "NotStarted"
    });

    db_deployments::update_controller_metadata(
        &state.db_pool,
        new_deployment.id,
        &controller_metadata,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update controller metadata: {}", e),
        )
    })?;

    info!(
        "Created rollback deployment {} from {}",
        new_deployment_id, source_deployment_id
    );

    Ok(Json(RollbackDeploymentResponse {
        new_deployment_id,
        rolled_back_from: source_deployment_id,
        image_tag,
    }))
}
