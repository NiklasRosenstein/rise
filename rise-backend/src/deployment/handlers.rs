use axum::{
    Json,
    extract::{State, Path, Extension},
    http::StatusCode,
};
use tracing::{info, debug};

use crate::state::AppState;
use crate::db::models::{User, DeploymentStatus as DbDeploymentStatus};
use crate::db::{projects, teams as db_teams, deployments as db_deployments};
use super::models::*;
use super::utils::{generate_deployment_id, construct_image_tag};
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

    // If project is owned by a team, check if user is an owner of that team
    if let Some(team_id) = project.owner_team_id {
        let is_owner = db_teams::is_owner(&state.db_pool, team_id, user_id)
            .await
            .map_err(|e| format!("Failed to check team ownership: {}", e))?;

        if is_owner {
            return Ok(());
        }

        let team = db_teams::find_by_id(&state.db_pool, team_id)
            .await
            .map_err(|e| format!("Failed to fetch team: {}", e))?
            .ok_or_else(|| "Team not found".to_string())?;

        return Err(format!(
            "You must be an owner of team '{}' to deploy to this project",
            team.name
        ));
    }

    Err("You do not have permission to deploy to this project".to_string())
}

/// Convert API DeploymentStatus to DB DeploymentStatus
fn convert_status_to_db(status: DeploymentStatus) -> DbDeploymentStatus {
    match status {
        DeploymentStatus::Pending => DbDeploymentStatus::Pending,
        DeploymentStatus::Building => DbDeploymentStatus::Building,
        DeploymentStatus::Pushing => DbDeploymentStatus::Pushing,
        DeploymentStatus::Pushed => DbDeploymentStatus::Pushed,
        DeploymentStatus::Deploying => DbDeploymentStatus::Deploying,
        DeploymentStatus::Completed => DbDeploymentStatus::Completed,
        DeploymentStatus::Failed => DbDeploymentStatus::Failed,
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
        DbDeploymentStatus::Completed => DeploymentStatus::Completed,
        DbDeploymentStatus::Failed => DeploymentStatus::Failed,
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
        error_message: deployment.error_message,
        completed_at: deployment.completed_at.map(|dt| dt.to_rfc3339()),
        build_logs: deployment.build_logs,
        controller_metadata: deployment.controller_metadata,
        deployment_url: deployment.deployment_url,
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

    // Query project by name
    let project = projects::find_by_name(&state.db_pool, &payload.project)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to query project: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Project '{}' not found", payload.project)))?;

    // Check deployment permissions
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    debug!("Generated deployment ID: {}", deployment_id);

    // Get registry credentials
    let registry_provider = state.registry_provider.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "No registry configured".to_string()))?;

    let credentials = registry_provider.get_credentials(&payload.project).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get credentials: {}", e)))?;

    // Construct image tag
    let namespace = if let Some(ref settings) = state.settings.registry {
        match settings {
            crate::settings::RegistrySettings::Docker { namespace, .. } => namespace.clone(),
            crate::settings::RegistrySettings::Ecr { account_id, region, .. } => {
                format!("{}.dkr.ecr.{}.amazonaws.com", account_id, region)
            }
        }
    } else {
        "rise-apps".to_string()
    };

    let image_tag = construct_image_tag(
        &credentials.registry_url,
        &namespace,
        &payload.project,
        &deployment_id
    );

    debug!("Image tag: {}", image_tag);

    // Create deployment record in database
    let _deployment = db_deployments::create(
        &state.db_pool,
        &deployment_id,
        project.id,
        user.id,
        DbDeploymentStatus::Pending,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create deployment: {}", e)))?;

    info!("Created deployment {} for project {}", deployment_id, payload.project);

    // Return response
    Ok(Json(CreateDeploymentResponse {
        deployment_id,
        image_tag,
        credentials,
    }))
}

/// PATCH /deployments/{deployment_id}/status - Update deployment status
pub async fn update_deployment_status(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(deployment_id): Path<String>,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!("Updating deployment {} status to {:?}", deployment_id, payload.status);

    // Find all deployments with this deployment_id (there should only be one)
    // We need to find the project first to check the deployment_id
    // For now, let's query by deployment_id across all projects
    // We'll need to add a function to find by deployment_id only

    // Query all projects to find the one with this deployment
    let all_projects = projects::list(&state.db_pool, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list projects: {}", e)))?;

    let mut found_deployment: Option<crate::db::models::Deployment> = None;
    let mut found_project: Option<crate::db::models::Project> = None;

    for project in all_projects {
        if let Ok(Some(deployment)) = db_deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id).await {
            found_deployment = Some(deployment);
            found_project = Some(project);
            break;
        }
    }

    let deployment = found_deployment
        .ok_or((StatusCode::NOT_FOUND, format!("Deployment '{}' not found", deployment_id)))?;

    let project = found_project
        .ok_or((StatusCode::NOT_FOUND, format!("Project for deployment '{}' not found", deployment_id)))?;

    // Check if user has permission (owns the project)
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Update status in database
    let status_copy = payload.status.clone();
    let updated_deployment = match payload.status {
        DeploymentStatus::Completed => {
            let deployment = db_deployments::mark_completed(&state.db_pool, deployment.id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update deployment: {}", e)))?;

            // Set this as the active deployment for the project
            projects::set_active_deployment(&state.db_pool, project.id, deployment.id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set active deployment: {}", e)))?;

            // Update project status to Running
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update project status: {}", e)))?;

            deployment
        }
        DeploymentStatus::Failed => {
            let error_msg = payload.error_message.as_deref().unwrap_or("Unknown error");
            let deployment = db_deployments::mark_failed(&state.db_pool, deployment.id, error_msg)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update deployment: {}", e)))?;

            // Update project status to Failed
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update project status: {}", e)))?;

            deployment
        }
        _ => {
            let db_status = convert_status_to_db(payload.status);
            let deployment = db_deployments::update_status(&state.db_pool, deployment.id, db_status)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update deployment: {}", e)))?;

            // Update project status (e.g., to Deploying)
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update project status: {}", e)))?;

            deployment
        }
    };

    info!("Updated deployment {} to status {:?}", deployment_id, status_copy);

    Ok(Json(convert_deployment(updated_deployment)))
}

/// List deployments for a project
pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
) -> Result<Json<Vec<Deployment>>, (StatusCode, String)> {
    debug!("Listing deployments for project: {}", project_name);

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find project: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Project '{}' not found", project_name)))?;

    // Check if user has permission to view deployments (owns the project or is team member)
    let has_permission = if let Some(owner_user_id) = project.owner_user_id {
        owner_user_id == user.id
    } else if let Some(team_id) = project.owner_team_id {
        db_teams::is_member(&state.db_pool, team_id, user.id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to check team membership: {}", e)))?
    } else {
        false
    };

    if !has_permission {
        return Err((StatusCode::FORBIDDEN, "You do not have permission to view deployments for this project".to_string()));
    }

    // Get deployments from database
    let db_deployments = db_deployments::list_for_project(&state.db_pool, project.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list deployments: {}", e)))?;

    // Convert to API models
    let deployments: Vec<Deployment> = db_deployments
        .into_iter()
        .map(convert_deployment)
        .collect();

    info!("Found {} deployments for project '{}'", deployments.len(), project_name);

    Ok(Json(deployments))
}

/// GET /projects/{project_name}/deployments/{deployment_id} - Get a specific deployment
pub async fn get_deployment_by_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    debug!("Getting deployment {} for project {}", deployment_id, project_name);

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find project: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Project '{}' not found", project_name)))?;

    // Check if user has permission to view deployments (owns the project or is team member)
    let has_permission = if let Some(owner_user_id) = project.owner_user_id {
        owner_user_id == user.id
    } else if let Some(team_id) = project.owner_team_id {
        db_teams::is_member(&state.db_pool, team_id, user.id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to check team membership: {}", e)))?
    } else {
        false
    };

    if !has_permission {
        return Err((StatusCode::FORBIDDEN, "You do not have permission to view deployments for this project".to_string()));
    }

    // Find deployment by project_id and deployment_id
    let deployment = db_deployments::find_by_project_and_deployment_id(&state.db_pool, project.id, &deployment_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find deployment: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Deployment '{}' not found for project '{}'", deployment_id, project_name)))?;

    info!("Found deployment {} for project {}", deployment_id, project_name);

    Ok(Json(convert_deployment(deployment)))
}

/// POST /projects/{project_name}/deployments/{deployment_id}/rollback - Rollback to a previous deployment
pub async fn rollback_deployment(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, source_deployment_id)): Path<(String, String)>,
) -> Result<Json<RollbackDeploymentResponse>, (StatusCode, String)> {
    info!("Rolling back project '{}' to deployment '{}'", project_name, source_deployment_id);

    // Find project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to query project: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Project '{}' not found", project_name)))?;

    // Check deployment permissions
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Find the source deployment (the one we're rolling back to)
    let source_deployment = db_deployments::find_by_project_and_deployment_id(&state.db_pool, project.id, &source_deployment_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find source deployment: {}", e)))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Source deployment '{}' not found for project '{}'", source_deployment_id, project_name)))?;

    // Verify source deployment is in a terminal successful state
    if source_deployment.status != DbDeploymentStatus::Completed {
        return Err((StatusCode::BAD_REQUEST, format!("Cannot rollback to deployment '{}' with status '{:?}'. Only Completed deployments can be used for rollback.", source_deployment_id, source_deployment.status)));
    }

    // Extract image tag from controller metadata
    let image_tag = source_deployment.controller_metadata
        .get("image_tag")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Source deployment does not have image_tag in metadata".to_string()))?
        .to_string();

    info!("Rollback will use image tag: {}", image_tag);

    // Generate new deployment ID
    let new_deployment_id = generate_deployment_id();
    debug!("Generated new deployment ID for rollback: {}", new_deployment_id);

    // Create new deployment with Pushed status and pre-filled controller metadata
    let controller_metadata = serde_json::json!({
        "image_tag": image_tag,
        "internal_port": 8080,  // Default port
        "reconcile_phase": "NotStarted"
    });

    let new_deployment = db_deployments::create(
        &state.db_pool,
        &new_deployment_id,
        project.id,
        user.id,
        DbDeploymentStatus::Pushed,  // Start in Pushed state so controller picks it up
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create rollback deployment: {}", e)))?;

    // Update the controller metadata
    db_deployments::update_controller_metadata(&state.db_pool, new_deployment.id, &controller_metadata)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update controller metadata: {}", e)))?;

    info!("Created rollback deployment {} from {}", new_deployment_id, source_deployment_id);

    Ok(Json(RollbackDeploymentResponse {
        new_deployment_id,
        rolled_back_from: source_deployment_id,
        image_tag,
    }))
}
