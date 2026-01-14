# Building Container Images

Rise CLI supports multiple build backends for creating container images from your application code.

## Build Backends

### Docker (Dockerfile)

Uses `docker build` or `podman build` to build from a Dockerfile:

```bash
rise build myapp:latest --backend docker
rise deployment create myproject --backend docker
```

### Pack (Cloud Native Buildpacks)

Uses `pack build` with Cloud Native Buildpacks:

```bash
rise build myapp:latest --backend pack
rise build myapp:latest --backend pack --builder paketobuildpacks/builder-jammy-base
rise deployment create myproject --backend pack
```

### Railpack (Railway Railpacks)

Uses Railway's Railpacks with BuildKit (buildx or buildctl):

```bash
# Railpack with buildx (default)
rise build myapp:latest --backend railpack
rise deployment create myproject --backend railpack

# Railpack with buildctl
rise build myapp:latest --backend railpack:buildctl
rise deployment create myproject --backend railpack:buildctl
```

**Troubleshooting**: If railpack builds fail with the error:
```
ERROR: failed to build: failed to solve: requested experimental feature mergeop has been disabled on the build server: only enabled with containerd image store backend
```

This occurs when using Docker Desktop's default builder. Create a custom buildx builder to work around this:
```bash
docker buildx create --use
```

## Auto-detection

When `--backend` is omitted, the CLI automatically detects the build method:
- If `Dockerfile` exists → uses `docker` backend
- Otherwise → uses `pack` backend

```bash
# Auto-detect (has Dockerfile → uses docker)
rise build myapp:latest

# Auto-detect (no Dockerfile → uses pack)
rise build myapp:latest

# Explicit backend selection
rise build myapp:latest --backend railpack
```

## Build-Time Environment Variables

You can pass environment variables to your build process using the `-e` or `--env` flag. This works consistently across all build backends:

```bash
# Pass environment variable with explicit value
rise build myapp:latest -e NODE_ENV=production

# Pass environment variable from current environment
export DATABASE_URL=postgres://localhost/mydb
rise build myapp:latest -e DATABASE_URL

# Multiple environment variables
rise build myapp:latest -e NODE_ENV=production -e API_KEY=secret123

# Works with all backends
rise build myapp:latest --backend docker -e BUILD_VERSION=1.2.3
rise build myapp:latest --backend pack -e BP_NODE_VERSION=20
rise build myapp:latest --backend railpack -e CUSTOM_VAR=value
```

### Backend-Specific Behavior

**Docker Backend:**
- Environment variables are passed as `--build-arg` arguments to Docker build
- Available in Dockerfile `ARG` declarations and `RUN` commands
- Example Dockerfile usage:
  ```dockerfile
  ARG NODE_ENV
  ARG BUILD_VERSION
  RUN echo "Building version $BUILD_VERSION in $NODE_ENV mode"
  ```

**Pack Backend:**
- Environment variables are passed as `--env` arguments to pack CLI
- Buildpacks can read these during detection and build phases
- Common uses: configuring buildpack versions, build flags

**Railpack Backend:**
- Environment variables are passed as BuildKit secrets
- Available in all build steps defined in the Railpack plan
- Railpack frontend exposes them as environment variables during build

## Project Configuration (rise.toml)

You can create a `rise.toml` or `.rise.toml` file in your project directory to define default build options. This allows you to avoid repeating CLI flags for every build.

**Example `rise.toml`:**

```toml
[build]
backend = "pack"
builder = "heroku/builder:24"
buildpacks = ["heroku/nodejs", "heroku/procfile"]
env = ["BP_NODE_VERSION=20"]
```

### Configuration Precedence

Build options are resolved in the following order (highest to lowest):

1. **CLI flags** (e.g., `--backend pack`)
2. **Project config file** (`rise.toml` or `.rise.toml`)
3. **Environment variables** (e.g., `RISE_CONTAINER_CLI`, `RISE_MANAGED_BUILDKIT`)
4. **Global config** (`~/.config/rise/config.json`)
5. **Auto-detection/defaults**

**Vector field behavior:**
- **All vector fields** (`buildpacks`, `env`): CLI values are **appended** to config values (merged)

This allows you to set common buildpacks and environment variables in the config file and add additional ones via CLI as needed.

### Available Options

All CLI build flags can be specified in the `[build]` section:

| Field | Type | Description |
|-------|------|-------------|
| `backend` | String | Build backend: `docker`, `pack`, `railpack`, `railpack:buildctl` |
| `builder` | String | Buildpack builder image (pack only) |
| `buildpacks` | Array | List of buildpacks to use (pack only) |
| `env` | Array | Environment variables for build (format: `KEY=VALUE` or `KEY`) |
| `container_cli` | String | Container CLI: `docker` or `podman` |
| `managed_buildkit` | Boolean | Enable managed BuildKit daemon |
| `railpack_embed_ssl_cert` | Boolean | Embed SSL certificate in Railpack builds |

### Examples

**Heroku buildpacks:**
```toml
[build]
backend = "pack"
builder = "heroku/builder:24"
buildpacks = ["heroku/nodejs", "heroku/procfile"]
```

**Railpack with SSL:**
```toml
[build]
backend = "railpack"
managed_buildkit = true
railpack_embed_ssl_cert = true
```

**Docker with build args:**
```toml
[build]
backend = "docker"
env = ["VERSION=1.0.0", "NODE_ENV=production"]
```

**Pack with custom environment:**
```toml
[build]
backend = "pack"
builder = "paketobuildpacks/builder-jammy-base"
env = ["BP_NODE_VERSION=20.*"]
```

### CLI Override

CLI flags always take precedence over project config:

```bash
# Uses docker backend despite project config specifying pack
rise build myapp:latest --backend docker

# Adds to env variables from config
# If config has env = ["NODE_ENV=production"]
# This results in: env = ["NODE_ENV=production", "API_KEY=secret"]
rise build myapp:latest -e API_KEY=secret

# Enable managed BuildKit (shorthand defaults to true)
rise build myapp:latest --managed-buildkit

# Disable managed BuildKit despite config enabling it
rise build myapp:latest --managed-buildkit=false

# Enable SSL certificate embedding (shorthand defaults to true)
rise build myapp:latest --railpack-embed-ssl-cert
```

### File Naming

Both `rise.toml` and `.rise.toml` are supported. If both exist in the same directory, `rise.toml` takes precedence (with a warning).

## SSL Certificate Handling (Managed BuildKit Daemon)

When building with BuildKit-based backends (`docker`, `railpack`) on macOS behind corporate proxies (Cloudflare, Zscaler, etc.) or environments with custom CA certificates, builds may fail with SSL certificate verification errors.

### The Problem

BuildKit runs as a separate daemon and requires CA certificates to be available at daemon startup. This affects two scenarios:
1. **BuildKit daemon operations**: Pulling base images, accessing registries
2. **Build-time operations**: Application builds (RUN instructions) downloading packages, cloning repos

### Solution: Managed BuildKit Daemon

Rise CLI provides an opt-in managed BuildKit daemon feature that automatically creates and manages a BuildKit daemon with SSL certificate support.

**Enable via CLI flag:**
```bash
# Shorthand (defaults to true)
rise build myapp:latest --backend railpack --managed-buildkit
rise deployment create myproject --backend railpack --managed-buildkit

# Explicit values
rise build myapp:latest --backend railpack --managed-buildkit=true
rise build myapp:latest --backend railpack --managed-buildkit=false
```

**Or set environment variable:**
```bash
export RISE_MANAGED_BUILDKIT=true
rise build myapp:latest --backend railpack
```

**Or configure permanently:**
```bash
# Set in config file
rise config set managed_buildkit true
```

### How It Works

When `--managed-buildkit` is enabled, Rise CLI follows this priority order:

1. **Existing BUILDKIT_HOST**: If the `BUILDKIT_HOST` environment variable is already set, Rise uses your existing buildkit daemon
2. **Managed daemon**: Otherwise, Rise creates a `rise-buildkit` daemon container:
   - With SSL certificate mounted at `/etc/ssl/certs/ca-certificates.crt` if `SSL_CERT_FILE` is set
   - Without SSL certificate if `SSL_CERT_FILE` is not set
   - Configured with `--platform linux/amd64` for Mac compatibility
3. **Automatic updates**: If `SSL_CERT_FILE` is added, removed, or changed, the daemon is automatically recreated

### Warning When Not Enabled

If `SSL_CERT_FILE` is set but `--managed-buildkit` is not enabled, you'll see a warning during builds that require BuildKit (docker, railpack):

```
Warning: SSL_CERT_FILE is set but managed BuildKit daemon is disabled.

Railpack builds may fail with SSL certificate errors in corporate environments.

To enable automatic BuildKit daemon management:
  rise build --managed-buildkit ...

Or set environment variable:
  export RISE_MANAGED_BUILDKIT=true

For manual setup, see: https://github.com/NiklasRosenstein/rise/issues/18
```

Note: The managed BuildKit feature works with or without `SSL_CERT_FILE` - it simply mounts the certificate when available.

### Affected Build Backends

- ✅ `pack` - Already supports `SSL_CERT_FILE` natively (no managed daemon needed)
- ⚠️ `docker` - Benefits from managed daemon when using BuildKit
- ⚠️ `railpack` / `railpack:buildx` - Benefits from managed daemon
- ⚠️ `railpack:buildctl` - Benefits from managed daemon

### Manual Setup (Advanced)

For users who prefer manual control, you can create your own BuildKit daemon:

```bash
# Start BuildKit daemon with certificate
docker run --platform linux/amd64 --privileged --name my-buildkit --rm -d \
  --volume $SSL_CERT_FILE:/etc/ssl/certs/ca-certificates.crt:ro \
  moby/buildkit

# Point Rise CLI to your daemon
export BUILDKIT_HOST=docker-container://my-buildkit
rise build myapp:latest --backend railpack
```

For more details, see [Issue #18](https://github.com/NiklasRosenstein/rise/issues/18).

## Build-Time SSL Certificate Embedding (Railpack)

The `--railpack-embed-ssl-cert` flag embeds SSL certificates directly into the Railpack build plan for use during RUN commands. This complements `--managed-buildkit` by handling build-time SSL requirements.

**Important differences:**
- `--managed-buildkit`: Injects SSL certs into BuildKit daemon (for pulling images, registry access). Does NOT embed cert into final image.
- `--railpack-embed-ssl-cert`: Embeds SSL certs into railpack plan.json as build assets (for RUN commands during build). DOES embed cert into final image.

Both flags can be used together for comprehensive SSL support.

**When to use:**
- Application builds need SSL certificates (pip install, npm install, git clone, curl requests)
- Running behind corporate proxies with certificate inspection
- Custom or self-signed certificates

**Default behavior:**
- **Automatically enabled** when `SSL_CERT_FILE` environment variable is set
- This ensures builds work by default in most SSL certificate scenarios
- Can be explicitly disabled with `--railpack-embed-ssl-cert=false`

**Usage:**
```bash
export SSL_CERT_FILE=/path/to/ca-certificates.crt

# Embedding is automatically enabled when SSL_CERT_FILE is set
rise build myapp:latest --backend railpack

# Explicitly disable even when SSL_CERT_FILE is set
rise build myapp:latest --backend railpack --railpack-embed-ssl-cert=false

# Explicitly enable (useful when SSL_CERT_FILE is not set)
rise build myapp:latest --backend railpack --railpack-embed-ssl-cert=true

# Combine with managed BuildKit for comprehensive SSL support
rise build myapp:latest --backend railpack --managed-buildkit
```

**Environment variable support:**
```bash
export RISE_RAILPACK_EMBED_SSL_CERT=true
rise build myapp:latest --backend railpack
# Embedding is enabled via env var
```

**Config file support:**
```bash
rise config set railpack_embed_ssl_cert true
rise build myapp:latest --backend railpack
# Embedding is enabled via config
```

**Precedence order:** CLI flag > Environment variable > Config file > Default (enabled if SSL_CERT_FILE is set)

## Proxy Support

Rise CLI automatically detects and injects HTTP/HTTPS proxy environment variables into all build backends. This is useful when your build environment requires going through a corporate proxy to access external resources.

### Supported Proxy Variables

Rise automatically detects these standard proxy environment variables:

- `HTTP_PROXY` / `http_proxy`
- `HTTPS_PROXY` / `https_proxy`
- `NO_PROXY` / `no_proxy`

All variants (uppercase and lowercase) are automatically detected from your environment and passed to the appropriate build backend.

### Localhost to host.docker.internal Transformation

Since builds execute in containers, `localhost` and `127.0.0.1` addresses are automatically transformed to `host.docker.internal` to allow container builds to reach a proxy server running on your host machine.

**Example transformations:**
- `http://localhost:3128` → `http://host.docker.internal:3128`
- `https://127.0.0.1:8080/path` → `https://host.docker.internal:8080/path`
- `http://user:pass@localhost:3128` → `http://user:pass@host.docker.internal:3128`
- `http://proxy.example.com:8080` → unchanged (not localhost)

**Note:** `NO_PROXY` and `no_proxy` values are passed through unchanged since they contain comma-separated lists, not URLs.

### Usage Examples

Set proxy variables in your environment before running rise:

```bash
export HTTP_PROXY=http://proxy.example.com:3128
export HTTPS_PROXY=http://proxy.example.com:3128
export NO_PROXY=localhost,127.0.0.1,.example.com

# Proxy settings automatically applied to all builds
rise build myapp:latest ./path
rise deployment create myproject --path ./app
```

**With localhost proxy:**
```bash
# Proxy running on your host machine
export HTTP_PROXY=http://localhost:3128
export HTTPS_PROXY=http://localhost:3128

# Automatically transformed to host.docker.internal for container builds
rise build myapp:latest --backend pack
rise build myapp:latest --backend railpack
rise build myapp:latest --backend docker
```

### Backend-Specific Behavior

**Pack Backend:**
- Proxy variables are passed via `--env` arguments to the pack CLI
- Pack forwards these to the buildpack lifecycle containers
- Works with pack's `--network host` networking mode

**Railpack Backend:**
- Proxy variables are passed via `--secret` flags to buildx/buildctl
- Secret references are added to build steps in `plan.json`
- BuildKit provides the secret values from environment variables
- Railpack frontend makes these available as environment variables in build steps

**Docker Backend:**
- Proxy variables are passed via `--build-arg` arguments
- Docker automatically respects `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` as build args
- Available during Dockerfile `RUN` commands

### No Configuration Required

Proxy support is completely automatic - no CLI flags or configuration needed. Rise CLI respects the standard proxy environment variables already set in your shell or CI/CD environment.

## Local Development with `rise run`

The `rise run` command builds and immediately runs your application locally for development purposes. This is useful for testing your application before deploying it to the Rise platform.

### Basic Usage

```bash
# Build and run from current directory (defaults to port 8080)
rise run

# Specify directory
rise run ./path/to/app

# Custom port
rise run --http-port 3000

# Expose on different host port
rise run --http-port 8080 --expose 3000
```

### With Project Environment Variables

When authenticated, you can load non-secret environment variables from a project:

```bash
# Load environment variables from project
rise run --project my-app
```

**Note:** Only non-secret environment variables are loaded. Secret values cannot be retrieved from the backend for security reasons.

### Setting Runtime Environment Variables

You can set custom runtime environment variables using the `--run-env` flag:

```bash
# Set a single environment variable
rise run --run-env DATABASE_URL=postgres://localhost/mydb

# Set multiple environment variables
rise run --run-env DATABASE_URL=postgres://localhost/mydb --run-env DEBUG=true --run-env API_KEY=test123

# Combine with project environment variables
rise run --project my-app --run-env OVERRIDE_VAR=custom_value
```

Runtime environment variables set via `--run-env` take precedence and can override project environment variables if they have the same key.

### Build Backend Selection

Use any build backend with `rise run`:

```bash
# Use pack backend
rise run --backend pack

# Use docker backend
rise run --backend docker

# With custom builder
rise run --backend pack --builder paketobuildpacks/builder-jammy-base
```

### How It Works

1. **Build**: Builds the container image locally using the selected backend
2. **Tag**: Tags the image as `rise-local-{project-name}` (or `rise-local-app` if no project specified)
3. **Run**: Executes `docker run --rm -it -p {expose}:{http-port} -e PORT={http-port} {image}`
4. **Environment**: Automatically sets `PORT` environment variable
5. **Project Variables**: Loads non-secret environment variables from the project if `--project` is specified
6. **Cleanup**: Container is automatically removed when stopped (`--rm` flag)

### Port Configuration

- `--http-port`: The port your application listens on inside the container (sets `PORT` env var)
- `--expose`: The port exposed on your host machine (defaults to same as `--http-port`)

Example:
```bash
# Application listens on port 8080, accessible at http://localhost:3000
rise run --http-port 8080 --expose 3000
```

### Interactive Mode

`rise run` uses interactive mode (`-it`) so you can:
- See real-time logs from your application
- Press Ctrl+C to stop the container
- Interact with your application if it accepts input

### Complete Example

```bash
# Create a project
rise project create my-app

# Set some environment variables
rise env set my-app DATABASE_URL postgres://localhost/mydb
rise env set my-app API_KEY secret123 --secret

# Run locally with project environment variables
rise run --project my-app --http-port 3000

# Application accessible at http://localhost:3000
# PORT=3000 and DATABASE_URL=postgres://localhost/mydb are set
# API_KEY is not loaded (secret values not retrievable)

# Run with additional runtime environment variables
rise run --project my-app --http-port 3000 --run-env DEBUG=true --run-env LOG_LEVEL=verbose

# Application now has PORT, DATABASE_URL, DEBUG, and LOG_LEVEL environment variables set
```
