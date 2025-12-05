use super::fuzzy::find_similar_projects;
use super::models::{
    CreateProjectRequest, CreateProjectResponse, GetProjectParams, OwnerInfo,
    Project as ApiProject, ProjectErrorResponse, ProjectOwner, ProjectStatus, ProjectVisibility,
    ProjectWithOwnerInfo, TeamInfo, UpdateProjectRequest, UpdateProjectResponse, UserInfo,
};
use crate::db::models::User;
use crate::db::{projects, teams as db_teams, users as db_users};
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

pub async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, (StatusCode, String)> {
    // Validate owner - exactly one of owner_user or owner_team must be set
    let (owner_user_id, owner_team_id) = match &payload.owner {
        ProjectOwner::User(user_id) => {
            let uuid = Uuid::parse_str(user_id)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid user ID: {}", e)))?;
            (Some(uuid), None)
        }
        ProjectOwner::Team(team_id) => {
            let uuid = Uuid::parse_str(team_id)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid team ID: {}", e)))?;

            // Verify user is a member of the team
            let is_member = db_teams::is_member(&state.db_pool, uuid, user.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to check team membership: {}", e),
                    )
                })?;

            if !is_member {
                return Err((
                    StatusCode::FORBIDDEN,
                    "You are not a member of this team".to_string(),
                ));
            }

            (None, Some(uuid))
        }
    };

    tracing::info!(
        "Creating project '{}' for user {}",
        payload.name,
        user.email
    );

    let project = projects::create(
        &state.db_pool,
        &payload.name,
        crate::db::models::ProjectStatus::Stopped,
        crate::db::models::ProjectVisibility::from(payload.visibility),
        owner_user_id,
        owner_team_id,
    )
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
            (
                StatusCode::CONFLICT,
                format!("Project '{}' already exists", payload.name),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create project: {}", e),
            )
        }
    })?;

    Ok(Json(CreateProjectResponse {
        project: convert_project(project),
    }))
}

pub async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Json<Vec<ApiProject>>, (StatusCode, String)> {
    let projects = projects::list_accessible_by_user(&state.db_pool, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list projects: {}", e),
            )
        })?;

    // Batch fetch deployment URLs and active deployment info for efficiency
    let project_ids: Vec<uuid::Uuid> = projects.iter().map(|p| p.id).collect();
    let deployment_urls = projects::get_deployment_urls_batch(&state.db_pool, &project_ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get deployment URLs: {}", e),
            )
        })?;

    let active_deployment_info =
        projects::get_active_deployment_info_batch(&state.db_pool, &project_ids)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get active deployment info: {}", e),
                )
            })?;

    // Batch fetch owner information (user emails and team names)
    let user_ids: Vec<Uuid> = projects.iter().filter_map(|p| p.owner_user_id).collect();
    let team_ids: Vec<Uuid> = projects.iter().filter_map(|p| p.owner_team_id).collect();

    let user_emails = if !user_ids.is_empty() {
        db_users::get_emails_batch(&state.db_pool, &user_ids)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get user emails: {}", e),
                )
            })?
    } else {
        std::collections::HashMap::new()
    };

    let team_names = if !team_ids.is_empty() {
        db_teams::get_names_batch(&state.db_pool, &team_ids)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get team names: {}", e),
                )
            })?
    } else {
        std::collections::HashMap::new()
    };

    let api_projects = projects
        .into_iter()
        .map(|project| {
            let deployment_url = deployment_urls.get(&project.id).and_then(|u| u.clone());
            let (active_deployment_id, active_deployment_status) = active_deployment_info
                .get(&project.id)
                .and_then(|info| info.as_ref())
                .map(|info| {
                    (
                        Some(info.deployment_id.clone()),
                        Some(info.status.to_string()),
                    )
                })
                .unwrap_or((None, None));

            let owner_user_email = project
                .owner_user_id
                .and_then(|id| user_emails.get(&id).cloned());
            let owner_team_name = project
                .owner_team_id
                .and_then(|id| team_names.get(&id).cloned());

            ApiProject {
                id: project.id.to_string(),
                created: project.created_at.to_rfc3339(),
                updated: project.updated_at.to_rfc3339(),
                name: project.name,
                status: ProjectStatus::from(project.status),
                visibility: ProjectVisibility::from(project.visibility),
                owner_user: project.owner_user_id.map(|id| id.to_string()),
                owner_team: project.owner_team_id.map(|id| id.to_string()),
                owner_user_email,
                owner_team_name,
                active_deployment_id,
                active_deployment_status,
                deployment_url,
                project_url: project.project_url,
            }
        })
        .collect();

    Ok(Json(api_projects))
}

pub async fn get_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ProjectErrorResponse>)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &id_or_name, params.by_id).await?;

    // Check read permission
    let can_read = check_read_permission(&state, &project, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to check permissions: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if !can_read {
        // Use 404 to hide project existence from unauthorized users
        return Err((
            StatusCode::NOT_FOUND,
            Json(ProjectErrorResponse {
                error: format!("Project '{}' not found", id_or_name),
                suggestions: None,
            }),
        ));
    }

    // Get deployment URL
    let deployment_url = projects::get_deployment_url(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to get deployment URL: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    // Check if we should expand owner information
    if params.should_expand("owner") {
        let mut expanded = expand_project_with_owner(&state, project)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ProjectErrorResponse {
                        error: format!("Failed to expand project data: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        expanded.deployment_url = deployment_url;
        Ok(Json(serde_json::to_value(expanded).unwrap()))
    } else {
        let mut api_project = convert_project(project);
        api_project.deployment_url = deployment_url;
        Ok(Json(serde_json::to_value(api_project).unwrap()))
    }
}

pub async fn update_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<Json<UpdateProjectResponse>, (StatusCode, Json<ProjectErrorResponse>)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &id_or_name, params.by_id).await?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to check permissions: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ProjectErrorResponse {
                error: "You do not have permission to update this project".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Update project fields
    let updated_project = if let Some(status) = payload.status {
        projects::update_status(
            &state.db_pool,
            project.id,
            crate::db::models::ProjectStatus::from(status),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to update project: {}", e),
                    suggestions: None,
                }),
            )
        })?
    } else {
        project
    };

    Ok(Json(UpdateProjectResponse {
        project: convert_project(updated_project),
    }))
}

pub async fn delete_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
) -> Result<StatusCode, (StatusCode, Json<ProjectErrorResponse>)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &id_or_name, params.by_id).await?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to check permissions: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ProjectErrorResponse {
                error: "You do not have permission to delete this project".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Check if already deleting
    if project.status == crate::db::models::ProjectStatus::Deleting {
        return Ok(StatusCode::ACCEPTED);
    }

    // Mark project as deleting
    projects::mark_deleting(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to mark project for deletion: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    tracing::info!("Project {} marked for deletion", project.name);

    // Return 202 Accepted - deletion is asynchronous
    Ok(StatusCode::ACCEPTED)
}

/// Query project by ID
async fn query_project_by_id(
    state: &AppState,
    project_id: &str,
) -> Result<crate::db::models::Project, String> {
    let uuid = Uuid::parse_str(project_id).map_err(|e| format!("Invalid project ID: {}", e))?;

    projects::find_by_id(&state.db_pool, uuid)
        .await
        .map_err(|e| format!("Project not found: {}", e))?
        .ok_or_else(|| "Project not found".to_string())
}

/// Query project by name
async fn query_project_by_name(
    state: &AppState,
    project_name: &str,
) -> Result<crate::db::models::Project, String> {
    tracing::info!("Querying project by name: {}", project_name);

    projects::find_by_name(&state.db_pool, project_name)
        .await
        .map_err(|e| format!("Failed to query project by name: {}", e))?
        .ok_or_else(|| format!("Project '{}' not found", project_name))
}

/// Expand project with owner information
async fn expand_project_with_owner(
    state: &AppState,
    project: crate::db::models::Project,
) -> Result<ProjectWithOwnerInfo, String> {
    let owner_info = if let Some(user_id) = project.owner_user_id {
        // Fetch user information
        let user = db_users::find_by_id(&state.db_pool, user_id)
            .await
            .map_err(|e| format!("Failed to fetch user: {}", e))?
            .ok_or_else(|| "User not found".to_string())?;

        Some(OwnerInfo::User(UserInfo {
            id: user.id.to_string(),
            email: user.email,
        }))
    } else if let Some(team_id) = project.owner_team_id {
        // Fetch team information
        let team = db_teams::find_by_id(&state.db_pool, team_id)
            .await
            .map_err(|e| format!("Failed to fetch team: {}", e))?
            .ok_or_else(|| "Team not found".to_string())?;

        Some(OwnerInfo::Team(TeamInfo {
            id: team.id.to_string(),
            name: team.name,
        }))
    } else {
        None
    };

    Ok(ProjectWithOwnerInfo {
        id: project.id.to_string(),
        name: project.name,
        status: ProjectStatus::from(project.status),
        visibility: ProjectVisibility::from(project.visibility),
        owner: owner_info,
        active_deployment_id: project.active_deployment_id.map(|id| id.to_string()),
        deployment_url: None, // Will be populated by caller
        project_url: project.project_url,
        created: project.created_at.to_rfc3339(),
        updated: project.updated_at.to_rfc3339(),
    })
}

/// Resolve project by ID or name with fuzzy matching support
async fn resolve_project(
    state: &AppState,
    id_or_name: &str,
    by_id: bool,
) -> Result<crate::db::models::Project, (StatusCode, Json<ProjectErrorResponse>)> {
    tracing::info!("Resolving project '{}', by_id={}", id_or_name, by_id);

    let project = if by_id {
        // Explicit ID lookup
        tracing::info!("Using explicit ID lookup");
        query_project_by_id(state, id_or_name).await.map_err(|e| {
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
        match query_project_by_name(state, id_or_name).await {
            Ok(project) => project,
            Err(e) => {
                tracing::info!("Name lookup failed: {}, trying ID fallback", e);
                query_project_by_id(state, id_or_name).await.map_err(|_e| {
                    tracing::info!("Both lookups failed, generating fuzzy suggestions");
                    // Both failed - provide fuzzy suggestions
                    let all_projects = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(projects::list(&state.db_pool, None))
                    });

                    let suggestions = match all_projects {
                        Ok(all_projects) => {
                            let api_projects: Vec<ApiProject> =
                                all_projects.into_iter().map(convert_project).collect();
                            let similar = find_similar_projects(id_or_name, &api_projects, 0.85);
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
            }
        }
    };

    Ok(project)
}

/// Convert database Project model to API Project model
fn convert_project(project: crate::db::models::Project) -> ApiProject {
    ApiProject {
        id: project.id.to_string(),
        created: project.created_at.to_rfc3339(),
        updated: project.updated_at.to_rfc3339(),
        name: project.name,
        status: ProjectStatus::from(project.status),
        visibility: ProjectVisibility::from(project.visibility),
        owner_user: project.owner_user_id.map(|id| id.to_string()),
        owner_team: project.owner_team_id.map(|id| id.to_string()),
        owner_user_email: None, // Will be populated by caller if needed
        owner_team_name: None,  // Will be populated by caller if needed
        active_deployment_id: project.active_deployment_id.map(|id| id.to_string()),
        active_deployment_status: None, // Will be populated by caller if needed
        deployment_url: None,           // Will be populated by caller
        project_url: project.project_url,
    }
}

/// Check if user can read a project (owner or team member)
async fn check_read_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user_id: Uuid,
) -> Result<bool, String> {
    // Check ownership or team membership
    projects::user_can_access(&state.db_pool, project.id, user_id)
        .await
        .map_err(|e| format!("Failed to check access: {}", e))
}

/// Check if user can write to a project (owner or team member)
async fn check_write_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user_id: Uuid,
) -> Result<bool, String> {
    // Write access requires ownership (user or team member)
    projects::user_can_access(&state.db_pool, project.id, user_id)
        .await
        .map_err(|e| format!("Failed to check access: {}", e))
}
