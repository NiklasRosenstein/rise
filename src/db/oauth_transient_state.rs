use std::time::Duration;

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

/// Store a value, overwriting any existing entry for the same lookup key.
pub async fn insert<T: Serialize>(
    pool: &PgPool,
    lookup_key: &str,
    data: &T,
    ttl: Duration,
) -> Result<()> {
    let data = serde_json::to_value(data)?;
    let ttl_secs = ttl.as_secs_f64();
    sqlx::query(
        "INSERT INTO oauth_transient_state (lookup_key, data, expires_at, claimed_at, claim_expires_at, claimed_by)
         VALUES ($1, $2, NOW() + ($3 * INTERVAL '1 second'), NULL, NULL, NULL)
         ON CONFLICT (lookup_key) DO UPDATE
         SET data = $2,
             expires_at = NOW() + ($3 * INTERVAL '1 second'),
             claimed_at = NULL,
             claim_expires_at = NULL,
             claimed_by = NULL",
    )
    .bind(lookup_key)
    .bind(data)
    .bind(ttl_secs)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn claim<T: DeserializeOwned>(
    pool: &PgPool,
    lookup_key: &str,
    claimant_id: Uuid,
    grace_ttl: Duration,
) -> Result<Option<T>> {
    let grace_secs = grace_ttl.as_secs_f64();
    let row = sqlx::query(
        "UPDATE oauth_transient_state
         SET claimed_at = NOW(),
             claim_expires_at = NOW() + ($3 * INTERVAL '1 second'),
             claimed_by = $2
         WHERE lookup_key = $1
           AND expires_at > NOW()
           AND (claimed_by IS NULL OR claim_expires_at < NOW())
         RETURNING data",
    )
    .bind(lookup_key)
    .bind(claimant_id)
    .bind(grace_secs)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(serde_json::from_value(
            row.try_get::<serde_json::Value, _>("data")?,
        )?)),
        None => Ok(None),
    }
}

pub async fn finalize(pool: &PgPool, lookup_key: &str, claimant_id: Uuid) -> Result<bool> {
    let result = sqlx::query(
        "DELETE FROM oauth_transient_state
         WHERE lookup_key = $1 AND claimed_by = $2",
    )
    .bind(lookup_key)
    .bind(claimant_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub async fn release_claim(pool: &PgPool, lookup_key: &str, claimant_id: Uuid) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE oauth_transient_state
         SET claimed_at = NULL,
             claim_expires_at = NULL,
             claimed_by = NULL
         WHERE lookup_key = $1 AND claimed_by = $2",
    )
    .bind(lookup_key)
    .bind(claimant_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Delete all expired rows. Call periodically to keep the table small.
pub async fn delete_expired(pool: &PgPool) -> Result<u64> {
    let result = sqlx::query!("DELETE FROM oauth_transient_state WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct TestState {
        value: String,
    }

    #[sqlx::test]
    async fn claim_blocks_other_claimants_until_grace_expires(pool: PgPool) -> Result<()> {
        insert(
            &pool,
            "lookup",
            &TestState {
                value: "hello".to_string(),
            },
            Duration::from_secs(60),
        )
        .await?;

        let first_claimant = Uuid::new_v4();
        let claimed =
            claim::<TestState>(&pool, "lookup", first_claimant, Duration::from_millis(800)).await?;
        assert_eq!(
            claimed,
            Some(TestState {
                value: "hello".to_string()
            })
        );

        let second_claim =
            claim::<TestState>(&pool, "lookup", Uuid::new_v4(), Duration::from_millis(800)).await?;
        assert!(second_claim.is_none());

        tokio::time::sleep(Duration::from_millis(900)).await;

        let claimed_again =
            claim::<TestState>(&pool, "lookup", Uuid::new_v4(), Duration::from_millis(800)).await?;
        assert!(claimed_again.is_some());
        Ok(())
    }

    #[sqlx::test]
    async fn release_returns_row_to_claimable_state(pool: PgPool) -> Result<()> {
        insert(
            &pool,
            "lookup",
            &TestState {
                value: "hello".to_string(),
            },
            Duration::from_secs(60),
        )
        .await?;

        let claimant = Uuid::new_v4();
        claim::<TestState>(&pool, "lookup", claimant, Duration::from_secs(60)).await?;
        assert!(release_claim(&pool, "lookup", claimant).await?);

        let reclaimed =
            claim::<TestState>(&pool, "lookup", Uuid::new_v4(), Duration::from_secs(60)).await?;
        assert!(reclaimed.is_some());
        Ok(())
    }

    #[sqlx::test]
    async fn finalize_removes_the_row(pool: PgPool) -> Result<()> {
        insert(
            &pool,
            "lookup",
            &TestState {
                value: "hello".to_string(),
            },
            Duration::from_secs(60),
        )
        .await?;

        let claimant = Uuid::new_v4();
        claim::<TestState>(&pool, "lookup", claimant, Duration::from_secs(60)).await?;
        assert!(finalize(&pool, "lookup", claimant).await?);

        let claimed =
            claim::<TestState>(&pool, "lookup", Uuid::new_v4(), Duration::from_secs(60)).await?;
        assert!(claimed.is_none());
        Ok(())
    }

    #[sqlx::test]
    async fn expired_rows_cannot_be_claimed(pool: PgPool) -> Result<()> {
        insert(
            &pool,
            "lookup",
            &TestState {
                value: "hello".to_string(),
            },
            Duration::from_millis(200),
        )
        .await?;

        tokio::time::sleep(Duration::from_millis(250)).await;

        let claimed =
            claim::<TestState>(&pool, "lookup", Uuid::new_v4(), Duration::from_secs(60)).await?;
        assert!(claimed.is_none());
        Ok(())
    }
}
