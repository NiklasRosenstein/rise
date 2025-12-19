use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::server::extensions::providers::oauth::models::UserOAuthToken;

/// Get a user OAuth token by session ID, project ID, and extension name
pub async fn get_by_session(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
    session_id: &str,
) -> Result<Option<UserOAuthToken>> {
    let token = sqlx::query_as!(
        UserOAuthToken,
        r#"
        SELECT id, project_id, extension, session_id,
               access_token_encrypted, refresh_token_encrypted, id_token_encrypted,
               expires_at, last_refreshed_at, last_accessed_at,
               created_at, updated_at
        FROM user_oauth_tokens
        WHERE project_id = $1 AND extension = $2 AND session_id = $3
        "#,
        project_id,
        extension,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get user OAuth token")?;

    Ok(token)
}

/// Create or update a user OAuth token (upsert)
pub async fn upsert(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
    session_id: &str,
    access_token_encrypted: &str,
    refresh_token_encrypted: Option<&str>,
    id_token_encrypted: Option<&str>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<UserOAuthToken> {
    let token = sqlx::query_as!(
        UserOAuthToken,
        r#"
        INSERT INTO user_oauth_tokens (
            project_id, extension, session_id,
            access_token_encrypted, refresh_token_encrypted, id_token_encrypted,
            expires_at, last_refreshed_at, last_accessed_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        ON CONFLICT (project_id, extension, session_id)
        DO UPDATE SET
            access_token_encrypted = EXCLUDED.access_token_encrypted,
            refresh_token_encrypted = EXCLUDED.refresh_token_encrypted,
            id_token_encrypted = EXCLUDED.id_token_encrypted,
            expires_at = EXCLUDED.expires_at,
            last_refreshed_at = NOW(),
            last_accessed_at = NOW(),
            updated_at = NOW()
        RETURNING id, project_id, extension, session_id,
                  access_token_encrypted, refresh_token_encrypted, id_token_encrypted,
                  expires_at, last_refreshed_at, last_accessed_at,
                  created_at, updated_at
        "#,
        project_id,
        extension,
        session_id,
        access_token_encrypted,
        refresh_token_encrypted,
        id_token_encrypted,
        expires_at
    )
    .fetch_one(pool)
    .await
    .context("Failed to upsert user OAuth token")?;

    Ok(token)
}

/// Update last_accessed_at timestamp for a token
pub async fn update_last_accessed(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE user_oauth_tokens
        SET last_accessed_at = NOW(), updated_at = NOW()
        WHERE id = $1
        "#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to update last_accessed_at")?;

    Ok(())
}

/// Delete a user OAuth token by ID
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM user_oauth_tokens
        WHERE id = $1
        "#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to delete user OAuth token")?;

    Ok(result.rows_affected() > 0)
}

/// Delete all user OAuth tokens for a specific project and extension
/// This is used when deleting an OAuth extension
pub async fn delete_by_extension(pool: &PgPool, project_id: Uuid, extension: &str) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        DELETE FROM user_oauth_tokens
        WHERE project_id = $1 AND extension = $2
        "#,
        project_id,
        extension
    )
    .execute(pool)
    .await
    .context("Failed to delete user OAuth tokens for extension")?;

    Ok(result.rows_affected())
}

/// Find tokens that are expired but have refresh tokens (for background refresh job)
pub async fn find_expiring_tokens(pool: &PgPool) -> Result<Vec<UserOAuthToken>> {
    let tokens = sqlx::query_as!(
        UserOAuthToken,
        r#"
        SELECT id, project_id, extension, session_id,
               access_token_encrypted, refresh_token_encrypted, id_token_encrypted,
               expires_at, last_refreshed_at, last_accessed_at,
               created_at, updated_at
        FROM user_oauth_tokens
        WHERE expires_at <= NOW()
          AND refresh_token_encrypted IS NOT NULL
        ORDER BY expires_at ASC
        LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to find expiring tokens")?;

    Ok(tokens)
}

/// Find inactive tokens for cleanup (not accessed within retention period)
pub async fn find_inactive_tokens(
    pool: &PgPool,
    inactive_since: DateTime<Utc>,
) -> Result<Vec<UserOAuthToken>> {
    let tokens = sqlx::query_as!(
        UserOAuthToken,
        r#"
        SELECT id, project_id, extension, session_id,
               access_token_encrypted, refresh_token_encrypted, id_token_encrypted,
               expires_at, last_refreshed_at, last_accessed_at,
               created_at, updated_at
        FROM user_oauth_tokens
        WHERE last_accessed_at < $1
        ORDER BY last_accessed_at ASC
        LIMIT 100
        "#,
        inactive_since
    )
    .fetch_all(pool)
    .await
    .context("Failed to find inactive tokens")?;

    Ok(tokens)
}
