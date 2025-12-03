use sqlx::PgPool;
use uuid::Uuid;
use anyhow::{Result, Context};

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
            owner_user_id, owner_team_id,
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
            owner_user_id, owner_team_id,
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
