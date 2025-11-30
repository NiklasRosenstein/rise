use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
};
use crate::state::AppState;
use super::models::{
    CreateTeamRequest, CreateTeamResponse, Team, UpdateTeamRequest, UpdateTeamResponse,
    TeamWithEmails, TeamErrorResponse, UserInfo, GetTeamParams,
};
use super::fuzzy::find_similar_teams;
use serde_json::json;
use serde::Deserialize;
use std::collections::HashMap;

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

    tracing::info!("Creating team with data: {:?}", team_data);

    // Use HTTP client to create since SDK's CreateResponse is incomplete
    let token = authenticated_client.auth_token
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No auth token".to_string()))?;

    let create_url = format!("{}/api/collections/teams/records",
        state.settings.pocketbase.url);

    let client = reqwest::Client::new();
    let response = client
        .post(&create_url)
        .header("Authorization", token)
        .json(&team_data)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create team: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err((StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                   format!("Failed to create team: {}", error_text)));
    }

    // Log the response body before trying to parse it
    let response_text = response.text().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read response: {}", e)))?;

    tracing::info!("PocketBase response: {}", response_text);

    let created_team: Team = serde_json::from_str(&response_text)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse created team: {} - Response was: {}", e, response_text)))?;

    Ok(Json(CreateTeamResponse { team: created_team }))
}

pub async fn get_team(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<TeamErrorResponse>)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(TeamErrorResponse {
                    error: format!("Authentication failed: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Resolve team by ID or name
    let team = resolve_team(&authenticated_client, &id_or_name, params.by_id)?;

    // Check if we should expand user emails
    if params.should_expand("members") || params.should_expand("owners") {
        let expanded = expand_team_with_emails(&authenticated_client, team).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to expand team data: {}", e),
                    suggestions: None,
                }),
            )
        })?;

        Ok(Json(serde_json::to_value(expanded).unwrap()))
    } else {
        Ok(Json(serde_json::to_value(team).unwrap()))
    }
}

pub async fn update_team(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
    Json(payload): Json<UpdateTeamRequest>,
) -> Result<Json<UpdateTeamResponse>, (StatusCode, Json<TeamErrorResponse>)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(TeamErrorResponse {
                    error: format!("Authentication failed: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Resolve team by ID or name
    let team = resolve_team(&authenticated_client, &id_or_name, params.by_id)?;

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
        .update(&team.id, &update_data)
        .call()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to update team: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Fetch the updated team
    let updated_team: Team = authenticated_client
        .records("teams")
        .view(&team.id)
        .call()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to fetch updated team: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    Ok(Json(UpdateTeamResponse { team: updated_team }))
}

pub async fn delete_team(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
) -> Result<StatusCode, (StatusCode, Json<TeamErrorResponse>)> {
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(TeamErrorResponse {
                    error: format!("Authentication failed: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Resolve team by ID or name
    let team = resolve_team(&authenticated_client, &id_or_name, params.by_id)?;

    // Use HTTP client to delete since SDK doesn't expose delete method
    let token = authenticated_client
        .auth_token
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TeamErrorResponse {
                error: "No auth token".to_string(),
                suggestions: None,
            }),
        ))?;

    let delete_url = format!(
        "{}/api/collections/teams/records/{}",
        state.settings.pocketbase.url, team.id
    );

    let client = reqwest::Client::new();
    let response = client
        .delete(&delete_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to delete team: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if response.status().is_success() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(TeamErrorResponse {
                error: format!("Failed to delete team: {}", error_text),
                suggestions: None,
            }),
        ))
    }
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

// Helper struct for deserializing PocketBase user records
#[derive(Debug, Deserialize, Default)]
struct PbUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    collectionId: String,
    #[serde(default)]
    collectionName: String,
}

/// Query team by ID using PocketBase
fn query_team_by_id(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    team_id: &str,
) -> Result<Team, String> {
    authenticated_client
        .records("teams")
        .view(team_id)
        .call()
        .map_err(|e| format!("Team not found: {}", e))
}

/// Query team by name using PocketBase filter
fn query_team_by_name(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    team_name: &str,
) -> Result<Team, String> {
    // Escape single quotes for SQL filter
    let escaped_name = team_name.replace("'", "\\'");
    let filter = format!("name='{}'", escaped_name);

    let result = authenticated_client
        .records("teams")
        .list()
        .filter(&filter)
        .call::<Team>()
        .map_err(|e| format!("Failed to query team by name: {}", e))?;

    result
        .items
        .into_iter()
        .next()
        .ok_or_else(|| format!("Team '{}' not found", team_name))
}

/// Expand team with user emails (batch query for efficiency)
fn expand_team_with_emails(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    team: Team,
) -> Result<TeamWithEmails, String> {
    // Collect all unique user IDs
    let all_user_ids: Vec<String> = team
        .owners
        .iter()
        .chain(team.members.iter())
        .cloned()
        .collect();

    if all_user_ids.is_empty() {
        // No users to expand
        return Ok(TeamWithEmails {
            id: team.id,
            name: team.name,
            members: vec![],
            owners: vec![],
            created: team.created,
            updated: team.updated,
            collection_id: team.collectionId,
            collection_name: team.collectionName,
        });
    }

    // Build filter for batch query: id='id1' || id='id2' || ...
    let filter_parts: Vec<String> = all_user_ids
        .iter()
        .map(|id| format!("id='{}'", id.replace("'", "\\'")))
        .collect();
    let filter = filter_parts.join(" || ");

    // Fetch all users in one query
    let users_result = authenticated_client
        .records("users")
        .list()
        .filter(&filter)
        .call::<PbUser>()
        .map_err(|e| format!("Failed to fetch users: {}", e))?;

    // Create ID -> UserInfo map
    let user_map: HashMap<String, UserInfo> = users_result
        .items
        .into_iter()
        .map(|u| {
            (
                u.id.clone(),
                UserInfo {
                    id: u.id,
                    email: u.email,
                },
            )
        })
        .collect();

    // Map owners and members to UserInfo (filter out missing users gracefully)
    let owners: Vec<UserInfo> = team
        .owners
        .iter()
        .filter_map(|id| user_map.get(id).cloned())
        .collect();

    let members: Vec<UserInfo> = team
        .members
        .iter()
        .filter_map(|id| user_map.get(id).cloned())
        .collect();

    Ok(TeamWithEmails {
        id: team.id,
        name: team.name,
        members,
        owners,
        created: team.created,
        updated: team.updated,
        collection_id: team.collectionId,
        collection_name: team.collectionName,
    })
}

/// Resolve team by ID or name with fuzzy matching support
fn resolve_team(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    id_or_name: &str,
    by_id: bool,
) -> Result<Team, (StatusCode, Json<TeamErrorResponse>)> {
    let team = if by_id {
        // Explicit ID lookup
        query_team_by_id(authenticated_client, id_or_name)
            .map_err(|e| {
                (
                    StatusCode::NOT_FOUND,
                    Json(TeamErrorResponse {
                        error: e,
                        suggestions: None,
                    }),
                )
            })?
    } else {
        // Try name first, fallback to ID
        query_team_by_name(authenticated_client, id_or_name)
            .or_else(|_| query_team_by_id(authenticated_client, id_or_name))
            .map_err(|_| {
                // Both failed - provide fuzzy suggestions
                let all_teams_result = authenticated_client.records("teams").list().call::<Team>();

                let suggestions = match all_teams_result {
                    Ok(teams_response) => {
                        let similar = find_similar_teams(id_or_name, &teams_response.items, 0.85);
                        if similar.is_empty() {
                            None
                        } else {
                            Some(similar)
                        }
                    }
                    Err(_) => None,
                };

                (
                    StatusCode::NOT_FOUND,
                    Json(TeamErrorResponse {
                        error: format!("Team '{}' not found", id_or_name),
                        suggestions,
                    }),
                )
            })?
    };

    Ok(team)
}
