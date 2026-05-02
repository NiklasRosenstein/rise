use std::time::Duration;

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

/// Try to acquire or renew the named leader lease.
///
/// Returns `true` if this instance (`holder_id`) now holds the lease, `false` if another
/// instance holds a non-expired lease.
///
/// Callers should invoke this at each work iteration. Non-leaders should back off before
/// retrying (e.g. 30 seconds). Leaders should call this frequently enough to keep
/// `lease_duration` from expiring between calls.
pub async fn try_acquire(
    pool: &PgPool,
    name: &str,
    holder_id: Uuid,
    lease_duration: Duration,
) -> Result<bool> {
    let lease_secs = lease_duration.as_secs() as f64;

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
