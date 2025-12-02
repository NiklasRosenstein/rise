# Architecture

## Components

### rise-backend

Axum-based REST API handling:
- Authentication (JWT validation via PocketBase)
- Project/team CRUD with ownership model
- Registry credential generation
- Future: Build orchestration, deployments

**Tech:** Rust, Axum, PocketBase SDK, AWS SDK

### rise-cli

Command-line interface for:
- Interactive authentication (device flow)
- Project and team management
- Future: Building and deploying applications

**Tech:** Rust, Clap, Reqwest

### PocketBase

Embedded database and auth provider:
- User authentication with JWT tokens
- Collections for projects, teams, users
- Auto-generated migrations
- Admin UI at http://localhost:8090/_/

**Schema:** `pb_migrations/*.js`

## Request Flow

```
┌──────────┐
│ rise-cli │
└────┬─────┘
     │ 1. POST /login
     ▼
┌─────────────┐      2. Validate      ┌────────────┐
│ rise-backend├──────credentials──────►│ PocketBase │
└────┬────────┘      3. Return JWT     └────────────┘
     │ 4. Return token to CLI
     ▼
┌──────────┐
│ CLI saves│
│   token  │
└──────────┘

Subsequent requests:
┌──────────┐
│ rise-cli │  Authorization: Bearer <token>
└────┬─────┘
     │ GET /projects
     ▼
┌─────────────┐      Validate token    ┌────────────┐
│ rise-backend├──────via /auth-────────►│ PocketBase │
└────┬────────┘      refresh            └────────────┘
     │
     │ Query database
     ▼
┌────────────┐
│ PocketBase │
│  Database  │
└────────────┘
```

## Security Model

**Token Validation:**
All protected endpoints validate JWT by calling PocketBase's `/auth-refresh` endpoint before processing requests.

**Ownership:**
Projects have `owner_user` or `owner_team`. PocketBase rules enforce that only owners can modify/delete.

**Workaround:**
Currently, after validating the JWT, backend uses hardcoded credentials to interact with PocketBase SDK. This is temporary—SDK doesn't support token-based auth yet.

## Extension Points

**New Registry Providers:**
Implement `RegistryProvider` trait in `rise-backend/src/registry/providers/`.

**New Build Methods:**
Future build module will support pluggable builders (buildpacks, nixpacks, Dockerfile).

**Additional Runtimes:**
Deploy module will support Kubernetes initially, with abstraction for future runtimes (ECS, Lambda, etc.).
