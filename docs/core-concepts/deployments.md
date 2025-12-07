# Deployments

Deployments in Rise represent immutable instances of your application running in the container runtime.

## What is a Deployment?

A **deployment** is a specific version of your project that has been built, pushed to a container registry, and deployed to the runtime (currently Docker, future: Kubernetes).

Key characteristics:
- **Immutable**: Once created, a deployment's configuration cannot be changed
- **Timestamped**: Each deployment has a unique timestamp ID (e.g., `my-app:20241205-1234`)
- **Tracked**: Deployments have status, health checks, and logs
- **Rollback-able**: You can rollback to any previous deployment

## Deployment Lifecycle

### 1. Creation

When you run `rise deployment create my-app`, the following happens:

1. **Build** (optional): If no `--image` is provided, Rise builds a container image from your application
2. **Push**: The image is pushed to the configured container registry
3. **Store**: Deployment metadata is saved to the database with a digest-pinned image reference
4. **Deploy**: The deployment controller creates/updates the container in the runtime

### 2. Running

Once deployed, the deployment enters the `running` state. The deployment controller:

- **Monitors health**: Periodically checks container health
- **Updates status**: Reflects actual runtime state in the database
- **Handles failures**: Marks deployments as `failed` if containers crash

### 3. Stopping

Deployments can be stopped manually or automatically:

```bash
# Stop all deployments in a group
rise deployment stop my-app --group default
```

Stopped deployments:
- Remain in the database
- Can be rolled back to
- Don't consume runtime resources

### 4. Expiration

Deployments can auto-delete after a specified duration:

```bash
# Delete automatically after 7 days
rise d c my-app --group mr/123 --expire 7d
```

This is useful for:
- Preview deployments for merge requests
- Staging environments
- Temporary testing environments

## Deployment Groups

Projects can have multiple active deployments using **deployment groups**:

### Default Group

The **`default`** group represents the primary deployment:

```bash
rise deployment create my-app
# Accessible at: https://my-app.rise.net
```

### Custom Groups

Create additional deployments with custom group names:

```bash
# Merge request preview
rise d c my-app --group mr/123 --expire 7d

# Staging environment
rise d c my-app --group staging

# Feature branch
rise d c my-app --group feature/new-auth
```

Custom groups allow:
- **Multiple concurrent deployments** of the same project
- **Isolated testing** without affecting production
- **Preview environments** for code review

**Note**: Currently, custom groups don't have dedicated URLs. This will be added when the Kubernetes controller is implemented.

## Pre-built Images

Skip the build step by deploying pre-built images:

```bash
# Deploy from Docker Hub
rise d c my-app --image nginx:latest

# Deploy from private registry
rise d c my-app --image myregistry.io/my-app:v1.2.3

# Deploy from AWS ECR
rise d c my-app --image 123456789.dkr.ecr.us-east-1.amazonaws.com/my-app:sha256-abc123
```

When using `--image`:
- No build occurs
- The image is pulled directly from the specified registry
- The deployment is pinned to the exact digest of the image

## Following Deployments

Monitor deployment progress in real-time:

```bash
# Follow until deployment reaches terminal state
rise d s my-app:latest --follow

# Follow with timeout
rise d s my-app:latest --follow --timeout 10m
```

The `--follow` flag auto-refreshes the deployment status and shows:
- Current state (`pending`, `running`, `failed`, `stopped`)
- Health status
- Deployment events (future)

## Rollback

Rollback creates a new deployment with the same configuration as a previous one:

```bash
# Rollback to specific deployment
rise deployment rollback my-app:20241205-1234
```

How it works:
1. Fetches the configuration of the target deployment (image digest, env vars, etc.)
2. Creates a **new deployment** with the same configuration
3. Deploys to the runtime

**Important**: Rollback creates a new deployment; it doesn't modify the original.

## Deployment Status

Deployments can be in one of these states:

| Status | Description |
|--------|-------------|
| `pending` | Deployment created, waiting to start |
| `running` | Container is running and healthy |
| `unhealthy` | Container is running but health check fails |
| `failed` | Container failed to start or crashed |
| `stopped` | Deployment was manually stopped |
| `expired` | Deployment was auto-deleted due to expiration |

## Best Practices

### Use Expiration for Preview Environments

```bash
# Auto-cleanup after 7 days
rise d c my-app --group mr/123 --expire 7d
```

### Pin to Specific Image Tags

```bash
# Good: Specific version
rise d c my-app --image myapp:v1.2.3

# Avoid: Mutable tags in production
rise d c my-app --image myapp:latest
```

Rise automatically pins deployments to image digests for reproducibility.

### Use Deployment Groups for Staging

```bash
# Staging deployment
rise d c my-app --group staging

# Production deployment
rise d c my-app --group default
```

### Follow Deployments in CI/CD

```bash
# Wait for deployment to succeed
rise d c my-app --follow --timeout 5m || exit 1
```

## Next Steps

- **Deploy your first app**: See [Getting Started](../getting-started/README.md)
- **Use service accounts in CI/CD**: See [Service Accounts](../features/service-accounts.md)
- **Learn CLI commands**: See [CLI Basics](../getting-started/cli-basics.md)
