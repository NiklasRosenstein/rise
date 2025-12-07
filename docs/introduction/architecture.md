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
- Interactive authentication (OAuth2 authorization code flow with PKCE)
- Project and team management
- Application building and deployment

**Tech:** Rust, Clap, Reqwest, Axum (local callback server)

### PostgreSQL

Relational database for persistent storage:
- Tables for users, projects, teams, deployments
- Managed via SQLX migrations
- Connection pooling via sqlx::PgPool
- Compile-time verified queries

**Schema:** `rise-backend/migrations/*.sql`

### Dex

OAuth2/OIDC authentication provider:
- JWT token issuance via OAuth2 authorization code flow
- JWKS endpoint for token validation
- Configured via dev/dex/config.yaml
- Local development credentials: admin@example.com / admin

## Request Flow

### Authentication Flow (OAuth2 Authorization Code with PKCE)

```
┌──────────┐
│ rise-cli │  1. rise login
└────┬─────┘
     │ 2. Start local callback server (port 8765-8767)
     │ 3. Generate PKCE challenge
     │ 4. Open browser to Dex
     ▼
   ┌─────┐
   │ Dex │  5. User authenticates
   └──┬──┘
      │ 6. Redirect to http://localhost:8765/callback?code=xxx
      ▼
┌──────────────────┐
│ CLI callback     │
│ server receives  │
│ authorization    │
│ code             │
└────┬─────────────┘
     │ 7. POST /auth/code/exchange
     │    (code, code_verifier, redirect_uri)
     ▼
┌─────────────┐      8. Validate code    ┌─────┐
│ rise-backend├──────with PKCE verifier──►│ Dex │
└────┬────────┘      9. Return JWT       └─────┘
     │ 10. Return token to CLI
     ▼
┌──────────┐
│ CLI saves│
│   token  │
└──────────┘
```

### Subsequent API Requests

```
┌──────────┐
│ rise-cli │  Authorization: Bearer <token>
└────┬─────┘
     │ GET /projects
     ▼
┌─────────────┐      Validate token    ┌─────┐
│ rise-backend├──────via JWKS──────────►│ Dex │
│  Middleware │                         └─────┘
└────┬────────┘
     │ Token valid, extract user claims
     │ Query database
     ▼
┌────────────┐
│ PostgreSQL │
│  Database  │
└────┬───────┘
     │
     ▼
   Return
   response
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
