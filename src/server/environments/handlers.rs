use super::models::{CreateEnvironmentRequest, EnvironmentResponse, UpdateEnvironmentRequest};
use crate::db::{environments as db_environments, projects};
use crate::server::auth::context::AuthContext;
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::project::handlers::ensure_project_access_or_admin;
use crate::server::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

/// Create a new environment for a project
pub async fn create_environment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id_or_name): Path<String>,
    Json(payload): Json<CreateEnvironmentRequest>,
) -> Result<(StatusCode, Json<EnvironmentResponse>), ServerError> {
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

    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    let env = db_environments::create(
        &state.db_pool,
        project.id,
        &payload.name,
        payload.primary_deployment_group.as_deref(),
        payload.is_default,
        payload.is_production,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("duplicate key") || msg.contains("unique constraint") {
            if msg.contains("primary_deployment_group") {
                ServerError::new(
                    StatusCode::CONFLICT,
                    format!(
                        "Deployment group '{}' is already the primary group of another environment",
                        payload
                            .primary_deployment_group
                            .as_deref()
                            .unwrap_or("(none)")
                    ),
                )
            } else {
                ServerError::new(
                    StatusCode::CONFLICT,
                    format!(
                        "Environment '{}' already exists in project '{}'",
                        payload.name, project.name
                    ),
                )
            }
        } else if msg.contains("valid_environment_name") {
            ServerError::bad_request(format!(
                "Invalid environment name '{}'. Must be lowercase alphanumeric with hyphens, no '--'.",
                payload.name
            ))
        } else {
            ServerError::internal_anyhow(e, "Failed to create environment")
        }
    })?;

    tracing::info!(
        "Created environment '{}' for project '{}'",
        env.name,
        project.name
    );

    Ok((StatusCode::CREATED, Json(env.into())))
}

/// List all environments for a project
pub async fn list_environments(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id_or_name): Path<String>,
) -> Result<Json<Vec<EnvironmentResponse>>, ServerError> {
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

    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    let envs = db_environments::list_for_project(&state.db_pool, project.id)
        .await
        .internal_err("Failed to list environments")?;

    Ok(Json(envs.into_iter().map(|e| e.into()).collect()))
}

/// Get a specific environment by name
pub async fn get_environment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id_or_name, env_name)): Path<(String, String)>,
) -> Result<Json<EnvironmentResponse>, ServerError> {
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

    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    let env = db_environments::find_by_name(&state.db_pool, project.id, &env_name)
        .await
        .internal_err("Failed to get environment")?
        .ok_or_else(|| ServerError::not_found(format!("Environment '{}' not found", env_name)))?;

    Ok(Json(env.into()))
}

/// Update an environment
pub async fn update_environment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id_or_name, env_name)): Path<(String, String)>,
    Json(payload): Json<UpdateEnvironmentRequest>,
) -> Result<Json<EnvironmentResponse>, ServerError> {
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

    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    let env = db_environments::find_by_name(&state.db_pool, project.id, &env_name)
        .await
        .internal_err("Failed to get environment")?
        .ok_or_else(|| ServerError::not_found(format!("Environment '{}' not found", env_name)))?;

    let updated = db_environments::update(
        &state.db_pool,
        env.id,
        project.id,
        payload.name.as_deref(),
        payload
            .primary_deployment_group
            .as_ref()
            .map(|o| o.as_deref()),
        payload.is_default,
        payload.is_production,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("duplicate key") || msg.contains("unique constraint") {
            ServerError::new(
                StatusCode::CONFLICT,
                "Conflict: name or group already taken",
            )
        } else if msg.contains("valid_environment_name") {
            ServerError::bad_request("Invalid environment name")
        } else {
            ServerError::internal_anyhow(e, "Failed to update environment")
        }
    })?;

    tracing::info!(
        "Updated environment '{}' in project '{}'",
        updated.name,
        project.name
    );

    Ok(Json(updated.into()))
}

/// Delete an environment
pub async fn delete_environment(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id_or_name, env_name)): Path<(String, String)>,
) -> Result<StatusCode, ServerError> {
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

    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

    let env = db_environments::find_by_name(&state.db_pool, project.id, &env_name)
        .await
        .internal_err("Failed to get environment")?
        .ok_or_else(|| ServerError::not_found(format!("Environment '{}' not found", env_name)))?;

    if env.is_default {
        return Err(ServerError::bad_request(
            "Cannot delete the default environment. Set another environment as default first.",
        ));
    }

    if env.is_production {
        return Err(ServerError::bad_request(
            "Cannot delete the production environment. Set another environment as production first.",
        ));
    }

    let deleted = db_environments::delete(&state.db_pool, env.id)
        .await
        .internal_err("Failed to delete environment")?;

    if !deleted {
        return Err(ServerError::not_found(format!(
            "Environment '{}' not found",
            env_name
        )));
    }

    tracing::info!(
        "Deleted environment '{}' from project '{}'",
        env_name,
        project.name
    );

    Ok(StatusCode::NO_CONTENT)
}
