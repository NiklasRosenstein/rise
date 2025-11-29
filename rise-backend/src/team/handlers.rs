use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use crate::state::AppState;
use super::models::{CreateTeamRequest, CreateTeamResponse, Team, UpdateTeamRequest, UpdateTeamResponse};
use serde_json::json;

pub async fn create_team(
    State(state): State<AppState>,
    Json(payload): Json<CreateTeamRequest>,
) -> Result<Json<CreateTeamResponse>, (StatusCode, String)> {
    // TODO: Extract JWT token from Authorization header and authenticate with PocketBase
    // For now, we'll use dummy authentication but this needs to be fixed
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    // TODO: Validate that the authenticated user is in the owners list
    // This should be enforced by PocketBase rules, but we can add backend validation too

    let team_data = json!({
        "name": payload.name,
        "members": payload.members,
        "owners": payload.owners,
    });

    let collection_name = "teams";

    let created_record_meta = authenticated_client
        .records(collection_name)
        .create(&team_data)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create team: {}", e)))?;

    let created_team: Team = authenticated_client
        .records(collection_name)
        .view(&created_record_meta.id)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch team: {}", e)))?;

    Ok(Json(CreateTeamResponse { team: created_team }))
}

pub async fn get_team(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
) -> Result<Json<Team>, (StatusCode, String)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    let team: Team = authenticated_client
        .records("teams")
        .view(&team_id)
        .call()
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Team not found: {}", e)))?;

    Ok(Json(team))
}

pub async fn update_team(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
    Json(payload): Json<UpdateTeamRequest>,
) -> Result<Json<UpdateTeamResponse>, (StatusCode, String)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    // Build update payload with only provided fields
    let mut update_data = json!({});
    if let Some(name) = payload.name {
        update_data["name"] = json!(name);
    }
    if let Some(members) = payload.members {
        update_data["members"] = json!(members);
    }
    if let Some(owners) = payload.owners {
        update_data["owners"] = json!(owners);
    }

    let _updated_record_meta = authenticated_client
        .records("teams")
        .update(&team_id, &update_data)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update team: {}", e)))?;

    // Fetch the updated team
    let updated_team: Team = authenticated_client
        .records("teams")
        .view(&team_id)
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch updated team: {}", e)))?;

    Ok(Json(UpdateTeamResponse { team: updated_team }))
}

pub async fn delete_team(
    State(_state): State<AppState>,
    Path(_team_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // TODO: Implement delete team once we figure out the correct pocketbase SDK method
    // The SDK version 0.1.1 doesn't seem to have a delete/remove method
    Err((StatusCode::NOT_IMPLEMENTED, "Delete team not yet implemented".to_string()))
}

pub async fn list_teams(
    State(state): State<AppState>,
) -> Result<Json<Vec<Team>>, (StatusCode, String)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    let teams: Vec<Team> = authenticated_client
        .records("teams")
        .list()
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list teams: {}", e)))?
        .items;

    Ok(Json(teams))
}
