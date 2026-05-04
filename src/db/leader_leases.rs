use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);

struct TaskGuard(JoinHandle<()>);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Persistent leader election service backed by the `leader_leases` PostgreSQL table.
///
/// Holds the lease for its entire lifetime via a single background heartbeat task.
/// Clone-safe: all clones share the same task and `is_leader` state; the task is
/// aborted when the last clone is dropped.
#[derive(Clone)]
pub struct LeaderElection {
    is_leader: Arc<AtomicBool>,
    pool: PgPool,
    name: String,
    holder_id: Uuid,
    _task: Arc<TaskGuard>,
}

impl LeaderElection {
    /// Spawn the background lease manager. Returns immediately; the first
    /// acquisition attempt happens asynchronously. `is_leader()` starts `false`
    /// and becomes `true` once the background task wins the election.
    pub fn spawn(pool: PgPool, name: &str, holder_id: Uuid, lease_duration: Duration) -> Self {
        let is_leader = Arc::new(AtomicBool::new(false));
        let is_leader_bg = Arc::clone(&is_leader);
        let pool_bg = pool.clone();
        let name_bg = name.to_string();

        let task = tokio::spawn(async move {
            run_election_loop(pool_bg, name_bg, holder_id, lease_duration, is_leader_bg).await;
        });

        Self {
            is_leader,
            pool,
            name: name.to_string(),
            holder_id,
            _task: Arc::new(TaskGuard(task)),
        }
    }

    /// Returns whether this instance currently holds the leader lease.
    /// O(1), no DB round-trip — safe to call in tight loops or before external API calls.
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::Acquire)
    }

    /// Verifies leadership with a DB round-trip.
    /// Call immediately before irreversible DB mutations (deletes, finalizer removals, status updates).
    pub async fn assert_leader(&self) -> Result<()> {
        if is_held_db(&self.pool, &self.name, self.holder_id).await? {
            Ok(())
        } else {
            Err(anyhow!(
                "leader lease '{}' is no longer held by {}",
                self.name,
                self.holder_id
            ))
        }
    }
}

async fn run_election_loop(
    pool: PgPool,
    name: String,
    holder_id: Uuid,
    lease_duration: Duration,
    is_leader: Arc<AtomicBool>,
) {
    let retry_interval = heartbeat_interval(lease_duration);
    loop {
        match try_acquire(&pool, &name, holder_id, lease_duration).await {
            Ok(true) => {
                is_leader.store(true, Ordering::Release);
                tracing::debug!(lease = %name, "leader lease acquired");
                heartbeat_loop(&pool, &name, holder_id, lease_duration).await;
                is_leader.store(false, Ordering::Release);
                tracing::debug!(lease = %name, "leader lease lost; will retry");
            }
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    lease = %name,
                    holder_id = %holder_id,
                    ?error,
                    "leader lease acquisition failed"
                );
            }
        }
        tokio::time::sleep(retry_interval).await;
    }
}

async fn heartbeat_loop(pool: &PgPool, name: &str, holder_id: Uuid, lease_duration: Duration) {
    let mut ticker = tokio::time::interval(heartbeat_interval(lease_duration));
    loop {
        ticker.tick().await;
        match renew(pool, name, holder_id, lease_duration).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(lease = %name, "leader lease stolen; stepping down");
                break;
            }
            Err(error) => {
                tracing::warn!(
                    lease = %name,
                    holder_id = %holder_id,
                    ?error,
                    "leader lease heartbeat failed; will retry next tick"
                );
            }
        }
    }
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
    let result = sqlx::query!(
        "UPDATE leader_leases
         SET heartbeat_at = NOW(), expires_at = NOW() + ($3 * INTERVAL '1 second')
         WHERE name = $1 AND holder_id = $2 AND expires_at > NOW()",
        name,
        holder_id,
        lease_secs,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

async fn is_held_db(pool: &PgPool, name: &str, holder_id: Uuid) -> Result<bool> {
    let row = sqlx::query_scalar!(
        "SELECT holder_id FROM leader_leases WHERE name = $1 AND expires_at > NOW()",
        name,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row == Some(holder_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn is_leader_starts_false_then_becomes_true(pool: PgPool) -> Result<()> {
        let election = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        );
        assert!(
            !election.is_leader(),
            "should be false before first acquisition"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            election.is_leader(),
            "should be true after background task acquires"
        );
        Ok(())
    }

    #[sqlx::test]
    async fn second_cannot_acquire_while_first_holds(pool: PgPool) -> Result<()> {
        let first = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(first.is_leader());

        let second = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !second.is_leader(),
            "second should not acquire while first holds"
        );
        Ok(())
    }

    #[sqlx::test]
    async fn second_acquires_after_first_drops(pool: PgPool) -> Result<()> {
        let first = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(800),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(first.is_leader());

        drop(first); // aborts heartbeat; lease expires within 800ms

        tokio::time::sleep(Duration::from_millis(900)).await;

        let second = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(800),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            second.is_leader(),
            "second should acquire after first's lease expires"
        );
        Ok(())
    }

    #[sqlx::test]
    async fn assert_leader_passes_for_holder_fails_for_non_holder(pool: PgPool) -> Result<()> {
        let holder = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        );
        let non_holder = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(1500),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(holder.is_leader());
        assert!(!non_holder.is_leader());

        holder.assert_leader().await?;
        assert!(
            non_holder.assert_leader().await.is_err(),
            "assert_leader should fail for non-holder"
        );
        Ok(())
    }

    #[sqlx::test]
    async fn lease_is_maintained_beyond_initial_ttl(pool: PgPool) -> Result<()> {
        let election = LeaderElection::spawn(
            pool.clone(),
            "rise-test-lease",
            Uuid::new_v4(),
            Duration::from_millis(600),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(election.is_leader());

        // Wait longer than the lease TTL — heartbeat should keep it alive
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(
            election.is_leader(),
            "heartbeat should have renewed the lease"
        );
        election.assert_leader().await?;
        Ok(())
    }
}
