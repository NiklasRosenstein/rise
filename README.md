# Rise

A Rust-based platform for deploying containerized applications to Kubernetes and other runtimes.

## Overview

Rise consists of:
- **rise-backend**: REST API backend built with Axum, PostgreSQL, and Dex OAuth2/OIDC with embedded web frontend
- **rise-cli**: Command-line interface, including the backend and commands to manage teams, projects and deployments
- **PostgreSQL**: Primary database with SQLX migrations
- **Dex**: OAuth2/OIDC authentication provider

## Web Frontend

The Rise backend includes a read-only web frontend for viewing projects, teams, and deployments through a browser. All static assets are embedded directly into the binary using `rust-embed`, so no external files are needed for deployment.

### Accessing the Frontend

1. Start the backend server:
   ```bash
   mise run rise-backend
   # Or manually: cargo run --bin rise -- backend server
   ```

2. Open your browser to http://localhost:3000

3. Click "Login with OAuth" to authenticate via Dex

### Features

- **OAuth2 PKCE Authentication**: Secure browser-based authentication flow matching the CLI
- **Projects Dashboard**: View all projects, their status, and deployments
- **Teams Dashboard**: View teams and member counts
- **Deployment Tracking**: Real-time deployment status updates with auto-refresh
- **Deployment Logs**: View build logs inline
- **Responsive Design**: Works on desktop and mobile browsers
- **Single Binary Deployment**: All HTML/CSS/JS embedded in the binary

### Technology Stack

- **Backend**: Axum with rust-embed for embedded static files
- **Frontend**: Vanilla HTML/CSS/JavaScript (no build step required)
- **CSS Framework**: Pico CSS (classless, minimal)
- **Authentication**: OAuth2 PKCE flow (Web Crypto API for SHA-256)
- **Real-time Updates**: JavaScript polling (3-5 second intervals for active deployments)

## Local Development

### Prerequisites

- Docker and Docker Compose (for local development and testing)
- [mise](https://mise.jdx.dev/) (for task management)

To build and run the development environment, first run `mise install`. Then

- Run `mise start` to start the development stack.
- After code changes, `mise reload` is usually enough to recompile and restart the backend processes.

Services will be available at:

- **Backend API**: http://localhost:3000
- **Dex Auth**: http://localhost:5556/dex
- **PostgreSQL**: localhost:5432
- **Docker Registry**: http://localhost:5000

### Default Credentials

The development environment includes pre-configured test accounts:

#### PostgreSQL
- **Host**: `localhost`
- **Port**: `5432`
- **Database**: `rise`
- **Username**: `rise`
- **Password**: `rise123`

#### Dex Admin User (not currently treated specially by backend)

- **Email**: `admin@example.com`
- **Password**: `password`
- **Access**: Admin user for Dex web interface

#### Dex Test User

- **Email**: `test@example.com`
- **Password**: `password`
- **Access**: Regular user for testing OAuth2 authentication

> **Note**: These credentials are for development only. Change them for production deployments via environment variables in `docker-compose.yml`.

## CLI Usage

### Build the CLI

```bash
cargo build --bin rise-cli
```

### Authentication

```bash
# Login with device flow (browser-based)
./target/debug/rise-cli login

# Login with password (for testing)
./target/debug/rise-cli login --email test@example.com --password test1234
```

### Team Management

```bash
# Create a team (aliases: t, c/new)
./target/debug/rise-cli team create my-team --owners owner@example.com --members member@example.com
./target/debug/rise-cli t c my-team --owners owner@example.com
./target/debug/rise-cli t new my-team --members member@example.com

# List teams (aliases: t, ls/l)
./target/debug/rise-cli team list
./target/debug/rise-cli t ls
./target/debug/rise-cli t l

# Show team details (alias: s)
./target/debug/rise-cli team show my-team
./target/debug/rise-cli t s my-team

# Update team (alias: u/edit)
./target/debug/rise-cli team update my-team --add-members new@example.com
./target/debug/rise-cli t u my-team --remove-owners old@example.com

# Delete team (alias: del/rm)
./target/debug/rise-cli team delete my-team
./target/debug/rise-cli t del my-team
```

### Project Management

```bash
# Create a project (aliases: p, c/new)
./target/debug/rise-cli project create my-app --visibility public
./target/debug/rise-cli p c my-app --visibility public
./target/debug/rise-cli p new my-app

# List projects (aliases: p, ls/l)
./target/debug/rise-cli project list
./target/debug/rise-cli p ls
./target/debug/rise-cli p l

# Show project details (alias: s)
./target/debug/rise-cli project show my-app
./target/debug/rise-cli p s my-app

# Update project (rename, change visibility, transfer ownership) (alias: u/edit)
./target/debug/rise-cli project update my-app --visibility private
./target/debug/rise-cli p u my-app --owner team:devops
./target/debug/rise-cli p edit my-app --name new-name

# Delete project (alias: del/rm)
./target/debug/rise-cli project delete my-app
./target/debug/rise-cli p del my-app
```

### Deployment Management

```bash
# Create a deployment (aliases: d, c/new)
./target/debug/rise-cli deployment create my-app
./target/debug/rise-cli d c my-app
./target/debug/rise-cli d new my-app

# Deploy from specific directory
./target/debug/rise-cli d c my-app --path ./my-application

# Deploy pre-built image (skip build)
./target/debug/rise-cli d c my-app --image nginx:latest
./target/debug/rise-cli d c my-app --image myregistry.io/my-app:v1.2.3

# Deploy with custom group and expiration
./target/debug/rise-cli d c my-app --group mr/123 --expire 7d
./target/debug/rise-cli d c my-app --group staging --expire 24h

# List deployments (aliases: d, ls/l)
./target/debug/rise-cli deployment list my-app
./target/debug/rise-cli d ls my-app
./target/debug/rise-cli d l my-app --group default --limit 5

# Show deployment details (alias: s)
./target/debug/rise-cli deployment show my-app:20241205-1234
./target/debug/rise-cli d s my-app:20241205-1234

# Follow deployment until completion
./target/debug/rise-cli d s my-app:20241205-1234 --follow
./target/debug/rise-cli d s my-app:20241205-1234 --follow --timeout 10m

# Rollback to previous deployment
./target/debug/rise-cli deployment rollback my-app:20241205-1234
./target/debug/rise-cli d rollback my-app:20241205-1234

# Stop deployments in a group
./target/debug/rise-cli deployment stop my-app --group mr/123
./target/debug/rise-cli d stop my-app --group default
```

### Container Registry Integration

Rise supports multiple container registry providers for storing and deploying container images.

#### Supported Registries

**AWS ECR** - Amazon Elastic Container Registry with automatic temporary credential generation (12-hour tokens)
**JFrog Artifactory** - Enterprise registry with support for static credentials or Docker credential helper

#### Configuration

Registry configuration is optional and defined in `rise-backend/config/default.toml`:

##### AWS ECR Example

```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
# Optional: Provide AWS credentials (if not using IAM role)
# access_key_id = "AKIAIOSFODNN7EXAMPLE"
# secret_access_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
```

When AWS credentials are not provided, the backend will use the default AWS credential chain (environment variables, IAM role, etc.).

##### JFrog Artifactory Example (Static Credentials)

```toml
[registry]
type = "artifactory"
base_url = "https://mycompany.jfrog.io"
repository = "docker-local"
username = "myusername"
password = "mypassword"
```

##### JFrog Artifactory Example (Docker Credential Helper)

```toml
[registry]
type = "artifactory"
base_url = "https://mycompany.jfrog.io"
repository = "docker-local"
use_credential_helper = true
```

This uses Docker's credential helper to retrieve credentials (requires `docker login` to be run first).

#### API Endpoint

Once configured, the registry credentials endpoint will be available:

```bash
# Get registry credentials for a project
curl http://localhost:3000/registry/credentials?project=my-app \
  -H "Authorization: Bearer YOUR_TOKEN_HERE"
```

Response:
```json
{
  "credentials": {
    "registry_url": "123456789.dkr.ecr.us-east-1.amazonaws.com",
    "username": "AWS",
    "password": "eyJwYXl...truncated...",
    "expires_in": 43200
  },
  "repository": "my-app"
}
```

The CLI will automatically use this endpoint when building and pushing container images.

## Development

### Project Structure

```
rise/
├── rise-backend/          # Axum REST API server
│   ├── src/
│   │   ├── auth/         # Authentication handlers
│   │   ├── team/         # Team management
│   │   └── main.rs       # Server entry point
│   └── Dockerfile.backend
├── rise-cli/             # Command-line interface
│   └── src/
│       ├── login.rs      # Device flow & password auth
│       ├── team.rs       # Team commands
│       └── main.rs       # CLI entry point
├── migrations/           # SQLX PostgreSQL migrations
└── docker-compose.yml    # Development services (PostgreSQL, Dex, Registry)
```

### Working with the Backend

```bash
# Run backend locally with mise (recommended)
mise run rise-backend

# Or build and run manually
cd rise-backend
cargo build
cargo run

# Run tests
cargo test
```

The backend uses:
- **PostgreSQL** for persistent data storage (users, teams, projects, deployments)
- **Dex** for OAuth2/OIDC authentication (JWT tokens)
- **SQLX** for compile-time verified queries and migrations

**Configuration**: The backend uses `config/local.toml` for local development overrides (gitignored).

### Working with PostgreSQL

The backend uses SQLX for database management:

**Connecting to PostgreSQL**:
```bash
psql postgres://rise:rise123@localhost:5432/rise
```

**Creating Migrations**:
```bash
cd rise-backend
sqlx migrate add <migration_name>
# Edit the generated migration file in migrations/
sqlx migrate run
```

**Updating SQLX Cache** (after schema changes):
```bash
cargo sqlx prepare
```

### Database

The PostgreSQL data is stored in a Docker volume. To reset:

```bash
docker compose down -v  # -v removes volumes
docker compose up -d
cd rise-backend
sqlx migrate run        # Apply all migrations
```

## API Authentication

All team management endpoints require JWT authentication:

```bash
# Get auth token
curl -X POST http://localhost:3000/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test@example.com","password":"test1234"}'

# Use token in requests
curl http://localhost:3000/teams \
  -H "Authorization: Bearer YOUR_TOKEN_HERE"
```

Without a valid token, you'll receive:
```json
{"error":"Unauthorized: No authentication token provided"}
```

With an invalid/expired token:
```json
{"error":"Unauthorized: Invalid or expired token"}
```

## Environment Variables

### Backend (rise-backend)

| Variable | Default | Description |
|----------|---------|-------------|
| `RISE_SERVER__HOST` | `0.0.0.0` | Server bind address |
| `RISE_SERVER__PORT` | `3000` | Server port |
| `RISE_DATABASE__URL` | - | PostgreSQL connection string |
| `RISE_AUTH__ISSUER` | - | Dex issuer URL |
| `RISE_AUTH__CLIENT_ID` | - | Dex client ID |
| `RISE_AUTH__CLIENT_SECRET` | - | Dex client secret |
| `RUST_LOG` | `info` | Log level (error/warn/info/debug/trace) |
| `DATABASE_URL` | - | PostgreSQL connection for SQLX migrations |

## Troubleshooting

### "Team not found" when you should see "Unauthorized"

This was a bug in early versions. Ensure you're running the latest code where all team endpoints validate JWT tokens before database queries.

### SQLX migration errors

Check that migrations have been applied:

```bash
cd rise-backend
sqlx migrate info
sqlx migrate run
```

### Backend can't connect to PostgreSQL

Verify PostgreSQL is running and accessible:

```bash
psql postgres://rise:rise123@localhost:5432/rise -c "SELECT 1;"
docker compose ps postgres
```

### Dex authentication errors

Verify Dex is running:

```bash
curl http://localhost:5556/dex/.well-known/openid-configuration
```

## Contributing

1. Make changes in feature branches
2. Run tests: `cargo test`
3. Commit with descriptive messages
4. Include migration files if schema changed

## License

[Add your license here]
