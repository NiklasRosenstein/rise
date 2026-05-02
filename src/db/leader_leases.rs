use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use sqlx::PgPool;
use sqlx::Row;
use tokio::task::JoinHandle;
use uuid::Uuid;

const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

pub struct LeaderLeaseGuard {
    state: Arc<LeaderLeaseState>,
    heartbeat_task: JoinHandle<()>,
}

struct LeaderLeaseState {
    pool: PgPool,
    name: String,
    holder_id: Uuid,
    lease_duration: Duration,
}

impl LeaderLeaseGuard {
    #[allow(dead_code)]
    pub fn holder_id(&self) -> Uuid {
        self.state.holder_id
    }

    pub async fn ensure_held(&self) -> Result<()> {
        if is_held(&self.state.pool, &self.state.name, self.state.holder_id).await? {
            Ok(())
        } else {
            Err(anyhow!(
                "leader lease '{}' is no longer held by {}",
                self.state.name,
                self.state.holder_id
            ))
        }
    }

    #[allow(dead_code)]
    pub async fn release(self) {}
}

impl Drop for LeaderLeaseGuard {
    fn drop(&mut self) {
        self.heartbeat_task.abort();
    }
}

pub async fn acquire(
    pool: &PgPool,
    name: &str,
    holder_id: Uuid,
    lease_duration: Duration,
) -> Result<Option<LeaderLeaseGuard>> {
    if !try_acquire(pool, name, holder_id, lease_duration).await? {
        return Ok(None);
    }

    let state = Arc::new(LeaderLeaseState {
        pool: pool.clone(),
        name: name.to_string(),
        holder_id,
        lease_duration,
    });

    let heartbeat_state = Arc::clone(&state);
    let heartbeat_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_interval(heartbeat_state.lease_duration));

        loop {
            ticker.tick().await;

            match renew(
                &heartbeat_state.pool,
                &heartbeat_state.name,
                heartbeat_state.holder_id,
                heartbeat_state.lease_duration,
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    tracing::warn!(
                        lease = %heartbeat_state.name,
                        holder_id = %heartbeat_state.holder_id,
                        ?error,
                        "leader lease heartbeat failed"
                    );
                }
            }
        }
    });

    Ok(Some(LeaderLeaseGuard {
        state,
        heartbeat_task,
    }))
}

fn heartbeat_interval(lease_duration: Duration) -> Duration {
    Duration::from_secs_f64(
        (lease_duration.as_secs_f64() / 3.0).max(MIN_HEARTBEAT_INTERVAL.as_secs_f64()),
    )
}

async fn try_acquire(
    pool: &PgPool,
    name: &str,
    holder_id: Uuid,
    lease_duration: Duration,
) -> Result<bool> {
    let lease_secs = lease_duration.as_secs_f64();

    let result = sqlx::query_scalar!(
        "INSERT INTO leader_leases (name, holder_id, heartbeat_at, expires_at)
         VALUES ($1, $2, NOW(), NOW() + ($3 * INTERVAL '1 second'))
         ON CONFLICT (name) DO UPDATE
           SET holder_id = $2, heartbeat_at = NOW(), expires_at = NOW() + ($3 * INTERVAL '1 second')
           WHERE leader_leases.expires_at < NOW()
              OR leader_leases.holder_id = $2
         RETURNING holder_id",
        name,
        holder_id,
        lease_secs,
    )
    .fetch_optional(pool)
    .await?;

    Ok(result == Some(holder_id))
}

async fn renew(
    pool: &PgPool,
    name: &str,
    holder_id: Uuid,
    lease_duration: Duration,
) -> Result<bool> {
    let lease_secs = lease_duration.as_secs_f64();
    let result = sqlx::query(
        "UPDATE leader_leases
         SET heartbeat_at = NOW(), expires_at = NOW() + ($3 * INTERVAL '1 second')
         WHERE name = $1 AND holder_id = $2 AND expires_at > NOW()",
    )
    .bind(name)
    .bind(holder_id)
    .bind(lease_secs)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub async fn is_held(pool: &PgPool, name: &str, holder_id: Uuid) -> Result<bool> {
    let row = sqlx::query(
        "SELECT holder_id
         FROM leader_leases
         WHERE name = $1 AND expires_at > NOW()",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(row
        .map(|row| row.try_get::<Uuid, _>("holder_id"))
        .transpose()?
        == Some(holder_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn holder_can_acquire_and_renew_its_own_lease(pool: PgPool) -> Result<()> {
        let holder_id = Uuid::new_v4();
        let guard = acquire(
            &pool,
            "rise-test-lease",
            holder_id,
            Duration::from_millis(1500),
        )
        .await?
        .expect("lease should be acquired");

        tokio::time::sleep(Duration::from_millis(2200)).await;

        assert!(is_held(&pool, "rise-test-lease", holder_id).await?);
        guard.ensure_held().await?;
        Ok(())
    }

    #[sqlx::test]
    async fn second_holder_cannot_acquire_while_first_is_renewing(pool: PgPool) -> Result<()> {
        let guard = acquire(
            &pool,
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        )
        .await?
        .expect("first holder should acquire");

        tokio::time::sleep(Duration::from_millis(2200)).await;

        let second = acquire(
            &pool,
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        )
        .await?;

        assert!(second.is_none());
        drop(guard);
        Ok(())
    }

    #[sqlx::test]
    async fn second_holder_can_acquire_after_first_stops_renewing(pool: PgPool) -> Result<()> {
        let first_holder = Uuid::new_v4();
        let guard = acquire(
            &pool,
            "rise-test-lease",
            first_holder,
            Duration::from_millis(1200),
        )
        .await?
        .expect("first holder should acquire");

        drop(guard);
        tokio::time::sleep(Duration::from_millis(1400)).await;

        let second_holder = Uuid::new_v4();
        let second = acquire(
            &pool,
            "rise-test-lease",
            second_holder,
            Duration::from_millis(1200),
        )
        .await?;

        assert!(second.is_some());
        assert!(is_held(&pool, "rise-test-lease", second_holder).await?);
        assert!(!is_held(&pool, "rise-test-lease", first_holder).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn ensure_held_fails_after_leadership_is_lost(pool: PgPool) -> Result<()> {
        let first_holder = Uuid::new_v4();
        let first = acquire(
            &pool,
            "rise-test-lease",
            first_holder,
            Duration::from_millis(1200),
        )
        .await?
        .expect("first holder should acquire");

        first.heartbeat_task.abort();
        tokio::time::sleep(Duration::from_millis(1400)).await;

        let second = acquire(
            &pool,
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1200),
        )
        .await?;
        assert!(second.is_some());

        assert!(first.ensure_held().await.is_err());
        Ok(())
    }
}
