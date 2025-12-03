# Rise

A Rust-based platform for deploying containerized applications to Kubernetes and other runtimes.

## Overview

Rise consists of:
- **rise-backend**: REST API backend built with Axum and PocketBase
- **rise-cli**: Command-line interface for managing projects and teams
- **PocketBase**: Database and authentication provider

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust 1.91+ (for local development)
- [mise](https://mise.jdx.dev/) (for task management)

### Start Development Environment

**Option 1: Using Procfile (Recommended for Development)**

```bash
# Start all services with overmind (pocketbase, registry, backend)
overmind start

# Or start individual services
mise run rise-backend
```

**Option 2: Using Docker Compose Only**

```bash
# Start supporting services (PocketBase + Registry)
docker compose up -d

# Backend runs locally via mise
mise run rise-backend
```

Services will be available at:
- **Backend API**: http://localhost:3000
- **PocketBase Admin UI**: http://localhost:8090/_/
- **PocketBase API**: http://localhost:8090/api/
- **Docker Registry**: http://localhost:5000

### Default Credentials

The development environment includes pre-configured test accounts:

#### PocketBase Admin
- **Email**: `admin@example.com`
- **Password**: `admin123`
- **Access**: Full admin access to PocketBase Admin UI

#### Test User
- **Email**: `test@example.com`
- **Password**: `test1234`
- **Access**: Regular user for testing authentication

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
├── dev/pocketbase/       # PocketBase container setup
│   ├── README.md         # Detailed PocketBase docs
│   └── entrypoint.sh     # Auto-init script
├── pb_migrations/        # PocketBase schema migrations
└── docker-compose.yml    # Development services
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

The backend uses PocketBase for:
- User authentication (JWT tokens)
- Database (teams, projects collections)
- Admin UI for data management

**Configuration**: The backend uses `config/local.toml` for local development overrides (gitignored). This file is auto-created when you first run via mise and points to localhost services instead of Docker service names.

### Working with PocketBase

See [dev/pocketbase/README.md](dev/pocketbase/README.md) for detailed information about:
- Accessing the Admin UI
- Managing collections and migrations
- Configuring API rules
- Troubleshooting

### Making Schema Changes

1. Access PocketBase Admin UI at http://localhost:8090/_/
2. Modify collections (teams, projects, etc.)
3. PocketBase auto-generates migration files in `pb_migrations/`
4. Review and commit the migration files to git

### Database

The development database is stored in `pb_data/` (gitignored). To reset:

```bash
docker compose down
rm -rf pb_data/
docker compose up -d
```

PocketBase will re-create the database and apply all migrations from `pb_migrations/`.

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
| `RUST_LOG` | `info` | Log level (error/warn/info/debug/trace) |

### PocketBase

See [dev/pocketbase/README.md](dev/pocketbase/README.md#environment-variables) for PocketBase-specific variables.

## Troubleshooting

### "Team not found" when you should see "Unauthorized"

This was a bug in early versions. Ensure you're running the latest code where all team endpoints validate JWT tokens before database queries.

### PocketBase migration errors

Check that `pb_migrations/` is properly mounted and files are valid JavaScript:

```bash
docker compose logs pocketbase | grep migration
```

### Backend can't connect to PocketBase

Verify PocketBase is healthy:

```bash
curl http://localhost:8090/api/health
```

## Contributing

1. Make changes in feature branches
2. Run tests: `cargo test`
3. Commit with descriptive messages
4. Include migration files if schema changed

## License

[Add your license here]
