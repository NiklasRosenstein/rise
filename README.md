# Rise

A Rust-based platform for deploying containerized applications with minimal configuration.

## What is Rise?

Rise simplifies container deployment by providing:
- **Simple CLI** for building and deploying apps
- **Multi-tenant projects** with team collaboration
- **OAuth2 authentication** via Dex
- **Multiple registry backends** (AWS ECR, Docker)
- **Service accounts** for CI/CD integration
- **Web dashboard** for monitoring deployments

## Features

- **Project & Team Management**: Organize apps and collaborate with teams
- **OAuth2/OIDC Authentication**: Secure authentication via Dex
- **Multi-Registry Support**: AWS ECR, Docker Registry (Harbor, Quay, etc.)
- **Service Accounts**: Workload identity for GitHub Actions, GitLab CI
- **Multi-Process Architecture**: Separate controllers for deployments, projects, ECR
- **Embedded Web Frontend**: Single-binary deployment with built-in UI

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust 1.91+
- [mise](https://mise.jdx.dev/) (recommended for development)

### Start Services

```bash
# Install development tools
mise install

# Start all services (postgres, dex, registry, backend)
mise backend:run
```

Services will be available at:
- **Backend API**: http://localhost:3000
- **Web UI**: http://localhost:3000
- **PostgreSQL**: localhost:5432

**Default credentials**:
- Email: `admin@example.com` or `test@example.com`
- Password: `password`

### Build and Use CLI

```bash
# Build the CLI
cargo build --bin rise

# The CLI is now available as 'rise' (if using direnv)
# Or use the full path: ./target/debug/rise

rise login
rise project create my-app
rise deployment create my-app --image nginx:latest
```

## Documentation

Comprehensive documentation is available in [`/docs`](./docs):

**Getting Started**:
- [Quick Start](docs/getting-started/README.md) - Setup and first steps
- [Local Development](docs/getting-started/local-development.md) - mise, docker-compose, Procfile
- [CLI Basics](docs/getting-started/cli-basics.md) - Common CLI workflows

**Core Concepts**:
- [Authentication](docs/core-concepts/authentication.md) - OAuth2 flows, tokens
- [Projects & Teams](docs/core-concepts/projects-teams.md) - Organizing applications
- [Deployments](docs/core-concepts/deployments.md) - Deployment lifecycle

**Features**:
- [Service Accounts](docs/features/service-accounts.md) - CI/CD integration
- [Container Registry](docs/features/registry.md) - Multi-registry support
- [Web Frontend](docs/features/web-frontend.md) - Embedded web UI

**Deployment**:
- [Configuration Guide](docs/deployment/configuration.md) - Environment variables and config files
- [AWS ECR](docs/deployment/aws-ecr.md) - Production ECR setup with Terraform
- [Docker (Local)](docs/deployment/docker-local.md) - Local registry
- [Production Setup](docs/deployment/production.md) - Security, monitoring, HA

**Development**:
- [Contributing](docs/development/contributing.md) - Development guidelines
- [Database](docs/development/database.md) - PostgreSQL, migrations, SQLX
- [Testing](docs/development/testing.md) - Testing strategies

## Architecture

Rise uses a multi-process architecture:

| Component | Purpose |
|-----------|---------|
| **rise-backend (server)** | HTTP API with embedded web frontend |
| **rise-backend (controllers)** | Deployment, project, and ECR reconciliation |
| **rise-cli** | Command-line interface |
| **PostgreSQL** | Database for projects, teams, deployments |
| **Dex** | OAuth2/OIDC provider for authentication |

See [Architecture](docs/introduction/architecture.md) for details.

## Project Status

**Production Ready**:
- ✅ OAuth2 PKCE authentication
- ✅ Project & team management
- ✅ Service accounts (workload identity for CI/CD)
- ✅ AWS ECR integration with Terraform module
- ✅ Docker controller with health monitoring
- ✅ Embedded web frontend
- ✅ Deployment rollback and expiration

**In Development**:
- 🚧 Kubernetes controller
- 🚧 Additional registry providers
- 🚧 Build integrations (buildpacks, nixpacks)

## Contributing

Contributions are welcome! See [Contributing](docs/development/contributing.md) for:
- Development environment setup
- Code style guidelines
- Testing requirements
- Commit conventions

## License

[Add your license here]
