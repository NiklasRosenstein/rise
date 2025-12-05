# Rise

A Rust-based platform for deploying containerized applications to Kubernetes and other runtimes.

## Overview

Rise consists of:
- **rise-backend**: REST API backend built with Axum, PostgreSQL, and Dex OAuth2/OIDC
- **rise-cli**: Command-line interface, including the backend and commands to manage teams, projects and deployments
- **PostgreSQL**: Primary database with SQLX migrations
- **Dex**: OAuth2/OIDC authentication provider

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
# Create a team
./target/debug/rise-cli team create my-team --owners owner@example.com --members member@example.com

# List teams
./target/debug/rise-cli team list

# Show team details
./target/debug/rise-cli team show my-team

# Update team
./target/debug/rise-cli team update my-team --add-members new@example.com

# Delete team
./target/debug/rise-cli team delete my-team
```

### Project Management

```bash
# Create a project
./target/debug/rise-cli project create my-app --visibility public

# List projects
./target/debug/rise-cli project list

# Show project details
./target/debug/rise-cli project show my-app

# Update project (rename, change visibility, transfer ownership)
./target/debug/rise-cli project update my-app --visibility private
./target/debug/rise-cli project update my-app --owner team:devops

# Delete project
./target/debug/rise-cli project delete my-app
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
