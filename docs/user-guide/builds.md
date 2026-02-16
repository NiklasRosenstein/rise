# Building Container Images

Rise supports multiple build backends for creating container images from your application code. Building happens automatically as part of `rise deploy`, or you can build standalone with `rise build`.

## Build Backends

| Backend | Build Tool | Best For |
|---------|------------|----------|
| `docker` / `docker:build` | docker build | Simple local builds, maximum compatibility |
| `docker:buildx` | docker buildx build | BuildKit features (secrets, caching, multi-platform) |
| `buildctl` | buildctl | BuildKit-first CI environments without Docker |
| `pack` | pack CLI | Cloud Native Buildpacks (no Dockerfile needed) |
| `railpack` / `railpack:buildx` | railpack + buildx | Railway Railpacks with BuildKit |
| `railpack:buildctl` | railpack + buildctl | Railpacks with direct buildctl |

## Auto-Detection

When `--backend` is not specified, Rise detects the build method automatically:

- If `Dockerfile` or `Containerfile` exists in the project directory → `docker` backend
- Otherwise → error (you must specify a backend explicitly)

Override auto-detection with `--backend` or in `rise.toml`:

```bash
rise deploy --backend railpack
```

```toml
[build]
backend = "pack"
```

## Docker Backend

### Basic Usage

```bash
rise build myapp:latest --backend docker
rise deploy --backend docker:buildx
```

### Custom Dockerfile Path

```bash
rise build myapp:latest --dockerfile Dockerfile.prod
```

Or in `rise.toml`:

```toml
[build]
backend = "docker"
dockerfile = "Dockerfile.prod"
```

### Build Contexts (Multi-Stage Builds)

Use additional directories in multi-stage Docker builds:

```bash
rise build myapp:latest \
  --build-context mylib=../my-library \
  --build-context tools=../build-tools
```

Or in `rise.toml`:

```toml
[build]
backend = "docker"
build_context = "./app"  # Custom default build context

[build.build_contexts]
mylib = "../my-library"
tools = "../build-tools"
```

Reference in your Dockerfile:

```dockerfile
COPY --from=mylib /src /app/lib
```

Build contexts are supported by all Docker-based backends. Paths are relative to the `rise.toml` location.

## Pack Backend

Uses Cloud Native Buildpacks via the `pack` CLI:

```bash
rise build myapp:latest --backend pack
rise deploy --backend pack --builder paketobuildpacks/builder-jammy-base
```

Configure builder and buildpacks in `rise.toml`:

```toml
[build]
backend = "pack"
builder = "heroku/builder:24"
buildpacks = ["heroku/nodejs", "heroku/procfile"]
```

## Railpack Backend

Uses Railway Railpacks with BuildKit:

```bash
rise build myapp:latest --backend railpack
rise deploy --backend railpack:buildctl
```

If builds fail with the error `requested experimental feature mergeop has been disabled`, create a custom buildx builder:

```bash
docker buildx create --use
```

## Build-Time Environment Variables

Pass variables to the build process with `-e`:

```bash
rise build myapp:latest -e NODE_ENV=production -e BUILD_VERSION=1.2.3
```

Or in `rise.toml`:

```toml
[build]
env = ["NODE_ENV=production", "BUILD_VERSION"]
```

Using `KEY` without `=VALUE` reads the variable from your shell environment (useful for CI metadata like git SHAs).

**How backends use these variables:**

- **Docker**: Passed as `--build-arg` (requires `ARG` declaration in Dockerfile)
- **Pack**: Passed as `--env` to pack CLI
- **Railpack**: Passed as BuildKit secrets

Build-time variables are for build configuration only (compiler flags, tool versions). For runtime secrets, use `rise env set --secret`. See [Environment Variables](environment-variables.md) for the distinction.

## Build Cache Control

Force a complete rebuild:

```bash
rise deploy --no-cache
```

Or in `rise.toml`:

```toml
[build]
no_cache = true
```

## SSL and Proxy

If you're behind a corporate proxy or have custom CA certificates, see [SSL & Proxy Configuration](ssl-proxy.md) for managed BuildKit daemon setup, certificate injection, and proxy variable handling.
