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

When `--managed-buildkit` is enabled and `SSL_CERT_FILE` environment variable is set:
1. Rise CLI creates a `rise-buildkit` daemon container with the certificate mounted at `/etc/ssl/certs/ca-certificates.crt`
2. The daemon is configured with `--platform linux/amd64` for Mac compatibility
3. Subsequent builds use this managed daemon via `BUILDKIT_HOST` environment variable
4. If `SSL_CERT_FILE` changes, the daemon is automatically recreated

### Warning When Not Enabled

If `SSL_CERT_FILE` is set but managed BuildKit is disabled, you'll see:
```
Warning: SSL_CERT_FILE is set but managed BuildKit daemon is disabled.

Railpack builds may fail with SSL certificate errors in corporate environments.

To enable automatic BuildKit daemon management:
  rise build --managed-buildkit ...

Or set environment variable:
  export RISE_MANAGED_BUILDKIT=true

For manual setup, see: https://github.com/NiklasRosenstein/rise/issues/18
```

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
