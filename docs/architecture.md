# Architecture

## Components

### rise-backend

Axum-based REST API handling:
- Authentication (JWT validation via Dex OAuth2/OIDC)
- Project/team CRUD with ownership model
- Registry credential generation
- Future: Build orchestration, deployments

**Tech:** Rust, Axum, SQLX, PostgreSQL, AWS SDK

### rise-cli

Command-line interface for:
- Interactive authentication (device flow)
- Project and team management
- Future: Building and deploying applications

**Tech:** Rust, Clap, Reqwest

### PostgreSQL

Relational database for persistent storage:
- Tables for users, projects, teams, deployments
- Managed via SQLX migrations
- Connection pooling via sqlx::PgPool
- Compile-time verified queries

**Schema:** `rise-backend/migrations/*.sql`

### Dex

OAuth2/OIDC authentication provider:
- JWT token issuance
- Device flow and password authentication
- JWKS endpoint for token validation
- Configured via dex/config.yaml

## Request Flow

```
┌──────────┐
│ rise-cli │
└────┬─────┘
     │ 1. POST /login
     ▼
┌─────────────┐      2. OAuth2 flow    ┌─────┐
│ rise-backend├──────credentials────────►│ Dex │
└────┬────────┘      3. Return JWT      └─────┘
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
┌─────────────┐      Validate token    ┌─────┐
│ rise-backend├──────via JWKS──────────►│ Dex │
└────┬────────┘                         └─────┘
     │
     │ Query database
     ▼
┌────────────┐
│ PostgreSQL │
│  Database  │
└────────────┘
```

## Security Model

**Token Validation:**
All protected endpoints validate JWT tokens by verifying signatures against Dex's JWKS endpoint. The middleware extracts user information from validated tokens.

**Ownership:**
Projects have `owner_user_id` or `owner_team_id`. Database queries and application logic enforce that only owners can modify/delete resources.

**Authorization:**
The backend uses database-level checks to verify ownership and team membership before allowing operations on protected resources.

## Extension Points

**New Registry Providers:**
Implement `RegistryProvider` trait in `rise-backend/src/registry/providers/`.

**New Build Methods:**
Future build module will support pluggable builders (buildpacks, nixpacks, Dockerfile).

**Additional Runtimes:**
Deploy module will support Kubernetes initially, with abstraction for future runtimes (ECS, Lambda, etc.).
