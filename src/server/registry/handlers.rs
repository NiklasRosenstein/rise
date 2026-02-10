use axum::{
    extract::{Extension, Query, State},
    Json,
};

use super::models::{GetRegistryCredsRequest, GetRegistryCredsResponse};
use crate::db::models::User;
use crate::db::{projects, teams as db_teams};
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::state::AppState;
use uuid::Uuid;

/// Check if user has permission to deploy to the project
async fn check_deploy_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user_id: Uuid,
) -> Result<(), String> {
    // If project is owned by the user directly, allow
    if let Some(owner_user_id) = project.owner_user_id {
        if owner_user_id == user_id {
            return Ok(());
        }
    }

    // If project is owned by a team, check if user is an owner of that team
    if let Some(team_id) = project.owner_team_id {
        let is_owner = db_teams::is_owner(&state.db_pool, team_id, user_id)
            .await
            .map_err(|e| format!("Failed to check team ownership: {}", e))?;

        if is_owner {
            return Ok(());
        }

        let team = db_teams::find_by_id(&state.db_pool, team_id)
            .await
            .map_err(|e| format!("Failed to fetch team: {}", e))?
            .ok_or_else(|| "Team not found".to_string())?;

        return Err(format!(
            "You must be an owner of team '{}' to deploy to this project",
            team.name
        ));
    }

    Err("You do not have permission to deploy to this project".to_string())
}

/// Get registry credentials for a project
pub async fn get_registry_credentials(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(params): Query<GetRegistryCredsRequest>,
) -> Result<Json<GetRegistryCredsResponse>, ServerError> {
    // Query project by name
    let project = projects::find_by_name(&state.db_pool, &params.project)
        .await
        .internal_err("Failed to query project")
        .map_err(|e| e.with_context("project_name", &params.project))?
        .ok_or_else(|| ServerError::not_found(format!("Project '{}' not found", params.project)))?;

    // Check if user has permission to deploy to this project
    check_deploy_permission(&state, &project, user.id)
        .await
        .map_err(ServerError::forbidden)?;

    // Get credentials from the registry provider
    // The repository name is typically the project name
    let repository = params.project.clone();

    let credentials = state
        .registry_provider
        .get_credentials(&repository)
        .await
        .internal_err("Failed to get registry credentials")
        .map_err(|e| {
            e.with_context("project_name", &params.project)
                .with_context("repository", &repository)
        })?;

    Ok(Json(GetRegistryCredsResponse {
        credentials,
        repository,
    }))
}
