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

Use `rise --help` or `rise <command> --help` for details.

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
# Create
rise project create my-app --visibility public
rise project create internal-api --visibility private --owner team:backend

# List
rise p ls

# Update
rise p update my-app --owner team:devops
```

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
