use sqlx::PgPool;
use uuid::Uuid;
use anyhow::{Result, Context};

use crate::db::models::{Team, TeamMember, TeamRole, User};

/// List all teams
pub async fn list(pool: &PgPool) -> Result<Vec<Team>> {
    let teams = sqlx::query_as!(
        Team,
        r#"
        SELECT id, name, created_at, updated_at
        FROM teams
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to list teams")?;

    Ok(teams)
}

/// List teams for a specific user
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Team>> {
    let teams = sqlx::query_as!(
        Team,
        r#"
        SELECT t.id, t.name, t.created_at, t.updated_at
        FROM teams t
        INNER JOIN team_members tm ON t.id = tm.team_id
        WHERE tm.user_id = $1
        ORDER BY t.created_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list teams for user")?;

    Ok(teams)
}

/// Find team by name
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<Team>> {
    let team = sqlx::query_as!(
        Team,
        r#"
        SELECT id, name, created_at, updated_at
        FROM teams
        WHERE name = $1
        "#,
        name
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find team by name")?;

    Ok(team)
}

/// Find team by ID
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Team>> {
    let team = sqlx::query_as!(
        Team,
        r#"
        SELECT id, name, created_at, updated_at
        FROM teams
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find team by ID")?;

    Ok(team)
}

/// Create a new team
pub async fn create(pool: &PgPool, name: &str) -> Result<Team> {
    let team = sqlx::query_as!(
        Team,
        r#"
        INSERT INTO teams (name)
        VALUES ($1)
        RETURNING id, name, created_at, updated_at
        "#,
        name
    )
    .fetch_one(pool)
    .await
    .context("Failed to create team")?;

    Ok(team)
}

/// Delete team by ID
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(
        "DELETE FROM teams WHERE id = $1",
        id
    )
    .execute(pool)
    .await
    .context("Failed to delete team")?;

    Ok(())
}

/// Get team members
pub async fn get_members(pool: &PgPool, team_id: Uuid) -> Result<Vec<User>> {
    let members = sqlx::query_as!(
        User,
        r#"
        SELECT u.id, u.email, u.created_at, u.updated_at
        FROM users u
        INNER JOIN team_members tm ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY u.email
        "#,
        team_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to get team members")?;

    Ok(members)
}

/// Get team owners
pub async fn get_owners(pool: &PgPool, team_id: Uuid) -> Result<Vec<User>> {
    let owners = sqlx::query_as!(
        User,
        r#"
        SELECT u.id, u.email, u.created_at, u.updated_at
        FROM users u
        INNER JOIN team_members tm ON u.id = tm.user_id
        WHERE tm.team_id = $1 AND tm.role = 'owner'
        ORDER BY u.email
        "#,
        team_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to get team owners")?;

    Ok(owners)
}

/// Add member to team
pub async fn add_member(pool: &PgPool, team_id: Uuid, user_id: Uuid, role: TeamRole) -> Result<TeamMember> {
    let role_str = role.to_string();

    let member = sqlx::query_as!(
        TeamMember,
        r#"
        INSERT INTO team_members (team_id, user_id, role)
        VALUES ($1, $2, $3)
        RETURNING team_id, user_id, role as "role: TeamRole", created_at
        "#,
        team_id,
        user_id,
        role_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to add team member")?;

    Ok(member)
}

/// Remove member from team
pub async fn remove_member(pool: &PgPool, team_id: Uuid, user_id: Uuid) -> Result<()> {
    sqlx::query!(
        "DELETE FROM team_members WHERE team_id = $1 AND user_id = $2",
        team_id,
        user_id
    )
    .execute(pool)
    .await
    .context("Failed to remove team member")?;

    Ok(())
}

/// Update member role
pub async fn update_member_role(pool: &PgPool, team_id: Uuid, user_id: Uuid, role: TeamRole) -> Result<TeamMember> {
    let role_str = role.to_string();

    let member = sqlx::query_as!(
        TeamMember,
        r#"
        UPDATE team_members
        SET role = $3
        WHERE team_id = $1 AND user_id = $2
        RETURNING team_id, user_id, role as "role: TeamRole", created_at
        "#,
        team_id,
        user_id,
        role_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to update member role")?;

    Ok(member)
}

/// Check if user is team owner
pub async fn is_owner(pool: &PgPool, team_id: Uuid, user_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM team_members
            WHERE team_id = $1 AND user_id = $2 AND role = 'owner'
        ) as "exists!"
        "#,
        team_id,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to check team ownership")?;

    Ok(result.exists)
}

/// Check if user is team member (owner or member)
pub async fn is_member(pool: &PgPool, team_id: Uuid, user_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM team_members
            WHERE team_id = $1 AND user_id = $2
        ) as "exists!"
        "#,
        team_id,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to check team membership")?;

    Ok(result.exists)
}
