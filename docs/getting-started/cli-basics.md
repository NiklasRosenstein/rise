# CLI Basics

The Rise CLI (`rise`) provides commands for managing projects, teams, deployments, and service accounts. This guide covers common workflows and usage patterns.

## Installation

Build the CLI from source:

```bash
cargo build --bin rise
```

The binary will be located at `./target/debug/rise` (or `./target/release/rise` for optimized builds).

**Using direnv** (recommended): The `.envrc` file automatically adds `./target/debug` to your `PATH`, so you can run `rise` directly.

**Without direnv**: Either use the full path `./target/debug/rise` or manually add to your PATH:

```bash
export PATH="$PATH:$PWD/target/debug"
```

## Configuration

The CLI stores configuration in `~/.config/rise/config.json`:

```json
{
  "backend_url": "http://localhost:3000",
  "access_token": "...",
  "refresh_token": "..."
}
```

This file is automatically created and updated when you run `rise login`.

## Command Structure

Rise CLI uses a hierarchical command structure with **aliases** for convenience:

| Command | Aliases | Subcommands |
|---------|---------|-------------|
| `rise login` | - | - |
| `rise project` | `p` | `create` (`c`, `new`), `list` (`ls`, `l`), `show` (`s`), `update` (`u`, `edit`), `delete` (`del`, `rm`) |
| `rise team` | `t` | `create` (`c`, `new`), `list` (`ls`, `l`), `show` (`s`), `update` (`u`, `edit`), `delete` (`del`, `rm`) |
| `rise deployment` | `d` | `create` (`c`, `new`), `list` (`ls`, `l`), `show` (`s`), `rollback`, `stop` |

**Tip**: Use `rise --help` or `rise <command> --help` for detailed information on any command.

## Common Workflows

### 1. Authenticate

Login to the Rise backend using OAuth2:

```bash
rise login
```

This opens your browser for authentication via Dex. After successful login, your tokens are stored locally.

### 2. Create a Project

Projects represent deployable applications:

```bash
# Create a public project
rise project create my-app --visibility public

# Create a private project owned by a team
rise project create internal-api --visibility private --owner team:backend

# Using aliases
rise p c my-app
```

**Visibility options:**
- `public`: Accessible to anyone (future: no ingress auth)
- `private`: Restricted access (future: ingress auth required)

### 3. List Your Projects

```bash
# List all projects
rise project list

# Using alias
rise p ls
```

Output shows:
- Project name
- Status (`running`, `stopped`, `pending`)
- URL (when deployed)
- Visibility
- Owner

### 4. Create a Deployment

Deploy your application:

```bash
# Deploy from current directory (looks for Dockerfile)
rise deployment create my-app

# Deploy from specific directory
rise d c my-app --path ./my-application

# Deploy pre-built image (skip build)
rise d c my-app --image nginx:latest
```

**Deployment options:**
- `--group <name>`: Deploy to custom group (e.g., `mr/123`, `staging`)
- `--expire <duration>`: Auto-delete after duration (e.g., `7d`, `24h`)
- `--image <image>`: Use pre-built image instead of building
- `--path <path>`: Application directory (default: current directory)

### 5. Monitor a Deployment

```bash
# Show deployment details
rise deployment show my-app:20241205-1234

# Follow deployment until completion
rise d s my-app:20241205-1234 --follow --timeout 10m
```

The `--follow` flag auto-refreshes the deployment status until it reaches a terminal state (running, failed, or stopped).

### 6. Manage Teams

Teams allow collaborative project management:

```bash
# Create a team
rise team create backend-team --owners alice@example.com --members bob@example.com

# List teams
rise t ls

# Add members
rise t update backend-team --add-members charlie@example.com

# Show team details
rise t show backend-team
```

### 7. Rollback a Deployment

```bash
# Rollback to previous deployment
rise deployment rollback my-app:20241205-1234
```

Rollback creates a new deployment with the same configuration as the target deployment.

### 8. Stop Deployments

```bash
# Stop all deployments in default group
rise deployment stop my-app

# Stop deployments in specific group
rise d stop my-app --group mr/123
```

## Advanced Features

### Fuzzy Matching

Commands support fuzzy matching for team and project names. If there's ambiguity, Rise suggests the closest match:

```bash
$ rise p c secret-app --owner team:devopsy
Team 'devopsy' does not exist or you do not have permission. Did you mean 'devops'?
```

### Deployment Groups

Deployment groups allow multiple deployments of the same project:

- **`default`**: Primary deployment (accessible at `https://my-app.rise.net`)
- **Custom groups**: Additional deployments (e.g., `mr/123`, `staging`, `preview/feature-x`)

Use `--group` to specify:

```bash
# Create preview deployment for MR 123
rise d c my-app --group mr/123 --expire 7d

# List deployments in specific group
rise d ls my-app --group mr/123
```

### Auto-Expiration

Set deployments to automatically delete after a duration:

```bash
# Expire after 7 days
rise d c my-app --group staging --expire 7d

# Expire after 24 hours
rise d c my-app --group preview/test --expire 24h
```

Supported units: `h` (hours), `d` (days), `w` (weeks).

## Tips and Tricks

### Use Aliases

Save typing with command aliases:

```bash
# Instead of:
rise project create my-app
rise deployment create my-app

# Use:
rise p c my-app
rise d c my-app
```

### Check Status

Always check `rise p ls` to see your projects and their URLs after deploying.

### Follow Deployments

Use `--follow` to watch deployments in real-time instead of manually checking status:

```bash
rise d s my-app:latest --follow
```

### Context from .rise.toml

Future: The CLI will support `.rise.toml` files in your project directory to specify defaults:

```toml
project = "my-app"

[build]
backend = "buildpacks"
```

## Troubleshooting

### "Unauthorized" errors

Your token may have expired. Re-authenticate:

```bash
rise login
```

### "Project not found"

Check the exact project name:

```bash
rise p ls
```

### Deployment fails

Check deployment logs (future feature) or check the backend logs locally:

```bash
# In development
overmind connect backend-deployment
```

## Next Steps

- **Full tutorial**: See [example/hello-world](../../example/hello-world/README.md)
- **Service accounts for CI/CD**: See [Service Accounts](../features/service-accounts.md)
- **Learn about deployments**: See [Deployments](../core-concepts/deployments.md)
- **Web UI**: See [Web Frontend](../features/web-frontend.md)
