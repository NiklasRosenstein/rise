use super::models::{EnvVarResponse, EnvVarsResponse, SetEnvVarRequest};
use crate::db::models::User;
use crate::db::{env_vars as db_env_vars, projects};
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};

/// Format an error and its full chain of causes for logging/display
fn format_error_chain(error: &anyhow::Error) -> String {
    let mut chain = vec![error.to_string()];

    // Collect all causes
    let mut current_error = error.source();
    while let Some(cause) = current_error {
        chain.push(cause.to_string());
        current_error = cause.source();
    }

    // Join them with " -> " to show the causal chain
    chain.join(" -> ")
}

/// Set or update a project environment variable
pub async fn set_project_env_var(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, key)): Path<(String, String)>,
    Json(payload): Json<SetEnvVarRequest>,
) -> Result<Json<EnvVarResponse>, (StatusCode, String)> {
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

    // Check if user has access to the project
    let has_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check project access: {}", e),
            )
        })?;

    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ));
    }

    // IMPORTANT: If this is a secret, we must have an encryption provider
    if payload.is_secret && state.encryption_provider.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot store secret variables: no encryption provider configured".to_string(),
        ));
    }

    // Encrypt the value if it's a secret
    let value_to_store = if payload.is_secret {
        let provider = state
            .encryption_provider
            .as_ref()
            .expect("Encryption provider checked above");

        provider.encrypt(&payload.value).await.map_err(|e| {
            // Log the full error chain for debugging
            tracing::error!("Encryption failed: {:?}", e);

            // Format error chain for the response
            let error_chain = format_error_chain(&e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encrypt secret: {}", error_chain),
            )
        })?
    } else {
        payload.value.clone()
    };

    // Store in database
    let env_var = db_env_vars::upsert_project_env_var(
        &state.db_pool,
        project.id,
        &key,
        &value_to_store,
        payload.is_secret,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to store environment variable: {}", e),
        )
    })?;

    tracing::info!(
        "Set environment variable '{}' for project '{}' (secret: {})",
        key,
        project.name,
        payload.is_secret
    );

    // Return masked response
    Ok(Json(EnvVarResponse::from_db_model(
        env_var.key,
        env_var.value,
        env_var.is_secret,
    )))
}

/// List all environment variables for a project
pub async fn list_project_env_vars(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
) -> Result<Json<EnvVarsResponse>, (StatusCode, String)> {
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

    // Check if user has access to the project
    let has_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check project access: {}", e),
            )
        })?;

    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ));
    }

    // Get all environment variables
    let db_env_vars = db_env_vars::list_project_env_vars(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list environment variables: {}", e),
            )
        })?;

    // Convert to API response, masking secrets
    let env_vars = db_env_vars
        .into_iter()
        .map(|var| EnvVarResponse::from_db_model(var.key, var.value, var.is_secret))
        .collect();

    Ok(Json(EnvVarsResponse { env_vars }))
}

/// Delete a project environment variable
pub async fn delete_project_env_var(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, key)): Path<(String, String)>,
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

    // Check if user has access to the project
    let has_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check project access: {}", e),
            )
        })?;

    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ));
    }

    // Delete environment variable
    let deleted = db_env_vars::delete_project_env_var(&state.db_pool, project.id, &key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete environment variable: {}", e),
            )
        })?;

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Environment variable '{}' not found", key),
        ));
    }

    tracing::info!(
        "Deleted environment variable '{}' from project '{}'",
        key,
        project.name
    );

    Ok(StatusCode::NO_CONTENT)
}

/// List all environment variables for a deployment (read-only)
pub async fn list_deployment_env_vars(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, deployment_id)): Path<(String, String)>,
) -> Result<Json<EnvVarsResponse>, (StatusCode, String)> {
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

    // Check if user has access to the project
    let has_access = projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check project access: {}", e),
            )
        })?;

    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have access to this project".to_string(),
        ));
    }

    // Get deployment by deployment_id within the project
    let deployment =
        crate::db::deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get deployment: {}", e),
                )
            })?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Deployment not found".to_string()))?;

    // Get all deployment environment variables
    let db_env_vars = db_env_vars::list_deployment_env_vars(&state.db_pool, deployment.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list deployment environment variables: {}", e),
            )
        })?;

    // Convert to API response, masking secrets
    let env_vars = db_env_vars
        .into_iter()
        .map(|var| EnvVarResponse::from_db_model(var.key, var.value, var.is_secret))
        .collect();

    Ok(Json(EnvVarsResponse { env_vars }))
}
