use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
};
use crate::state::AppState;
use super::models::{
    CreateProjectRequest, CreateProjectResponse, Project, UpdateProjectRequest, UpdateProjectResponse,
    ProjectWithOwnerInfo, ProjectErrorResponse, UserInfo, TeamInfo, OwnerInfo, GetProjectParams,
    ProjectStatus, ProjectOwner,
};
use super::fuzzy::find_similar_projects;
use serde_json::json;
use serde::Deserialize;

pub async fn create_project(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, (StatusCode, String)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: No authentication token provided".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid Authorization header format".to_string()))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to verify token: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid or expired token".to_string()));
    }

    // Token is valid - use test credentials to get an authenticated client for SDK calls
    // TODO: Update pocketbase SDK to support token-based authentication directly
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Authentication failed: {}", e)))?;

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

    tracing::info!("Creating project with data: {:?}", project_data);

    // Use HTTP client to create since SDK's CreateResponse is incomplete
    let token = authenticated_client.auth_token
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No auth token".to_string()))?;

    let create_url = format!("{}/api/collections/projects/records",
        state.settings.pocketbase.url);

    let client = reqwest::Client::new();
    let response = client
        .post(&create_url)
        .header("Authorization", token)
        .json(&project_data)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create project: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err((StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                   format!("Failed to create project: {}", error_text)));
    }

    // Log the response body before trying to parse it
    let response_text = response.text().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read response: {}", e)))?;

    tracing::info!("PocketBase response: {}", response_text);

    let created_project: Project = serde_json::from_str(&response_text)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse created project: {} - Response was: {}", e, response_text)))?;

    Ok(Json(CreateProjectResponse { project: created_project }))
}

pub async fn list_projects(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: No authentication token provided".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid Authorization header format".to_string()))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to verify token: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid or expired token".to_string()));
    }

    // Token is valid - use test credentials to get an authenticated client for SDK calls
    // TODO: Update pocketbase SDK to support token-based authentication directly
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Authentication failed: {}", e)))?;

    let projects: Vec<Project> = authenticated_client
        .records("projects")
        .list()
        .call()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list projects: {}", e)))?
        .items;

    Ok(Json(projects))
}

pub async fn get_project(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ProjectErrorResponse>)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: No authentication token provided".to_string(),
                suggestions: None,
            }),
        ))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid Authorization header format".to_string(),
                suggestions: None,
            }),
        ))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Failed to verify token: {}", e),
                suggestions: None,
            }),
        ))?;

    if !response.status().is_success() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid or expired token".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Token is valid - use test credentials to get an authenticated client for SDK calls
    // TODO: Update pocketbase SDK to support token-based authentication directly
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Authentication failed: {}", e),
                suggestions: None,
            }),
        ))?;

    // Resolve project by ID or name
    let project = resolve_project(&authenticated_client, &id_or_name, params.by_id)?;

    // Check if we should expand owner information
    if params.should_expand("owner") {
        let expanded = expand_project_with_owner(&authenticated_client, project).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to expand project data: {}", e),
                    suggestions: None,
                }),
            )
        })?;

        Ok(Json(serde_json::to_value(expanded).unwrap()))
    } else {
        Ok(Json(serde_json::to_value(project).unwrap()))
    }
}

pub async fn update_project(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<Json<UpdateProjectResponse>, (StatusCode, Json<ProjectErrorResponse>)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: No authentication token provided".to_string(),
                suggestions: None,
            }),
        ))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid Authorization header format".to_string(),
                suggestions: None,
            }),
        ))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Failed to verify token: {}", e),
                suggestions: None,
            }),
        ))?;

    if !response.status().is_success() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid or expired token".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Token is valid - use test credentials to get an authenticated client for SDK calls
    // TODO: Update pocketbase SDK to support token-based authentication directly
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Authentication failed: {}", e),
                suggestions: None,
            }),
        ))?;

    // Resolve project by ID or name
    let project = resolve_project(&authenticated_client, &id_or_name, params.by_id)?;

    // Build update payload with only provided fields
    let mut update_data = json!({});
    if let Some(name) = payload.name {
        update_data["name"] = json!(name);
    }
    if let Some(visibility) = payload.visibility {
        update_data["visibility"] = json!(visibility);
    }
    if let Some(status) = payload.status {
        update_data["status"] = json!(status);
    }

    // Handle ownership transfer
    if let Some(owner) = payload.owner {
        match owner {
            ProjectOwner::User(user_id) => {
                update_data["owner_user"] = json!(user_id);
                update_data["owner_team"] = json!(null);
            }
            ProjectOwner::Team(team_id) => {
                update_data["owner_user"] = json!(null);
                update_data["owner_team"] = json!(team_id);
            }
        }
    }

    let _updated_record_meta = authenticated_client
        .records("projects")
        .update(&project.id, &update_data)
        .call()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to update project: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Fetch the updated project
    let updated_project: Project = authenticated_client
        .records("projects")
        .view(&project.id)
        .call()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to fetch updated project: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    Ok(Json(UpdateProjectResponse { project: updated_project }))
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ProjectErrorResponse>)> {
    // Extract and validate JWT token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: No authentication token provided".to_string(),
                suggestions: None,
            }),
        ))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid Authorization header format".to_string(),
                suggestions: None,
            }),
        ))?;

    // Validate token with PocketBase
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Failed to verify token: {}", e),
                suggestions: None,
            }),
        ))?;

    if !response.status().is_success() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ProjectErrorResponse {
                error: "Unauthorized: Invalid or expired token".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Token is valid - use test credentials to get an authenticated client for SDK calls
    // TODO: Update pocketbase SDK to support token-based authentication directly
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", "test@example.com", "test1234")
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: format!("Authentication failed: {}", e),
                suggestions: None,
            }),
        ))?;

    // Resolve project by ID or name
    let project = resolve_project(&authenticated_client, &id_or_name, params.by_id)?;

    // Use HTTP client to delete since SDK doesn't expose delete method
    let token = authenticated_client
        .auth_token
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProjectErrorResponse {
                error: "No auth token".to_string(),
                suggestions: None,
            }),
        ))?;

    let delete_url = format!(
        "{}/api/collections/projects/records/{}",
        state.settings.pocketbase.url, project.id
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
                Json(ProjectErrorResponse {
                    error: format!("Failed to delete project: {}", e),
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
            Json(ProjectErrorResponse {
                error: format!("Failed to delete project: {}", error_text),
                suggestions: None,
            }),
        ))
    }
}

// Helper struct for deserializing PocketBase user records
#[derive(Debug, Deserialize, Default)]
struct PbUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    email: String,
}

// Helper struct for deserializing PocketBase team records
#[derive(Debug, Deserialize, Default)]
struct PbTeam {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

/// Query project by ID using PocketBase
fn query_project_by_id(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    project_id: &str,
) -> Result<Project, String> {
    authenticated_client
        .records("projects")
        .view(project_id)
        .call()
        .map_err(|e| format!("Project not found: {}", e))
}

/// Query project by name using PocketBase filter
fn query_project_by_name(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    project_name: &str,
) -> Result<Project, String> {
    // Escape single quotes for SQL filter
    let escaped_name = project_name.replace("'", "\\'");
    let filter = format!("name='{}'", escaped_name);

    tracing::info!("Querying project by name with filter: {}", filter);

    let result = authenticated_client
        .records("projects")
        .list()
        .filter(&filter)
        .call::<Project>()
        .map_err(|e| format!("Failed to query project by name: {}", e))?;

    tracing::info!("Query returned {} projects", result.items.len());

    result
        .items
        .into_iter()
        .next()
        .ok_or_else(|| format!("Project '{}' not found", project_name))
}

/// Expand project with owner information
fn expand_project_with_owner(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    project: Project,
) -> Result<ProjectWithOwnerInfo, String> {
    let owner_info = if let Some(ref user_id) = project.owner_user {
        // Fetch user information
        let user = authenticated_client
            .records("users")
            .view(user_id)
            .call::<PbUser>()
            .map_err(|e| format!("Failed to fetch user: {}", e))?;

        Some(OwnerInfo::User(UserInfo {
            id: user.id,
            email: user.email,
        }))
    } else if let Some(ref team_id) = project.owner_team {
        // Fetch team information
        let team = authenticated_client
            .records("teams")
            .view(team_id)
            .call::<PbTeam>()
            .map_err(|e| format!("Failed to fetch team: {}", e))?;

        Some(OwnerInfo::Team(TeamInfo {
            id: team.id,
            name: team.name,
        }))
    } else {
        None
    };

    Ok(ProjectWithOwnerInfo {
        id: project.id,
        name: project.name,
        status: project.status,
        visibility: project.visibility,
        owner: owner_info,
        created: project.created,
        updated: project.updated,
        collection_id: project.collectionId,
        collection_name: project.collectionName,
    })
}

/// Resolve project by ID or name with fuzzy matching support
fn resolve_project(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    id_or_name: &str,
    by_id: bool,
) -> Result<Project, (StatusCode, Json<ProjectErrorResponse>)> {
    tracing::info!("Resolving project '{}', by_id={}", id_or_name, by_id);

    let project = if by_id {
        // Explicit ID lookup
        tracing::info!("Using explicit ID lookup");
        query_project_by_id(authenticated_client, id_or_name)
            .map_err(|e| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ProjectErrorResponse {
                        error: e,
                        suggestions: None,
                    }),
                )
            })?
    } else {
        // Try name first, fallback to ID
        tracing::info!("Trying name lookup first, will fallback to ID");
        query_project_by_name(authenticated_client, id_or_name)
            .or_else(|e| {
                tracing::info!("Name lookup failed: {}, trying ID fallback", e);
                query_project_by_id(authenticated_client, id_or_name)
            })
            .map_err(|_e| {
                tracing::info!("Both lookups failed, generating fuzzy suggestions");
                // Both failed - provide fuzzy suggestions
                let all_projects_result = authenticated_client.records("projects").list().call::<Project>();

                let suggestions = match all_projects_result {
                    Ok(projects_response) => {
                        let similar = find_similar_projects(id_or_name, &projects_response.items, 0.85);
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
                    Json(ProjectErrorResponse {
                        error: format!("Project '{}' not found", id_or_name),
                        suggestions,
                    }),
                )
            })?
    };

    Ok(project)
}
