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
    let expires_at = chrono::Utc::now() + chrono::Duration::from_std(lease_duration)?;

    // Insert a new lease or update ours if we already hold it or it has expired.
    sqlx::query!(
        "INSERT INTO leader_leases (name, holder_id, heartbeat_at, expires_at)
         VALUES ($1, $2, NOW(), $3)
         ON CONFLICT (name) DO UPDATE
           SET holder_id = $2, heartbeat_at = NOW(), expires_at = $3
           WHERE leader_leases.expires_at < NOW()
              OR leader_leases.holder_id = $2",
        name,
        holder_id,
        expires_at,
    )
    .execute(pool)
    .await?;

    // Check whether we actually hold the lease now.
    let current_holder =
        sqlx::query_scalar!("SELECT holder_id FROM leader_leases WHERE name = $1", name,)
            .fetch_optional(pool)
            .await?;

    Ok(current_holder == Some(holder_id))
}
