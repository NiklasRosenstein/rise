# CLI Basics

The Rise CLI (`rise`) provides commands for managing projects, teams, deployments, and service accounts. This guide covers common workflows and usage patterns.

## Installation

```bash
cargo build --bin rise
```

Binary location: `./target/debug/rise` (or use direnv to add to PATH automatically).

## Configuration

CLI stores configuration in `~/.config/rise/config.json` (created automatically on `rise login`).

## Command Structure

| Command | Aliases | Subcommands |
|---------|---------|-------------|
| `rise login` | - | - |
| `rise project` | `p` | `create` (`c`), `list` (`ls`), `show` (`s`), `update` (`u`), `delete` (`del`, `rm`) |
| `rise team` | `t` | `create` (`c`), `list` (`ls`), `show` (`s`), `update` (`u`), `delete` (`del`, `rm`) |
| `rise deployment` | `d` | `create` (`c`), `list` (`ls`), `show` (`s`), `rollback`, `stop` |
| `rise build` | - | - |
| `rise run` | - | - |
| `rise backend` | - | `server`, `check-config`, `dev-oidc-issuer` |

Use `rise --help` or `rise <command> --help` for details.

### Backend Commands

Backend commands are used for running and managing the Rise backend server:

```bash
# Start the backend server
rise backend server

# Check backend configuration for errors
rise backend check-config

# Run a local OIDC issuer for testing
rise backend dev-oidc-issuer --port 5678
```

The `check-config` command is particularly useful for:
- Validating configuration before deployment
- Checking for typos in configuration files
- Identifying unused/deprecated configuration options
- CI/CD pipeline validation steps

## Common Workflows

### Authentication

```bash
rise login  # Opens browser for OAuth2 via Dex

# Authenticate with a different backend
rise login --url https://rise.example.com

# Use device flow (limited compatibility)
rise login --device
```

**Environment variables:**
- `RISE_URL`: Set default backend URL
- `RISE_TOKEN`: Set authentication token

### Project Management

```bash
# Create project on backend only (remote mode - auto-selected if rise.toml exists)
rise project create my-app --access-class public
rise project create internal-api --access-class private --owner team:backend

# Create project on backend and rise.toml (remote+local mode - auto-selected if no rise.toml)
rise project create my-new-app

# Explicit mode selection
rise project create my-app --mode remote              # Backend only
rise project create my-app --mode local               # rise.toml only  
rise project create my-app --mode remote+local        # Both backend and rise.toml

# Create from existing rise.toml (auto-detects remote mode, reads name from rise.toml)
rise project create

# Or explicitly with mode flag
rise project create --mode remote

# List
rise p ls

# Update
rise p update my-app --owner team:devops
```

**Project creation modes:**
- **`--mode remote`** (default if rise.toml exists): Creates/updates project on backend only
- **`--mode local`**: Creates/updates `rise.toml` only, does not touch backend
- **`--mode remote+local`** (default if no rise.toml): Creates project on backend AND creates `rise.toml`
- **Auto-detection**: If `--mode` is not specified, automatically uses `remote` if `rise.toml` exists, otherwise `remote+local`

### Deployments

```bash
# Deploy from current directory
rise deployment create my-app

# Deploy from specific directory (positional arg)
rise deployment create my-app ./path/to/app

# Deploy pre-built image
rise d c my-app --image nginx:latest --http-port 80

# Deploy to custom group with expiration
rise d c my-app --group mr/123 --expire 7d

# Monitor
rise d show my-app:20241205-1234 --follow --timeout 10m

# Rollback
rise deployment rollback my-app:20241205-1234

# Stop
rise deployment stop my-app --group mr/123
```

**Key deployment options:**
- `path` (positional): Application directory (defaults to current directory)
- `--group <name>`: Deploy to custom group (e.g., `mr/123`, `staging`)
- `--expire <duration>`: Auto-delete after duration (e.g., `7d`, `24h`)
- `--image <image>`: Use pre-built image (requires `--http-port`)
- `--http-port <port>`: HTTP port application listens on (required with `--image`, defaults to 8080 for builds)

### Local Development

```bash
# Build and run locally (defaults to port 8080)
rise run

# Specify directory
rise run ./path/to/app

# Custom port (sets PORT env var and exposes on host)
rise run --http-port 3000

# Expose on different host port
rise run --http-port 8080 --expose 3000

# Load environment variables from a project
rise run --project my-app

# Set runtime environment variables
rise run --run-env DATABASE_URL=postgres://localhost/mydb --run-env DEBUG=true

# With custom build backend
rise run --backend pack
```

**Key options:**
- `path` (positional): Application directory (defaults to current directory)
- `--project <name>`: Project name to load non-secret environment variables from
- `--http-port <port>`: HTTP port the application listens on (also sets PORT env var) [default: 8080]
- `--expose <port>`: Port to expose on the host (defaults to same as http-port)
- `--run-env <KEY=VALUE>`: Runtime environment variables (can be specified multiple times)
- Build flags: `--backend`, `--builder`, `--buildpack`, `--container-cli`, etc.

**Notes:**
- Sets `PORT` environment variable automatically
- Loads non-secret environment variables from the project if `--project` is specified
- Secret environment variables are not loaded (their actual values are not retrievable)
- Runs with `docker run --rm -it` (automatically removes container on exit)

### Team Management

```bash
# Create
rise team create backend-team --owners alice@example.com --members bob@example.com

# List
rise t ls

# Add members
rise t update backend-team --add-members charlie@example.com
```

## Advanced Features

### Deployment Groups

- **`default`**: Primary deployment
- **Custom groups**: Additional deployments (e.g., `mr/123`, `staging`)

```bash
rise d c my-app --group mr/123 --expire 7d
```

### Auto-Expiration

```bash
rise d c my-app --group staging --expire 7d  # Days
rise d c my-app --group preview --expire 24h  # Hours
```

Supported units: `h`, `d`, `w`.

## Next Steps

- **Learn about deployments**: See [Deployments](deployments.md)
- **Service accounts for CI/CD**: See [Authentication](authentication.md#service-accounts-workload-identity)
