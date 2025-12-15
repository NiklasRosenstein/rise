# Local Development

## Prerequisites

- Docker and Docker Compose
- Rust 1.91+
- [mise](https://mise.jdx.dev/) - Task runner and tool version manager
- [direnv](https://direnv.net/) (optional) - Auto-loads `.envrc`

## Development Stack

### Docker Compose Services

| Service | Port | Purpose |
|---------|------|---------|
| **postgres** | 5432 | PostgreSQL database |
| **dex** | 5556 | OAuth2/OIDC provider |
| **registry** | 5000 | Docker registry |
| **registry-ui** | 5001 | Registry web UI |

### Rise Backend

Single process running HTTP API server + controllers (deployment, project, ECR) as concurrent tokio tasks. Controllers enabled automatically based on config.

### Mise Tasks

- `mise docs:serve` - Serve docs (port 3001)
- `mise db:migrate` - Run migrations
- `mise backend:deps` - Start docker-compose services
- `mise backend:run` (alias: `mise br`) - Start backend with overmind
- `mise minikube:launch` - Start Minikube with local registry

## Quick Start

```bash
mise install
mise backend:run  # Starts services + backend
```

Services: http://localhost:3000 (API, Web UI), localhost:5432 (PostgreSQL), http://localhost:5000 (Registry)

### Registry Configuration for Local Development

When using Docker Compose with Minikube, you may need different registry URLs for:
- **Deployment controllers** (running in Minikube): `rise-registry:5000` (Docker internal network)
- **CLI** (running on host): `localhost:5000` (host network)

Configure this in `config/default.yaml`:

```yaml
registry:
  type: "oci-client-auth"
  registry_url: "rise-registry:5000"      # Internal URL for deployment controllers
  namespace: "rise-apps/"
  client_registry_url: "localhost:5000"   # Client-facing URL for CLI push operations
```

The `client_registry_url` is optional and defaults to `registry_url` if not specified. The API returns `client_registry_url` to CLI clients for push operations, while deployment controllers use `registry_url` for image references.

### Build CLI

```bash
cargo build --bin rise
```

## Environment Variables

`.envrc` (loaded by direnv): `DATABASE_URL`, `RISE_CONFIG_RUN_MODE`, `PATH`

Server config in `config/default.toml`.

## Development Workflow

### Making Changes

**Backend:**
```bash
# Edit code
mise backend:reload  # or: mise br
```

**CLI:**
```bash
cargo build --bin rise
rise <command>
```

**Schema:**
```bash
sqlx migrate add <migration_name>
# Edit migration in migrations/
sqlx migrate run
cargo sqlx prepare  # Update query cache
```

### Viewing Logs

**Overmind**: Terminal running `mise backend:run`

**Docker Compose:**
```bash
docker-compose logs -f postgres
docker-compose logs -f dex
```

**Connect to process:**
```bash
overmind connect server
# Ctrl+B then D to detach
```

## Local Kubernetes Development

For testing the Kubernetes controller locally, use Minikube:

```bash
mise minikube:launch
```

This starts a local Kubernetes cluster with an embedded container registry. The backend will automatically deploy applications to Minikube when configured for Kubernetes.

For installation and advanced usage, see the [Minikube documentation](https://minikube.sigs.k8s.io/docs/start/).

### Accessing Database

```bash
# Using psql
docker-compose exec postgres psql -U rise -d rise

# Or connection string
psql postgres://rise:rise123@localhost:5432/rise
```

### Default Credentials

**PostgreSQL:** `postgres://rise:rise123@localhost:5432/rise`

**Dex:** `admin@example.com` / `password` or `test@example.com` / `password`

## Code Style

- Avoid over-engineering; add abstractions only when needed
- Use `anyhow::Result` for application code, typed errors only when callers need specific handling
- Document non-obvious behavior and rationale
- Update docs when adding features

## Testing

```bash
cargo test
```

## Commit Messages

Use conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`

## Troubleshooting

See [Troubleshooting](troubleshooting.md) for common issues.

**Reset everything:**
```bash
overmind quit
docker-compose down -v
cargo clean
mise install
mise backend:run
```
