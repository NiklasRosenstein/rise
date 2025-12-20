use super::models::*;
use crate::db::models::User;
use crate::db::{extensions as db_extensions, projects};
use crate::server::state::AppState;
use axum::{
    extract::{Extension as AxumExtension, Path, State},
    http::StatusCode,
    Json,
};

/// List all available extension types (registered providers)
pub async fn list_extension_types(
    State(state): State<AppState>,
    AxumExtension(_user): AxumExtension<User>,
) -> Result<Json<ListExtensionTypesResponse>, (StatusCode, String)> {
    // Note: This endpoint doesn't require project access - it lists all available
    // extension types that any authenticated user can see and potentially enable on their projects

    let extension_types: Vec<ExtensionTypeMetadata> = state
        .extension_registry
        .iter()
        .map(|(_registry_key, extension)| ExtensionTypeMetadata {
            extension_type: extension.extension_type().to_string(),
            display_name: extension.display_name().to_string(),
            description: extension.description().to_string(),
            documentation: extension.documentation().to_string(),
            spec_schema: extension.spec_schema(),
        })
        .collect();

    Ok(Json(ListExtensionTypesResponse { extension_types }))
}

/// Create or upsert extension for project
pub async fn create_extension(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Json(payload): Json<CreateExtensionRequest>,
) -> Result<Json<CreateExtensionResponse>, (StatusCode, String)> {
    // Get project and verify access
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check project ownership/access
    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    // Get extension handler by type
    let extension = state
        .extension_registry
        .get(&payload.extension_type)
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("Unknown extension type: {}", payload.extension_type),
        ))?;

    // Validate spec
    extension
        .validate_spec(&payload.spec)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid spec: {}", e)))?;

    // Create extension (will fail if already exists)
    let ext_record = db_extensions::create(
        &state.db_pool,
        project.id,
        &extension_name,
        &payload.extension_type,
        &payload.spec,
    )
    .await
    .map_err(|e| {
        // Check if it's a unique constraint violation
        let error_msg = e.to_string();
        if error_msg.contains("duplicate key") || error_msg.contains("unique constraint") {
            (
                StatusCode::CONFLICT,
                format!("Extension '{}' already exists", extension_name),
            )
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
        }
    })?;

    // Format status using the extension provider
    let status_summary = extension.format_status(&ext_record.status);

    Ok(Json(CreateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            extension_type: extension.extension_type().to_string(),
            spec: ext_record.spec,
            status: ext_record.status,
            status_summary,
            created: ext_record.created_at.to_rfc3339(),
            updated: ext_record.updated_at.to_rfc3339(),
        },
    }))
}

/// Update extension (PUT for full replace, PATCH for partial update)
pub async fn update_extension(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Json(payload): Json<UpdateExtensionRequest>,
) -> Result<Json<UpdateExtensionResponse>, (StatusCode, String)> {
    // Get project and verify access
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check project ownership/access
    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    // Get existing extension to determine its type
    let existing =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Extension not found".to_string()))?;

    // Get extension handler by type
    let extension = state
        .extension_registry
        .get(&existing.extension_type)
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("Unknown extension type: {}", existing.extension_type),
        ))?;

    // Validate new spec
    extension
        .validate_spec(&payload.spec)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid spec: {}", e)))?;

    // Update extension (upsert will update existing, keeping the extension_type)
    let ext_record = db_extensions::upsert(
        &state.db_pool,
        project.id,
        &extension_name,
        &existing.extension_type,
        &payload.spec,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // For OAuth extensions, reset auth_verified when spec changes
    if existing.extension_type == "oauth" && spec_changed_for_oauth(&existing.spec, &payload.spec) {
        reset_oauth_auth_verified(
            &state.db_pool,
            project.id,
            &extension_name,
            &ext_record.status,
        )
        .await?;
    }

    // Fetch updated extension to get the latest status
    let ext_record =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Extension not found".to_string()))?;

    // Format status using the extension provider
    let status_summary = extension.format_status(&ext_record.status);

    Ok(Json(UpdateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            extension_type: extension.extension_type().to_string(),
            spec: ext_record.spec,
            status: ext_record.status,
            status_summary,
            created: ext_record.created_at.to_rfc3339(),
            updated: ext_record.updated_at.to_rfc3339(),
        },
    }))
}

/// Patch extension (merge with nulls removing fields)
pub async fn patch_extension(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path((project_name, extension_name)): Path<(String, String)>,
    Json(payload): Json<UpdateExtensionRequest>,
) -> Result<Json<UpdateExtensionResponse>, (StatusCode, String)> {
    // Get project and verify access
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check project ownership/access
    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    // Get existing extension
    let existing =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Extension not found".to_string()))?;

    // Merge specs (null values in payload remove fields from existing)
    let merged_spec = merge_json_with_nulls(&existing.spec, &payload.spec);

    // Get extension handler by type
    let extension = state
        .extension_registry
        .get(&existing.extension_type)
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("Unknown extension type: {}", existing.extension_type),
        ))?;

    // Validate merged spec
    extension.validate_spec(&merged_spec).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid spec after merge: {}", e),
        )
    })?;

    // Update extension with merged spec
    let ext_record = db_extensions::upsert(
        &state.db_pool,
        project.id,
        &extension_name,
        &existing.extension_type,
        &merged_spec,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // For OAuth extensions, reset auth_verified when spec changes
    if existing.extension_type == "oauth" && spec_changed_for_oauth(&existing.spec, &merged_spec) {
        reset_oauth_auth_verified(
            &state.db_pool,
            project.id,
            &extension_name,
            &ext_record.status,
        )
        .await?;
    }

    // Fetch updated extension to get the latest status
    let ext_record =
        db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Extension not found".to_string()))?;

    // Format status using the extension provider
    let status_summary = extension.format_status(&ext_record.status);

    Ok(Json(UpdateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            extension_type: extension.extension_type().to_string(),
            spec: ext_record.spec,
            status: ext_record.status,
            status_summary,
            created: ext_record.created_at.to_rfc3339(),
            updated: ext_record.updated_at.to_rfc3339(),
        },
    }))
}

/// List extensions for project
pub async fn list_extensions(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path(project_name): Path<String>,
) -> Result<Json<ListExtensionsResponse>, (StatusCode, String)> {
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let extensions = db_extensions::list_by_project(&state.db_pool, project.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let extensions: Vec<Extension> = extensions
        .into_iter()
        .map(|e| {
            // Get extension provider by type to format status
            let status_summary = state
                .extension_registry
                .get(&e.extension_type)
                .map(|ext| ext.format_status(&e.status))
                .unwrap_or_else(|| "Unknown".to_string());

            Extension {
                extension: e.extension,
                extension_type: e.extension_type,
                spec: e.spec,
                status: e.status,
                status_summary,
                created: e.created_at.to_rfc3339(),
                updated: e.updated_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(Json(ListExtensionsResponse { extensions }))
}

/// Get extension by name
pub async fn get_extension(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path((project_name, extension_name)): Path<(String, String)>,
) -> Result<Json<Extension>, (StatusCode, String)> {
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let ext = db_extensions::find_by_project_and_name(&state.db_pool, project.id, &extension_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Extension not found".to_string()))?;

    // Get extension provider by type to format status
    let status_summary = state
        .extension_registry
        .get(&ext.extension_type)
        .map(|ext_provider| ext_provider.format_status(&ext.status))
        .unwrap_or_else(|| "Unknown".to_string());

    Ok(Json(Extension {
        extension: ext.extension,
        extension_type: ext.extension_type,
        spec: ext.spec,
        status: ext.status,
        status_summary,
        created: ext.created_at.to_rfc3339(),
        updated: ext.updated_at.to_rfc3339(),
    }))
}

/// Delete extension (mark for deletion)
pub async fn delete_extension(
    State(state): State<AppState>,
    AxumExtension(user): AxumExtension<User>,
    Path((project_name, extension_name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    let has_access = check_project_access(&state, &user, project.id).await?;
    if !has_access {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    db_extensions::mark_deleted(&state.db_pool, project.id, &extension_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Helper to check if user has access to project
async fn check_project_access(
    state: &AppState,
    user: &User,
    project_id: uuid::Uuid,
) -> Result<bool, (StatusCode, String)> {
    // Check if user is admin
    if state.admin_users.contains(&user.email) {
        return Ok(true);
    }

    // Check if user has access to project (owner or team member)
    let accessible_projects = projects::list_accessible_by_user(&state.db_pool, user.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(accessible_projects.iter().any(|p| p.id == project_id))
}

/// Merge JSON values, treating null in update as field deletion
fn merge_json_with_nulls(
    existing: &serde_json::Value,
    update: &serde_json::Value,
) -> serde_json::Value {
    use serde_json::Value;

    match (existing, update) {
        (Value::Object(existing_map), Value::Object(update_map)) => {
            let mut result = existing_map.clone();
            for (key, value) in update_map.iter() {
                if value.is_null() {
                    // Null means remove the field
                    result.remove(key);
                } else if let Some(existing_value) = existing_map.get(key) {
                    // Recursively merge nested objects
                    result.insert(key.clone(), merge_json_with_nulls(existing_value, value));
                } else {
                    // New field
                    result.insert(key.clone(), value.clone());
                }
            }
            Value::Object(result)
        }
        _ => {
            // For non-objects, just replace with update value
            update.clone()
        }
    }
}

/// Check if OAuth spec changed in fields that affect auth flow
fn spec_changed_for_oauth(old_spec: &serde_json::Value, new_spec: &serde_json::Value) -> bool {
    // Fields that affect OAuth flow
    let auth_sensitive_fields = [
        "client_id",
        "client_secret_ref",
        "authorization_endpoint",
        "token_endpoint",
        "scopes",
    ];

    for field in &auth_sensitive_fields {
        if old_spec.get(field) != new_spec.get(field) {
            return true;
        }
    }

    false
}

/// Reset auth_verified to false for OAuth extension
async fn reset_oauth_auth_verified(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    extension_name: &str,
    current_status: &serde_json::Value,
) -> Result<(), (StatusCode, String)> {
    use crate::server::extensions::providers::oauth::models::OAuthExtensionStatus;

    // Parse current status
    let mut status: OAuthExtensionStatus =
        serde_json::from_value(current_status.clone()).unwrap_or_default();

    // Reset auth_verified
    status.auth_verified = false;

    // Update status
    db_extensions::update_status(
        pool,
        project_id,
        extension_name,
        &serde_json::to_value(&status).unwrap(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}
