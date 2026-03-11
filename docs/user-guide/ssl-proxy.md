# SSL & Proxy Configuration

When building behind corporate proxies (Zscaler, Cloudflare) or in environments with custom CA certificates, builds may fail with SSL certificate verification errors. Rise provides several mechanisms to handle this.

## Managed BuildKit Daemon

Rise can manage a BuildKit daemon container with SSL certificates automatically mounted.

### Auto-Detection (Default)

When no explicit setting is provided, managed BuildKit **automatically enables** if:

1. The build method requires BuildKit (`docker:buildx`, `buildctl`, `railpack`)
2. `SSL_CERT_FILE` is set in the environment
3. `BUILDKIT_HOST` is not already set

This means in most corporate/proxy environments, managed BuildKit "just works" without any flags.

### Disabling

If you don't need managed BuildKit (e.g., you manage your own daemon or don't need SSL certificate support), disable it explicitly:

```bash
# CLI flag
rise deploy --managed-buildkit=false

# Environment variable
export RISE_MANAGED_BUILDKIT=false

# rise.toml
[build]
managed_buildkit = false
```

### Force Enabling

To force managed BuildKit even without `SSL_CERT_FILE` (e.g., for insecure registries):

```bash
rise deploy --managed-buildkit
```

### How It Works

1. If `BUILDKIT_HOST` is already set, Rise uses your existing daemon
2. Otherwise, Rise creates a `rise-buildkit` container with SSL certificates mounted (if `SSL_CERT_FILE` is set)
3. If `SSL_CERT_FILE` changes, the daemon is automatically recreated

### Backend Compatibility

| Backend | Managed BuildKit | Notes |
|---------|:---:|-------|
| `pack` | N/A | Supports `SSL_CERT_FILE` natively |
| `docker` / `docker:build` | No | Use `docker:buildx` for SSL support |
| `docker:buildx` | Yes | Full SSL via BuildKit secrets |
| `buildctl` | Yes | Full SSL via BuildKit secrets |
| `railpack` / `railpack:buildx` | Yes | Benefits from managed daemon |
| `railpack:buildctl` | Yes | Benefits from managed daemon |

## SSL Certificate Injection (Docker/Buildctl)

When using `docker:buildx` or `buildctl` with `SSL_CERT_FILE` set, Rise automatically injects certificates into Dockerfile `RUN` commands using BuildKit bind mounts.

Rise preprocesses your Dockerfile to mount certificates at standard system paths during each `RUN` command, and exports SSL environment variables (`SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, etc.) so build tools can find the certificates.

Certificates are only available during build — they are **not** embedded in the final image.

The `docker:build` backend does not support this feature. Use `docker:buildx` instead.

## SSL Certificate Embedding (Railpack)

For Railpack builds, when `SSL_CERT_FILE` is set, Rise automatically embeds the certificate into the Railpack build plan. This ensures `RUN` commands during the build can access the certificate for SSL verification.

Unlike the Docker injection above, this **does** embed the certificate in the final image.

When `SSL_CERT_FILE` is set, managed BuildKit also auto-enables, giving comprehensive SSL support at both the daemon and build level:

```bash
rise deploy --backend railpack
```

## Proxy Support

Rise automatically detects and injects HTTP/HTTPS proxy variables into all build backends:

- `HTTP_PROXY` / `http_proxy`
- `HTTPS_PROXY` / `https_proxy`
- `NO_PROXY` / `no_proxy`

Both uppercase and lowercase variants are detected from your environment.

### Localhost Transformation

Since builds run in containers, `localhost` and `127.0.0.1` addresses are automatically transformed to `host.docker.internal`:

- `http://localhost:3128` → `http://host.docker.internal:3128`
- `https://127.0.0.1:8080` → `https://host.docker.internal:8080`

`NO_PROXY` values are passed through unchanged.

No configuration is needed — proxy support is fully automatic.

## BuildKit Network Connectivity

When using the managed BuildKit daemon with Docker Compose services (e.g., a local registry), connect BuildKit to your compose network:

```bash
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
rise deploy
```

The daemon is recreated if the network name changes.

## Insecure Registries (Local Development)

For local HTTP registries, configure BuildKit to allow insecure connections:

```bash
export RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES="rise-registry:5000,localhost:5000"
rise deploy --managed-buildkit
```

Note: `--managed-buildkit` is needed here since insecure registries typically don't involve `SSL_CERT_FILE`, so auto-detection won't enable it.

This generates a `buildkitd.toml` config at `~/.rise/buildkitd.toml`. For local development only.

## Manual BuildKit Setup

For manual control, create your own BuildKit daemon:

```bash
docker run --platform linux/amd64 --privileged --name my-buildkit --rm -d \
  --volume $SSL_CERT_FILE:/etc/ssl/certs/ca-certificates.crt:ro \
  moby/buildkit

export BUILDKIT_HOST=docker-container://my-buildkit
rise deploy --backend railpack
```

## Podman Desktop

If using Podman Desktop behind a corporate proxy, you may need to configure SSL certificates in Podman's machine settings. Consult Podman Desktop documentation for your proxy setup.
