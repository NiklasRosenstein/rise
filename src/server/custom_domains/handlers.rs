use super::models::{AddCustomDomainRequest, CustomDomainResponse, CustomDomainsResponse};
use super::validation;
use crate::db::{custom_domains as db_custom_domains, deployments as db_deployments, projects};
use crate::server::auth::context::AuthContext;
use crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP;
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::project::handlers::ensure_project_access_or_admin;
use crate::server::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

/// Add a custom domain to a project
pub async fn add_custom_domain(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id_or_name): Path<String>,
    Json(payload): Json<AddCustomDomainRequest>,
) -> Result<(StatusCode, Json<CustomDomainResponse>), ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    // Validate that the custom domain doesn't overlap with project default domain patterns
    if let Some(ref production_template) = state.production_ingress_url_template {
        if let Err(reason) = validation::validate_custom_domain(
            &payload.domain,
            production_template,
            state.staging_ingress_url_template.as_deref(),
            Some(&state.public_url),
        ) {
            return Err(ServerError::bad_request(reason));
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
                ServerError::conflict(format!("Domain '{}' is already in use", payload.domain))
            } else if error_message.contains("check constraint") {
                ServerError::bad_request(format!("Invalid domain format: {}", payload.domain))
            } else {
                ServerError::internal_anyhow(e, "Failed to add custom domain")
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
    auth: AuthContext,
    Path(project_id_or_name): Path<String>,
) -> Result<Json<CustomDomainsResponse>, ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project)
            .await
            .map_err(|e| {
                if e.status == StatusCode::FORBIDDEN {
                    ServerError::not_found("Project not found")
                } else {
                    e
                }
            })?;
    }

    // Get all custom domains for the project
    let domains = db_custom_domains::list_project_custom_domains(&state.db_pool, project.id)
        .await
        .internal_err("Failed to list custom domains")?;

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
    auth: AuthContext,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<Json<CustomDomainResponse>, ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project)
            .await
            .map_err(|e| {
                if e.status == StatusCode::FORBIDDEN {
                    ServerError::not_found("Project not found")
                } else {
                    e
                }
            })?;
    }

    // Get the custom domain
    let domain = db_custom_domains::get_custom_domain(&state.db_pool, project.id, &domain)
        .await
        .internal_err("Failed to get custom domain")?
        .ok_or_else(|| ServerError::not_found("Custom domain not found"))?;

    Ok(Json(CustomDomainResponse::from_db_model(&domain)))
}

/// Delete a custom domain
pub async fn delete_custom_domain(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<StatusCode, ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    // Delete the custom domain
    let deleted = db_custom_domains::delete_custom_domain(&state.db_pool, project.id, &domain)
        .await
        .internal_err("Failed to delete custom domain")?;

    if !deleted {
        return Err(ServerError::not_found("Custom domain not found"));
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
    auth: AuthContext,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<Json<CustomDomainResponse>, ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    // Set the domain as primary
    let updated_domain = db_custom_domains::set_primary_domain(&state.db_pool, project.id, &domain)
        .await
        .map_err(|e| {
            let error_message = e.to_string();
            if error_message.contains("no rows") {
                ServerError::not_found("Custom domain not found")
            } else {
                ServerError::internal_anyhow(e, "Failed to set primary domain")
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
    auth: AuthContext,
    Path((project_id_or_name, domain)): Path<(String, String)>,
) -> Result<StatusCode, ServerError> {
    // Find project by ID or name
    let project = if let Ok(uuid) = project_id_or_name.parse() {
        projects::find_by_id(&state.db_pool, uuid)
            .await
            .internal_err("Failed to get project")?
    } else {
        projects::find_by_name(&state.db_pool, &project_id_or_name)
            .await
            .internal_err("Failed to get project")?
    }
    .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth
        .resolve_for_project(&state.db_pool, &project)
        .await
        .map_err(|_| ServerError::not_found("Project not found"))?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    // Unset the primary status
    let unset = db_custom_domains::unset_primary_domain(&state.db_pool, project.id, &domain)
        .await
        .internal_err("Failed to unset primary domain")?;

    if !unset {
        return Err(ServerError::not_found(
            "Custom domain not found or is not primary",
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
