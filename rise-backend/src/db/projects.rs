use sqlx::PgPool;
use uuid::Uuid;
use anyhow::{Result, Context};

use crate::db::models::{Deployment, DeploymentStatus, Project, ProjectStatus, ProjectVisibility};

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
pub async fn update_visibility(pool: &PgPool, id: Uuid, visibility: ProjectVisibility) -> Result<Project> {
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
    sqlx::query!(
        "DELETE FROM projects WHERE id = $1",
        id
    )
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
pub async fn set_active_deployment(pool: &PgPool, project_id: Uuid, deployment_id: Uuid) -> Result<Project> {
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

    // Get project to check active deployment
    let project = find_by_id(pool, project_id)
        .await?
        .context("Project not found")?;

    // Determine status based on active deployment and last deployment
    let status = if let Some(active_id) = project.active_deployment_id {
        // Get active deployment to check its status
        let active_deployment = sqlx::query_as::<_, Deployment>(
            "SELECT * FROM deployments WHERE id = $1"
        )
        .bind(active_id)
        .fetch_optional(pool)
        .await?;

        match active_deployment {
            Some(deployment) => match deployment.status {
                DeploymentStatus::Healthy => ProjectStatus::Running,
                DeploymentStatus::Unhealthy => ProjectStatus::Failed,
                // Active deployment in transition - should not happen normally
                _ => ProjectStatus::Failed
            },
            None => ProjectStatus::Stopped, // Active deployment was deleted
        }
    } else if let Some(last) = last_deployment {
        // No active deployment, check last deployment status
        match last.status {
            DeploymentStatus::Failed => ProjectStatus::Failed,

            // In-progress states
            DeploymentStatus::Pending | DeploymentStatus::Building |
            DeploymentStatus::Pushing | DeploymentStatus::Pushed |
            DeploymentStatus::Deploying => ProjectStatus::Deploying,

            // Cancellation/Termination in progress
            DeploymentStatus::Cancelling | DeploymentStatus::Terminating => ProjectStatus::Deploying,

            // Terminal states (no active deployment)
            DeploymentStatus::Cancelled | DeploymentStatus::Stopped |
            DeploymentStatus::Superseded => ProjectStatus::Stopped,

            // Running states without being active (shouldn't happen)
            DeploymentStatus::Healthy | DeploymentStatus::Unhealthy |
            DeploymentStatus::Completed => {
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
