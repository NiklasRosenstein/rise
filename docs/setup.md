# Quick Setup

## Prerequisites

- Docker and Docker Compose
- Rust 1.91+
- [mise](https://mise.jdx.dev/) (recommended)

## Launch Services

```bash
mise install
mise backend:run
```

Services available at http://localhost:3000 (API and Web UI).

**Default credentials**: `admin@example.com` / `password` or `test@example.com` / `password`

## Build CLI

```bash
cargo build --bin rise
```

## First Steps

```bash
# Login (opens browser for OAuth)
rise login

# Create a project
rise project create my-app --visibility public

# Deploy
rise deployment create my-app --image nginx:latest
```

See [Authentication](authentication.md) for authentication details and [CLI Guide](cli.md) for all commands.

## Web UI

Navigate to http://localhost:3000 for the web dashboard (OAuth2 PKCE authentication, projects/teams management, deployment tracking).



## Reset Environment

```bash
docker-compose down -v
cargo clean
mise backend:run
```

## Next Steps

- **Learn CLI commands**: See [CLI Guide](cli.md)
- **Understand deployments**: See [Deployments](deployments.md)
- **Build images**: See [Building Images](builds.md)
- **Set up production**: See [Production Deployment](production.md)
