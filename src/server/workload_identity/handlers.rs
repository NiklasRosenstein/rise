use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use uuid::Uuid;

use crate::db::{projects, service_accounts, users, User};
use crate::server::project::handlers::{check_read_permission, check_write_permission};
use crate::server::state::AppState;
use crate::server::workload_identity::models::{
    CreateWorkloadIdentityRequest, ListWorkloadIdentitiesResponse, WorkloadIdentityResponse,
};

type Result<T> = std::result::Result<T, (StatusCode, String)>;

/// Create a new service account for a project
pub async fn create_workload_identity(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
    Json(req): Json<CreateWorkloadIdentityRequest>,
) -> Result<Json<WorkloadIdentityResponse>> {
    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check permission: user must be able to write to project
    if !check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Cannot manage service accounts for this project".to_string(),
        ));
    }

    // Validate issuer URL
    if req.issuer_url.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Issuer URL cannot be empty".to_string(),
        ));
    }

    // In production, should validate HTTPS
    // For now, just check it's a valid URL format
    if !req.issuer_url.starts_with("http://") && !req.issuer_url.starts_with("https://") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Issuer URL must be a valid HTTP(S) URL".to_string(),
        ));
    }

    // Validate claims requirements
    if req.claims.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one claim is required".to_string(),
        ));
    }

    // Require 'aud' claim
    if !req.claims.contains_key("aud") {
        return Err((
            StatusCode::BAD_REQUEST,
            "The 'aud' (audience) claim is required for service accounts".to_string(),
        ));
    }

    // Require at least one additional claim besides 'aud'
    if req.claims.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one claim in addition to 'aud' is required (e.g., project_path, ref_protected)".to_string(),
        ));
    }

    // Create service account
    let sa = service_accounts::create(&state.db_pool, project.id, &req.issuer_url, &req.claims)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get user for response
    let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Service account user not found".to_string(),
            )
        })?;

    // Convert JSONB claims to HashMap for response
    let claims: std::collections::HashMap<String, String> = serde_json::from_value(sa.claims)
        .map_err(|e| {
            tracing::error!("Failed to deserialize claims: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to deserialize claims".to_string(),
            )
        })?;

    Ok(Json(WorkloadIdentityResponse {
        id: sa.id.to_string(),
        email: sa_user.email,
        project_name: project.name,
        issuer_url: sa.issuer_url,
        claims,
        created_at: sa.created_at.to_rfc3339(),
    }))
}

/// List all service accounts for a project
pub async fn list_workload_identities(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
) -> Result<Json<ListWorkloadIdentitiesResponse>> {
    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check read permission
    if !check_read_permission(&state, &project, &user)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Err((StatusCode::NOT_FOUND, "Project not found".to_string()));
    }

    // Get active service accounts
    let sas = service_accounts::list_by_project(&state.db_pool, project.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Convert to response
    let mut workload_identities = Vec::new();
    for sa in sas {
        let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Service account user not found".to_string(),
                )
            })?;

        // Convert JSONB claims to HashMap
        let claims: std::collections::HashMap<String, String> =
            serde_json::from_value(sa.claims.clone()).map_err(|e| {
                tracing::error!("Failed to deserialize claims: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to deserialize claims".to_string(),
                )
            })?;

        workload_identities.push(WorkloadIdentityResponse {
            id: sa.id.to_string(),
            email: sa_user.email,
            project_name: project.name.clone(),
            issuer_url: sa.issuer_url,
            claims,
            created_at: sa.created_at.to_rfc3339(),
        });
    }

    Ok(Json(ListWorkloadIdentitiesResponse {
        workload_identities,
    }))
}

/// Get a specific service account
pub async fn get_workload_identity(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, sa_id)): Path<(String, Uuid)>,
) -> Result<Json<WorkloadIdentityResponse>> {
    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check read permission
    if !check_read_permission(&state, &project, &user)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Err((StatusCode::NOT_FOUND, "Project not found".to_string()));
    }

    // Get service account
    let sa = service_accounts::get_by_id(&state.db_pool, sa_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Service account not found".to_string(),
            )
        })?;

    // Verify SA belongs to this project
    if sa.project_id != project.id {
        return Err((
            StatusCode::NOT_FOUND,
            "Service account not found".to_string(),
        ));
    }

    // Check if deleted
    if sa.deleted_at.is_some() {
        return Err((
            StatusCode::NOT_FOUND,
            "Service account not found".to_string(),
        ));
    }

    // Get user
    let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Service account user not found".to_string(),
            )
        })?;

    // Convert JSONB claims to HashMap
    let claims: std::collections::HashMap<String, String> = serde_json::from_value(sa.claims)
        .map_err(|e| {
            tracing::error!("Failed to deserialize claims: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to deserialize claims".to_string(),
            )
        })?;

    Ok(Json(WorkloadIdentityResponse {
        id: sa.id.to_string(),
        email: sa_user.email,
        project_name: project.name,
        issuer_url: sa.issuer_url,
        claims,
        created_at: sa.created_at.to_rfc3339(),
    }))
}

/// Delete a service account (soft delete)
pub async fn delete_workload_identity(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, sa_id)): Path<(String, Uuid)>,
) -> Result<StatusCode> {
    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Project not found".to_string()))?;

    // Check write permission
    if !check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Cannot manage service accounts for this project".to_string(),
        ));
    }

    // Get service account
    let sa = service_accounts::get_by_id(&state.db_pool, sa_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Service account not found".to_string(),
            )
        })?;

    // Verify SA belongs to this project
    if sa.project_id != project.id {
        return Err((
            StatusCode::NOT_FOUND,
            "Service account not found".to_string(),
        ));
    }

    // Check if already deleted
    if sa.deleted_at.is_some() {
        return Err((
            StatusCode::NOT_FOUND,
            "Service account not found".to_string(),
        ));
    }

    // Soft delete
    service_accounts::soft_delete(&state.db_pool, sa_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
