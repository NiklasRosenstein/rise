# Building Container Images

Rise CLI supports multiple build backends for creating container images from your application code.

## Build Backends

### Docker (Dockerfile)

Multiple Docker-based backends are available for building from a Dockerfile:

```bash
# Standard docker/podman build (default)
rise build myapp:latest --backend docker
rise build myapp:latest --backend docker:build  # alias for docker

# Docker buildx (with BuildKit features like secrets)
rise build myapp:latest --backend docker:buildx

# Plain buildctl (BuildKit directly, requires buildctl CLI)
rise build myapp:latest --backend buildctl
```

**Backend comparison:**
| Backend | Build Tool | SSL Secrets | Best For |
|---------|------------|-------------|----------|
| `docker` / `docker:build` | docker build | No | Simple local builds and maximum compatibility |
| `docker:buildx` | docker buildx build | Yes | BuildKit features (secrets, advanced caching, multi-platform) |
| `buildctl` | buildctl | Yes | BuildKit-first CI environments without Docker daemon dependencies |

**When to use each:**
- `docker:build` - Simple builds, maximum compatibility
- `docker:buildx` - Need BuildKit features (secrets, caching, multi-platform)
- `buildctl` - Direct BuildKit access, CI environments without Docker

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
- If `Containerfile` exists (and no Dockerfile) → uses `docker` backend
- Otherwise → uses `pack` backend

```bash
# Auto-detect (has Dockerfile → uses docker)
rise build myapp:latest

# Auto-detect (no Dockerfile → uses pack)
rise build myapp:latest

# Explicit backend selection
rise build myapp:latest --backend railpack
```

### Custom Dockerfile Path

By default, Rise looks for `Dockerfile` or `Containerfile` in the project directory. Use `--dockerfile` to specify a different file:

```bash
# Use a custom Dockerfile
rise build myapp:latest --dockerfile Dockerfile.prod

# Use Dockerfile from subdirectory
rise build myapp:latest --dockerfile docker/Dockerfile.build

# Works with all Docker-based backends
rise build myapp:latest --backend docker:buildx --dockerfile Dockerfile.dev
```

**In `rise.toml`:**
```toml
[build]
backend = "docker"
dockerfile = "Dockerfile.prod"
```

### Build Contexts (Docker/Podman Multi-Stage Builds)

Build contexts allow you to use additional directories or files in your multi-stage Docker builds. This is useful when you need to access files outside the main build context or reference other directories.

**CLI Usage:**

```bash
# Add a single build context
rise build myapp:latest --build-context mylib=../my-library

# Add multiple build contexts
rise build myapp:latest \
  --build-context mylib=../my-library \
  --build-context tools=../build-tools

# Specify custom default build context (the main context directory)
rise build myapp:latest --context ./app

# Combine with other options
rise build myapp:latest \
  --backend docker:buildx \
  --build-context mylib=../my-library \
  --dockerfile Dockerfile.prod
```

**In `rise.toml`:**
```toml
[build]
backend = "docker"
dockerfile = "Dockerfile"
build_context = "./app"  # Optional: custom default build context

[build.build_contexts]
mylib = "../my-library"
tools = "../build-tools"
shared = "../shared-components"
```

**Using Build Contexts in Dockerfile:**

Once defined, you can reference build contexts in your Dockerfile:

```dockerfile
# Copy files from a named build context
FROM alpine AS base
COPY --from=mylib /src /app/lib

# Or use the context as a build stage
FROM scratch AS mylib
# This stage can access files from ../my-library

FROM node:20 AS build
# Copy from the mylib context
COPY --from=mylib /package.json /app/lib/package.json
```

**Configuration Precedence:**
- CLI `--build-context` flags override config file contexts with the same name
- CLI `--context` flag overrides config file `build_context`
- Default build context is the app path (project directory) if not specified

**Notes:**
- Build contexts are only supported by Docker and Podman backends
- Paths are relative to the `rise.toml` file location (typically the project root directory)
- Available with all Docker-based backends: `docker`, `docker:buildx`, `buildctl`

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

### Security Considerations

**IMPORTANT: Build-time variables are for build configuration only!**

Build-time environment variables are used during the image build process and should only contain build-related configuration (compiler versions, build flags, feature toggles, etc.). They should NEVER contain runtime secrets.

**❌ Bad - Runtime secrets as build-time variables:**
```toml
[build]
# NEVER do this - these are runtime secrets!
env = ["DATABASE_PASSWORD=hunter2", "API_KEY=secret123"]
```

```bash
# NEVER do this - runtime secrets don't belong in builds!
rise build myapp:latest -e DATABASE_PASSWORD=secret123
```

**✅ Good - Build configuration only:**
```toml
[build]
# These configure the build process, not the running application
env = ["NODE_ENV=production", "BUILD_VERSION=1.2.3", "BP_NODE_VERSION=20"]
```

```bash
# Build-time configuration that affects compilation/packaging
rise build myapp:latest -e NODE_ENV=production -e OPTIMIZATION_LEVEL=2
```

**Build-Time vs Runtime Variables:**

| Aspect | Build-Time Variables | Runtime Variables |
|--------|---------------------|-------------------|
| **Purpose** | Configure build process (compiler flags, tool versions) | Configure running application (DB credentials, API keys) |
| **Set via** | `build.env` in `rise.toml`, `-e` flag | `rise env set` command |
| **Used during** | Image building only | Container runtime |
| **Storage** | Not stored (ephemeral, config files) | Database (encrypted for secrets) |
| **Examples** | `NODE_ENV`, `BUILD_VERSION`, `BP_PYTHON_VERSION` | `DATABASE_URL`, `API_KEY`, `JWT_SECRET` |

**For runtime secrets**, always use `rise env set --secret`:
```bash
# Runtime secrets - injected into running containers
rise env set my-app DATABASE_PASSWORD hunter2 --secret
rise env set my-app API_KEY abc123xyz --secret

# Non-secret runtime config
rise env set my-app LOG_LEVEL info
```

**Reading from environment (build-time only):**
You can reference environment variables without hardcoding values in `rise.toml`:
```toml
[build]
env = ["BUILD_VERSION"]  # Reads BUILD_VERSION from your shell environment
```

This is useful for CI/CD where you want to inject build metadata (git commit SHA, build number) without hardcoding it in config files.

### Troubleshooting Build-Time Variables

**Problem: Environment variable not available in Dockerfile**

Docker backend requires explicit `ARG` declarations:
```dockerfile
ARG NODE_ENV
ARG BUILD_VERSION
RUN echo "Building version $BUILD_VERSION"
```

Then pass via CLI or rise.toml:
```bash
rise build myapp:latest -e NODE_ENV=production -e BUILD_VERSION=1.0.0
```

**Problem: Variable contains secret but needs to be in rise.toml**

Don't put the secret value in rise.toml. Instead, use the `KEY` format (without `=VALUE`) to read from your environment:

```toml
[build]
env = ["API_KEY"]  # Will read from environment
```

Then set in your shell before building:
```bash
export API_KEY=secret123
rise build myapp:latest
```

**Problem: CLI -e flags are overriding rise.toml values**

This is by design! CLI flags are **merged** with rise.toml values, not replaced:
- All `env` values from rise.toml are included
- CLI `-e` flags are appended
- Result: rise.toml env + CLI env

To completely override, don't use rise.toml for that variable.

## Build Cache Control

Use `--no-cache` to disable build caching and force a complete rebuild:

```bash
rise build myapp:latest --no-cache
rise deploy myproject --no-cache
```

Or set in `rise.toml`:

```toml
[build]
no_cache = true
```

Useful when dependencies have updated but source files haven't changed, or when debugging build issues.

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
| `backend` | String | Build backend: `docker`, `docker:build`, `docker:buildx`, `buildctl`, `pack`, `railpack`, `railpack:buildctl` |
| `dockerfile` | String | Path to Dockerfile (relative to `rise.toml` location). Defaults to `Dockerfile` or `Containerfile` |
| `build_context` | String | Default build context (docker/podman only). The path argument to `docker build <path>`. Defaults to `rise.toml` location. Path is relative to `rise.toml` location. |
| `build_contexts` | Object | Named build contexts for multi-stage builds (docker/podman only). Format: `{ "name" = "path" }`. Paths are relative to `rise.toml` location. |
| `builder` | String | Buildpack builder image (pack only) |
| `buildpacks` | Array | List of buildpacks to use (pack only) |
| `env` | Array | Environment variables for build (format: `KEY=VALUE` or `KEY`) |
| `container_cli` | String | Container CLI: `docker` or `podman` |
| `managed_buildkit` | Boolean | Enable managed BuildKit daemon |
| `railpack_embed_ssl_cert` | Boolean | Embed SSL certificate in Railpack builds |
| `no_cache` | Boolean | Disable build cache (equivalent to `--no-cache` flag) |

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
- ⚠️ `docker` / `docker:build` - Does not support BuildKit secrets (use `docker:buildx` instead)
- ✅ `docker:buildx` - Full SSL support via BuildKit secrets (auto-injected into Dockerfile)
- ✅ `buildctl` - Full SSL support via BuildKit secrets (auto-injected into Dockerfile)
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

### BuildKit Network Connectivity

When using the managed BuildKit daemon, you may need to connect it to a custom Docker network to allow BuildKit to access other containers in your Docker Compose setup (like a local registry).

**The challenge:**
- The managed BuildKit daemon (`rise-buildkit`) runs in isolation
- It can't access Docker Compose services like `rise-registry` by default
- The `host.docker.internal` mapping only provides host access, not container-to-container networking

**The solution:**
Set the `RISE_MANAGED_BUILDKIT_NETWORK_NAME` environment variable to connect BuildKit to your Docker Compose network:

```bash
# Find your Docker Compose network name (usually <directory>_default)
docker network ls | grep rise

# Set the environment variable
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default

# BuildKit will automatically connect to this network on next build
rise build myapp:latest --managed-buildkit
```

**How it works:**
1. Rise reads the `RISE_MANAGED_BUILDKIT_NETWORK_NAME` environment variable
2. Creates the network if it doesn't exist
3. Connects the `rise-buildkit` container to that network
4. Tracks the network name in container labels
5. Automatically recreates the daemon if the network name changes

**Verify connectivity:**
```bash
# Check BuildKit is connected to the network
docker inspect rise-buildkit --format '{{range $net := .NetworkSettings.Networks}}{{$net}} {{end}}'
# Should show: bridge rise_default
```

For comprehensive setup instructions, see the [Local Development Networking Guide](local-development.md).

### Insecure Registries (Local Development)

For local HTTP registries, configure BuildKit to allow insecure connections:

```bash
export RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES="rise-registry:5000,localhost:5000"
rise build myapp:latest --managed-buildkit
```

Rise generates a `buildkitd.toml` config at `~/.rise/buildkitd.toml`, mounts it into the BuildKit container, and automatically recreates the daemon when the registry list changes. **Local development only** - never use in production.

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

## Build-Time SSL Certificate Injection (Docker/Buildctl)

When using `docker:buildx` or `buildctl` backends with `SSL_CERT_FILE` set, Rise automatically injects SSL certificates into your Dockerfile's RUN commands using BuildKit bind mounts.

**How it works:**
1. Rise creates a temporary directory containing only the certificate file
2. The temp directory is passed as an internal named build context (`rise-internal-ssl-cert`)
3. Rise preprocesses your Dockerfile to add `--mount=type=bind,from=rise-internal-ssl-cert,source=ca-certificates.crt,target=<path>,readonly` to each RUN command
4. The bind mount makes the certificate available at multiple standard system paths during RUN commands
5. All SSL-related environment variables are exported for cross-ecosystem compatibility
6. The temp directory is automatically cleaned up after the build
7. Certificates are NOT embedded in the final image (only available during build)

**Supported certificate paths:**
- `/etc/ssl/certs/ca-certificates.crt` (Debian, Ubuntu, Arch)
- `/etc/pki/tls/certs/ca-bundle.crt` (RedHat, CentOS, Fedora)
- `/etc/ssl/ca-bundle.pem` (OpenSUSE, SLES)
- `/etc/ssl/cert.pem` (Alpine Linux)
- `/usr/lib/ssl/cert.pem` (OpenSSL default)

**Exported SSL environment variables:**
- `SSL_CERT_FILE` - Standard (curl, wget, Git)
- `NIX_SSL_CERT_FILE` - Nix package manager
- `NODE_EXTRA_CA_CERTS` - Node.js and npm
- `REQUESTS_CA_BUNDLE` - Python requests library
- `AWS_CA_BUNDLE` - AWS SDK/CLI

All variables are set to `/etc/ssl/certs/ca-certificates.crt` during RUN commands, ensuring compatibility with various build tools and ecosystems.

**Example:**
```bash
export SSL_CERT_FILE=/path/to/ca-certificates.crt

# SSL certificates automatically available during RUN commands
rise build myapp:latest --backend docker:buildx
rise build myapp:latest --backend buildctl

# Debug logging shows the preprocessed Dockerfile
RUST_LOG=debug rise build myapp:latest --backend docker:buildx
```

**What your Dockerfile sees:**

Original:
```dockerfile
RUN apt-get update && apt-get install -y curl
RUN pip install -r requirements.txt
```

Processed (internal):
```dockerfile
RUN --mount=type=bind,from=rise-internal-ssl-cert,source=ca-certificates.crt,target=/etc/ssl/certs/ca-certificates.crt,readonly --mount=type=bind,from=rise-internal-ssl-cert,source=ca-certificates.crt,target=/etc/pki/tls/certs/ca-bundle.crt,readonly ... export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt && export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt && export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt && export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt && export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt && apt-get update && apt-get install -y curl
RUN --mount=type=bind,from=rise-internal-ssl-cert,source=ca-certificates.crt,target=/etc/ssl/certs/ca-certificates.crt,readonly --mount=type=bind,from=rise-internal-ssl-cert,source=ca-certificates.crt,target=/etc/pki/tls/certs/ca-bundle.crt,readonly ... export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt && export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt && export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt && export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt && export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt && pip install -r requirements.txt
```

### Security and Named Build Context

Rise uses a **named build context** approach for SSL certificates, which provides important security benefits:

- The certificate is kept in a **separate temporary directory**, not in your main build context
- The temp directory is passed as an internal named build context (`rise-internal-ssl-cert`)
- The certificate reduces risk of accidental inclusion via `COPY . .` or similar commands (though advanced users can still reference it explicitly via `COPY --from=rise-internal-ssl-cert` if needed)
- Rise automatically injects the certificate into `RUN` commands as readonly bind mounts
- The temp directory is automatically cleaned up after the build

This approach works seamlessly with any certificate size, avoiding BuildKit's 500KiB secret size limit while maintaining security.

**Note:** The `docker:build` backend does not support BuildKit features required for SSL certificate bind mounts. If SSL_CERT_FILE is set, you'll see a warning recommending `docker:buildx` instead.

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
