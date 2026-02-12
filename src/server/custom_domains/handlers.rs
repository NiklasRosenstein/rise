use super::models::{AddCustomDomainRequest, CustomDomainResponse, CustomDomainsResponse};
use super::validation;
use crate::db::models::User;
use crate::db::{custom_domains as db_custom_domains, deployments as db_deployments, projects};
use crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP;
use crate::server::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

/// Check if user has access to a project (admin bypass)
///
/// Admins always have access. Non-admins must pass the project ownership/team membership check.
async fn ensure_project_access_or_admin(
    state: &AppState,
    user: &User,
    project: &crate::db::models::Project,
) -> Result<(), (StatusCode, String)> {
    // Admins bypass all access checks
    if state.is_admin(&user.email) {
        return Ok(());
    }

    // Check if user has access via ownership or team membership
    let can_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check project access: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    if !can_access {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ));
    }

    Ok(())
}

/// Add a custom domain to a project
pub async fn add_custom_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
    Json(payload): Json<AddCustomDomainRequest>,
) -> Result<(StatusCode, Json<CustomDomainResponse>), (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Validate that the custom domain doesn't overlap with project default domain patterns
    if let Some(ref production_template) = state.production_ingress_url_template {
        if let Err(reason) = validation::validate_custom_domain(
            &payload.domain,
            production_template,
            state.staging_ingress_url_template.as_deref(),
            Some(&state.public_url),
        ) {
            return Err((StatusCode::BAD_REQUEST, reason));
        }
    }

    // Add the custom domain
    let domain = db_custom_domains::add_custom_domain(&state.db_pool, project.id, &payload.domain)
        .await
        .map_err(|e| {
            // Check if it's a duplicate key error or validation error
            let error_message = e.to_string();
            if error_message.contains("duplicate key")
                || error_message.contains("unique constraint")
            {
                (
                    StatusCode::CONFLICT,
                    format!("Domain '{}' is already in use", payload.domain),
                )
            } else if error_message.contains("check constraint") {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid domain format: {}", payload.domain),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to add custom domain: {}", e),
                )
            }
        })?;

    // Trigger reconciliation of the active deployment in the default group
    // Custom domains are only applied to the default deployment group
    match db_deployments::find_active_for_project_and_group(
        &state.db_pool,
        project.id,
        DEFAULT_DEPLOYMENT_GROUP,
    )
    .await
    {
        Ok(Some(active_deployment)) => {
            info!(
                "Found active deployment {} in default group for project '{}', marking for reconciliation",
                active_deployment.deployment_id, project.name
            );

            if let Err(e) =
                db_deployments::mark_needs_reconcile(&state.db_pool, active_deployment.id).await
            {
                // Log the error but don't fail the request - the domain was added successfully
                info!(
                    "Failed to trigger reconciliation for deployment {} after adding domain: {}",
                    active_deployment.deployment_id, e
                );
            } else {
                info!(
                    "Successfully marked deployment {} for reconciliation after adding custom domain '{}'",
                    active_deployment.deployment_id, payload.domain
                );
            }
        }
        Ok(None) => {
            info!(
                "No active deployment found in default group for project '{}', custom domain added but no reconciliation needed",
                project.name
            );
        }
        Err(e) => {
            info!(
                "Failed to find active deployment for project '{}': {}",
                project.name, e
            );
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(CustomDomainResponse::from_db_model(&domain)),
    ))
}

/// List all custom domains for a project
pub async fn list_custom_domains(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
) -> Result<Json<CustomDomainsResponse>, (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project)
        .await
        .map_err(|(status, msg)| {
            // Map FORBIDDEN to NOT_FOUND for consistency with original behavior
            if status == StatusCode::FORBIDDEN {
                (StatusCode::NOT_FOUND, "Project not found".to_string())
            } else {
                (status, msg)
            }
        })?;

    // Get all custom domains for the project
    let domains = db_custom_domains::list_project_custom_domains(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list custom domains: {}", e),
            )
        })?;

    Ok(Json(CustomDomainsResponse {
        domains: domains
            .iter()
            .map(CustomDomainResponse::from_db_model)
            .collect(),
    }))
}

/// Get a specific custom domain
pub async fn get_custom_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<Json<CustomDomainResponse>, (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project)
        .await
        .map_err(|(status, msg)| {
            // Map FORBIDDEN to NOT_FOUND for consistency with original behavior
            if status == StatusCode::FORBIDDEN {
                (StatusCode::NOT_FOUND, "Project not found".to_string())
            } else {
                (status, msg)
            }
        })?;

    // Get the custom domain
    let domain = db_custom_domains::get_custom_domain(&state.db_pool, project.id, &domain)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get custom domain: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Custom domain not found".to_string()))?;

    Ok(Json(CustomDomainResponse::from_db_model(&domain)))
}

/// Delete a custom domain
pub async fn delete_custom_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Delete the custom domain
    let deleted = db_custom_domains::delete_custom_domain(&state.db_pool, project.id, &domain)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete custom domain: {}", e),
            )
        })?;

    if !deleted {
        return Err((StatusCode::NOT_FOUND, "Custom domain not found".to_string()));
    }

    // Trigger reconciliation of the active deployment in the default group
    // Custom domains are only applied to the default deployment group
    match db_deployments::find_active_for_project_and_group(
        &state.db_pool,
        project.id,
        DEFAULT_DEPLOYMENT_GROUP,
    )
    .await
    {
        Ok(Some(active_deployment)) => {
            info!(
                "Found active deployment {} in default group for project '{}', marking for reconciliation",
                active_deployment.deployment_id, project.name
            );

            if let Err(e) =
                db_deployments::mark_needs_reconcile(&state.db_pool, active_deployment.id).await
            {
                // Log the error but don't fail the request - the domain was deleted successfully
                info!(
                    "Failed to trigger reconciliation for deployment {} after deleting domain: {}",
                    active_deployment.deployment_id, e
                );
            } else {
                info!(
                    "Successfully marked deployment {} for reconciliation after deleting custom domain '{}'",
                    active_deployment.deployment_id, domain
                );
            }
        }
        Ok(None) => {
            info!(
                "No active deployment found in default group for project '{}', custom domain deleted but no reconciliation needed",
                project.name
            );
        }
        Err(e) => {
            info!(
                "Failed to find active deployment for project '{}': {}",
                project.name, e
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Set a custom domain as primary for a project
pub async fn set_primary_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<Json<CustomDomainResponse>, (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Set the domain as primary
    let updated_domain = db_custom_domains::set_primary_domain(&state.db_pool, project.id, &domain)
        .await
        .map_err(|e| {
            let error_message = e.to_string();
            if error_message.contains("no rows") {
                (StatusCode::NOT_FOUND, "Custom domain not found".to_string())
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to set primary domain: {}", e),
                )
            }
        })?;

    // Trigger reconciliation of the active deployment in the default group
    match db_deployments::find_active_for_project_and_group(
        &state.db_pool,
        project.id,
        DEFAULT_DEPLOYMENT_GROUP,
    )
    .await
    {
        Ok(Some(active_deployment)) => {
            info!(
                "Found active deployment {} in default group for project '{}', marking for reconciliation",
                active_deployment.deployment_id, project.name
            );

            if let Err(e) =
                db_deployments::mark_needs_reconcile(&state.db_pool, active_deployment.id).await
            {
                info!(
                    "Failed to trigger reconciliation for deployment {} after setting primary domain: {}",
                    active_deployment.deployment_id, e
                );
            } else {
                info!(
                    "Successfully marked deployment {} for reconciliation after setting primary domain '{}'",
                    active_deployment.deployment_id, domain
                );
            }
        }
        Ok(None) => {
            info!(
                "No active deployment found in default group for project '{}', primary domain set but no reconciliation needed",
                project.name
            );
        }
        Err(e) => {
            info!(
                "Failed to find active deployment for project '{}': {}",
                project.name, e
            );
        }
    }

    Ok(Json(CustomDomainResponse::from_db_model(&updated_domain)))
}

/// Unset the primary status of a custom domain
pub async fn unset_primary_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get project: {}", e),
                )
            })?
    }
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Unset the primary status
    let unset = db_custom_domains::unset_primary_domain(&state.db_pool, project.id, &domain)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to unset primary domain: {}", e),
            )
        })?;

    if !unset {
        return Err((
            StatusCode::NOT_FOUND,
            "Custom domain not found or is not primary".to_string(),
        ));
    }

    // Trigger reconciliation of the active deployment in the default group
    match db_deployments::find_active_for_project_and_group(
        &state.db_pool,
        project.id,
        DEFAULT_DEPLOYMENT_GROUP,
    )
    .await
    {
        Ok(Some(active_deployment)) => {
            info!(
                "Found active deployment {} in default group for project '{}', marking for reconciliation",
                active_deployment.deployment_id, project.name
            );

            if let Err(e) =
                db_deployments::mark_needs_reconcile(&state.db_pool, active_deployment.id).await
            {
                info!(
                    "Failed to trigger reconciliation for deployment {} after unsetting primary domain: {}",
                    active_deployment.deployment_id, e
                );
            } else {
                info!(
                    "Successfully marked deployment {} for reconciliation after unsetting primary domain '{}'",
                    active_deployment.deployment_id, domain
                );
            }
        }
        Ok(None) => {
            info!(
                "No active deployment found in default group for project '{}', primary domain unset but no reconciliation needed",
                project.name
            );
        }
        Err(e) => {
            info!(
                "Failed to find active deployment for project '{}': {}",
                project.name, e
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
