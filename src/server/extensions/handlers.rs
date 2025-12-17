use super::models::*;
use crate::db::models::User;
use crate::db::{extensions as db_extensions, projects};
use crate::server::state::AppState;
use axum::{
    extract::{Extension as AxumExtension, Path, State},
    http::StatusCode,
    Json,
};

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

    // Get extension implementation
    let extension = state.extension_registry.get(&extension_name).ok_or((
        StatusCode::BAD_REQUEST,
        format!("Unknown extension: {}", extension_name),
    ))?;

    // Validate spec
    extension
        .validate_spec(&payload.spec)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid spec: {}", e)))?;

    // Create/update extension
    let ext_record =
        db_extensions::upsert(&state.db_pool, project.id, &extension_name, &payload.spec)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            spec: ext_record.spec,
            status: ext_record.status,
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

    // Get extension implementation
    let extension = state.extension_registry.get(&extension_name).ok_or((
        StatusCode::BAD_REQUEST,
        format!("Unknown extension: {}", extension_name),
    ))?;

    // Validate new spec
    extension
        .validate_spec(&payload.spec)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid spec: {}", e)))?;

    // Update extension (upsert will create if not exists, or update if exists)
    let ext_record =
        db_extensions::upsert(&state.db_pool, project.id, &extension_name, &payload.spec)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(UpdateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            spec: ext_record.spec,
            status: ext_record.status,
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

    // Get extension implementation
    let extension = state.extension_registry.get(&extension_name).ok_or((
        StatusCode::BAD_REQUEST,
        format!("Unknown extension: {}", extension_name),
    ))?;

    // Validate merged spec
    extension.validate_spec(&merged_spec).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid spec after merge: {}", e),
        )
    })?;

    // Update extension with merged spec
    let ext_record =
        db_extensions::upsert(&state.db_pool, project.id, &extension_name, &merged_spec)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(UpdateExtensionResponse {
        extension: Extension {
            extension: ext_record.extension,
            spec: ext_record.spec,
            status: ext_record.status,
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
        .map(|e| Extension {
            extension: e.extension,
            spec: e.spec,
            status: e.status,
            created: e.created_at.to_rfc3339(),
            updated: e.updated_at.to_rfc3339(),
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

    Ok(Json(Extension {
        extension: ext.extension,
        spec: ext.spec,
        status: ext.status,
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
