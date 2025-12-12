use super::fuzzy::find_similar_teams;
use super::models::{
    CreateTeamRequest, CreateTeamResponse, GetTeamParams, Team as ApiTeam, TeamErrorResponse,
    TeamWithEmails, UpdateTeamRequest, UpdateTeamResponse, UserInfo,
};
use crate::server::db::models::{TeamRole, User};
use crate::server::db::{service_accounts, teams as db_teams};
use crate::server::state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

pub async fn create_team(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateTeamRequest>,
) -> Result<Json<CreateTeamResponse>, (StatusCode, String)> {
    tracing::info!("Creating team '{}' for user {}", payload.name, user.email);

    // Validate that at least one owner is specified
    if payload.owners.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one owner must be specified".to_string(),
        ));
    }

    // Parse owner IDs
    let owner_ids: Vec<Uuid> = payload
        .owners
        .iter()
        .map(|id| Uuid::parse_str(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid owner ID: {}", e)))?;

    // Verify the authenticated user is in the owners list
    if !owner_ids.contains(&user.id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "You must be an owner of the team you create".to_string(),
        ));
    }

    // Parse member IDs
    let member_ids: Vec<Uuid> = payload
        .members
        .iter()
        .map(|id| Uuid::parse_str(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid member ID: {}", e)))?;

    // Create the team
    let team = db_teams::create(&state.db_pool, &payload.name)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key")
                || e.to_string().contains("unique constraint")
            {
                (
                    StatusCode::CONFLICT,
                    format!("Team '{}' already exists", payload.name),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create team: {}", e),
                )
            }
        })?;

    // Add owners
    for owner_id in owner_ids {
        db_teams::add_member(&state.db_pool, team.id, owner_id, TeamRole::Owner)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to add owner: {}", e),
                )
            })?;
    }

    // Add members
    for member_id in member_ids {
        // Skip if already added as owner
        if !payload
            .owners
            .iter()
            .any(|id| Uuid::parse_str(id).ok() == Some(member_id))
        {
            db_teams::add_member(&state.db_pool, team.id, member_id, TeamRole::Member)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to add member: {}", e),
                    )
                })?;
        }
    }

    Ok(Json(CreateTeamResponse {
        team: convert_team(team, payload.members, payload.owners),
    }))
}

pub async fn get_team(
    State(state): State<AppState>,
    Extension(_user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<TeamErrorResponse>)> {
    // Resolve team by ID or name
    let team = resolve_team(&state, &id_or_name, params.by_id).await?;

    // Check if we should expand user emails
    if params.should_expand("members") || params.should_expand("owners") {
        let expanded = expand_team_with_emails(&state, team).await.map_err(|e| {
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
        // Fetch members and owners to build the API response
        let members = db_teams::get_members(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to get team members: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        let owners = db_teams::get_owners(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to get team owners: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        let member_ids: Vec<String> = members.iter().map(|u| u.id.to_string()).collect();
        let owner_ids: Vec<String> = owners.iter().map(|u| u.id.to_string()).collect();

        Ok(Json(
            serde_json::to_value(convert_team(team, member_ids, owner_ids)).unwrap(),
        ))
    }
}

pub async fn update_team(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
    Json(payload): Json<UpdateTeamRequest>,
) -> Result<Json<UpdateTeamResponse>, (StatusCode, Json<TeamErrorResponse>)> {
    // Resolve team by ID or name
    let team = resolve_team(&state, &id_or_name, params.by_id).await?;

    // Check if user is an admin or owner of the team
    let is_admin = state.admin_users.contains(&user.email);
    let is_owner = if !is_admin {
        db_teams::is_owner(&state.db_pool, team.id, user.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to check team ownership: {}", e),
                        suggestions: None,
                    }),
                )
            })?
    } else {
        true // Admins bypass ownership check
    };

    if !is_owner {
        return Err((
            StatusCode::FORBIDDEN,
            Json(TeamErrorResponse {
                error: "You must be an owner of the team to update it".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Check if team is IdP-managed (only admins can modify)
    if team.idp_managed && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(TeamErrorResponse {
                error: "This team is managed by your Identity Provider. Only administrators can modify IdP-managed teams.".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Update name if provided
    let updated_team = if let Some(_name) = payload.name {
        // For now, we don't have an update_name function, we'll need to add it
        // or handle it differently. Let's skip name updates for now.
        team.clone()
    } else {
        team.clone()
    };

    // Update members if provided
    if let Some(new_members) = payload.members {
        let member_ids: Vec<Uuid> = new_members
            .iter()
            .map(|id| Uuid::parse_str(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(TeamErrorResponse {
                        error: format!("Invalid member ID: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        // Get current members
        let current_members = db_teams::get_members(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to get current members: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        let current_member_ids: Vec<Uuid> = current_members.iter().map(|m| m.id).collect();

        // Remove members that are no longer in the list
        for current_member_id in &current_member_ids {
            if !member_ids.contains(current_member_id) {
                db_teams::remove_member(&state.db_pool, team.id, *current_member_id)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(TeamErrorResponse {
                                error: format!("Failed to remove member: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;
            }
        }

        // Validate that none of the new members are service accounts
        for member_id in &member_ids {
            let is_sa = service_accounts::is_service_account(&state.db_pool, *member_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(TeamErrorResponse {
                            error: format!("Failed to check service account status: {}", e),
                            suggestions: None,
                        }),
                    )
                })?;

            if is_sa {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(TeamErrorResponse {
                        error: "Service accounts cannot be team members".to_string(),
                        suggestions: None,
                    }),
                ));
            }
        }

        // Add new members
        for member_id in member_ids {
            if !current_member_ids.contains(&member_id) {
                db_teams::add_member(&state.db_pool, team.id, member_id, TeamRole::Member)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(TeamErrorResponse {
                                error: format!("Failed to add member: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;
            }
        }
    }

    // Update owners if provided
    if let Some(new_owners) = payload.owners {
        let owner_ids: Vec<Uuid> = new_owners
            .iter()
            .map(|id| Uuid::parse_str(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(TeamErrorResponse {
                        error: format!("Invalid owner ID: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        // Get current owners
        let current_owners = db_teams::get_owners(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to get current owners: {}", e),
                        suggestions: None,
                    }),
                )
            })?;

        let current_owner_ids: Vec<Uuid> = current_owners.iter().map(|o| o.id).collect();

        // Remove owners that are no longer in the list
        for current_owner_id in &current_owner_ids {
            if !owner_ids.contains(current_owner_id) {
                db_teams::remove_member(&state.db_pool, team.id, *current_owner_id)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(TeamErrorResponse {
                                error: format!("Failed to remove owner: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;
            }
        }

        // Validate that none of the new owners are service accounts
        for owner_id in &owner_ids {
            let is_sa = service_accounts::is_service_account(&state.db_pool, *owner_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(TeamErrorResponse {
                            error: format!("Failed to check service account status: {}", e),
                            suggestions: None,
                        }),
                    )
                })?;

            if is_sa {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(TeamErrorResponse {
                        error: "Service accounts cannot be team members".to_string(),
                        suggestions: None,
                    }),
                ));
            }
        }

        // Add new owners
        for owner_id in owner_ids {
            if !current_owner_ids.contains(&owner_id) {
                db_teams::add_member(&state.db_pool, team.id, owner_id, TeamRole::Owner)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(TeamErrorResponse {
                                error: format!("Failed to add owner: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;
            } else {
                // Update role if already a member but not an owner
                db_teams::update_member_role(&state.db_pool, team.id, owner_id, TeamRole::Owner)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(TeamErrorResponse {
                                error: format!("Failed to update member role: {}", e),
                                suggestions: None,
                            }),
                        )
                    })?;
            }
        }
    }

    // Fetch updated members and owners
    let members = db_teams::get_members(&state.db_pool, updated_team.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to get team members: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    let owners = db_teams::get_owners(&state.db_pool, updated_team.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TeamErrorResponse {
                    error: format!("Failed to get team owners: {}", e),
                    suggestions: None,
                }),
            )
        })?;

    let member_ids: Vec<String> = members.iter().map(|u| u.id.to_string()).collect();
    let owner_ids: Vec<String> = owners.iter().map(|u| u.id.to_string()).collect();

    Ok(Json(UpdateTeamResponse {
        team: convert_team(updated_team, member_ids, owner_ids),
    }))
}

pub async fn delete_team(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(id_or_name): Path<String>,
    Query(params): Query<GetTeamParams>,
) -> Result<StatusCode, (StatusCode, Json<TeamErrorResponse>)> {
    // Resolve team by ID or name
    let team = resolve_team(&state, &id_or_name, params.by_id).await?;

    // Check if user is an admin or owner of the team
    let is_admin = state.admin_users.contains(&user.email);
    let is_owner = if !is_admin {
        db_teams::is_owner(&state.db_pool, team.id, user.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TeamErrorResponse {
                        error: format!("Failed to check team ownership: {}", e),
                        suggestions: None,
                    }),
                )
            })?
    } else {
        true // Admins bypass ownership check
    };

    if !is_owner {
        return Err((
            StatusCode::FORBIDDEN,
            Json(TeamErrorResponse {
                error: "You must be an owner of the team to delete it".to_string(),
                suggestions: None,
            }),
        ));
    }

    // Check if team is IdP-managed (only admins can delete)
    if team.idp_managed && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(TeamErrorResponse {
                error: "This team is managed by your Identity Provider. Only administrators can delete IdP-managed teams.".to_string(),
                suggestions: None,
            }),
        ));
    }

    db_teams::delete(&state.db_pool, team.id)
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

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_teams(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Json<Vec<ApiTeam>>, (StatusCode, String)> {
    let teams = db_teams::list_for_user(&state.db_pool, user.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list teams: {}", e),
            )
        })?;

    let mut api_teams = Vec::new();

    for team in teams {
        let members = db_teams::get_members(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get team members: {}", e),
                )
            })?;

        let owners = db_teams::get_owners(&state.db_pool, team.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get team owners: {}", e),
                )
            })?;

        let member_ids: Vec<String> = members.iter().map(|u| u.id.to_string()).collect();
        let owner_ids: Vec<String> = owners.iter().map(|u| u.id.to_string()).collect();

        api_teams.push(convert_team(team, member_ids, owner_ids));
    }

    Ok(Json(api_teams))
}

/// Query team by ID
async fn query_team_by_id(
    state: &AppState,
    team_id: &str,
) -> Result<crate::db::models::Team, String> {
    let uuid = Uuid::parse_str(team_id).map_err(|e| format!("Invalid team ID: {}", e))?;

    db_teams::find_by_id(&state.db_pool, uuid)
        .await
        .map_err(|e| format!("Team not found: {}", e))?
        .ok_or_else(|| "Team not found".to_string())
}

/// Query team by name
async fn query_team_by_name(
    state: &AppState,
    team_name: &str,
) -> Result<crate::db::models::Team, String> {
    tracing::info!("Querying team by name: {}", team_name);

    db_teams::find_by_name(&state.db_pool, team_name)
        .await
        .map_err(|e| format!("Failed to query team by name: {}", e))?
        .ok_or_else(|| format!("Team '{}' not found", team_name))
}

/// Expand team with user emails (batch query for efficiency)
async fn expand_team_with_emails(
    state: &AppState,
    team: crate::db::models::Team,
) -> Result<TeamWithEmails, String> {
    let members = db_teams::get_members(&state.db_pool, team.id)
        .await
        .map_err(|e| format!("Failed to get team members: {}", e))?;

    let owners = db_teams::get_owners(&state.db_pool, team.id)
        .await
        .map_err(|e| format!("Failed to get team owners: {}", e))?;

    let member_infos: Vec<UserInfo> = members
        .iter()
        .map(|u| UserInfo {
            id: u.id.to_string(),
            email: u.email.clone(),
        })
        .collect();

    let owner_infos: Vec<UserInfo> = owners
        .iter()
        .map(|u| UserInfo {
            id: u.id.to_string(),
            email: u.email.clone(),
        })
        .collect();

    Ok(TeamWithEmails {
        id: team.id.to_string(),
        name: team.name,
        members: member_infos,
        owners: owner_infos,
        idp_managed: team.idp_managed,
        created: team.created_at.to_rfc3339(),
        updated: team.updated_at.to_rfc3339(),
    })
}

/// Resolve team by ID or name with fuzzy matching support
async fn resolve_team(
    state: &AppState,
    id_or_name: &str,
    by_id: bool,
) -> Result<crate::db::models::Team, (StatusCode, Json<TeamErrorResponse>)> {
    tracing::info!("Resolving team '{}', by_id={}", id_or_name, by_id);

    let team = if by_id {
        // Explicit ID lookup
        tracing::info!("Using explicit ID lookup");
        query_team_by_id(state, id_or_name).await.map_err(|e| {
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
        tracing::info!("Trying name lookup first, will fallback to ID");
        match query_team_by_name(state, id_or_name).await {
            Ok(team) => team,
            Err(e) => {
                tracing::info!("Name lookup failed: {}, trying ID fallback", e);
                query_team_by_id(state, id_or_name).await.map_err(|_e| {
                    tracing::info!("Both lookups failed, generating fuzzy suggestions");
                    // Both failed - provide fuzzy suggestions
                    let all_teams = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(db_teams::list(&state.db_pool))
                    });

                    let suggestions = match all_teams {
                        Ok(all_teams) => {
                            let api_teams: Vec<ApiTeam> = all_teams
                                .into_iter()
                                .map(|t| convert_team(t, vec![], vec![]))
                                .collect();
                            let similar = find_similar_teams(id_or_name, &api_teams, 0.85);
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
            }
        }
    };

    Ok(team)
}

/// Convert database Team model to API Team model
fn convert_team(
    team: crate::db::models::Team,
    members: Vec<String>,
    owners: Vec<String>,
) -> ApiTeam {
    ApiTeam {
        id: team.id.to_string(),
        name: team.name,
        members,
        owners,
        idp_managed: team.idp_managed,
        created: team.created_at.to_rfc3339(),
        updated: team.updated_at.to_rfc3339(),
    }
}
