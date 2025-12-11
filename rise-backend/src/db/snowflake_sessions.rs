use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{SnowflakeAppToken, SnowflakeSession};

/// Create a new Snowflake session and return the session ID
pub async fn create_session(pool: &PgPool, user_email: &str) -> Result<String> {
    let session_id = Uuid::new_v4().to_string();

    sqlx::query!(
        r#"
        INSERT INTO rise_snowflake_sessions (session_id, user_email)
        VALUES ($1, $2)
        "#,
        session_id,
        user_email
    )
    .execute(pool)
    .await
    .context("Failed to create Snowflake session")?;

    Ok(session_id)
}

/// Get a Snowflake session by ID
pub async fn get_session(pool: &PgPool, session_id: &str) -> Result<Option<SnowflakeSession>> {
    let session = sqlx::query_as!(
        SnowflakeSession,
        r#"
        SELECT session_id, user_email, created_at, updated_at
        FROM rise_snowflake_sessions
        WHERE session_id = $1
        "#,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get Snowflake session")?;

    Ok(session)
}

/// Delete a Snowflake session (cascades to tokens)
pub async fn delete_session(pool: &PgPool, session_id: &str) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM rise_snowflake_sessions
        WHERE session_id = $1
        "#,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete Snowflake session")?;

    Ok(result.rows_affected() > 0)
}

/// Upsert (insert or update) an app token for a session+project combination
pub async fn upsert_app_token(
    pool: &PgPool,
    session_id: &str,
    project_name: &str,
    access_token_encrypted: &str,
    refresh_token_encrypted: &str,
    token_expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO snowflake_app_tokens
            (session_id, project_name, access_token_encrypted, refresh_token_encrypted, token_expires_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (session_id, project_name)
        DO UPDATE SET
            access_token_encrypted = EXCLUDED.access_token_encrypted,
            refresh_token_encrypted = EXCLUDED.refresh_token_encrypted,
            token_expires_at = EXCLUDED.token_expires_at,
            updated_at = NOW()
        "#,
        session_id,
        project_name,
        access_token_encrypted,
        refresh_token_encrypted,
        token_expires_at
    )
    .execute(pool)
    .await
    .context("Failed to upsert Snowflake app token")?;

    Ok(())
}

/// Get an app token for a session+project combination
pub async fn get_app_token(
    pool: &PgPool,
    session_id: &str,
    project_name: &str,
) -> Result<Option<SnowflakeAppToken>> {
    let token = sqlx::query_as!(
        SnowflakeAppToken,
        r#"
        SELECT session_id, project_name, access_token_encrypted, refresh_token_encrypted,
               token_expires_at, created_at, updated_at
        FROM snowflake_app_tokens
        WHERE session_id = $1 AND project_name = $2
        "#,
        session_id,
        project_name
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get Snowflake app token")?;

    Ok(token)
}

/// Delete an app token for a session+project combination
pub async fn delete_app_token(pool: &PgPool, session_id: &str, project_name: &str) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM snowflake_app_tokens
        WHERE session_id = $1 AND project_name = $2
        "#,
        session_id,
        project_name
    )
    .execute(pool)
    .await
    .context("Failed to delete Snowflake app token")?;

    Ok(result.rows_affected() > 0)
}

/// Find tokens that are expiring within the given duration (for background refresh)
pub async fn find_expiring_tokens(
    pool: &PgPool,
    expires_before: DateTime<Utc>,
) -> Result<Vec<SnowflakeAppToken>> {
    let tokens = sqlx::query_as!(
        SnowflakeAppToken,
        r#"
        SELECT session_id, project_name, access_token_encrypted, refresh_token_encrypted,
               token_expires_at, created_at, updated_at
        FROM snowflake_app_tokens
        WHERE token_expires_at < $1
        ORDER BY token_expires_at ASC
        "#,
        expires_before
    )
    .fetch_all(pool)
    .await
    .context("Failed to find expiring Snowflake tokens")?;

    Ok(tokens)
}

/// List all tokens for a session
pub async fn list_session_tokens(
    pool: &PgPool,
    session_id: &str,
) -> Result<Vec<SnowflakeAppToken>> {
    let tokens = sqlx::query_as!(
        SnowflakeAppToken,
        r#"
        SELECT session_id, project_name, access_token_encrypted, refresh_token_encrypted,
               token_expires_at, created_at, updated_at
        FROM snowflake_app_tokens
        WHERE session_id = $1
        ORDER BY project_name ASC
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list Snowflake session tokens")?;

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[sqlx::test]
    async fn test_create_and_get_session(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;

        let session = get_session(&pool, &session_id).await?;
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.user_email, "test@example.com");

        Ok(())
    }

    #[sqlx::test]
    async fn test_delete_session(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;

        let deleted = delete_session(&pool, &session_id).await?;
        assert!(deleted);

        let session = get_session(&pool, &session_id).await?;
        assert!(session.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn test_upsert_and_get_token(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;
        let expires_at = Utc::now() + Duration::hours(1);

        upsert_app_token(
            &pool,
            &session_id,
            "my-project",
            "encrypted_access",
            "encrypted_refresh",
            expires_at,
        )
        .await?;

        let token = get_app_token(&pool, &session_id, "my-project").await?;
        assert!(token.is_some());
        let token = token.unwrap();
        assert_eq!(token.access_token_encrypted, "encrypted_access");
        assert_eq!(token.refresh_token_encrypted, "encrypted_refresh");

        Ok(())
    }

    #[sqlx::test]
    async fn test_upsert_updates_existing(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;
        let expires_at = Utc::now() + Duration::hours(1);

        // Insert
        upsert_app_token(
            &pool,
            &session_id,
            "my-project",
            "old_access",
            "old_refresh",
            expires_at,
        )
        .await?;

        // Update
        upsert_app_token(
            &pool,
            &session_id,
            "my-project",
            "new_access",
            "new_refresh",
            expires_at,
        )
        .await?;

        let token = get_app_token(&pool, &session_id, "my-project").await?;
        assert!(token.is_some());
        let token = token.unwrap();
        assert_eq!(token.access_token_encrypted, "new_access");
        assert_eq!(token.refresh_token_encrypted, "new_refresh");

        Ok(())
    }

    #[sqlx::test]
    async fn test_delete_session_cascades_tokens(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;
        let expires_at = Utc::now() + Duration::hours(1);

        upsert_app_token(
            &pool,
            &session_id,
            "my-project",
            "encrypted_access",
            "encrypted_refresh",
            expires_at,
        )
        .await?;

        // Delete session should cascade to tokens
        delete_session(&pool, &session_id).await?;

        let token = get_app_token(&pool, &session_id, "my-project").await?;
        assert!(token.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn test_find_expiring_tokens(pool: PgPool) -> Result<()> {
        let session_id = create_session(&pool, "test@example.com").await?;

        // Create token expiring soon
        let soon = Utc::now() + Duration::minutes(5);
        upsert_app_token(
            &pool,
            &session_id,
            "expiring-project",
            "access",
            "refresh",
            soon,
        )
        .await?;

        // Create token expiring later
        let later = Utc::now() + Duration::hours(2);
        upsert_app_token(
            &pool,
            &session_id,
            "valid-project",
            "access",
            "refresh",
            later,
        )
        .await?;

        // Find tokens expiring in next 10 minutes
        let threshold = Utc::now() + Duration::minutes(10);
        let expiring = find_expiring_tokens(&pool, threshold).await?;

        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].project_name, "expiring-project");

        Ok(())
    }
}
