# Deployments

A deployment is an immutable, timestamped instance of your application running in the container runtime. Each deployment has a unique ID (e.g., `my-app:20241205-1234`), tracks its own status, and can be rolled back to.

## Creating a Deployment

The primary command is `rise deploy`:

```bash
# Deploy from current directory (builds, pushes, deploys)
rise deploy

# Deploy from a specific directory
rise deploy ./path/to/app

# Specify a project explicitly
rise deploy -p my-app
```

`rise deploy` is a shortcut for `rise deployment create` (`rise d c`). After creating the deployment, Rise automatically follows its progress.

### Pre-Built Images

Skip the build step by providing an image directly:

```bash
rise deploy --image nginx:latest --http-port 80
rise deploy --image myregistry.io/my-app:v1.2.3
```

When using `--image`, no build occurs and `--http-port` is required.

### Deploying from an Existing Deployment

Reuse the image from a previous deployment:

```bash
rise deploy --from 20241205-1234
```

By default, the new deployment uses the project's current environment variables. To copy environment variables from the source deployment instead:

```bash
rise deploy --from 20241205-1234 --use-source-env-vars
```

## Deployment Lifecycle

Deployments progress through the following states:

### Build & Deploy States

| Status | Description |
|--------|-------------|
| `Pending` | Deployment created, waiting to start |
| `Building` | Container image is being built |
| `Pushing` | Image is being pushed to the registry |
| `Pushed` | Image pushed; handoff to the deployment controller |
| `Deploying` | Controller is creating the container in the runtime |

### Running States

| Status | Description |
|--------|-------------|
| `Healthy` | Running and passing health checks |
| `Unhealthy` | Running but failing health checks |

### Cancellation States (Before Infrastructure)

| Status | Description |
|--------|-------------|
| `Cancelling` | Being cancelled before infrastructure was provisioned |
| `Cancelled` | Cancelled before infrastructure was provisioned (terminal) |

### Termination States (After Infrastructure)

| Status | Description |
|--------|-------------|
| `Terminating` | Being gracefully terminated |
| `Stopped` | User-initiated termination (terminal) |
| `Superseded` | Replaced by a newer deployment in the same group (terminal) |

### Other Terminal States

| Status | Description |
|--------|-------------|
| `Failed` | Could not reach Healthy state (terminal) |
| `Expired` | Auto-deleted after reaching Healthy (terminal) |

## Deployment Groups

Projects can have multiple active deployments using deployment groups.

### Default Group

The `default` group represents the primary deployment:

```bash
rise deploy
# Accessible at: https://my-app.rise.dev
```

### Custom Groups

Create additional deployments with custom group names:

```bash
# Merge request preview
rise deploy --group mr/123 --expire 7d

# Staging environment
rise deploy --group staging
```

Each custom group gets its own URL: `https://{project}-{group}.rise.dev`

Group names must match `[a-z0-9][a-z0-9/-]*[a-z0-9]` (max 100 characters). When a new deployment in a group reaches `Healthy`, the previous deployment in that group is `Superseded`.

### Auto-Expiration

Set deployments to expire automatically:

```bash
rise deploy --group mr/123 --expire 7d   # Days
rise deploy --group preview --expire 24h  # Hours
rise deploy --group temp --expire 1w      # Weeks
```

Expired deployments are automatically cleaned up.

## Monitoring Deployments

### Following a Deployment

`rise deploy` follows automatically. You can also follow an existing deployment:

```bash
rise deployment show my-app:20241205-1234 --follow
rise d s my-app:latest --follow --timeout 10m
```

### Listing Deployments

```bash
rise deployment list my-app
rise d ls my-app --group staging
```

### Viewing Deployment Details

```bash
rise deployment show my-app:20241205-1234
rise d s my-app:latest
```

### Deployment Logs

```bash
# Show recent logs
rise deployment logs my-app 20241205-1234

# Follow logs in real-time
rise deployment logs my-app 20241205-1234 --follow

# Show last 100 lines
rise deployment logs my-app 20241205-1234 --tail 100

# Show logs since a duration ago
rise deployment logs my-app 20241205-1234 --since 5m

# Show timestamps
rise deployment logs my-app 20241205-1234 --timestamps
```

## Rollback

Rollback creates a new deployment using the same image as a previous one:

```bash
rise deployment rollback my-app:20241205-1234
```

This fetches the target deployment's image digest and creates a new deployment with it. The original deployment is not modified.

## Stopping Deployments

Stop all deployments in a group:

```bash
rise deployment stop my-app --group default
rise d stop my-app --group mr/123
```

Stopped deployments remain in the database for rollback purposes.

## Auto-Injected Environment Variables

Rise automatically injects these variables into every deployment:

| Variable | Description | Example |
|----------|-------------|---------|
| `PORT` | HTTP port the container should listen on | `8080` |
| `RISE_ISSUER` | Rise server URL and JWT issuer | `https://rise.example.com` |
| `RISE_APP_URL` | Canonical URL where your app is accessible | `https://myapp.example.com` |
| `RISE_APP_URLS` | JSON array of all URLs where your app is accessible | `["https://myapp.rise.dev", "https://myapp.example.com"]` |

`PORT` defaults to 8080 and can be overridden with `--http-port`. `RISE_APP_URL` is your primary custom domain if set, otherwise the default project URL.

For JWT validation using `RISE_ISSUER`, see [Authentication for Applications](authentication-for-apps.md).

## CI/CD Deployments

```bash
# Wait for deployment to succeed in CI
rise deploy --image $CI_REGISTRY_IMAGE:$CI_COMMIT_TAG

# Follow with timeout (exit non-zero on failure)
rise d s my-app:latest --follow --timeout 5m || exit 1
```

See [Authentication](authentication.md#service-accounts-workload-identity) for setting up service accounts.
