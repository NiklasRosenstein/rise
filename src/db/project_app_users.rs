use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Add a user to project's app users (view-only access to deployed app)
pub async fn add_user(pool: &PgPool, project_id: Uuid, user_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO project_app_users (project_id, user_id)
        VALUES ($1, $2)
        ON CONFLICT (project_id, user_id) DO NOTHING
        "#,
        project_id,
        user_id
    )
    .execute(pool)
    .await
    .context("Failed to add app user to project")?;

    Ok(())
}

/// Remove a user from project's app users
pub async fn remove_user(pool: &PgPool, project_id: Uuid, user_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM project_app_users
        WHERE project_id = $1 AND user_id = $2
        "#,
        project_id,
        user_id
    )
    .execute(pool)
    .await
    .context("Failed to remove app user from project")?;

    Ok(())
}

/// Add a team to project's app teams (view-only access to deployed app)
pub async fn add_team(pool: &PgPool, project_id: Uuid, team_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO project_app_teams (project_id, team_id)
        VALUES ($1, $2)
        ON CONFLICT (project_id, team_id) DO NOTHING
        "#,
        project_id,
        team_id
    )
    .execute(pool)
    .await
    .context("Failed to add app team to project")?;

    Ok(())
}

/// Remove a team from project's app teams
pub async fn remove_team(pool: &PgPool, project_id: Uuid, team_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM project_app_teams
        WHERE project_id = $1 AND team_id = $2
        "#,
        project_id,
        team_id
    )
    .execute(pool)
    .await
    .context("Failed to remove app team from project")?;

    Ok(())
}

/// List user IDs who are app users for a project
pub async fn list_users(pool: &PgPool, project_id: Uuid) -> Result<Vec<Uuid>> {
    let records = sqlx::query!(
        r#"
        SELECT user_id
        FROM project_app_users
        WHERE project_id = $1
        ORDER BY created_at ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list app users")?;

    Ok(records.into_iter().map(|r| r.user_id).collect())
}

/// List team IDs that are app teams for a project
pub async fn list_teams(pool: &PgPool, project_id: Uuid) -> Result<Vec<Uuid>> {
    let records = sqlx::query!(
        r#"
        SELECT team_id
        FROM project_app_teams
        WHERE project_id = $1
        ORDER BY created_at ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list app teams")?;

    Ok(records.into_iter().map(|r| r.team_id).collect())
}

/// Check if a user can access the deployed application (via app users or app teams)
/// This is for ingress auth only - it does NOT grant project management permissions
pub async fn user_can_access_app(pool: &PgPool, project_id: Uuid, user_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            -- Direct app user
            SELECT 1 FROM project_app_users pau
            WHERE pau.project_id = $1 AND pau.user_id = $2
            
            UNION
            
            -- Team member of an app team
            SELECT 1 FROM project_app_teams pat
            INNER JOIN team_members tm ON tm.team_id = pat.team_id
            WHERE pat.project_id = $1 AND tm.user_id = $2
        ) as "exists!"
        "#,
        project_id,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to check app user access")?;

    Ok(result.exists)
}
