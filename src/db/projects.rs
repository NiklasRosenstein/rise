use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::deployments;
use crate::db::models::{Project, ProjectStatus, ProjectVisibility};

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
                owner_user_id, owner_team_id,
                COALESCE(finalizers, '{}') as "finalizers!",
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
                owner_user_id, owner_team_id,
                COALESCE(finalizers, '{}') as "finalizers!",
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

/// List all projects accessible by a user (owned directly, via team membership, or via service account)
pub async fn list_accessible_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Project>> {
    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT DISTINCT
            p.id, p.name,
            p.status as "status: ProjectStatus",
            p.visibility as "visibility: ProjectVisibility",
            p.owner_user_id, p.owner_team_id,
            COALESCE(p.finalizers, '{}') as "finalizers!",
            p.created_at, p.updated_at
        FROM projects p
        WHERE
            p.owner_user_id = $1
            OR EXISTS(
                SELECT 1 FROM team_members tm
                WHERE tm.team_id = p.owner_team_id
                AND tm.user_id = $1
            )
            OR EXISTS(
                SELECT 1 FROM service_accounts sa
                WHERE sa.project_id = p.id
                AND sa.user_id = $1
                AND sa.deleted_at IS NULL
            )
        ORDER BY p.created_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list accessible projects")?;

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
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
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
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
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
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
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
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
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
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
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

/// Update project owner (either user or team, mutually exclusive)
pub async fn update_owner(
    pool: &PgPool,
    id: Uuid,
    owner_user_id: Option<Uuid>,
    owner_team_id: Option<Uuid>,
) -> Result<Project> {
    let project = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET owner_user_id = $2, owner_team_id = $3
        WHERE id = $1
        RETURNING
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
            created_at, updated_at
        "#,
        id,
        owner_user_id,
        owner_team_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to update project owner")?;

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

    // Get last deployment from the "default" group only
    // Other deployment groups (e.g., for merge requests) don't affect project status
    let last_deployment = deployments::find_last_for_project_and_group(
        pool,
        project_id,
        crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP,
    )
    .await?;

    // Determine status based on active deployment (using is_active flag)
    // or last deployment if no active deployment exists
    let status = if let Some(last) = last_deployment.as_ref() {
        // Check if this deployment is marked as active
        if last.is_active {
            // Active deployment determines project status
            match last.status {
                DeploymentStatus::Healthy => ProjectStatus::Running,
                DeploymentStatus::Unhealthy => ProjectStatus::Failed,
                // Termination/cancellation in progress - show as Deploying (transitional)
                DeploymentStatus::Terminating | DeploymentStatus::Cancelling => {
                    ProjectStatus::Deploying
                }
                // Other in-progress states
                DeploymentStatus::Pending
                | DeploymentStatus::Building
                | DeploymentStatus::Pushing
                | DeploymentStatus::Pushed
                | DeploymentStatus::Deploying => ProjectStatus::Deploying,
                // Terminal states shouldn't be active, but handle gracefully
                DeploymentStatus::Stopped
                | DeploymentStatus::Cancelled
                | DeploymentStatus::Superseded
                | DeploymentStatus::Failed
                | DeploymentStatus::Expired => ProjectStatus::Stopped,
            }
        } else {
            // Not active deployment - use last deployment status
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
                | DeploymentStatus::Superseded
                | DeploymentStatus::Expired => ProjectStatus::Stopped,

                // Running states without being active (shouldn't happen)
                DeploymentStatus::Healthy | DeploymentStatus::Unhealthy => ProjectStatus::Stopped,
            }
        }
    } else {
        // No deployments in default group at all
        ProjectStatus::Stopped
    };

    update_status(pool, project_id, status).await
}

/// Active deployment info returned by batch queries
#[derive(Debug, Clone)]
pub struct ActiveDeploymentInfo {
    pub id: Uuid,
    pub status: crate::db::models::DeploymentStatus,
}

/// Get active deployment info (deployment_id and status) for multiple projects (batch operation)
/// Returns a map of project_id -> ActiveDeploymentInfo
/// Fetches the active deployment from the default deployment group using the is_active flag
pub async fn get_active_deployment_info_batch(
    pool: &PgPool,
    project_ids: &[Uuid],
) -> Result<HashMap<Uuid, Option<ActiveDeploymentInfo>>> {
    let results = sqlx::query!(
        r#"
        SELECT
            p.id as project_id,
            d.id as "id?",
            d.status as "status?: crate::db::models::DeploymentStatus"
        FROM unnest($1::uuid[]) AS p(id)
        LEFT JOIN deployments d ON d.project_id = p.id
            AND d.is_active = TRUE
            AND d.deployment_group = 'default'
        "#,
        project_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to get active deployment info in batch")?;

    Ok(results
        .into_iter()
        .filter_map(|r| {
            r.project_id.map(|project_id| {
                // In sqlx 0.8, LEFT JOIN makes fields Option<T> (already nullable)
                let info = match (r.id, r.status) {
                    (Some(id), Some(status)) => Some(ActiveDeploymentInfo { id, status }),
                    _ => None,
                };
                (project_id, info)
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
            COALESCE(finalizers, '{}') as "finalizers!",
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
            COALESCE(finalizers, '{}') as "finalizers!",
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

// ==================== Finalizer Operations ====================

/// Add a finalizer to a project (idempotent - won't add if already exists)
#[cfg(any(feature = "k8s", feature = "aws"))]
pub async fn add_finalizer(pool: &PgPool, id: Uuid, finalizer: &str) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE projects
        SET finalizers = array_append(
            CASE WHEN $2 = ANY(finalizers) THEN finalizers
            ELSE finalizers END,
            CASE WHEN $2 = ANY(finalizers) THEN NULL
            ELSE $2 END
        )
        WHERE id = $1
        "#,
        id,
        finalizer
    )
    .execute(pool)
    .await
    .context("Failed to add finalizer")?;

    Ok(())
}

/// Remove a finalizer from a project
#[cfg(any(feature = "k8s", feature = "aws"))]
pub async fn remove_finalizer(pool: &PgPool, id: Uuid, finalizer: &str) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE projects
        SET finalizers = array_remove(finalizers, $2)
        WHERE id = $1
        "#,
        id,
        finalizer
    )
    .execute(pool)
    .await
    .context("Failed to remove finalizer")?;

    Ok(())
}

/// Find projects in Deleting status that have a specific finalizer
#[cfg(any(feature = "k8s", feature = "aws"))]
pub async fn find_deleting_with_finalizer(
    pool: &PgPool,
    finalizer: &str,
    limit: i64,
) -> Result<Vec<Project>> {
    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
            created_at, updated_at
        FROM projects
        WHERE status = 'Deleting' AND $1 = ANY(finalizers)
        ORDER BY updated_at ASC
        LIMIT $2
        "#,
        finalizer,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to find deleting projects with finalizer")?;

    Ok(projects)
}

/// Check if a project has any finalizers remaining
pub async fn has_finalizers(pool: &PgPool, id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT cardinality(finalizers) > 0 as "has_finalizers!"
        FROM projects
        WHERE id = $1
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to check project finalizers")?;

    Ok(result.has_finalizers)
}

/// List all active projects (not in Deleting or Terminated status)
#[cfg(feature = "aws")]
pub async fn list_active(pool: &PgPool) -> Result<Vec<Project>> {
    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT
            id, name,
            status as "status: ProjectStatus",
            visibility as "visibility: ProjectVisibility",
            owner_user_id, owner_team_id,
            COALESCE(finalizers, '{}') as "finalizers!",
            created_at, updated_at
        FROM projects
        WHERE status NOT IN ('Deleting', 'Terminated')
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to list active projects")?;

    Ok(projects)
}
