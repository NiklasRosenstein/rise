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

## Project Configuration (rise.toml)

You can create a `rise.toml` or `.rise.toml` file in your project directory to define default build options. This allows you to avoid repeating CLI flags for every build.

**Example `rise.toml`:**

```toml
[build]
backend = "pack"
builder = "heroku/builder:24"
buildpacks = ["heroku/nodejs", "heroku/procfile"]
```

### Configuration Precedence

Build options are resolved in the following order (highest to lowest):

1. **CLI flags** (e.g., `--backend pack`)
2. **Project config file** (`rise.toml` or `.rise.toml`)
3. **Environment variables** (e.g., `RISE_CONTAINER_CLI`, `RISE_MANAGED_BUILDKIT`)
4. **Global config** (`~/.config/rise/config.json`)
5. **Auto-detection/defaults**

**Vector field behavior:**
- **All vector fields** (`buildpacks`, `pack_env`, `build_args`): CLI values are **appended** to config values (merged)

This allows you to set common buildpacks, environment variables, or build arguments in the config file and add additional ones via CLI as needed.

### Available Options

All CLI build flags can be specified in the `[build]` section:

| Field | Type | Description |
|-------|------|-------------|
| `backend` | String | Build backend: `docker`, `pack`, `railpack`, `railpack:buildctl` |
| `builder` | String | Buildpack builder image (pack only) |
| `buildpacks` | Array | List of buildpacks to use (pack only) |
| `pack_env` | Array | Environment variables for pack CLI (pack only) |
| `container_cli` | String | Container CLI: `docker` or `podman` |
| `managed_buildkit` | Boolean | Enable managed BuildKit daemon |
| `railpack_embed_ssl_cert` | Boolean | Embed SSL certificate in Railpack builds |
| `build_args` | Array | Docker build arguments (docker only, format: `KEY=VALUE` or `KEY`) |

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
build_args = ["VERSION=1.0.0", "NODE_ENV=production"]
```

**Pack with custom environment:**
```toml
[build]
backend = "pack"
builder = "paketobuildpacks/builder-jammy-base"
pack_env = ["BP_NODE_VERSION=20.*"]
```

### CLI Override

CLI flags always take precedence over project config:

```bash
# Uses docker backend despite project config specifying pack
rise build myapp:latest --backend docker

# Disable managed BuildKit despite config enabling it
rise build myapp:latest --managed-buildkit=false

# Enable SSL certificate embedding despite config disabling it
rise build myapp:latest --railpack-embed-ssl-cert=true
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
rise build myapp:latest --backend railpack --managed-buildkit
rise deployment create myproject --backend railpack --managed-buildkit

# Or use explicit true/false values
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

**Usage:**
```bash
export SSL_CERT_FILE=/path/to/ca-certificates.crt

# Embed certificate for build-time use
rise build myapp:latest --backend railpack --railpack-embed-ssl-cert

# Or use explicit true/false values
rise build myapp:latest --backend railpack --railpack-embed-ssl-cert=true
rise build myapp:latest --backend railpack --railpack-embed-ssl-cert=false

# Combine with managed BuildKit for comprehensive SSL support
rise build myapp:latest --backend railpack --managed-buildkit --railpack-embed-ssl-cert
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

**Precedence order:** CLI flag > Environment variable > Config file > Default (false)

**Warning:** If `SSL_CERT_FILE` is set but `--railpack-embed-ssl-cert` is not specified, a warning will be logged to alert you that build-time SSL errors may occur.

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
