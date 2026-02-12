# Local Development Setup

Quick setup guide for running Rise locally with Docker Compose, Minikube, and the managed BuildKit daemon.

## Prerequisites

- Docker or Podman
- Minikube
- Docker Compose
- Root access for `/etc/hosts`

## Quick Setup

### 1. Clone and Configure Hosts

```bash
git clone https://github.com/NiklasRosenstein/rise.git
cd rise

# Add to /etc/hosts
sudo tee -a /etc/hosts <<EOF
127.0.0.1 rise.local
127.0.0.1 rise-registry
EOF
```

### 2. Configure Docker Daemon

Add to `/etc/docker/daemon.json` (Linux) or Docker Desktop settings:

```json
{
  "insecure-registries": [
    "rise-registry:5000",
    "localhost:5000",
    "127.0.0.1:5000"
  ]
}
```

Restart Docker after changes.

### 3. Start Services

```bash
docker-compose up -d
```

### 4. Configure BuildKit Network

```bash
# Configure environment
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
export RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES="rise-registry:5000,localhost:5000,127.0.0.1:5000"

# Add to shell profile
cat >> ~/.bashrc <<EOF
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
export RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES="rise-registry:5000,localhost:5000,127.0.0.1:5000"
EOF
```

### 5. Start Minikube

```bash
minikube start \
  --driver=docker \
  --insecure-registry="rise-registry:5000" \
  --host-dns-resolver=false
```

### 6. Run Backend

```bash
cargo build --features backend
mise sqlx:migrate
cargo run --features backend
```

In a new terminal:

```bash
# Test CLI
cargo run -- login http://rise.local:8080

# Create and deploy a project
cargo run -- project create my-app
cargo run -- deploy create --image nginx:latest
```

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `RISE_CONFIG_RUN_MODE` | Configuration mode | `development` |
| `RISE_MANAGED_BUILDKIT_NETWORK_NAME` | BuildKit network | `rise_default` |
| `RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES` | HTTP registries | `rise-registry:5000` |
| `DATABASE_URL` | PostgreSQL connection | `postgres://rise:rise123@localhost/rise` |

## Troubleshooting

### Registry Connection Errors

**Error:** `http: server gave HTTP response to HTTPS client`

**Fix:** Ensure `RISE_MANAGED_BUILDKIT_INSECURE_REGISTRIES` is set and contains your registry.

### BuildKit Can't Reach Registry

**Error:** BuildKit can't push to `rise-registry:5000`

**Fix:** Verify `RISE_MANAGED_BUILDKIT_NETWORK_NAME` is set correctly:
```bash
docker inspect rise-buildkit --format '{{range $net := .NetworkSettings.Networks}}{{$net}} {{end}}'
# Should show: bridge rise_default
```

### OAuth Redirect Issues

**Error:** OAuth redirects fail or redirect to wrong URL

**Fix:** Ensure `rise.local` is in `/etc/hosts` and Dex is configured with correct redirect URLs.

### Minikube Can't Pull Images

**Error:** Minikube pods fail with `ImagePullBackOff`

**Fix:** Verify registry access from within Minikube:
```bash
minikube ssh
curl http://rise-registry:5000/v2/
# Should return: {}
```

If it fails, check:
1. Minikube started with `--insecure-registry="rise-registry:5000"`
2. Host aliases configured in Minikube (should happen automatically)

## Architecture

```
┌─────────────────────────────────────────┐
│ Host Machine                             │
│  Rise CLI → Backend (rise.local:8080)   │
│  BuildKit → Registry (rise-registry)    │
│  Minikube ← Registry                    │
└─────────────────────────────────────────┘
```

**Key Points:**
- Registry runs in Docker Compose network
- BuildKit connects to Compose network via `RISE_MANAGED_BUILDKIT_NETWORK_NAME`
- Minikube accesses registry via host aliases
- All HTTP connections require insecure registry configuration

## Additional Resources

- Build system details: [docs/builds.md](builds.md)
- Development workflow: [docs/development.md](development.md)
- OAuth configuration: [docs/oauth.md](oauth.md)
