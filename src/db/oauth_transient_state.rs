use std::time::Duration;

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::PgPool;

/// Store a value, overwriting any existing entry for the same lookup key.
pub async fn insert<T: Serialize>(
    pool: &PgPool,
    lookup_key: &str,
    data: &T,
    ttl: Duration,
) -> Result<()> {
    let data = serde_json::to_value(data)?;
    let expires_at = chrono::Utc::now() + chrono::Duration::from_std(ttl)?;
    sqlx::query!(
        "INSERT INTO oauth_transient_state (lookup_key, data, expires_at)
         VALUES ($1, $2, $3)
         ON CONFLICT (lookup_key) DO UPDATE SET data = $2, expires_at = $3",
        lookup_key,
        data,
        expires_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Retrieve a value by lookup key, returning None if missing or expired.
pub async fn get<T: DeserializeOwned>(pool: &PgPool, lookup_key: &str) -> Result<Option<T>> {
    let row = sqlx::query!(
        "SELECT data FROM oauth_transient_state WHERE lookup_key = $1 AND expires_at > NOW()",
        lookup_key,
    )
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(Some(serde_json::from_value(r.data)?)),
        None => Ok(None),
    }
}

/// Delete a value by lookup key.
pub async fn delete(pool: &PgPool, lookup_key: &str) -> Result<()> {
    sqlx::query!(
        "DELETE FROM oauth_transient_state WHERE lookup_key = $1",
        lookup_key,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically remove and return a value. Returns None if missing or expired.
/// Used for single-use tokens (auth codes) to prevent replay.
pub async fn consume<T: DeserializeOwned>(pool: &PgPool, lookup_key: &str) -> Result<Option<T>> {
    let row = sqlx::query!(
        "DELETE FROM oauth_transient_state
         WHERE lookup_key = $1 AND expires_at > NOW()
         RETURNING data",
        lookup_key,
    )
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(Some(serde_json::from_value(r.data)?)),
        None => Ok(None),
    }
}

/// Delete all expired rows. Call periodically to keep the table small.
pub async fn delete_expired(pool: &PgPool) -> Result<u64> {
    let result = sqlx::query!("DELETE FROM oauth_transient_state WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
