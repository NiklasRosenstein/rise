use anyhow::{bail, Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::Environment;

/// Create a new environment for a project
pub async fn create(
    pool: &PgPool,
    project_id: Uuid,
    name: &str,
    primary_deployment_group: Option<&str>,
    is_default: bool,
    is_production: bool,
    color: &str,
) -> Result<Environment> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        INSERT INTO environments (project_id, name, primary_deployment_group, is_default, is_production, color)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        "#,
        project_id,
        name,
        primary_deployment_group,
        is_default,
        is_production,
        color
    )
    .fetch_one(pool)
    .await
    .context("Failed to create environment")?;

    Ok(env)
}

/// List all environments for a project
pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<Environment>> {
    let envs = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE project_id = $1
        ORDER BY
            is_production DESC,
            is_default DESC,
            name ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list environments for project")?;

    Ok(envs)
}

/// Find an environment by name within a project
pub async fn find_by_name(
    pool: &PgPool,
    project_id: Uuid,
    name: &str,
) -> Result<Option<Environment>> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE project_id = $1 AND name = $2
        "#,
        project_id,
        name
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find environment by name")?;

    Ok(env)
}

/// Find an environment by its primary deployment group within a project
pub async fn find_by_primary_group(
    pool: &PgPool,
    project_id: Uuid,
    group: &str,
) -> Result<Option<Environment>> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE project_id = $1 AND primary_deployment_group = $2
        "#,
        project_id,
        group
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find environment by primary group")?;

    Ok(env)
}

/// Find the default environment for a project
pub async fn find_default(pool: &PgPool, project_id: Uuid) -> Result<Option<Environment>> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE project_id = $1 AND is_default = true
        "#,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find default environment")?;

    Ok(env)
}

/// Find the production environment for a project
#[allow(dead_code)]
pub async fn find_production(pool: &PgPool, project_id: Uuid) -> Result<Option<Environment>> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE project_id = $1 AND is_production = true
        "#,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find production environment")?;

    Ok(env)
}

/// Find an environment by ID
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Environment>> {
    let env = sqlx::query_as!(
        Environment,
        r#"
        SELECT id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        FROM environments
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find environment by ID")?;

    Ok(env)
}

/// Update an environment.
///
/// Uses a transaction to atomically swap `is_default` and `is_production` flags
/// when setting them on this environment (clearing them from any other environment first).
#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    project_id: Uuid,
    name: Option<&str>,
    primary_deployment_group: Option<Option<&str>>,
    is_default: Option<bool>,
    is_production: Option<bool>,
    color: Option<&str>,
) -> Result<Environment> {
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    // If setting is_default=true, clear it from any other environment in the same project
    if is_default == Some(true) {
        sqlx::query!(
            "UPDATE environments SET is_default = false, updated_at = NOW() WHERE project_id = $1 AND id != $2 AND is_default = true",
            project_id,
            id
        )
        .execute(&mut *tx)
        .await
        .context("Failed to clear is_default flag")?;
    }

    // If setting is_production=true, clear it from any other environment in the same project
    if is_production == Some(true) {
        sqlx::query!(
            "UPDATE environments SET is_production = false, updated_at = NOW() WHERE project_id = $1 AND id != $2 AND is_production = true",
            project_id,
            id
        )
        .execute(&mut *tx)
        .await
        .context("Failed to clear is_production flag")?;
    }

    let env = sqlx::query_as!(
        Environment,
        r#"
        UPDATE environments
        SET
            name = COALESCE($2, name),
            primary_deployment_group = CASE WHEN $3 THEN $4 ELSE primary_deployment_group END,
            is_default = COALESCE($5, is_default),
            is_production = COALESCE($6, is_production),
            color = COALESCE($7, color),
            updated_at = NOW()
        WHERE id = $1
        RETURNING id, project_id, name, primary_deployment_group, is_default, is_production, color, created_at, updated_at
        "#,
        id,
        name,
        primary_deployment_group.is_some(),  // $3: whether to update primary_deployment_group
        primary_deployment_group.flatten(),    // $4: the new value (can be NULL)
        is_default,
        is_production,
        color
    )
    .fetch_one(&mut *tx)
    .await
    .context("Failed to update environment")?;

    tx.commit().await.context("Failed to commit transaction")?;

    Ok(env)
}

/// Delete an environment by ID.
///
/// Returns an error if the environment has `is_default` or `is_production` set, since those
/// flags must be transferred to another environment first.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool> {
    // Check flags before deleting
    let env = find_by_id(pool, id).await?;
    if let Some(ref env) = env {
        if env.is_default {
            bail!("Cannot delete the default environment. Transfer the default flag to another environment first.");
        }
        if env.is_production {
            bail!("Cannot delete the production environment. Transfer the production flag to another environment first.");
        }
    }

    let result = sqlx::query!("DELETE FROM environments WHERE id = $1", id)
        .execute(pool)
        .await
        .context("Failed to delete environment")?;

    Ok(result.rows_affected() > 0)
}

/// Bootstrap the default "production" environment for a newly created project.
///
/// Creates a single environment named "production" with `is_default=true`, `is_production=true`,
/// and `primary_deployment_group="default"`.
pub async fn create_default_for_project(pool: &PgPool, project_id: Uuid) -> Result<Environment> {
    create(
        pool,
        project_id,
        "production",
        Some("default"),
        true,
        true,
        "green",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{models::ProjectStatus, projects, users};

    #[sqlx::test]
    async fn test_create_and_find_environment(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        let env = create(
            &pool,
            project.id,
            "staging",
            Some("staging"),
            false,
            false,
            "green",
        )
        .await?;
        assert_eq!(env.name, "staging");
        assert_eq!(env.primary_deployment_group.as_deref(), Some("staging"));
        assert!(!env.is_default);
        assert!(!env.is_production);

        let found = find_by_name(&pool, project.id, "staging").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, env.id);

        Ok(())
    }

    #[sqlx::test]
    async fn test_create_default_for_project(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        let env = create_default_for_project(&pool, project.id).await?;
        assert_eq!(env.name, "production");
        assert!(env.is_default);
        assert!(env.is_production);
        assert_eq!(env.primary_deployment_group.as_deref(), Some("default"));

        Ok(())
    }

    #[sqlx::test]
    async fn test_unique_default_and_production(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        // Create production env
        let prod = create(
            &pool,
            project.id,
            "production",
            Some("default"),
            true,
            true,
            "green",
        )
        .await?;

        // Create staging env
        let staging = create(
            &pool,
            project.id,
            "staging",
            Some("staging"),
            false,
            false,
            "green",
        )
        .await?;

        // Update staging to be default - should swap
        let staging = update(
            &pool,
            staging.id,
            project.id,
            None,
            None,
            Some(true),
            None,
            None,
        )
        .await?;
        assert!(staging.is_default);

        // Verify prod is no longer default
        let prod = find_by_id(&pool, prod.id).await?.unwrap();
        assert!(!prod.is_default);
        // But prod should still be production
        assert!(prod.is_production);

        Ok(())
    }

    #[sqlx::test]
    async fn test_cannot_delete_default_environment(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        let env = create_default_for_project(&pool, project.id).await?;

        let result = delete(&pool, env.id);
        assert!(result.await.is_err());

        Ok(())
    }

    #[sqlx::test]
    async fn test_find_by_primary_group(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        create(
            &pool,
            project.id,
            "production",
            Some("default"),
            true,
            true,
            "green",
        )
        .await?;
        create(
            &pool,
            project.id,
            "staging",
            Some("staging"),
            false,
            false,
            "green",
        )
        .await?;

        let found = find_by_primary_group(&pool, project.id, "default").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "production");

        let found = find_by_primary_group(&pool, project.id, "staging").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "staging");

        let found = find_by_primary_group(&pool, project.id, "nonexistent").await?;
        assert!(found.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn test_list_for_project_ordered(pool: PgPool) -> Result<()> {
        let user = users::create(&pool, "env-test@example.com").await?;
        let project = projects::create(
            &pool,
            "env-test-project",
            ProjectStatus::Stopped,
            "public".to_string(),
            Some(user.id),
            None,
        )
        .await?;

        create(&pool, project.id, "dev", None, false, false, "green").await?;
        create(
            &pool,
            project.id,
            "production",
            Some("default"),
            true,
            true,
            "green",
        )
        .await?;
        create(
            &pool,
            project.id,
            "staging",
            Some("staging"),
            false,
            false,
            "green",
        )
        .await?;

        let envs = list_for_project(&pool, project.id).await?;
        assert_eq!(envs.len(), 3);
        // production first (is_production=true), then dev, then staging (alphabetical)
        assert_eq!(envs[0].name, "production");
        assert_eq!(envs[1].name, "dev");
        assert_eq!(envs[2].name, "staging");

        Ok(())
    }
}
