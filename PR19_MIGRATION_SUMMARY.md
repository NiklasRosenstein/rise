# PR #19 Migration Summary: Snowflake OAuth Integration

## Overview

PR #19 implements Snowflake OAuth integration and was created against the old workspace structure. This document outlines the changes needed to migrate it to the consolidated `rise-deploy` crate structure.

## Pull Request Summary

**PR #19: Snowflake OAuth Integration**
- **Files Modified**: 77 files
- **Changes**: +3,008 / -166 lines
- **Key Features**:
  - Database tables: `snowflake_sessions` and `snowflake_app_tokens`
  - `snowflake_enabled` flag on projects
  - Snowflake OAuth2 client implementation
  - OAuth endpoints at `/.rise/oauth/snowflake/`
  - Token injection into ingress authentication
  - Background token refresh controller
  - Encrypted token storage

## Structural Changes in `consolidate-workspace` Branch

### 1. Workspace Consolidation

**Old Structure** (main):
```
rise/
├── Cargo.toml (workspace)
├── rise-backend/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── db/
│   │   ├── auth/
│   │   ├── project/
│   │   ├── deployment/
│   │   └── ...
│   ├── migrations/
│   └── static/
└── rise-cli/
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── backend.rs (API client)
        ├── build/
        └── ...
```

**New Structure** (consolidate-workspace):
```
rise/
├── Cargo.toml (single package: rise-deploy)
├── migrations/
├── static/
└── src/
    ├── main.rs
    ├── api/ (NEW - client-side API models)
    ├── build/
    ├── cli/
    ├── db/
    └── server/
        ├── mod.rs (formerly rise-backend/src/lib.rs)
        ├── auth/
        ├── project/
        ├── deployment/
        └── ...
```

### 2. Feature Flags

The new crate uses granular feature flags for modular compilation:

```toml
[features]
default = ["cli"]
cli = ["dep:clap", "dep:comfy-table", ...]  # CLI commands
server = ["dep:config", "dep:oauth2", ...]  # Backend server
aws = ["server", "dep:aws-config", ...]     # AWS ECR/KMS
docker = ["server", "dep:bollard", ...]     # Docker controller
k8s = ["server", "dep:kube", ...]           # K8s controller
```

### 3. Import Path Changes

All imports must be updated:

| Old Path | New Path | Notes |
|----------|----------|-------|
| `rise_backend::db::*` | `crate::db::*` | DB module now at crate root |
| `rise_backend::auth::*` | `crate::server::auth::*` | Auth in server module |
| `rise_backend::project::*` | `crate::server::project::*` | Project in server module |
| `rise_backend::deployment::*` | `crate::server::deployment::*` | Deployment in server module |
| `rise_backend::settings::*` | `crate::server::settings::*` | Settings in server module |
| `rise_backend::state::*` | `crate::server::state::*` | State in server module |
| `rise_backend::registry::*` | `crate::server::registry::*` | Registry in server module |
| `rise_backend::encryption::*` | `crate::server::encryption::*` | Encryption in server module |

### 4. Database Module Location

- **Old**: `rise-backend/src/db/`
- **New**: `src/db/` (at crate root, shared by server modules)
- All database query code remains in `src/db/*`
- Import as `crate::db::*` from anywhere

### 5. Migration Files

- **Old**: `rise-backend/migrations/`
- **New**: `migrations/` (at crate root)

### 6. Static Files (Frontend)

- **Old**: `rise-backend/static/`
- **New**: `static/` (at crate root)

### 7. Removed Code

The following were removed as unused/dead code:

#### Project Models (`src/server/project/models.rs`):
- `CreateProjectRequest`
- `CreateProjectResponse`
- `UpdateProjectRequest`
- `UpdateProjectResponse`
- `UserInfo`
- `TeamInfo`
- `OwnerInfo`
- `ProjectWithOwnerInfo`
- `ProjectErrorResponse`
- `GetProjectParams`

#### Registry Models (`src/server/registry/models.rs`):
- `RegistryCredentials` struct
- `DockerConfig` struct
- Various request/response types

#### Database Functions:
Check `src/db/projects.rs` for removed functions like:
- Removed fuzzy matching helpers from models
- Removed unused query functions

### 8. API Client Structure

The new structure introduces `src/api/` for shared client-server types:

```rust
// src/api/models.rs
#[cfg(feature = "server")]
pub use crate::server::deployment::models::*;

#[cfg(not(feature = "server"))]
pub use self::client_models::*;
```

This allows CLI (without `server` feature) to use the same types as the server.

### 9. Main Entry Point Changes

**Old** (`rise-cli/src/main.rs`):
```rust
use rise_backend::settings::Settings;

pub enum BackendCommands {
    Server,
    DevOidcIssuer { ... },
}
```

**New** (`src/main.rs`):
```rust
#[cfg(feature = "server")]
use crate::server::settings::Settings;

#[cfg(feature = "server")]
pub enum BackendCommands {
    Server,
    DevOidcIssuer { ... },
}
```

Backend commands are now feature-gated.

### 10. Controller Loop Changes

Controllers now have feature gates with runtime error messages:

```rust
async fn run_controller(settings: Settings, is_k8s: bool) -> Result<()> {
    if is_k8s {
        #[cfg(feature = "k8s")]
        { run_kubernetes_controller_loop(settings).await }
        #[cfg(not(feature = "k8s"))]
        { anyhow::bail!("Kubernetes feature not enabled") }
    } else {
        #[cfg(feature = "docker")]
        { run_deployment_controller_loop(settings).await }
        #[cfg(not(feature = "docker"))]
        { anyhow::bail!("Docker feature not enabled") }
    }
}
```

## Migration Checklist for PR #19

### Phase 1: File Locations

- [ ] Move database migrations from `rise-backend/migrations/` to `migrations/`
- [ ] If new static files were added, move from `rise-backend/static/` to `static/`

### Phase 2: Module Structure

- [ ] Move new server modules to `src/server/` (e.g., `snowflake/` module)
- [ ] Update `src/server/mod.rs` to declare new modules
- [ ] Ensure database-related code stays in `src/db/`

### Phase 3: Import Path Updates

For every file in PR #19, update imports:

```rust
// OLD
use rise_backend::db::*;
use rise_backend::auth::*;
use rise_backend::encryption::*;

// NEW
use crate::db::*;
use crate::server::auth::*;
use crate::server::encryption::*;
```

**Search and Replace Patterns**:
```bash
# Find all rise_backend imports that need updating
grep -r "use rise_backend::" --include="*.rs"

# Replace patterns (verify each before applying):
rise_backend::db::        → crate::db::
rise_backend::auth::      → crate::server::auth::
rise_backend::project::   → crate::server::project::
rise_backend::settings::  → crate::server::settings::
rise_backend::state::     → crate::server::state::
rise_backend::encryption:: → crate::server::encryption::
rise_backend::registry::  → crate::server::registry::
rise_backend::deployment:: → crate::server::deployment::
```

### Phase 4: Dependencies

- [ ] Add any new dependencies to `Cargo.toml` [dependencies] section
- [ ] Determine if dependencies are `cli`, `server`, or both
- [ ] Mark server-only deps as `optional = true` and add to `server` feature

Example:
```toml
[dependencies]
# If Snowflake needs a specific client library:
snowflake-client = { version = "x.y", optional = true }

[features]
server = [
    # ... existing deps ...
    "dep:snowflake-client",
]
```

### Phase 5: Database Changes

- [ ] Add `snowflake_sessions` table migration to `migrations/`
- [ ] Add `snowflake_app_tokens` table migration to `migrations/`
- [ ] Add `snowflake_enabled` column migration to projects table
- [ ] Create database query functions in `src/db/snowflake.rs` or similar
- [ ] Update `src/db/mod.rs` to include new module

### Phase 6: Server Code

- [ ] Create `src/server/snowflake/` module structure:
  ```
  src/server/snowflake/
  ├── mod.rs
  ├── oauth.rs         # OAuth client
  ├── handlers.rs      # HTTP handlers
  ├── routes.rs        # Route definitions
  ├── controller.rs    # Token refresh controller
  └── models.rs        # Snowflake-specific types
  ```

- [ ] Update `src/server/mod.rs`:
  ```rust
  pub mod snowflake;
  ```

- [ ] Register routes in `src/server/mod.rs`:
  ```rust
  pub fn create_app(state: AppState) -> Router {
      Router::new()
          // ... existing routes ...
          .nest("/.rise/oauth/snowflake", snowflake::routes::create_routes())
  }
  ```

- [ ] Add controller spawn in `run_server()`:
  ```rust
  // Start Snowflake token refresh controller
  if settings.snowflake.enabled {
      let settings_clone = settings.clone();
      let handle = tokio::spawn(async move {
          if let Err(e) = run_snowflake_controller_loop(settings_clone).await {
              tracing::error!("Snowflake controller error: {}", e);
          }
      });
      controller_handles.push(handle);
  }
  ```

### Phase 7: CLI Changes

If PR #19 added CLI commands for Snowflake:

- [ ] Move CLI command code to `src/cli/`
- [ ] Update imports to use `crate::api::*` for API types
- [ ] Feature-gate CLI code with `#[cfg(feature = "cli")]`

### Phase 8: Settings/Configuration

- [ ] Add Snowflake settings to `src/server/settings.rs`:
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct Settings {
      // ... existing fields ...
      #[serde(default)]
      pub snowflake: SnowflakeSettings,
  }

  #[derive(Debug, Clone, Deserialize, Default)]
  pub struct SnowflakeSettings {
      pub enabled: bool,
      pub client_id: Option<String>,
      pub client_secret: Option<String>,
      pub redirect_url: Option<String>,
      pub scopes: Vec<String>,
  }
  ```

### Phase 9: Testing & Verification

- [ ] Run `cargo check --features cli` (CLI-only build)
- [ ] Run `cargo check --features server,docker` (Server build)
- [ ] Run `cargo check --all-features` (Full build)
- [ ] Run `mise sqlx:check` to verify database queries
- [ ] Run `mise sqlx:prepare` if needed
- [ ] Update tests to use new import paths
- [ ] Verify migrations run successfully

### Phase 10: Documentation

- [ ] Update any documentation references to old crate structure
- [ ] Update `CLAUDE.md` if needed
- [ ] Add Snowflake OAuth documentation to `docs/`
- [ ] Update configuration examples

## Common Pitfalls

1. **Forgetting Feature Gates**: New server code must be gated with `#[cfg(feature = "server")]`

2. **Import Paths**: Easy to miss imports in:
   - Test modules (`#[cfg(test)]`)
   - Example code in comments
   - Documentation tests

3. **Database vs Server**: Keep database code in `src/db/`, not `src/server/`

4. **AppState Access**: Controllers need `AppState` which is in `src/server/state.rs`

5. **Route Nesting**: Routes may need adjustment for new structure

6. **SQLX Cache**: After updating queries, must run `mise sqlx:prepare`

## Verification Commands

```bash
# Check all feature combinations
cargo check --no-default-features
cargo check --features cli
cargo check --features server
cargo check --features server,docker
cargo check --features server,k8s
cargo check --features server,aws
cargo check --all-features

# Format and lint
cargo fmt
mise lint

# Database checks
mise sqlx:check
mise sqlx:prepare  # if needed

# Run tests
cargo test --all-features
```

## Reference Commits

Key commits from the consolidation branch:

- `e40b606` - WIP: Consolidate workspace to single rise-deploy crate
- `b9e62d7` - fix: Complete import path fixes for workspace consolidation
- `39582ea` - chore: Complete workspace consolidation to rise-deploy
- `6a1aa48` - fix: Add feature gates for modular compilation
- `5e755a1` - refactor: remove unused imports and database functions
- `f7843ad` - refactor: remove all remaining dead code

## Questions to Answer During Migration

1. **Where should the Snowflake OAuth client live?**
   - Suggested: `src/server/snowflake/oauth.rs`

2. **Should Snowflake support be a separate feature flag?**
   - Suggested: No, include in `server` feature initially
   - Could add `snowflake` feature later if it becomes optional

3. **Where do Snowflake-specific database queries go?**
   - Suggested: `src/db/snowflake_sessions.rs` and `src/db/snowflake_tokens.rs`

4. **How to handle token injection in ingress auth?**
   - Update `src/server/auth/middleware.rs` or handlers
   - May need conditional logic based on project's `snowflake_enabled` flag

5. **Does the CLI need Snowflake commands?**
   - If yes, add to `src/cli/` with appropriate API client calls
   - If no, purely server-side feature

## Additional Notes

- The consolidated structure is cleaner and reduces duplication
- Feature flags allow for smaller binaries when features aren't needed
- All database code is now shared between CLI and server via `src/db/`
- The `src/api/` module provides shared types without circular dependencies
- Controllers are now feature-gated with helpful runtime error messages
- Removed code from old structure should not be relied upon in new code
