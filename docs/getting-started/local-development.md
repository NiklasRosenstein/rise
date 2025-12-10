# Local Development

This guide explains how to set up Rise for local development using the integrated tooling: `mise`, `docker-compose`, and `overmind`.

## Prerequisites

Before you begin, ensure you have the following installed:

- **Docker and Docker Compose**: Required for running dependencies (PostgreSQL, Dex, Docker Registry)
- **Rust 1.91+**: For building the Rise backend and CLI
- **[mise](https://mise.jdx.dev/)**: Task runner and tool version manager
- **[direnv](https://direnv.net/)** (optional but recommended): Automatically loads environment variables from `.envrc`

## Understanding the Development Stack

Rise uses a multi-component development setup that integrates several tools:

### Docker Compose Services

The `docker-compose.yml` defines four services that the Rise backend depends on:

| Service | Port | Purpose |
|---------|------|---------|
| **postgres** | 5432 | PostgreSQL database for storing projects, teams, deployments, etc. |
| **dex** | 5556 | OAuth2/OIDC provider for authentication |
| **registry** | 5000 | Docker registry for storing container images |
| **registry-ui** | 5001 | Web UI for browsing container images in the registry |

All services use Docker volumes for persistence and are configured for development (not production).

### Procfile Processes

The `Procfile.dev` defines the Rise backend process managed by `overmind`:

| Process | Command | Purpose |
|---------|---------|---------|
| **server** | `cargo run --bin rise -- backend server` | HTTP API server + all controllers (port 3000) |

The backend runs as a single process with all controllers (deployment, project, ECR) running as concurrent tasks within the same process. Controllers are automatically enabled based on the configuration.

### Mise Tasks

The `mise.toml` file defines convenient tasks for development:

| Task | Description |
|------|-------------|
| `mise docs:serve` | Serve documentation with live reload on port 3001 |
| `mise db:migrate` | Run database migrations (auto-run before backend starts) |
| `mise backend:deps` | Start all docker-compose services |
| `mise backend:run` (alias: `mise br`) | Start all backend processes with overmind |
| `mise minikube:launch` | Start minikube with local registry access |

## Getting Started

### 1. Install Development Tools

```bash
# Install mise-managed tools (overmind, pack, minikube)
mise install
```

This installs:
- **overmind**: Process manager for running multiple services
- **pack**: Buildpacks CLI (future feature for building images)
- **minikube**: Local Kubernetes (future Kubernetes controller)

### 2. Start Docker Compose Services

```bash
# Start postgres, dex, registry, and registry-ui
mise backend:deps
```

This is equivalent to `docker-compose up -d`.

### 3. Run Database Migrations

Migrations run automatically when you use `mise backend:run`, but you can run them manually:

```bash
mise db:migrate
```

### 4. Start the Backend

```bash
# Start all backend processes (server + controllers)
mise backend:run

# Or use the alias
mise br
```

This command:
1. Ensures docker-compose services are running (`backend:deps`)
2. Runs database migrations (`db:migrate`)
3. Starts the backend server (with all controllers) using `overmind`

You'll see log output from the server process with controller activity logged inline:
```
server | HTTP server listening on http://0.0.0.0:3000
server | Starting deployment controller (backend: docker)
server | Starting project controller
server | Starting ECR controller
```

### 5. Verify Services Are Running

Once started, these services are available:

- **Backend API**: http://localhost:3000
- **Web UI**: http://localhost:3000 (embedded in backend)
- **Dex Auth**: http://localhost:5556/dex
- **PostgreSQL**: localhost:5432
- **Docker Registry**: http://localhost:5000
- **Registry UI**: http://localhost:5001

### 6. Build and Use the CLI

In a separate terminal:

```bash
# Build the CLI
cargo build --bin rise

# Login to the backend
# (if using direnv, 'rise' is available directly)
rise login

# Create a project
rise project create my-first-app

# Deploy it (when you have a Dockerfile or image)
rise deployment create my-first-app
```

**Note**: If you're using `direnv`, the `.envrc` file adds `./target/debug` to your `PATH`, so you can run `rise` directly instead of `./target/debug/rise`.

## Environment Variables

The `.envrc` file (loaded automatically by `direnv`) sets environment variables for development:

```bash
# Database connection
DATABASE_URL="postgres://rise:rise123@localhost:5432/rise"

# Rise configuration
RISE_CONFIG_RUN_MODE="development"

# Docker API version
DOCKER_API_VERSION=1.44

# Add debug binaries to PATH
PATH="$PATH:$PWD/target/debug"
```

Configuration settings like server host and port are specified in `rise-backend/config/default.toml`.

If you don't use `direnv`, manually source this file:
```bash
source .envrc
```

## Development Workflow

### Making Code Changes

1. **Edit code** in your editor
2. **Reload the backend**:
   ```bash
   mise backend:reload  # or:br
   ```
3. **Check logs** in the overmind terminal for errors
4. **Test changes** using the CLI or web UI

The `backend:reload` task recompiles and restarts all backend processes.

### Viewing Logs

**Overmind logs**: The terminal running `mise backend:run` shows logs from all processes.

**Docker Compose logs**:
```bash
docker-compose logs -f postgres
docker-compose logs -f dex
docker-compose logs -f registry
```

**Connect to the process** (when using overmind):
```bash
overmind connect server
# Press Ctrl+B then D to detach
```

### Accessing the Database

Connect to PostgreSQL directly:

```bash
# Using psql
docker-compose exec postgres psql -U rise -d rise

# Or use connection string
psql postgres://rise:rise123@localhost:5432/rise
```

### Accessing the Registry

**Web UI**: http://localhost:5001

**Command line**:
```bash
# List repositories
curl http://localhost:5000/v2/_catalog

# List tags for a repository
curl http://localhost:5000/v2/my-app/tags/list
```

## Default Credentials

The development environment includes pre-configured accounts for testing:

### PostgreSQL
- **Host**: `localhost`
- **Port**: `5432`
- **Database**: `rise`
- **Username**: `rise`
- **Password**: `rise123`

### Dex Users

**Admin User**
- **Email**: `admin@example.com`
- **Password**: `password`

**Test User**
- **Email**: `test@example.com`
- **Password**: `password`

> **Warning**: These credentials are for development only. Change them for production deployments via environment variables in `docker-compose.yml`.

## Troubleshooting

### "Connection refused" to PostgreSQL

**Problem**: Backend can't connect to database.

**Solution**:
```bash
# Check if postgres is running
docker-compose ps

# Restart postgres
docker-compose restart postgres

# Check health
docker-compose exec postgres pg_isready -U rise
```

### "Address already in use" on port 3000

**Problem**: Another process is using port 3000.

**Solution**:
```bash
# Find the process
lsof -i :3000

# Kill it or change the port in rise-backend/config/local.toml
```

### Overmind won't start processes

**Problem**: Procfile.dev processes fail to start.

**Solution**:
```bash
# Stop overmind
overmind quit

# Ensure dependencies are running
mise backend:deps

# Run migrations manually
mise db:migrate

# Try starting again
mise backend:run
```

### Docker Compose services won't start

**Problem**: Docker containers fail to start.

**Solution**:
```bash
# Check logs
docker-compose logs

# Remove containers and volumes (WARNING: deletes data)
docker-compose down -v

# Start fresh
mise backend:deps
```

### Reset Everything

If you need to completely reset your development environment:

```bash
# Stop all processes
overmind quit
docker-compose down -v

# Remove build artifacts
cargo clean

# Start fresh
mise install
mise backend:run
```

## Next Steps

- **Learn CLI commands**: See [CLI Basics](./cli-basics.md)
- **Understand deployments**: See [Deployments](../core-concepts/deployments.md)
- **Contribute code**: See [Contributing](../development/contributing.md)
- **Follow a tutorial**: Check out [example/hello-world](../../example/hello-world/README.md)
