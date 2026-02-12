use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::User;

/// Find user by email address
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, is_platform_user, created_at, updated_at
        FROM users
        WHERE email = $1
        "#,
        email
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find user by email")?;

    Ok(user)
}

/// Find user by ID
pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, is_platform_user, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(executor)
    .await
    .context("Failed to find user by ID")?;

    Ok(user)
}

/// Create a new user
pub async fn create(pool: &PgPool, email: &str, is_platform_user: bool) -> Result<User> {
    let user = sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email, is_platform_user)
        VALUES ($1, $2)
        RETURNING id, email, is_platform_user, created_at, updated_at
        "#,
        email,
        is_platform_user
    )
    .fetch_one(pool)
    .await
    .context("Failed to create user")?;

    Ok(user)
}

/// Find user by email, or create if not exists
pub async fn find_or_create(
    pool: &PgPool,
    email: &str,
    platform_access_config: &crate::server::settings::PlatformAccessConfig,
    admin_users: &[String],
) -> Result<User> {
    // Try to find existing user first
    if let Some(user) = find_by_email(pool, email).await? {
        return Ok(user);
    }

    // Determine initial platform access (without IdP groups - evaluated during sync)
    let is_platform_user = should_grant_platform_access(
        email,
        None, // Groups will be evaluated during group sync
        platform_access_config,
        admin_users,
    );

    tracing::info!(
        email = %email,
        is_platform_user = %is_platform_user,
        "Creating new user"
    );

    // User doesn't exist, create new one
    create(pool, email, is_platform_user).await
}

/// Batch fetch user emails by IDs
pub async fn get_emails_batch(
    pool: &PgPool,
    user_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>> {
    let records = sqlx::query!(
        r#"
        SELECT id, email
        FROM users
        WHERE id = ANY($1)
        "#,
        user_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to batch fetch user emails")?;

    Ok(records.into_iter().map(|r| (r.id, r.email)).collect())
}

/// Batch fetch full user details by IDs
pub async fn get_users_batch(
    pool: &PgPool,
    user_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, User>> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, is_platform_user, created_at, updated_at
        FROM users
        WHERE id = ANY($1)
        "#,
        user_ids
    )
    .fetch_all(pool)
    .await
    .context("Failed to batch fetch users")?;

    Ok(users.into_iter().map(|u| (u.id, u)).collect())
}

/// Determine if user should have platform access
///
/// Grants access if ANY of these conditions are met:
/// 1. User is in admin_users (always)
/// 2. Policy is "allow_all"
/// 3. Policy is "restrictive" AND (email in allowlist OR has allowed group)
pub fn should_grant_platform_access(
    user_email: &str,
    user_idp_groups: Option<&[String]>,
    platform_access_config: &crate::server::settings::PlatformAccessConfig,
    admin_users: &[String],
) -> bool {
    use crate::server::settings::PlatformAccessPolicy;

    // Admin users always have platform access
    if admin_users
        .iter()
        .any(|admin| admin.eq_ignore_ascii_case(user_email))
    {
        return true;
    }

    match platform_access_config.policy {
        PlatformAccessPolicy::AllowAll => true,
        PlatformAccessPolicy::Restrictive => {
            // Check email allowlist
            if platform_access_config
                .allowed_user_emails
                .iter()
                .any(|email| email.eq_ignore_ascii_case(user_email))
            {
                return true;
            }

            // Check IdP group allowlist
            if let Some(groups) = user_idp_groups {
                if platform_access_config
                    .allowed_idp_groups
                    .iter()
                    .any(|allowed_group| {
                        groups
                            .iter()
                            .any(|user_group| user_group.eq_ignore_ascii_case(allowed_group))
                    })
                {
                    return true;
                }
            }

            false
        }
    }
}

/// Update user's platform access status
pub async fn update_platform_access(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    is_platform_user: bool,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE users
        SET is_platform_user = $1, updated_at = NOW()
        WHERE id = $2
        "#,
        is_platform_user,
        user_id
    )
    .execute(executor)
    .await
    .context("Failed to update user platform access")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn test_create_user(pool: PgPool) -> Result<()> {
        let user = create(&pool, "test@example.com", true).await?;
        assert_eq!(user.email, "test@example.com");
        assert!(user.is_platform_user);
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_by_email(pool: PgPool) -> Result<()> {
        let created = create(&pool, "test@example.com", true).await?;
        let found = find_by_email(&pool, "test@example.com").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_by_id(pool: PgPool) -> Result<()> {
        let created = create(&pool, "test@example.com", true).await?;
        let found = find_by_id(&pool, created.id).await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "test@example.com");
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_or_create_existing(pool: PgPool) -> Result<()> {
        use crate::server::settings::PlatformAccessConfig;
        let created = create(&pool, "test@example.com", true).await?;
        let config = PlatformAccessConfig::default();
        let found = find_or_create(&pool, "test@example.com", &config, &[]).await?;
        assert_eq!(found.id, created.id);
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_or_create_new(pool: PgPool) -> Result<()> {
        use crate::server::settings::PlatformAccessConfig;
        let config = PlatformAccessConfig::default();
        let user = find_or_create(&pool, "new@example.com", &config, &[]).await?;
        assert_eq!(user.email, "new@example.com");
        assert!(user.is_platform_user); // Default policy is allow_all
        Ok(())
    }
}
