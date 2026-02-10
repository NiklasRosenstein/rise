use super::fuzzy::find_similar_projects;
use super::models::{
    AccessClassInfo, CreateProjectRequest, CreateProjectResponse, GetProjectParams,
    ListAccessClassesResponse, OwnerInfo, Project as ApiProject, ProjectErrorResponse,
    ProjectOwner, ProjectStatus, ProjectWithOwnerInfo, TeamInfo, UpdateProjectRequest,
    UpdateProjectResponse, UserInfo,
};
use crate::db::models::User;
use crate::db::{projects, service_accounts, teams as db_teams, users as db_users};
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

/// List available access classes for the deployment controller
pub async fn list_access_classes(
    State(state): State<AppState>,
    Extension(_user): Extension<User>,
) -> Result<Json<ListAccessClassesResponse>, ServerError> {
    let access_classes = state
        .access_classes
        .iter()
        .map(|(id, class)| AccessClassInfo {
            id: id.clone(),
            display_name: class.display_name.clone(),
            description: class.description.clone(),
        })
        .collect();

    Ok(Json(ListAccessClassesResponse { access_classes }))
}

pub async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, ServerError> {
    // Validate access_class against configured access classes
    let is_valid_access_class = state.access_classes.contains_key(&payload.access_class);

    if !is_valid_access_class {
        let available = state
            .access_classes
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        return Err(ServerError::bad_request(format!(
            "Invalid access class '{}'. Available: {}",
            payload.access_class, available
        )));
    }

    // Validate owner - exactly one of owner_user or owner_team must be set
    let (owner_user_id, owner_team_id) = match &payload.owner {
        ProjectOwner::User(user_id) => {
            let uuid =
                Uuid::parse_str(user_id).server_err(StatusCode::BAD_REQUEST, "Invalid user ID")?;
            (Some(uuid), None)
        }
        ProjectOwner::Team(team_id) => {
            let uuid =
                Uuid::parse_str(team_id).server_err(StatusCode::BAD_REQUEST, "Invalid team ID")?;

            // Verify user is a member of the team
            let is_member = db_teams::is_member(&state.db_pool, uuid, user.id)
                .await
                .internal_err("Failed to check team membership")
                .map_err(|e| {
                    e.with_context("team_id", uuid.to_string())
                        .with_context("user_id", user.id.to_string())
                })?;

            if !is_member {
                return Err(ServerError::forbidden("You are not a member of this team"));
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
        payload.access_class,
        owner_user_id,
        owner_team_id,
    )
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
            ServerError::new(
                StatusCode::CONFLICT,
                format!("Project '{}' already exists", payload.name),
            )
        } else {
            ServerError::internal_anyhow(e, "Failed to create project")
                .with_context("project_name", &payload.name)
                .with_context("user_email", &user.email)
        }
    })?;

    Ok(Json(CreateProjectResponse {
        project: convert_project(project),
    }))
}

pub async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Json<Vec<ApiProject>>, ServerError> {
    // Admins can see all projects, others only see projects they have access to
    let projects = if is_admin(&state, &user.email) {
        projects::list(&state.db_pool, None)
            .await
            .internal_err("Failed to list projects")?
    } else {
        projects::list_accessible_by_user(&state.db_pool, user.id)
            .await
            .internal_err("Failed to list projects")
            .map_err(|e| e.with_context("user_id", user.id.to_string()))?
    };

    // Batch fetch active deployment info for efficiency
    let project_ids: Vec<uuid::Uuid> = projects.iter().map(|p| p.id).collect();
    let active_deployment_info =
        projects::get_active_deployment_info_batch(&state.db_pool, &project_ids)
            .await
            .internal_err("Failed to get active deployment info")?;

    // Batch fetch active deployments to calculate URLs
    let deployment_ids: Vec<Uuid> = active_deployment_info
        .values()
        .filter_map(|info| info.as_ref().map(|i| i.id))
        .collect();

    let deployments_map = if !deployment_ids.is_empty() {
        crate::db::deployments::get_deployments_batch(&state.db_pool, &deployment_ids)
            .await
            .internal_err("Failed to get deployments")?
    } else {
        std::collections::HashMap::new()
    };

    // Batch fetch owner information (user emails and team names)
    let user_ids: Vec<Uuid> = projects.iter().filter_map(|p| p.owner_user_id).collect();
    let team_ids: Vec<Uuid> = projects.iter().filter_map(|p| p.owner_team_id).collect();

    let user_emails = if !user_ids.is_empty() {
        db_users::get_emails_batch(&state.db_pool, &user_ids)
            .await
            .internal_err("Failed to get user emails")?
    } else {
        std::collections::HashMap::new()
    };

    let team_names = if !team_ids.is_empty() {
        db_teams::get_names_batch(&state.db_pool, &team_ids)
            .await
            .internal_err("Failed to get team names")?
    } else {
        std::collections::HashMap::new()
    };

    // Calculate URLs for all projects
    let mut api_projects = Vec::new();
    for project in projects {
        let (active_deployment_status, primary_url, custom_domain_urls) =
            if let Some(Some(info)) = active_deployment_info.get(&project.id) {
                if let Some(deployment) = deployments_map.get(&info.id) {
                    match state
                        .deployment_backend
                        .get_deployment_urls(deployment, &project)
                        .await
                    {
                        Ok(urls) => (
                            Some(info.status.to_string()),
                            Some(urls.primary_url),
                            urls.custom_domain_urls,
                        ),
                        Err(e) => {
                            return Err(ServerError::internal_anyhow(
                                e,
                                "Failed to calculate deployment URLs",
                            )
                            .with_context("project_name", &project.name));
                        }
                    }
                } else {
                    (Some(info.status.to_string()), None, vec![])
                }
            } else {
                (None, None, vec![])
            };

        let owner_user_email = project
            .owner_user_id
            .and_then(|id| user_emails.get(&id).cloned());
        let owner_team_name = project
            .owner_team_id
            .and_then(|id| team_names.get(&id).cloned());

        api_projects.push(ApiProject {
            id: project.id.to_string(),
            created: project.created_at.to_rfc3339(),
            updated: project.updated_at.to_rfc3339(),
            name: project.name,
            status: ProjectStatus::from(project.status),
            access_class: project.access_class,
            owner_user: project.owner_user_id.map(|id| id.to_string()),
            owner_team: project.owner_team_id.map(|id| id.to_string()),
            owner_user_email,
            owner_team_name,
            active_deployment_status,
            primary_url,
            custom_domain_urls,
            deployment_groups: None, // Not populated in list view for performance
        });
    }

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
    let can_read = check_read_permission(&state, &project, &user)
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

    // Calculate deployment URLs if there's an active deployment
    let (primary_url, custom_domain_urls) =
        match crate::db::deployments::get_active_deployments_for_project(&state.db_pool, project.id)
            .await
        {
            Ok(active_deployments) => {
                // Find the active deployment in the default group
                if let Some(deployment) = active_deployments.iter().find(|d| {
                    d.deployment_group
                        == crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP
                }) {
                    match state
                        .deployment_backend
                        .get_deployment_urls(deployment, &project)
                        .await
                    {
                        Ok(urls) => (Some(urls.primary_url), urls.custom_domain_urls),
                        Err(e) => {
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ProjectErrorResponse {
                                    error: format!("Failed to calculate URLs: {}", e),
                                    suggestions: None,
                                }),
                            ));
                        }
                    }
                } else {
                    (None, vec![])
                }
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ProjectErrorResponse {
                        error: format!("Failed to get active deployments: {}", e),
                        suggestions: None,
                    }),
                ));
            }
        };

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

        expanded.primary_url = primary_url;
        expanded.custom_domain_urls = custom_domain_urls;
        Ok(Json(serde_json::to_value(expanded).unwrap()))
    } else {
        let mut api_project = convert_project(project.clone());
        api_project.primary_url = primary_url;
        api_project.custom_domain_urls = custom_domain_urls;

        // Get active deployment groups
        let deployment_groups =
            crate::db::deployments::get_active_deployment_groups(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ProjectErrorResponse {
                            error: format!("Failed to get deployment groups: {}", e),
                            suggestions: None,
                        }),
                    )
                })?;
        api_project.deployment_groups = if deployment_groups.is_empty() {
            None
        } else {
            Some(deployment_groups)
        };

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
    let can_write = check_write_permission(&state, &project, &user)
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

    // Service accounts cannot update projects
    let is_sa = service_accounts::is_service_account(&state.db_pool, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to check service account status: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if is_sa {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ProjectErrorResponse {
                error: "Service accounts cannot modify projects".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Update project fields
    let mut updated_project = project;

    // Update owner if provided
    if let Some(owner) = payload.owner {
        let (owner_user_id, owner_team_id) = match owner {
            ProjectOwner::User(user_identifier) => {
                // Try to resolve as email first, then as UUID
                let user = if let Ok(uuid) = Uuid::parse_str(&user_identifier) {
                    // Valid UUID - look up by ID
                    db_users::find_by_id(&state.db_pool, uuid)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ProjectErrorResponse {
                                    error: format!("Failed to verify user: {}", e),
                                    suggestions: None,
                                }),
                            )
                        })?
                } else {
                    // Not a UUID - treat as email
                    db_users::find_by_email(&state.db_pool, &user_identifier)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ProjectErrorResponse {
                                    error: format!("Failed to verify user: {}", e),
                                    suggestions: None,
                                }),
                            )
                        })?
                };

                let user = user.ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(ProjectErrorResponse {
                            error: format!("User '{}' not found", user_identifier),
                            suggestions: None,
                        }),
                    )
                })?;

                (Some(user.id), None)
            }
            ProjectOwner::Team(team_identifier) => {
                // Try to resolve as name first, then as UUID
                let team = if let Ok(uuid) = Uuid::parse_str(&team_identifier) {
                    // Valid UUID - look up by ID
                    db_teams::find_by_id(&state.db_pool, uuid)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ProjectErrorResponse {
                                    error: format!("Failed to verify team: {}", e),
                                    suggestions: None,
                                }),
                            )
                        })?
                } else {
                    // Not a UUID - treat as team name
                    db_teams::find_by_name(&state.db_pool, &team_identifier)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ProjectErrorResponse {
                                    error: format!("Failed to verify team: {}", e),
                                    suggestions: None,
                                }),
                            )
                        })?
                };

                let team = team.ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(ProjectErrorResponse {
                            error: format!("Team '{}' not found", team_identifier),
                            suggestions: None,
                        }),
                    )
                })?;

                // Verify the requesting user is a member of the team they're transferring to
                let is_member = db_teams::is_member(&state.db_pool, team.id, user.id)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ProjectErrorResponse {
                                error: format!("Failed to check team membership: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;

                if !is_member {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(ProjectErrorResponse {
                            error: format!(
                                "You must be a member of team '{}' to transfer projects to it",
                                team.name
                            ),
                            suggestions: None,
                        }),
                    ));
                }

                (None, Some(team.id))
            }
        };

        updated_project = projects::update_owner(
            &state.db_pool,
            updated_project.id,
            owner_user_id,
            owner_team_id,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to update project owner: {}", e),
                    suggestions: None,
                }),
            )
        })?;
    }

    // Update access_class if provided
    if let Some(access_class) = payload.access_class {
        // Validate against configured access classes
        let is_valid = state.access_classes.contains_key(&access_class);

        if !is_valid {
            let available = state
                .access_classes
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");

            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProjectErrorResponse {
                    error: format!(
                        "Invalid access class '{}'. Available: {}",
                        access_class, available
                    ),
                    suggestions: None,
                }),
            ));
        }

        updated_project =
            projects::update_access_class(&state.db_pool, updated_project.id, access_class)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ProjectErrorResponse {
                            error: format!("Failed to update project access class: {}", e),
                            suggestions: None,
                        }),
                    )
                })?;
    }

    // Update status if provided
    if let Some(status) = payload.status {
        updated_project = projects::update_status(
            &state.db_pool,
            updated_project.id,
            crate::db::models::ProjectStatus::from(status),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to update project status: {}", e),
                    suggestions: None,
                }),
            )
        })?;
    }

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
    let can_write = check_write_permission(&state, &project, &user)
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

    // Service accounts cannot delete projects
    let is_sa = service_accounts::is_service_account(&state.db_pool, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProjectErrorResponse {
                    error: format!("Failed to check service account status: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    if is_sa {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ProjectErrorResponse {
                error: "Service accounts cannot modify projects".to_string(),
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
        access_class: project.access_class,
        owner: owner_info,
        primary_url: None,          // Will be populated by caller
        custom_domain_urls: vec![], // Will be populated by caller
        finalizers: project.finalizers.clone(),
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
        access_class: project.access_class,
        owner_user: project.owner_user_id.map(|id| id.to_string()),
        owner_team: project.owner_team_id.map(|id| id.to_string()),
        owner_user_email: None, // Will be populated by caller if needed
        owner_team_name: None,  // Will be populated by caller if needed
        active_deployment_status: None, // Will be populated by caller if needed
        primary_url: None,      // Will be populated by caller
        custom_domain_urls: vec![], // Will be populated by caller
        deployment_groups: None, // Will be populated by caller if needed
    }
}

/// Check if a user is an admin (based on email in config)
fn is_admin(state: &AppState, user_email: &str) -> bool {
    state.admin_users.contains(&user_email.to_string())
}

/// Check if user can read a project (owner, team member, or admin)
pub async fn check_read_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user: &User,
) -> Result<bool, String> {
    // Admins have full access
    if is_admin(state, &user.email) {
        return Ok(true);
    }

    // Check ownership or team membership
    projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| format!("Failed to check access: {}", e))
}

/// Check if user can write to a project (owner, team member, or admin)
pub async fn check_write_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user: &User,
) -> Result<bool, String> {
    // Admins have full access
    if is_admin(state, &user.email) {
        return Ok(true);
    }

    // Write access requires ownership (user or team member)
    projects::user_can_access(&state.db_pool, project.id, user.id)
        .await
        .map_err(|e| format!("Failed to check access: {}", e))
}
