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

### Start Development Environment

```bash
# Start all services (PocketBase + Backend)
docker compose up -d

# Check service status
docker compose ps

# View logs
docker compose logs -f
```

Services will be available at:
- **Backend API**: http://localhost:3001
- **PocketBase Admin UI**: http://localhost:8090/_/
- **PocketBase API**: http://localhost:8090/api/

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
# Build backend
cd rise-backend
cargo build

# Run tests
cargo test

# Run locally (requires PocketBase running)
RUST_LOG=debug cargo run
```

The backend uses PocketBase for:
- User authentication (JWT tokens)
- Database (teams, projects collections)
- Admin UI for data management

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
curl -X POST http://localhost:3001/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test@example.com","password":"test1234"}'

# Use token in requests
curl http://localhost:3001/teams \
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
