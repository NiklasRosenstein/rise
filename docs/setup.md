# Quick Setup

## Prerequisites

- Docker and Docker Compose
- Rust 1.91+ (for building from source)
- [mise](https://mise.jdx.dev/) (recommended for development)

## Launch Services

```bash
# Install development tools (if using mise)
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

## Build CLI

```bash
# Build from source
cargo build --bin rise

# The CLI is now available as 'rise' (if using direnv)
# Or use: ./target/debug/rise
```

## First Steps

### 1. Login

```bash
rise login
```

This will:
1. Open your browser to Dex authentication
2. Start a local callback server on port 8765 (or 8766/8767 if occupied)
3. Redirect back to CLI after successful authentication

See [Authentication](authentication.md) for more details on authentication flows.

### 2. Create a Project

```bash
rise project create my-app --visibility public
```

### 3. Create a Team

```bash
rise team create devops
```

### 4. Transfer Ownership

```bash
rise project update my-app --owner team:devops
```

### 5. Deploy an Application

```bash
# Deploy a pre-built image
rise deployment create my-app --image nginx:latest

# Or build and deploy from local directory
rise deployment create my-app
```

## Web UI

The Rise backend includes an embedded web frontend for monitoring projects and deployments through a browser. Simply navigate to http://localhost:3000 after starting the backend and log in with OAuth.

**Features:**
- OAuth2 PKCE authentication
- Projects and teams dashboard
- Real-time deployment tracking
- Deployment logs and status
- Responsive design (desktop and mobile)

See [Web Frontend](features/web-frontend.md) for details.

## Reset Environment

To completely reset your development environment:

```bash
# Stop all processes
overmind quit
docker-compose down -v  # -v removes volumes

# Remove build artifacts
cargo clean

# Start fresh
mise install
mise backend:run
```

## Next Steps

- **Learn CLI commands**: See [CLI Guide](cli.md)
- **Understand deployments**: See [Deployments](deployments.md)
- **Build images**: See [Building Images](builds.md)
- **Set up production**: See [Production Deployment](production.md)
