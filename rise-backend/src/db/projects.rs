use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::models::{Deployment, Project, ProjectStatus, ProjectVisibility};

/// List all projects (optionally filtered by owner)
pub async fn list(pool: &PgPool, owner_user_id: Option<Uuid>) -> Result<Vec<Project>> {
    let projects = if let Some(user_id) = owner_user_id {
        sqlx::query_as!(
            Project,
            r#"
            SELECT
                id, name,
                status as "status: ProjectStatus",
                visibility as "visibility: ProjectVisibility",
                owner_user_id, owner_team_id, active_deployment_id,
            project_url,
                created_at, updated_at
            FROM projects
            WHERE owner_user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as!(
            Project,
            r#"
            SELECT
                id, name,
                status as "status: ProjectStatus",
                visibility as "visibility: ProjectVisibility",
                owner_user_id, owner_team_id, active_deployment_id,
            project_url,
                created_at, updated_at
            FROM projects
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(pool)
        .await?
    };

    Ok(projects)
}

/// Find project by name
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<Project>> {
    let project = sqlx::query_as!(
        Project,
        r#"
        SELECT
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        FROM projects
        WHERE name = $1
        "#,
        name
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find project by name")?;

    Ok(project)
}

/// Find project by ID
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Project>> {
    let project = sqlx::query_as!(
        Project,
        r#"
        SELECT
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        FROM projects
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find project by ID")?;

    Ok(project)
}

/// Create a new project
pub async fn create(
    pool: &PgPool,
    name: &str,
    status: ProjectStatus,
    visibility: ProjectVisibility,
    owner_user_id: Option<Uuid>,
    owner_team_id: Option<Uuid>,
) -> Result<Project> {
    let status_str = status.to_string();
    let visibility_str = visibility.to_string();

    let project = sqlx::query_as!(
        Project,
        r#"
        INSERT INTO projects (name, status, visibility, owner_user_id, owner_team_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        name,
        status_str,
        visibility_str,
        owner_user_id,
        owner_team_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to create project")?;

    Ok(project)
}

/// Update project status
pub async fn update_status(pool: &PgPool, id: Uuid, status: ProjectStatus) -> Result<Project> {
    let status_str = status.to_string();

    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET status = $2
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        id,
        status_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to update project status")?;

    Ok(project)
}

/// Update project visibility
pub async fn update_visibility(
    pool: &PgPool,
    id: Uuid,
    visibility: ProjectVisibility,
) -> Result<Project> {
    let visibility_str = visibility.to_string();

    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET visibility = $2
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        id,
        visibility_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to update project visibility")?;

    Ok(project)
}

/// Delete project by ID
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!("DELETE FROM projects WHERE id = $1", id)
        .execute(pool)
        .await
        .context("Failed to delete project")?;

    Ok(())
}

/// Check if user can access project (directly or via team)
pub async fn user_can_access(pool: &PgPool, project_id: Uuid, user_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM projects p
            WHERE p.id = $1 AND (
                p.owner_user_id = $2
                OR EXISTS(
                    SELECT 1 FROM team_members tm
                    WHERE tm.team_id = p.owner_team_id
                    AND tm.user_id = $2
                )
            )
        ) as "exists!"
        "#,
        project_id,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to check project access")?;

    Ok(result.exists)
}

/// Set the active deployment for a project
pub async fn set_active_deployment(
    pool: &PgPool,
    project_id: Uuid,
    deployment_id: Uuid,
) -> Result<Project> {
    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET active_deployment_id = $2
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id, active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        project_id,
        deployment_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to set active deployment")?;

    Ok(project)
}

/// Calculate and update project status based on active deployment and last deployment
pub async fn update_calculated_status(pool: &PgPool, project_id: Uuid) -> Result<Project> {
    use crate::db::models::DeploymentStatus;

    // Get current project to check if it's in a protected lifecycle state
    let project = find_by_id(pool, project_id)
        .await?
        .context("Project not found")?;

    // Don't recalculate status for projects in deletion lifecycle
    // The deletion controller manages transitions for these states
    if matches!(
        project.status,
        ProjectStatus::Deleting | ProjectStatus::Terminated
    ) {
        return Ok(project);
    }

    // Get last deployment
    let last_deployment = sqlx::query!(
        r#"
        SELECT id, status as "status: DeploymentStatus"
        FROM deployments
        WHERE project_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get last deployment")?;

    // Determine status based on active deployment and last deployment
    let status = if let Some(active_id) = project.active_deployment_id {
        // Get active deployment to check its status
        let active_deployment =
            sqlx::query_as::<_, Deployment>("SELECT * FROM deployments WHERE id = $1")
                .bind(active_id)
                .fetch_optional(pool)
                .await?;

        match active_deployment {
            Some(deployment) => match deployment.status {
                DeploymentStatus::Healthy => ProjectStatus::Running,
                DeploymentStatus::Unhealthy => ProjectStatus::Failed,
                // Active deployment in transition - should not happen normally
                _ => ProjectStatus::Failed,
            },
            None => ProjectStatus::Stopped, // Active deployment was deleted
        }
    } else if let Some(last) = last_deployment {
        // No active deployment, check last deployment status
        match last.status {
            DeploymentStatus::Failed => ProjectStatus::Failed,

            // In-progress states
            DeploymentStatus::Pending
            | DeploymentStatus::Building
            | DeploymentStatus::Pushing
            | DeploymentStatus::Pushed
            | DeploymentStatus::Deploying => ProjectStatus::Deploying,

            // Cancellation/Termination in progress
            DeploymentStatus::Cancelling | DeploymentStatus::Terminating => {
                ProjectStatus::Deploying
            }

            // Terminal states (no active deployment)
            DeploymentStatus::Cancelled
            | DeploymentStatus::Stopped
            | DeploymentStatus::Superseded => ProjectStatus::Stopped,

            // Running states without being active (shouldn't happen)
            DeploymentStatus::Healthy | DeploymentStatus::Unhealthy => {
                // This shouldn't happen (running deployment should be active)
                // but treat as Running anyway
                ProjectStatus::Running
            }
        }
    } else {
        // No deployments at all -> Stopped
        ProjectStatus::Stopped
    };

    update_status(pool, project_id, status).await
}

/// Get deployment URL for a project
/// Returns URL from active deployment if exists, otherwise from most recent deployment
pub async fn get_deployment_url(pool: &PgPool, project_id: Uuid) -> Result<Option<String>> {
    let result = sqlx::query!(
        r#"
        SELECT d.deployment_url
        FROM projects p
        LEFT JOIN deployments d ON (
            CASE
                WHEN p.active_deployment_id IS NOT NULL
                THEN d.id = p.active_deployment_id
                ELSE d.id = (
                    SELECT id FROM deployments
                    WHERE project_id = p.id
                    ORDER BY created_at DESC
                    LIMIT 1
                )
            END
        )
        WHERE p.id = $1
        LIMIT 1
        "#,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get deployment URL")?;

    Ok(result.and_then(|r| r.deployment_url))
}

/// Get deployment URLs for multiple projects (batch operation)
/// Returns a map of project_id -> deployment_url
pub async fn get_deployment_urls_batch(
    pool: &PgPool,
    project_ids: &[Uuid],
) -> Result<HashMap<Uuid, Option<String>>> {
    let results = sqlx::query!(
        r#"
        WITH latest_deployments AS (
            SELECT DISTINCT ON (project_id)
                project_id,
                id,
                deployment_url,
                created_at
            FROM deployments
            WHERE project_id = ANY($1)
            ORDER BY project_id, created_at DESC
        )
        SELECT
            p.id as project_id,
            COALESCE(
                active_d.deployment_url,
                latest_d.deployment_url
            ) as deployment_url
        FROM unnest($1::uuid[]) AS p(id)
        LEFT JOIN deployments active_d ON active_d.id = (
            SELECT active_deployment_id FROM projects WHERE id = p.id
        )
        LEFT JOIN latest_deployments latest_d ON latest_d.project_id = p.id
        "#,
        project_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to get deployment URLs in batch")?;

    Ok(results
        .into_iter()
        .filter_map(|r| r.project_id.map(|id| (id, r.deployment_url)))
        .collect())
}

/// Active deployment info returned by batch queries
#[derive(Debug, Clone)]
pub struct ActiveDeploymentInfo {
    pub deployment_id: String,
    pub status: crate::db::models::DeploymentStatus,
}

/// Get active deployment IDs (deployment_id strings) for multiple projects (batch operation)
/// Returns a map of project_id -> deployment_id (the string identifier, not UUID)
pub async fn get_active_deployment_ids_batch(
    pool: &PgPool,
    project_ids: &[Uuid],
) -> Result<HashMap<Uuid, Option<String>>> {
    let results = sqlx::query!(
        r#"
        SELECT
            p.id as project_id,
            d.deployment_id
        FROM unnest($1::uuid[]) AS p(id)
        LEFT JOIN deployments d ON d.id = (
            SELECT active_deployment_id FROM projects WHERE id = p.id
        )
        "#,
        project_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to get active deployment IDs in batch")?;

    Ok(results
        .into_iter()
        .filter_map(|r| r.project_id.map(|id| (id, r.deployment_id)))
        .collect())
}

/// Get active deployment info (deployment_id and status) for multiple projects (batch operation)
/// Returns a map of project_id -> ActiveDeploymentInfo
pub async fn get_active_deployment_info_batch(
    pool: &PgPool,
    project_ids: &[Uuid],
) -> Result<HashMap<Uuid, Option<ActiveDeploymentInfo>>> {
    let results = sqlx::query!(
        r#"
        SELECT
            p.id as project_id,
            d.deployment_id,
            d.status as "status: crate::db::models::DeploymentStatus"
        FROM unnest($1::uuid[]) AS p(id)
        LEFT JOIN deployments d ON d.id = (
            SELECT active_deployment_id FROM projects WHERE id = p.id
        )
        "#,
        project_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to get active deployment info in batch")?;

    Ok(results
        .into_iter()
        .filter_map(|r| {
            r.project_id.map(|id| {
                let info = if let (Some(deployment_id), Some(status)) = (r.deployment_id, r.status)
                {
                    Some(ActiveDeploymentInfo {
                        deployment_id,
                        status,
                    })
                } else {
                    None
                };
                (id, info)
            })
        })
        .collect())
}

/// Mark project as deleting
pub async fn mark_deleting(pool: &PgPool, id: Uuid) -> Result<Project> {
    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET status = 'Deleting', updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark project as deleting")?;

    Ok(project)
}

/// Find all projects in Deleting status
pub async fn find_deleting(pool: &PgPool, limit: i64) -> Result<Vec<Project>> {
    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            active_deployment_id,
            project_url,
            created_at, updated_at
        FROM projects
        WHERE status = 'Deleting'
        ORDER BY updated_at ASC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to find deleting projects")?;

    Ok(projects)
}

/// Update project URL
pub async fn update_project_url(pool: &PgPool, project_id: Uuid, url: &str) -> Result<Project> {
    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET project_url = $2, updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            active_deployment_id,
            project_url,
            created_at, updated_at
        "#,
        project_id,
        url
    )
    .fetch_one(pool)
    .await
    .context("Failed to update project URL")?;

    Ok(project)
}
