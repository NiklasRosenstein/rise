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

### Rise Backend Process

Single process running HTTP API server + all controllers (deployment, project, ECR) as concurrent tokio tasks.

Controllers are enabled automatically based on configuration:
- **Deployment**: Always enabled (backend determined by presence of `kubernetes` config)
- **Project**: Always enabled
- **ECR**: Enabled only when `registry.type = "ecr"`

### Mise Tasks

- `mise docs:serve` - Serve docs with live reload (port 3001)
- `mise db:migrate` - Run database migrations
- `mise backend:deps` - Start docker-compose services
- `mise backend:run` (alias: `mise br`) - Start backend with overmind
- `mise minikube:launch` - Start minikube with local registry

## Quick Start

```bash
# Install tools
mise install

# Start services
mise backend:deps

# Run migrations (auto-run before backend starts)
mise db:migrate

# Start backend (server + controllers)
mise backend:run  # or: mise br
```

Services available:
- Backend API: http://localhost:3000
- Web UI: http://localhost:3000
- Dex Auth: http://localhost:5556/dex
- PostgreSQL: localhost:5432
- Docker Registry: http://localhost:5000
- Registry UI: http://localhost:5001

### Build CLI

```bash
cargo build --bin rise
rise login  # If using direnv, 'rise' is in PATH
```

## Environment Variables

`.envrc` (loaded by direnv):

```bash
DATABASE_URL="postgres://rise:rise123@localhost:5432/rise"
RISE_CONFIG_RUN_MODE="development"
DOCKER_API_VERSION=1.44
PATH="$PATH:$PWD/target/debug"
```

Server config (host, port) in `rise-backend/config/default.toml`.

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
cd rise-backend
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

### Accessing Database

```bash
# Using psql
docker-compose exec postgres psql -U rise -d rise

# Or connection string
psql postgres://rise:rise123@localhost:5432/rise
```

### Default Credentials

**PostgreSQL:**
- Host: localhost:5432
- Database: rise
- Username: rise
- Password: rise123

**Dex:**
- Email: `admin@example.com` or `test@example.com`
- Password: `password`

⚠️ Development only. Change for production.

## Code Style

**Keep it simple:**
- Avoid over-engineering for hypothetical cases
- Don't add abstractions until needed
- Three similar lines > premature abstraction

**Error handling:**
- Use `anyhow::Result` for application code
- Use typed errors only when callers handle specific cases
- Provide context: `.context("what failed")`

**Documentation:**
- Document non-obvious behavior
- Explain "why" not "what"
- Update mdbook when adding features

## Testing

```bash
# Backend tests
cd rise-backend && cargo test

# Full integration test
docker-compose down -v
docker-compose up -d
cd rise-backend && sqlx migrate run && cd ..
rise login
rise project create test-app
```

## Commit Messages

Use conventional commits:

```
feat: add ECR registry support
fix: validate JWT before database queries
docs: update registry security notes
refactor: extract fuzzy matching to module
```

Include co-authorship when using AI:
```
Co-Authored-By: Claude <noreply@anthropic.com>
```

## Pull Requests

1. Create feature branch
2. Make focused changes (one feature per PR)
3. Add tests if adding features
4. Update documentation
5. Ensure `cargo test` passes
6. Include migration files if schema changed

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
