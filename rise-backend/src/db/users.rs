use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::User;

/// Find user by email address
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, created_at, updated_at
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
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find user by ID")?;

    Ok(user)
}

/// Create a new user
pub async fn create(pool: &PgPool, email: &str) -> Result<User> {
    let user = sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email)
        VALUES ($1)
        RETURNING id, email, created_at, updated_at
        "#,
        email
    )
    .fetch_one(pool)
    .await
    .context("Failed to create user")?;

    Ok(user)
}

/// Find user by email, or create if not exists
pub async fn find_or_create(pool: &PgPool, email: &str) -> Result<User> {
    // Try to find existing user first
    if let Some(user) = find_by_email(pool, email).await? {
        return Ok(user);
    }

    // User doesn't exist, create new one
    create(pool, email).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn test_create_user(pool: PgPool) -> Result<()> {
        let user = create(&pool, "test@example.com").await?;
        assert_eq!(user.email, "test@example.com");
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_by_email(pool: PgPool) -> Result<()> {
        let created = create(&pool, "test@example.com").await?;
        let found = find_by_email(&pool, "test@example.com").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_by_id(pool: PgPool) -> Result<()> {
        let created = create(&pool, "test@example.com").await?;
        let found = find_by_id(&pool, created.id).await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "test@example.com");
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_or_create_existing(pool: PgPool) -> Result<()> {
        let created = create(&pool, "test@example.com").await?;
        let found = find_or_create(&pool, "test@example.com").await?;
        assert_eq!(found.id, created.id);
        Ok(())
    }

    #[sqlx::test]
    async fn test_find_or_create_new(pool: PgPool) -> Result<()> {
        let user = find_or_create(&pool, "new@example.com").await?;
        assert_eq!(user.email, "new@example.com");
        Ok(())
    }
}
