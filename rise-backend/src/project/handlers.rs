use axum::{
    Json,
    extract::State,
};
use crate::state::AppState;
use anyhow::Result;
use uuid::Uuid;
use super::models::{CreateProjectRequest, CreateProjectResponse, Project, ProjectStatus};
use chrono;
// use pocketbase_sdk::records::RecordsManager; // Removed

pub async fn create_project(
    State(state): State<AppState>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, String> {
    // TODO: This is a temporary solution to make the code compile.
    // In a real application, you would get the token from the request headers
    // and create an authenticated client from it.
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@test.com", "12345678") // Dummy credentials
        .map_err(|e| format!("Failed to authenticate for project creation: {}", e.to_string()))?;

    let project_id = Uuid::new_v4().to_string();
    let project_url = format!("https://{}.rise.net", payload.name);

    let new_project = Project {
        id: project_id.clone(),
        name: payload.name.clone(),
        status: ProjectStatus::Stopped,
        url: project_url,
        visibility: payload.visibility,
        owner: payload.owner,
        created: chrono::Utc::now().to_rfc3339(),
        updated: chrono::Utc::now().to_rfc3339(),
    };

    let collection_name = "projects"; // Assuming a 'projects' collection in Pocketbase

    let created_record_meta = authenticated_client
        .records(collection_name)
        .create(&new_project)
        .call()
        .map_err(|e| format!("Failed to create project in PocketBase: {}", e.to_string()))?;

    let created_project: Project = authenticated_client
        .records(collection_name)
        .view(&created_record_meta.id)
        .call()
        .map_err(|e| format!("Failed to fetch created project: {}", e.to_string()))?;

    Ok(Json(CreateProjectResponse { project: created_project }))
}
