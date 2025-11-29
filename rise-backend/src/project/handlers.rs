use axum::{
    Json,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use crate::state::AppState;
use anyhow::Result;
use super::models::{CreateProjectRequest, CreateProjectResponse, Project, ProjectStatus, ProjectOwner};
use serde_json::json;

pub async fn create_project(
    State(state): State<AppState>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, (StatusCode, String)> {
    // TODO: Extract JWT token from Authorization header and authenticate with PocketBase
    // For now, we'll use dummy authentication but this needs to be fixed
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    // Validate owner - exactly one of owner_user or owner_team must be set
    let (owner_user, owner_team) = match &payload.owner {
        ProjectOwner::User(user_id) => (Some(user_id.clone()), None),
        ProjectOwner::Team(team_id) => (None, Some(team_id.clone())),
    };

    // Create project payload for PocketBase
    let project_data = json!({
        "name": payload.name,
        "status": ProjectStatus::Stopped,
        "visibility": payload.visibility,
        "owner_user": owner_user,
        "owner_team": owner_team,
    });

    let collection_name = "projects";

    let created_record_meta = authenticated_client
        .records(collection_name)
        .create(&project_data)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create project: {}", e)))?;

    let created_project: Project = authenticated_client
        .records(collection_name)
        .view(&created_record_meta.id)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch project: {}", e)))?;

    Ok(Json(CreateProjectResponse { project: created_project }))
}
