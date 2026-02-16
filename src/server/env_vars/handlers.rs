use super::models::{EnvVarResponse, EnvVarValueResponse, EnvVarsResponse, SetEnvVarRequest};
use crate::db::models::User;
use crate::db::{env_vars as db_env_vars, projects};
use crate::server::extensions::InjectedEnvVarValue;
use crate::server::state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;

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

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Normalize: when is_protected is omitted, infer from is_secret
    // This preserves backward compatibility: secrets default to protected, plain vars default to unprotected
    let is_protected = payload.is_protected.unwrap_or(payload.is_secret);

    // Validate: is_protected requires is_secret (non-secrets cannot be "protected")
    if is_protected && !payload.is_secret {
        return Err((
            StatusCode::BAD_REQUEST,
            "Non-secret variables cannot be protected. Protection only applies to secrets."
                .to_string(),
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
        is_protected,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to store environment variable: {}", e),
        )
    })?;

    tracing::info!(
        "Set environment variable '{}' for project '{}' (secret: {}, protected: {}). This will apply to new deployments only.",
        key,
        project.name,
        payload.is_secret,
        is_protected
    );

    // Note: Environment variables are snapshots at deployment time.
    // Changing project env vars does not affect existing deployments.
    // New deployments will use the updated values.

    // Return masked response
    Ok(Json(EnvVarResponse::from_db_model(
        env_var.key,
        env_var.value,
        env_var.is_secret,
        env_var.is_protected,
    )))
}

/// List all environment variables for a project
pub async fn list_project_env_vars(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
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

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    // Check if we should include unprotected values
    let include_unprotected = params
        .get("include_unprotected_values")
        .map(|v| v == "true")
        .unwrap_or(false);

    // Get all environment variables
    let db_env_vars = db_env_vars::list_project_env_vars(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list environment variables: {}", e),
            )
        })?;

    // Convert to API response
    let mut env_vars = Vec::new();
    for var in db_env_vars {
        let value = if include_unprotected && var.is_secret && !var.is_protected {
            // Decrypt unprotected secret
            match &state.encryption_provider {
                Some(provider) => provider.decrypt(&var.value).await.map_err(|e| {
                    tracing::error!(
                        "Failed to decrypt unprotected secret '{}': {:?}",
                        var.key,
                        e
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to decrypt secret '{}': {}", var.key, e),
                    )
                })?,
                None => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Cannot decrypt secrets: no encryption provider configured".to_string(),
                    ))
                }
            }
        } else {
            var.value.clone()
        };

        env_vars.push(
            if var.is_secret && (!include_unprotected || var.is_protected) {
                // Mask protected secrets
                EnvVarResponse::from_db_model(var.key, var.value, var.is_secret, var.is_protected)
            } else {
                // Return plaintext or decrypted value
                EnvVarResponse {
                    key: var.key,
                    value,
                    is_secret: var.is_secret,
                    is_protected: var.is_protected,
                }
            },
        );
    }

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

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

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
        "Deleted environment variable '{}' from project '{}'. This will apply to new deployments only.",
        key,
        project.name
    );

    // Note: Environment variables are snapshots at deployment time.
    // Deleting project env vars does not affect existing deployments.
    // New deployments will not have the deleted variable.

    Ok(StatusCode::NO_CONTENT)
}

/// List all environment variables for a deployment (read-only)
pub async fn list_deployment_env_vars(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, deployment_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
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

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

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

    // Check if we should include unprotected values
    let include_unprotected = params
        .get("include_unprotected_values")
        .map(|v| v == "true")
        .unwrap_or(false);

    // Get all deployment environment variables
    let db_env_vars = db_env_vars::list_deployment_env_vars(&state.db_pool, deployment.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list deployment environment variables: {}", e),
            )
        })?;

    // Convert to API response
    let mut env_vars = Vec::new();
    for var in db_env_vars {
        let value = if include_unprotected && var.is_secret && !var.is_protected {
            // Decrypt unprotected secret
            match &state.encryption_provider {
                Some(provider) => provider.decrypt(&var.value).await.map_err(|e| {
                    tracing::error!(
                        "Failed to decrypt unprotected secret '{}': {:?}",
                        var.key,
                        e
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to decrypt secret '{}': {}", var.key, e),
                    )
                })?,
                None => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Cannot decrypt secrets: no encryption provider configured".to_string(),
                    ))
                }
            }
        } else {
            var.value.clone()
        };

        env_vars.push(
            if var.is_secret && (!include_unprotected || var.is_protected) {
                // Mask protected secrets
                EnvVarResponse::from_db_model(var.key, var.value, var.is_secret, var.is_protected)
            } else {
                // Return plaintext or decrypted value
                EnvVarResponse {
                    key: var.key,
                    value,
                    is_secret: var.is_secret,
                    is_protected: var.is_protected,
                }
            },
        );
    }

    Ok(Json(EnvVarsResponse { env_vars }))
}

/// Get the decrypted value of a specific retrievable secret
pub async fn get_project_env_var_value(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, key)): Path<(String, String)>,
) -> Result<Json<EnvVarValueResponse>, (StatusCode, String)> {
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

    // Get the specific environment variable
    let env_var = db_env_vars::get_project_env_var(&state.db_pool, project.id, &key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get environment variable: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Environment variable '{}' not found", key),
            )
        })?;

    // Validate: must be an unprotected secret
    if !env_var.is_secret || env_var.is_protected {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Environment variable '{}' is a protected secret and cannot be retrieved. \
                 Update it with --protected=false to allow retrieval.",
                key
            ),
        ));
    }

    // Decrypt the value
    let decrypted_value = match &state.encryption_provider {
        Some(provider) => provider.decrypt(&env_var.value).await.map_err(|e| {
            tracing::error!("Failed to decrypt unprotected secret '{}': {:?}", key, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to decrypt secret '{}': {}", key, e),
            )
        })?,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot decrypt secrets: no encryption provider configured".to_string(),
            ))
        }
    };

    tracing::info!(
        "Retrieved decrypted value for secret '{}' in project '{}' by user '{}'",
        key,
        project.name,
        user.email
    );

    Ok(Json(EnvVarValueResponse {
        value: decrypted_value,
    }))
}

/// Preview the full set of environment variables a deployment would receive.
///
/// Returns:
/// - User-set environment variables
/// - System vars (PORT, RISE_ISSUER, RISE_APP_URL, RISE_APP_URLS)
/// - Extension-injected vars
///
/// Protected vars are masked. This enables `rise run` to inject the same env vars as a real deployment.
pub async fn preview_deployment_env_vars(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
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

    // Check permission (admin bypass)
    ensure_project_access_or_admin(&state, &user, &project).await?;

    let deployment_group = params
        .get("deployment_group")
        .cloned()
        .unwrap_or_else(|| "default".to_string());

    // Collect all env vars into a map (later entries override earlier ones)
    let mut env_map: HashMap<String, EnvVarResponse> = HashMap::new();

    // 1. Load user-set project vars
    let db_vars = db_env_vars::list_project_env_vars(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list environment variables: {}", e),
            )
        })?;

    for var in db_vars {
        if var.is_secret && !var.is_protected {
            // Unprotected secret — decrypt for preview
            let decrypted = match &state.encryption_provider {
                Some(provider) => provider.decrypt(&var.value).await.map_err(|e| {
                    tracing::error!(
                        "Failed to decrypt unprotected secret '{}': {:?}",
                        var.key,
                        e
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to decrypt secret '{}': {}", var.key, e),
                    )
                })?,
                None => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Cannot decrypt secrets: no encryption provider configured".to_string(),
                    ))
                }
            };
            env_map.insert(
                var.key.clone(),
                EnvVarResponse {
                    key: var.key,
                    value: decrypted,
                    is_secret: true,
                    is_protected: false,
                },
            );
        } else if var.is_secret {
            // Protected secret — mask
            env_map.insert(
                var.key.clone(),
                EnvVarResponse {
                    key: var.key,
                    value: "••••••••".to_string(),
                    is_secret: true,
                    is_protected: true,
                },
            );
        } else {
            // Plain var
            env_map.insert(
                var.key.clone(),
                EnvVarResponse {
                    key: var.key.clone(),
                    value: var.value,
                    is_secret: false,
                    is_protected: false,
                },
            );
        }
    }

    // 2. Add system vars
    if !env_map.contains_key("PORT") {
        env_map.insert(
            "PORT".to_string(),
            EnvVarResponse {
                key: "PORT".to_string(),
                value: "8080".to_string(),
                is_secret: false,
                is_protected: false,
            },
        );
    }

    env_map.insert(
        "RISE_ISSUER".to_string(),
        EnvVarResponse {
            key: "RISE_ISSUER".to_string(),
            value: state.public_url.clone(),
            is_secret: false,
            is_protected: false,
        },
    );

    // Get project URLs from deployment backend (if available)
    match state
        .deployment_backend
        .get_project_urls(&project, &deployment_group)
        .await
    {
        Ok(urls) => {
            env_map.insert(
                "RISE_APP_URL".to_string(),
                EnvVarResponse {
                    key: "RISE_APP_URL".to_string(),
                    value: urls.primary_url.clone(),
                    is_secret: false,
                    is_protected: false,
                },
            );

            let mut all_urls = vec![urls.default_url.clone()];
            all_urls.extend(urls.custom_domain_urls);
            let urls_json = serde_json::to_string(&all_urls).unwrap_or_else(|_| "[]".to_string());
            env_map.insert(
                "RISE_APP_URLS".to_string(),
                EnvVarResponse {
                    key: "RISE_APP_URLS".to_string(),
                    value: urls_json,
                    is_secret: false,
                    is_protected: false,
                },
            );
        }
        Err(e) => {
            tracing::debug!(
                "Could not compute project URLs for preview (no deployment controller?): {:?}",
                e
            );
        }
    }

    // 3. Collect extension env vars
    for (_, extension) in state.extension_registry.iter() {
        match extension
            .preview_env_vars(project.id, &deployment_group)
            .await
        {
            Ok(vars) => {
                for var in vars {
                    let response = match var.value {
                        InjectedEnvVarValue::Plain(v) => EnvVarResponse {
                            key: var.key.clone(),
                            value: v,
                            is_secret: false,
                            is_protected: false,
                        },
                        InjectedEnvVarValue::Secret { decrypted, .. } => EnvVarResponse {
                            key: var.key.clone(),
                            value: decrypted,
                            is_secret: true,
                            is_protected: false,
                        },
                        InjectedEnvVarValue::Protected { .. } => EnvVarResponse {
                            key: var.key.clone(),
                            value: "••••••••".to_string(),
                            is_secret: true,
                            is_protected: true,
                        },
                    };
                    // Extension vars override user vars for the same key
                    env_map.insert(var.key, response);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Extension '{}' failed to provide preview env vars: {:?}",
                    extension.extension_type(),
                    e
                );
            }
        }
    }

    // Convert to sorted vec
    let mut env_vars: Vec<EnvVarResponse> = env_map.into_values().collect();
    env_vars.sort_by(|a, b| a.key.cmp(&b.key));

    Ok(Json(EnvVarsResponse { env_vars }))
}
