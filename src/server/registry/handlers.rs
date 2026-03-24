use axum::{
    extract::{Query, State},
    Json,
};

use super::models::{GetRegistryCredsRequest, GetRegistryCredsResponse};
use crate::db::projects;
use crate::server::auth::context::AuthContext;
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::project::handlers::ensure_project_access_or_admin;
use crate::server::state::AppState;

/// Get registry credentials for a project
pub async fn get_registry_credentials(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(params): Query<GetRegistryCredsRequest>,
) -> Result<Json<GetRegistryCredsResponse>, ServerError> {
    // Query project by name
    let project = projects::find_by_name(&state.db_pool, &params.project)
        .await
        .internal_err("Failed to query project")
        .map_err(|e| e.with_context("project_name", &params.project))?
        .ok_or_else(|| ServerError::not_found(format!("Project '{}' not found", params.project)))?;

    // Resolve auth for project scope
    let (user, is_sa) = auth.resolve_for_project(&state.db_pool, &project).await?;

    // Check if user has permission to deploy to this project (SA access already validated)
    if !is_sa {
        ensure_project_access_or_admin(&state, &user, &project).await?;
    }

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
