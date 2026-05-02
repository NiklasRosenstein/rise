# Leader Election Redesign

**Date:** 2026-05-02  
**Branch:** ha-fix  
**Scope:** `src/db/leader_leases.rs` and all backend controllers that use it

## Problem

The current `LeaderLeaseGuard` is acquired and dropped once per controller loop iteration. This causes two problems:

1. **Churn** — a new heartbeat task is spawned and torn down every ~30 seconds per controller, even when the replica is the stable long-term leader.
2. **Intrusive threading** — the guard must be passed as a `&LeaderLeaseGuard` parameter through every function that wants to check or assert leadership, which pollutes call signatures and makes it easy to miss a check when adding new code paths.

Additionally, `is_held()` (the underlying DB check) is the only way to verify leadership, which makes it expensive to check before external side effects (AWS/Snowflake API calls) that should abort early if leadership is lost.

## Design

### `LeaderElection` — a persistent background service

Replace `LeaderLeaseGuard` with a `LeaderElection` struct that lives for the full lifetime of the controller (not one iteration). It owns a single background Tokio task that manages the complete lease lifecycle.

```rust
pub struct LeaderElection { ... }

impl LeaderElection {
    /// Spawn the background lease manager. Returns immediately; acquisition happens async.
    pub fn spawn(pool: PgPool, name: &str, holder_id: Uuid, lease_duration: Duration) -> Self

    /// O(1), no DB — safe to call before external side effects
    pub fn is_leader(&self) -> bool

    /// DB round-trip — call immediately before irreversible DB mutations
    pub async fn assert_leader(&self) -> Result<()>
}
```

`is_leader()` reads an `Arc<AtomicBool>` that is shared with the background task — no DB round-trip, safe to call in tight loops or before every external API call. The bool starts `false`; the background task sets it `true` once the first acquisition succeeds. Controller loops that call `is_leader()` as their gate will simply skip iterations until the replica wins the election.

`assert_leader()` queries the `leader_leases` table directly — used as a last-resort fence immediately before writes that cannot be undone (deleting DB rows, removing finalizers, etc.).

On `drop`, the background task's `JoinHandle` is aborted; the lease expires naturally within `lease_duration`.

### Background task state machine

```
loop:
  try_acquire (UPSERT — succeeds if row is absent or expired or already ours)
  if acquired:
    set is_leader = true
    heartbeat loop (fires every lease_duration / 3):
      renew lease (UPDATE WHERE holder_id = ours AND not expired)
      if renew returns false (lease stolen): break heartbeat loop
      if renew errors: log warning, continue (transient DB failure should not lose leadership)
    set is_leader = false
  sleep(lease_duration) before retrying acquire
```

The `lease_duration / 3` heartbeat interval ensures the lease is renewed with two full intervals of headroom before expiry, matching the existing behaviour.

### Integration with controllers

Each controller struct gains a `LeaderElection` field constructed once at startup:

```rust
pub struct ProjectController {
    state: Arc<ControllerState>,
    election: LeaderElection,
}
```

The main loop becomes a cheap non-blocking gate:

```rust
loop {
    ticker.tick().await;
    if !self.election.is_leader() { continue; }
    self.process_deleting_projects().await?;
}
```

Inside work functions, `is_leader()` guards external side effects and `assert_leader()` guards irreversible DB writes:

```rust
async fn process_deleting_projects(&self) -> Result<()> {
    for project in deleting {
        // before external/AWS/K8s side effect:
        if !self.election.is_leader() { bail!("lost leadership"); }
        some_external_call().await?;

        // immediately before irreversible DB write:
        self.election.assert_leader().await?;
        db_projects::delete(&self.state.db_pool, project.id).await?;
    }
}
```

No guard parameter is threaded through function signatures. The `election` field is accessed via `self`.

### What stays the same

The underlying SQL functions (`try_acquire`, `renew`, `is_held`) remain as private helpers in `src/db/leader_leases.rs`. The public surface of the module shrinks to just `LeaderElection`. The `leader_leases` migration and table schema are unchanged.

### Affected controllers

All six locations that currently call `crate::db::leader_leases::acquire(...)` per iteration:

| Controller | Lease name |
|---|---|
| `src/server/project/controller.rs` | `rise-project-controller` |
| `src/server/ecr/controller.rs` (×3 loops) | `rise-ecr-controller` |
| `src/server/auth/entra_sync.rs` | `rise-entra-sync` |
| `src/server/extensions/providers/aws_rds.rs` | `rise-ext-rds` |
| `src/server/extensions/providers/snowflake_oauth.rs` | `rise-ext-snowflake` |
| `src/server/extensions/providers/oauth/provider.rs` | `rise-ext-oauth` |

### What is NOT in scope

- OAuth transient state (claim/finalize/release semantics) — separate concern, separate PR.
- Any changes to the `leader_leases` DB schema or migrations.
- The `sqlx::query!` macro regression in `renew`/`is_held` — fix those to use `sqlx::query!` (or `sqlx::query_as!`) as part of this rewrite since we're touching the file anyway.

## Testing

Existing `#[sqlx::test]` tests in `src/db/leader_leases.rs` cover the core lease scenarios. Update them to use the new `LeaderElection` API. Add one test that verifies `is_leader()` transitions correctly: `false` before acquisition, `true` after, `false` after the background task is aborted and the lease expires.

The controller integration does not need new tests beyond what already exists — the behaviour is unchanged, only the structure of how the lease is held.
