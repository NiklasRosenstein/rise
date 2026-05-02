# Leader Election Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-iteration `LeaderLeaseGuard` acquisition with a persistent `LeaderElection` service that holds the lease via a background heartbeat task and exposes a cheap `is_leader()` O(1) check plus a DB-backed `assert_leader()` fence.

**Architecture:** `LeaderElection` wraps an `Arc<AtomicBool>` (shared with a background Tokio task) and an `Arc<TaskGuard>` (aborts the task on last drop). The background task runs acquire→heartbeat→retry continuously. Controllers store one `LeaderElection` per controller (as a struct field or local in the spawned task) instead of re-acquiring every loop iteration.

**Tech Stack:** Rust, Tokio, SQLx (PostgreSQL)

---

## File Map

| File | Change |
|---|---|
| `src/db/leader_leases.rs` | Full rewrite — new `LeaderElection` type, fix raw sqlx queries |
| `src/server/project/controller.rs` | Add `election` field, update loop + helpers |
| `src/server/ecr/controller.rs` | Add `election` field, update 3 loops + helpers |
| `src/server/auth/entra_sync.rs` | Create election before loop, update `sync_once` signature |
| `src/server/extensions/providers/aws_rds.rs` | Add `election` field, update `start()` + `reconcile_single` |
| `src/server/extensions/providers/snowflake_oauth.rs` | Add `election` field, update `start()` + `reconcile_single` |
| `src/server/extensions/providers/oauth/provider.rs` | Create election in spawned task, update `reconcile_deletion` + `migrate_client_secret_ref` |

---

## Task 1: Rewrite `src/db/leader_leases.rs`

**Files:**
- Modify: `src/db/leader_leases.rs`

- [ ] **Step 1: Replace the file contents**

Replace the entire file with:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

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
                self.name, self.holder_id
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
                heartbeat_loop(&pool, &name, holder_id, lease_duration, &is_leader).await;
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

async fn heartbeat_loop(
    pool: &PgPool,
    name: &str,
    holder_id: Uuid,
    lease_duration: Duration,
    is_leader: &AtomicBool,
) {
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
        let _ = is_leader; // referenced to ensure it lives as long as the loop
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
    let result = sqlx::query_scalar!(
        "SELECT holder_id FROM leader_leases WHERE name = $1 AND expires_at > NOW()",
        name,
    )
    .fetch_optional(pool)
    .await?;
    Ok(result == Some(holder_id))
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
        assert!(!election.is_leader(), "should be false before first acquisition");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(election.is_leader(), "should be true after background task acquires");
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
        assert!(!second.is_leader(), "second should not acquire while first holds");
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
        assert!(second.is_leader(), "second should acquire after first's lease expires");
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
        assert!(election.is_leader(), "heartbeat should have renewed the lease");
        election.assert_leader().await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Regenerate the SQLX offline query cache**

`renew` and `is_held_db` now use `sqlx::query!` macros — update the cache:

```bash
mise run sqlx:prepare
```

Expected: updates files in `.sqlx/`. Stage the changes:

```bash
git add .sqlx/
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check --features backend
```

Expected: no errors. Fix any type mismatches before continuing.

- [ ] **Step 4: Run the new tests**

```bash
cargo test --features backend leader_leases
```

Expected: all 5 tests pass. If a test is flaky due to timing, increase the sleep durations by 2×.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add src/db/leader_leases.rs .sqlx/
git commit -m "refactor(leader-election): replace LeaderLeaseGuard with persistent LeaderElection service"
```

---

## Task 2: Update `ProjectController`

**Files:**
- Modify: `src/server/project/controller.rs`

- [ ] **Step 1: Add `election` field to the struct and construct it in `new()`**

Change the struct and `new()`:

```rust
use crate::db::{
    deployments as db_deployments, extensions as db_extensions,
    leader_leases::LeaderElection, projects as db_projects,
};

pub struct ProjectController {
    state: Arc<ControllerState>,
    deletion_interval: Duration,
    cleanup_tick: AtomicU64,
    election: LeaderElection,
}

impl ProjectController {
    pub fn new(state: Arc<ControllerState>) -> Self {
        let election = LeaderElection::spawn(
            state.db_pool.clone(),
            "rise-project-controller",
            Uuid::new_v4(),
            Duration::from_secs(60),
        );
        Self {
            state,
            deletion_interval: Duration::from_secs(5),
            cleanup_tick: AtomicU64::new(1),
            election,
        }
    }
```

- [ ] **Step 2: Simplify `deletion_loop` — remove `holder_id`, gate on `is_leader()`**

Replace the `deletion_loop` function body:

```rust
async fn deletion_loop(&self) {
    info!("Project deletion loop started");
    let mut ticker = interval(self.deletion_interval);

    loop {
        ticker.tick().await;

        if !self.election.is_leader() {
            continue;
        }

        if let Err(e) = self.process_deleting_projects().await {
            error!("Error in deletion loop: {}", e);
        }

        if let Err(e) = self.cleanup_expired_transient_state().await {
            warn!("Error cleaning up expired transient state: {:?}", e);
        }
    }
}
```

Update `start()` to not pass `holder_id`:

```rust
pub fn start(self: Arc<Self>) {
    tokio::spawn(async move {
        self.deletion_loop().await;
    });
}
```

- [ ] **Step 3: Update `process_deleting_projects` — remove guard param, use `self.election`**

Change the signature and replace every `lease_guard.ensure_held().await?` with `self.election.assert_leader().await?`:

```rust
async fn process_deleting_projects(&self) -> anyhow::Result<()> {
    let deleting = db_projects::find_deleting(&self.state.db_pool, 10).await?;

    for project in deleting {
        debug!("Processing deletion for project {}", project.name);

        let deployments =
            db_deployments::list_for_project(&self.state.db_pool, project.id).await?;

        let mut has_non_terminal = false;

        for deployment in &deployments {
            if state_machine::is_terminal(&deployment.status) {
                continue;
            }

            has_non_terminal = true;

            let is_pre_infrastructure = matches!(
                deployment.status,
                DeploymentStatus::Pending
                    | DeploymentStatus::Building
                    | DeploymentStatus::Pushing
            );

            if is_pre_infrastructure {
                if deployment.status != DeploymentStatus::Cancelling {
                    self.election.assert_leader().await?;
                    info!(
                        "Cancelling pre-infrastructure deployment {} (status={:?})",
                        deployment.deployment_id, deployment.status
                    );
                    db_deployments::mark_cancelling(&self.state.db_pool, deployment.id).await?;
                }
            } else {
                if deployment.status != DeploymentStatus::Terminating {
                    self.election.assert_leader().await?;
                    info!(
                        "Terminating post-infrastructure deployment {} (status={:?})",
                        deployment.deployment_id, deployment.status
                    );
                    db_deployments::mark_terminating(
                        &self.state.db_pool,
                        deployment.id,
                        crate::db::models::TerminationReason::UserStopped,
                    )
                    .await?;
                }
            }
        }

        if !has_non_terminal {
            if db_projects::has_finalizers(&self.state.db_pool, project.id).await? {
                debug!(
                    "Project {} has finalizers remaining, waiting for cleanup controllers",
                    project.name
                );
                continue;
            }

            let extensions =
                db_extensions::list_by_project(&self.state.db_pool, project.id).await?;
            if !extensions.is_empty() {
                debug!(
                    "Project {} has {} extension(s) remaining, waiting for extension controllers to clean up",
                    project.name,
                    extensions.len()
                );
                continue;
            }

            info!(
                "All deployments for project {} are terminated and no finalizers or extensions remain, marking as Terminated",
                project.name
            );

            self.election.assert_leader().await?;
            db_projects::update_status(
                &self.state.db_pool,
                project.id,
                crate::db::models::ProjectStatus::Terminated,
            )
            .await?;

            info!("Project {} is Terminated, deleting from database", project.name);

            self.election.assert_leader().await?;
            db_projects::delete(&self.state.db_pool, project.id).await?;
        } else {
            debug!(
                "Project {} still has non-terminal deployments, waiting",
                project.name
            );
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/project/controller.rs
git commit -m "refactor: use LeaderElection in ProjectController"
```

---

## Task 3: Update `EcrController`

**Files:**
- Modify: `src/server/ecr/controller.rs`

- [ ] **Step 1: Add `election` to the struct and `new()`**

```rust
use crate::db::leader_leases::LeaderElection;

pub struct EcrController {
    state: Arc<ControllerState>,
    manager: Arc<EcrRepoManager>,
    provision_interval: Duration,
    cleanup_interval: Duration,
    drift_interval: Duration,
    election: LeaderElection,
}

impl EcrController {
    pub fn new(state: Arc<ControllerState>, manager: Arc<EcrRepoManager>) -> Self {
        let election = LeaderElection::spawn(
            state.db_pool.clone(),
            "rise-ecr-controller",
            Uuid::new_v4(),
            Duration::from_secs(60),
        );
        Self {
            state,
            manager,
            provision_interval: Duration::from_secs(10),
            cleanup_interval: Duration::from_secs(5),
            drift_interval: Duration::from_secs(60),
            election,
        }
    }
```

- [ ] **Step 2: Update `start()` to not create `holder_id`**

```rust
pub fn start(self: Arc<Self>) {
    let provision_self = Arc::clone(&self);
    tokio::spawn(async move {
        provision_self.provision_loop().await;
    });

    let cleanup_self = Arc::clone(&self);
    tokio::spawn(async move {
        cleanup_self.cleanup_loop().await;
    });

    let drift_self = Arc::clone(&self);
    tokio::spawn(async move {
        drift_self.drift_detection_loop().await;
    });
}
```

- [ ] **Step 3: Update `provision_loop`, `cleanup_loop`, `drift_detection_loop`**

Remove `holder_id: Uuid` parameter, replace per-iteration acquire with `is_leader()` gate. Pattern is identical for all three — shown for `provision_loop`:

```rust
async fn provision_loop(&self) {
    let mut ticker = interval(self.provision_interval);
    loop {
        ticker.tick().await;
        if !self.election.is_leader() {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            continue;
        }
        if let Err(e) = self.provision_repositories().await {
            error!("Error in ECR provision loop: {}", e);
        }
    }
}

async fn cleanup_loop(&self) {
    let mut ticker = interval(self.cleanup_interval);
    loop {
        ticker.tick().await;
        if !self.election.is_leader() {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            continue;
        }
        if let Err(e) = self.cleanup_repositories().await {
            error!("Error in ECR cleanup loop: {}", e);
        }
    }
}

async fn drift_detection_loop(&self) {
    let mut ticker = interval(self.drift_interval);
    loop {
        ticker.tick().await;
        if !self.election.is_leader() {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            continue;
        }
        if let Err(e) = self.detect_repository_drift().await {
            error!("Error in ECR drift detection loop: {}", e);
        }
    }
}
```

- [ ] **Step 4: Update `provision_repositories`, `cleanup_repositories`, `detect_repository_drift`**

Remove `lease_guard: &LeaderLeaseGuard` parameter from all three. Replace `lease_guard.ensure_held().await?` with `self.election.assert_leader().await?`. Signature changes:

```rust
async fn provision_repositories(&self) -> anyhow::Result<()> { ... }
async fn cleanup_repositories(&self) -> anyhow::Result<()> { ... }
async fn detect_repository_drift(&self) -> anyhow::Result<()> { ... }
```

Inside each, every occurrence of `lease_guard.ensure_held().await?` becomes `self.election.assert_leader().await?`.

- [ ] **Step 5: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/ecr/controller.rs
git commit -m "refactor: use LeaderElection in EcrController"
```

---

## Task 4: Update `entra_sync`

**Files:**
- Modify: `src/server/auth/entra_sync.rs`

- [ ] **Step 1: Create `LeaderElection` before the sync loop in `run_entra_sync_loop`**

Remove the `holder_id` binding and `acquire` call from the loop. Add `LeaderElection::spawn` before the loop starts:

```rust
use crate::db::leader_leases::LeaderElection;

// At the top of run_entra_sync_loop, after setting up the interval and shutdown signal:
let election = LeaderElection::spawn(
    pool.clone(),
    "rise-entra-sync",
    uuid::Uuid::new_v4(),
    Duration::from_secs(interval_secs + 30),
);

loop {
    tokio::select! {
        _ = interval.tick() => {}
        _ = &mut shutdown => {
            tracing::info!("Entra active sync shutting down");
            break;
        }
    }

    if !election.is_leader() {
        tracing::debug!("Skipping Entra sync cycle — another replica is the leader");
        continue;
    }

    tracing::debug!("Running Entra active sync cycle");
    if let Err(e) = sync_once(&pool, &mut client, &election).await {
        tracing::error!("Entra active sync failed: {:?}", e);
    }
    tracing::info!("Next Entra active sync in {}s", interval_secs);
}
```

- [ ] **Step 2: Update `sync_once` signature and body**

Change `lease_guard: &LeaderLeaseGuard` to `election: &LeaderElection` and replace `lease_guard.ensure_held().await?` with `election.assert_leader().await?`:

```rust
async fn sync_once(
    pool: &PgPool,
    client: &mut GraphClient,
    election: &LeaderElection,
) -> Result<()> {
    // ... existing body unchanged except:
    // Every: lease_guard.ensure_held().await?
    // Becomes: election.assert_leader().await?
}
```

There are two `ensure_held()` calls in `sync_once` — one inside the per-group loop and one before removing unmatched teams. Replace both.

- [ ] **Step 3: Remove the old `LeaderLeaseGuard` import**

```rust
// Remove this line:
use crate::db::leader_leases::LeaderLeaseGuard;
// Replace with:
use crate::db::leader_leases::LeaderElection;
```

- [ ] **Step 4: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/auth/entra_sync.rs
git commit -m "refactor: use LeaderElection in entra sync loop"
```

---

## Task 5: Update `AwsRdsProvisioner`

**Files:**
- Modify: `src/server/extensions/providers/aws_rds.rs`

- [ ] **Step 1: Add `election` field to `AwsRdsProvisioner`**

```rust
use crate::db::{
    self, deployments as db_deployments, extensions as db_extensions,
    leader_leases::LeaderElection, postgres_admin, projects as db_projects,
};

pub struct AwsRdsProvisioner {
    rds_client: RdsClient,
    db_pool: sqlx::PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    region: String,
    instance_size: String,
    disk_size: i32,
    instance_id_template: String,
    instance_id_prefix: String,
    default_engine_version: String,
    vpc_security_group_ids: Option<Vec<String>>,
    db_subnet_group_name: Option<String>,
    backup_retention_days: i32,
    backup_window: Option<String>,
    maintenance_window: Option<String>,
    election: LeaderElection,
}
```

- [ ] **Step 2: Create `election` in `start()` before constructing the task-local `provisioner`**

In `fn start(&self)`, create the election first and include it when building the moved `provisioner`:

```rust
fn start(&self) {
    let election = LeaderElection::spawn(
        self.db_pool.clone(),
        "rise-ext-rds",
        uuid::Uuid::new_v4(),
        Duration::from_secs(60),
    );

    let provisioner = Self {
        rds_client: self.rds_client.clone(),
        db_pool: self.db_pool.clone(),
        encryption_provider: self.encryption_provider.clone(),
        region: self.region.clone(),
        instance_size: self.instance_size.clone(),
        disk_size: self.disk_size,
        instance_id_template: self.instance_id_template.clone(),
        instance_id_prefix: self.instance_id_prefix.clone(),
        default_engine_version: self.default_engine_version.clone(),
        vpc_security_group_ids: self.vpc_security_group_ids.clone(),
        db_subnet_group_name: self.db_subnet_group_name.clone(),
        backup_retention_days: self.backup_retention_days,
        backup_window: self.backup_window.clone(),
        maintenance_window: self.maintenance_window.clone(),
        election,
    };

    tokio::spawn(async move {
        let mut error_state: HashMap<Uuid, (usize, DateTime<Utc>)> = HashMap::new();
        let mut ticker = interval(Duration::from_secs(10));

        loop {
            ticker.tick().await;

            if !provisioner.election.is_leader() {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }

            match db_extensions::list_by_extension_type(&provisioner.db_pool, "aws-rds").await {
                Ok(extensions) => {
                    for ext in extensions {
                        // Track error state and backoff as before...
                        // Before reconciling:
                        if !provisioner.election.is_leader() {
                            warn!("Lost leadership before RDS reconcile");
                            break;
                        }

                        match provisioner.reconcile_single(ext.clone()).await {
                            Ok(_) => {
                                error_state.remove(&ext.project_id);
                            }
                            Err(e) => {
                                // existing error tracking logic unchanged
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to list RDS extensions: {}", e);
                }
            }
        }
    });
}
```

Note: preserve the existing error-state/backoff logic in the loop body; only the acquire block and leadership check change.

- [ ] **Step 3: Update `reconcile_single` — remove `&LeaderLeaseGuard`, use `self.election`**

Change signature and replace all `lease_guard.ensure_held().await?` with `self.election.assert_leader().await?`:

```rust
async fn reconcile_single(
    &self,
    project_extension: db::models::ProjectExtension,
) -> Result<bool> {
    // ... body unchanged except:
    // Every: lease_guard.ensure_held().await?
    // Becomes: self.election.assert_leader().await?
}
```

There are four `ensure_held()` call sites in `reconcile_single` — replace all four.

- [ ] **Step 4: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/extensions/providers/aws_rds.rs
git commit -m "refactor: use LeaderElection in AwsRdsProvisioner"
```

---

## Task 6: Update `SnowflakeOAuthProvisioner`

**Files:**
- Modify: `src/server/extensions/providers/snowflake_oauth.rs`

Follows the exact same pattern as Task 5. The `SnowflakeOAuthProvisioner` has a handwritten `Clone` impl — the `election` field must be added to it.

- [ ] **Step 1: Add `election: LeaderElection` to the struct**

```rust
use crate::db::{
    extensions as db_extensions, leader_leases::LeaderElection, projects as db_projects,
};

pub struct SnowflakeOAuthProvisioner {
    db_pool: PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    http_client: reqwest::Client,
    api_domain: String,
    oauth_provider: Option<Arc<dyn Extension>>,
    account: String,
    user: String,
    role: Option<String>,
    warehouse: Option<String>,
    auth: SnowflakeAuth,
    integration_name_prefix: String,
    default_blocked_roles: Vec<String>,
    default_scopes: Vec<String>,
    refresh_token_validity_seconds: i64,
    election: LeaderElection,
}
```

- [ ] **Step 2: Add `election` to the `Clone` impl**

Find the `impl Clone for SnowflakeOAuthProvisioner` block and add:

```rust
election: self.election.clone(),
```

- [ ] **Step 3: Update `start()` — create election, include it in self-clone, update loop**

In `fn start(&self)`, create the election before `self.clone()` and patch it into the cloned struct:

```rust
fn start(&self) {
    let mut provisioner = self.clone();
    provisioner.election = LeaderElection::spawn(
        self.db_pool.clone(),
        "rise-ext-snowflake",
        uuid::Uuid::new_v4(),
        Duration::from_secs(60),
    );

    tokio::spawn(async move {
        let mut error_state: HashMap<Uuid, (usize, DateTime<Utc>)> = HashMap::new();

        loop {
            sleep(std::time::Duration::from_secs(10)).await;

            if !provisioner.election.is_leader() {
                sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }

            match db_extensions::list_by_extension_type(&provisioner.db_pool, "snowflake-oauth")
                .await
            {
                Ok(extensions) => {
                    for ext in extensions {
                        if !provisioner.election.is_leader() {
                            warn!("Lost leadership before Snowflake reconcile");
                            break;
                        }

                        match provisioner.reconcile_single(ext.clone()).await {
                            Ok(_) => {
                                error_state.remove(&ext.project_id);
                            }
                            Err(e) => {
                                // existing error tracking unchanged
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error listing Snowflake OAuth extensions: {:?}", e);
                }
            }
        }
    });
}
```

Note: `SnowflakeOAuthProvisioner`'s `start()` uses `sleep` (not a `ticker`), and the loop body iterates only over a single extension type — preserve the existing error-state/backoff logic.

- [ ] **Step 4: Update `reconcile_single` — remove `&LeaderLeaseGuard`, use `self.election`**

Change signature and replace all `lease_guard.ensure_held().await?`:

```rust
async fn reconcile_single(
    &self,
    project_extension: crate::db::models::ProjectExtension,
) -> Result<bool> {
    // ... body unchanged except:
    // Every: lease_guard.ensure_held().await?
    // Becomes: self.election.assert_leader().await?
}
```

There are three `ensure_held()` call sites in `reconcile_single` — replace all three.

- [ ] **Step 5: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/extensions/providers/snowflake_oauth.rs
git commit -m "refactor: use LeaderElection in SnowflakeOAuthProvisioner"
```

---

## Task 7: Update `OAuthProvider`

**Files:**
- Modify: `src/server/extensions/providers/oauth/provider.rs`

`OAuthProvider` has a manual `Clone` impl and its `start()` does `let provider = self.clone()`. The election is created inside the spawned task (not on the struct) because `OAuthProvider` is cloned as a whole.

- [ ] **Step 1: Create `election` at the top of the spawned task in `start()`**

In the `fn start(&self)` implementation:

```rust
fn start(&self) {
    let provider = self.clone();
    tokio::spawn(async move {
        let election = crate::db::leader_leases::LeaderElection::spawn(
            provider.db_pool.clone(),
            "rise-ext-oauth",
            uuid::Uuid::new_v4(),
            std::time::Duration::from_secs(60),
        );

        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));

        loop {
            ticker.tick().await;

            if !election.is_leader() {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }

            match crate::db::extensions::list_by_extension_type(&provider.db_pool, "oauth").await {
                Ok(extensions) => {
                    for ext in extensions {
                        if ext.deleted_at.is_some() {
                            if !election.is_leader() {
                                warn!("Lost leadership before OAuth deletion reconcile");
                                break;
                            }
                            if let Err(e) = provider.reconcile_deletion(ext, &election).await {
                                error!("Failed to reconcile OAuth extension deletion: {:?}", e);
                            }
                            continue;
                        }

                        if !election.is_leader() {
                            warn!("Lost leadership before OAuth migration reconcile");
                            break;
                        }
                        if let Err(e) = provider.migrate_client_secret_ref(&ext, &election).await {
                            error!(
                                "Failed to migrate client_secret_ref for OAuth extension {}/{}: {:?}",
                                ext.project_id, ext.extension, e
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to list OAuth extensions: {:?}", e);
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}
```

- [ ] **Step 2: Add `election: &LeaderElection` parameter to `reconcile_deletion` and `migrate_client_secret_ref`**

Update `reconcile_deletion`:

```rust
async fn reconcile_deletion(
    &self,
    ext: crate::db::models::ProjectExtension,
    election: &crate::db::leader_leases::LeaderElection,
) -> Result<()> {
    use crate::db::extensions as db_extensions;

    info!(
        "Reconciling deletion for OAuth extension: project_id={}, extension={}",
        ext.project_id, ext.extension
    );

    election.assert_leader().await?;
    db_extensions::delete_permanently(&self.db_pool, ext.project_id, &ext.extension)
        .await
        .context("Failed to permanently delete OAuth extension")?;

    info!(
        "Permanently deleted OAuth extension: project_id={}, extension={}",
        ext.project_id, ext.extension
    );

    Ok(())
}
```

Update `migrate_client_secret_ref`:

```rust
async fn migrate_client_secret_ref(
    &self,
    ext: &crate::db::models::ProjectExtension,
    election: &crate::db::leader_leases::LeaderElection,
) -> Result<()> {
    // ... existing logic for resolving/encrypting the secret unchanged ...

    election.assert_leader().await?;
    crate::db::extensions::update_spec(
        &self.db_pool,
        ext.project_id,
        &ext.extension,
        &serde_json::to_value(&updated_spec)
            .context("Failed to serialize updated OAuth spec")?,
    )
    .await
    .context("Failed to update OAuth extension spec during migration")?;

    // Best-effort cleanup: delete the legacy env var (unchanged)
    // ...

    Ok(())
}
```

- [ ] **Step 3: Verify and commit**

```bash
cargo check --features backend
cargo fmt --all
git add src/server/extensions/providers/oauth/provider.rs
git commit -m "refactor: use LeaderElection in OAuthProvider"
```

---

## Task 8: Final validation

- [ ] **Step 1: Full lint check**

```bash
cargo clippy --all-features --all-targets -- -D warnings
```

Fix any warnings before proceeding.

- [ ] **Step 2: Full test run**

```bash
cargo test --features backend
```

Expected: all tests pass. The `leader_leases` tests require a running PostgreSQL instance (via `mise run db:migrate` if not already done).

- [ ] **Step 3: Verify no remaining references to the old API**

```bash
grep -rn "LeaderLeaseGuard\|ensure_held\|leader_leases::acquire\|leader_leases::try_acquire\|leader_leases::is_held" src/
```

Expected: no output. If any hits remain, update those call sites to the new API.

- [ ] **Step 4: Format and final commit**

```bash
cargo fmt --all
git add -p  # stage any remaining formatting changes
git commit -m "chore: final fmt after leader election refactor"
```

---

## Self-Review Notes

- All six controller locations from the spec's affected-controllers table are covered (Tasks 2–7).
- The `sqlx::query!` macro fix for `renew` and `is_held_db` is in Task 1 Step 2 (sqlx:prepare).
- The `LeaderElection: Clone` requirement (needed by `SnowflakeOAuthProvisioner` and `AwsRdsProvisioner`) is satisfied via `Arc<TaskGuard>` — all clones share one background task, which aborts on last drop.
- `OAuthProvider`'s election lives as a task-local (not on the struct) because its `start()` needs to set a fresh holder ID — the `reconcile_deletion` and `migrate_client_secret_ref` methods receive `&LeaderElection` as a parameter (one level of threading, not a multi-level chain).
- The `is_leader()` false→true transition test and lease-maintained-beyond-TTL test are in Task 1.
