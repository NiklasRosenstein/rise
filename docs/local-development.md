# Local Development Networking Guide

This guide provides comprehensive instructions for setting up local development networking with Rise. Understanding the networking flow and proper configuration is essential for running Rise locally with Docker Compose, Minikube, and BuildKit.

## Table of Contents

- [Introduction](#introduction)
- [Networking Architecture](#networking-architecture)
- [Prerequisites](#prerequisites)
- [Name Resolution](#name-resolution)
- [Docker Registry Configuration](#docker-registry-configuration)
- [BuildKit Network Connectivity](#buildkit-network-connectivity)
- [Minikube Configuration](#minikube-configuration)
- [Complete Setup Walkthrough](#complete-setup-walkthrough)
- [Troubleshooting](#troubleshooting)
- [Environment Variables Reference](#environment-variables-reference)

## Introduction

Rise's local development environment consists of several interconnected components:

- **Rise Backend**: The API server running on `http://rise.local:8080`
- **Dex**: OIDC provider for authentication
- **PostgreSQL**: Database backend
- **Docker Registry**: Internal container registry at `rise-registry:5000`
- **Minikube**: Local Kubernetes cluster for deployment testing
- **BuildKit**: Container image build daemon

The networking setup ensures these components can communicate properly, with special attention to:
- Name resolution (DNS and /etc/hosts)
- Container network connectivity
- SSL/TLS certificate handling
- Registry authentication

## Networking Architecture

### Component Communication

```
┌─────────────────────────────────────────────────────────────────┐
│                        Host Machine                              │
│                                                                   │
│  ┌──────────────┐         ┌──────────────┐                      │
│  │ Rise CLI     │────────▶│ Rise Backend │                      │
│  │              │         │ rise.local   │                      │
│  └──────────────┘         └──────┬───────┘                      │
│                                   │                               │
│  ┌──────────────┐         ┌──────▼───────┐                      │
│  │ BuildKit     │────────▶│ Registry     │                      │
│  │ (managed)    │         │ rise-registry│                      │
│  └──────────────┘         └──────────────┘                      │
│         │                                                         │
│         │                 ┌──────────────┐                      │
│         └────────────────▶│ Minikube     │                      │
│                           │ (K8s cluster)│                      │
│                           └──────────────┘                      │
└─────────────────────────────────────────────────────────────────┘
```

### Network Flows

1. **CLI → Backend**: HTTP API calls to `http://rise.local:8080`
2. **Backend → Registry**: Image push/pull operations
3. **BuildKit → Registry**: Image push during builds
4. **Minikube → Registry**: Image pull for deployments
5. **BuildKit → Minikube**: Network connectivity for builds (optional)

## Prerequisites

Before setting up local development, ensure you have:

- Docker or Podman installed
- Minikube installed (for Kubernetes deployment testing)
- Docker Compose or equivalent
- Root/sudo access for /etc/hosts modification
- Rise source code cloned locally

## Name Resolution

### Host Machine Configuration

The Rise backend and registry need to be accessible via consistent hostnames. Add these entries to `/etc/hosts`:

```bash
# Rise local development
127.0.0.1 rise.local
127.0.0.1 rise-registry
```

**On Linux/macOS:**
```bash
sudo tee -a /etc/hosts <<EOF
# Rise local development
127.0.0.1 rise.local
127.0.0.1 rise-registry
EOF
```

**On Windows (PowerShell as Administrator):**
```powershell
Add-Content -Path C:\Windows\System32\drivers\etc\hosts -Value "`n# Rise local development`n127.0.0.1 rise.local`n127.0.0.1 rise-registry"
```

### Docker Compose Network

The Docker Compose setup creates a bridge network where services can reference each other by service name. The registry is accessible at:

- From host: `rise-registry:5000`
- From containers: `rise-registry:5000`
- Inside compose network: `rise-registry:5000`

### Minikube Network

Minikube runs in its own VM/container and needs special configuration to access host services. See [Minikube Configuration](#minikube-configuration) below.

## Docker Registry Configuration

### Insecure Registry Setup

The local Docker registry runs without HTTPS (insecure). You must configure your Docker daemon to allow insecure registry access.

**Location of daemon.json:**
- Linux: `/etc/docker/daemon.json`
- macOS: `~/.docker/daemon.json` or Docker Desktop settings
- Windows: `C:\ProgramData\docker\config\daemon.json` or Docker Desktop settings

**Configuration:**

Add the following to your Docker daemon configuration:

```json
{
  "insecure-registries": [
    "rise-registry:5000",
    "localhost:5000",
    "127.0.0.1:5000"
  ]
}
```

If the file already exists, merge the `insecure-registries` array with existing content.

**Apply the configuration:**

```bash
# Linux (systemd)
sudo systemctl restart docker

# macOS/Windows
# Restart Docker Desktop from the system tray
```

**Verify the configuration:**

```bash
docker info | grep -A5 "Insecure Registries"
```

You should see:
```
Insecure Registries:
  rise-registry:5000
  localhost:5000
  127.0.0.1:5000
  127.0.0.0/8
```

### Testing Registry Access

Once configured, test registry access:

```bash
# Pull a small image
docker pull alpine:latest

# Tag it for your local registry
docker tag alpine:latest rise-registry:5000/test:latest

# Push to the registry
docker push rise-registry:5000/test:latest

# Clean up
docker rmi rise-registry:5000/test:latest
```

## BuildKit Network Connectivity

### The Challenge

When using Rise's managed BuildKit daemon (the `rise-buildkit` container), the daemon runs in isolation from the Docker Compose network. This creates a problem:

- BuildKit needs to push images to `rise-registry:5000`
- The registry runs inside the Docker Compose network
- BuildKit can't access `rise-registry` without network connectivity

### The Solution: Managed BuildKit Network

Rise supports connecting the managed BuildKit daemon to a custom Docker network via the `RISE_MANAGED_BUILDKIT_NETWORK_NAME` environment variable.

**How it works:**

1. Set `RISE_MANAGED_BUILDKIT_NETWORK_NAME` to your Docker Compose network name
2. Rise automatically:
   - Creates the network if it doesn't exist
   - Connects the `rise-buildkit` container to that network
   - Tracks network configuration in container labels
   - Recreates the daemon if the network changes

**Configuration:**

First, identify your Docker Compose network name:

```bash
# List networks
docker network ls

# Find the Rise compose network (usually rise_default)
docker network ls | grep rise
```

The network name is typically `<project>_default` where `<project>` is your directory name or the compose project name.

**Set the environment variable:**

```bash
# Bash/Zsh
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default

# Fish
set -x RISE_MANAGED_BUILDKIT_NETWORK_NAME rise_default

# PowerShell
$env:RISE_MANAGED_BUILDKIT_NETWORK_NAME = "rise_default"
```

Add this to your shell profile (`.bashrc`, `.zshrc`, etc.) for persistence.

**For per-project configuration**, add to your `.envrc` (if using direnv):

```bash
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
```

**Verify BuildKit connectivity:**

After setting the environment variable, the next build will connect BuildKit to the network:

```bash
# Build an image (BuildKit will be connected)
rise build --managed-buildkit

# Verify network connection
docker inspect rise-buildkit --format '{{range $net, $config := .NetworkSettings.Networks}}{{$net}} {{end}}'
# Should show: bridge rise_default
```

### Alternative: host.docker.internal

If you don't want to configure the network, you can map the registry through the host:

1. The managed BuildKit daemon already includes `--add-host host.docker.internal:host-gateway`
2. Configure your registry to listen on the host at port 5000
3. Reference the registry as `host.docker.internal:5000` instead of `rise-registry:5000`

This approach works but requires changing registry configuration and is less isolated than using Docker networks.

### BuildKit Network Labels

Rise tracks network configuration in container labels:

- `rise.network_name`: The network name BuildKit is connected to
- `rise.ssl_cert_hash`: SSL certificate hash (if configured)
- `rise.ssl_cert_file`: SSL certificate path (if configured)
- `rise.no_ssl_cert`: Marker for no SSL certificate

If the network name changes, Rise automatically recreates the daemon.

## Minikube Configuration

### Host Aliases

Minikube runs in a separate VM/container and needs to resolve `rise.local` and `rise-registry` to the host machine.

**Start Minikube with host aliases:**

```bash
minikube start --driver=docker \
  --host-dns-resolver=false \
  --extra-config=kubeadm.pod-network-cidr=10.244.0.0/16
```

**Configure host aliases in deployments:**

The Rise Kubernetes controller automatically adds host aliases to pods for registry access. However, you may need to add `rise.local` for OAuth redirects:

```yaml
# Example pod spec
spec:
  hostAliases:
  - ip: "192.168.65.2"  # Minikube host IP (varies by platform)
    hostnames:
    - "rise.local"
    - "rise-registry"
```

**Find your Minikube host IP:**

```bash
# Linux
minikube ssh "ip route | grep ^default | awk '{print \$3}'"

# macOS (Docker Desktop)
# Usually 192.168.65.2 or 192.168.64.1

# Windows (Docker Desktop)
# Usually 192.168.65.2
```

### Insecure Registry in Minikube

Configure Minikube to allow pulling from the insecure registry:

```bash
minikube start \
  --insecure-registry="rise-registry:5000" \
  --insecure-registry="192.168.65.2:5000"
```

Or edit the Minikube Docker daemon config:

```bash
minikube ssh
sudo vi /etc/docker/daemon.json
```

Add:
```json
{
  "insecure-registries": ["rise-registry:5000"]
}
```

Then restart:
```bash
minikube ssh "sudo systemctl restart docker"
```

### Test Minikube Registry Access

```bash
# SSH into Minikube
minikube ssh

# Try to pull from the registry
docker pull rise-registry:5000/test:latest

# Exit Minikube
exit
```

## Complete Setup Walkthrough

This section provides a step-by-step guide for setting up Rise local development from scratch.

### 1. Clone Repository

```bash
git clone https://github.com/your-org/rise.git
cd rise
```

### 2. Configure /etc/hosts

```bash
# Add host entries
sudo tee -a /etc/hosts <<EOF
# Rise local development
127.0.0.1 rise.local
127.0.0.1 rise-registry
EOF
```

### 3. Configure Docker Registry

Edit `/etc/docker/daemon.json` (create if it doesn't exist):

```json
{
  "insecure-registries": [
    "rise-registry:5000",
    "localhost:5000",
    "127.0.0.1:5000"
  ]
}
```

Restart Docker:
```bash
sudo systemctl restart docker  # Linux
# Or restart Docker Desktop on macOS/Windows
```

### 4. Start Docker Compose Services

```bash
# Start PostgreSQL, Dex, and Registry
docker-compose up -d

# Wait for services to be ready
docker-compose ps
```

### 5. Configure BuildKit Network

```bash
# Find your compose network name
NETWORK_NAME=$(docker network ls --filter "name=rise" --format "{{.Name}}" | grep default)
echo "Network name: $NETWORK_NAME"

# Set environment variable
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=$NETWORK_NAME

# Add to your shell profile for persistence
echo "export RISE_MANAGED_BUILDKIT_NETWORK_NAME=$NETWORK_NAME" >> ~/.bashrc
```

### 6. Start Minikube

```bash
minikube start \
  --driver=docker \
  --insecure-registry="rise-registry:5000" \
  --host-dns-resolver=false
```

### 7. Build and Run Rise Backend

```bash
# Build the backend (server features)
cargo build --features backend

# Run migrations
mise sqlx:migrate

# Start the backend
cargo run --features backend
```

In a new terminal:

```bash
# Build the CLI
cargo build --features cli

# Login to Rise
rise login

# Create a project
rise project create my-app

# Deploy an app
cd /path/to/your/app
rise deploy create my-app
```

### 8. Verify Everything Works

```bash
# Check registry has images
curl http://rise-registry:5000/v2/_catalog

# Check Minikube can access registry
minikube ssh "docker pull rise-registry:5000/my-app:latest"

# Check deployment status
rise deployment list my-app
```

## Troubleshooting

### x509: Certificate Signed by Unknown Authority

**Symptom:**
```
Error: failed to push image: ... x509: certificate signed by unknown authority
```

**Causes:**
1. Docker daemon not configured for insecure registry
2. BuildKit not configured for insecure registry
3. SSL certificate not properly mounted

**Solutions:**

1. Verify Docker daemon configuration:
   ```bash
   docker info | grep -A5 "Insecure Registries"
   ```

2. Verify BuildKit network connectivity:
   ```bash
   docker inspect rise-buildkit --format '{{range $net := .NetworkSettings.Networks}}{{$net}} {{end}}'
   ```

3. Set BuildKit network:
   ```bash
   export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
   ```

4. Restart BuildKit daemon:
   ```bash
   docker stop rise-buildkit
   rise build --managed-buildkit  # Will recreate daemon
   ```

### Cannot Connect to rise-registry:5000

**Symptom:**
```
Error: failed to connect to rise-registry:5000: dial tcp: lookup rise-registry: no such host
```

**Causes:**
1. /etc/hosts not configured
2. BuildKit not connected to compose network
3. Minikube not configured with host aliases

**Solutions:**

1. Verify /etc/hosts:
   ```bash
   grep rise-registry /etc/hosts
   # Should show: 127.0.0.1 rise-registry
   ```

2. Verify BuildKit network (if using managed BuildKit):
   ```bash
   docker inspect rise-buildkit | grep -A10 Networks
   ```

3. Set BuildKit network:
   ```bash
   export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
   docker stop rise-buildkit
   ```

### OAuth Redirect Failures

**Symptom:**
```
Error: OAuth redirect failed: could not resolve rise.local
```

**Causes:**
1. /etc/hosts not configured
2. Browser DNS cache
3. Dex configuration incorrect

**Solutions:**

1. Verify /etc/hosts:
   ```bash
   grep rise.local /etc/hosts
   ```

2. Clear browser DNS cache:
   - Chrome: `chrome://net-internals/#dns` → Clear host cache
   - Firefox: Restart browser
   - Safari: Clear cache

3. Test DNS resolution:
   ```bash
   ping rise.local
   curl http://rise.local:8080/health
   ```

### Minikube Cannot Pull Images

**Symptom:**
```
Failed to pull image "rise-registry:5000/my-app:latest": rpc error: code = Unknown desc = failed to pull and unpack image
```

**Causes:**
1. Minikube not configured for insecure registry
2. Host aliases not configured
3. Registry not accessible from Minikube

**Solutions:**

1. Restart Minikube with insecure registry:
   ```bash
   minikube delete
   minikube start --insecure-registry="rise-registry:5000"
   ```

2. Test registry access from Minikube:
   ```bash
   minikube ssh "ping -c2 rise-registry"
   minikube ssh "curl http://rise-registry:5000/v2/_catalog"
   ```

3. Verify Minikube host IP:
   ```bash
   minikube ssh "ip route | grep ^default"
   ```

### BuildKit Daemon Won't Start

**Symptom:**
```
Error: Failed to create BuildKit daemon
```

**Causes:**
1. Container runtime (Docker/Podman) not running
2. Insufficient permissions
3. Port conflicts
4. Network conflicts

**Solutions:**

1. Check Docker/Podman:
   ```bash
   docker ps  # or podman ps
   ```

2. Check existing BuildKit containers:
   ```bash
   docker ps -a | grep buildkit
   docker rm -f rise-buildkit  # Remove if stuck
   ```

3. Check logs:
   ```bash
   docker logs rise-buildkit
   ```

4. Verify network exists:
   ```bash
   docker network ls | grep rise
   docker network inspect rise_default
   ```

## Environment Variables Reference

### Core Configuration

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `RISE_API_URL` | Backend API URL | `http://rise.local:8080` | `http://localhost:8080` |
| `RISE_MANAGED_BUILDKIT` | Enable managed BuildKit daemon | `false` | `true` |
| `RISE_MANAGED_BUILDKIT_NETWORK_NAME` | Docker network for BuildKit | `None` | `rise_default` |
| `SSL_CERT_FILE` | Path to SSL certificate bundle | `None` | `/etc/ssl/certs/ca-certificates.crt` |

### Registry Configuration

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `DOCKER_REGISTRY` | Internal registry URL | `rise-registry:5000` | `localhost:5000` |

### Authentication

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `DEX_ISSUER` | Dex OIDC issuer URL | `http://rise.local:5556/dex` | `https://dex.example.com` |
| `DEX_CLIENT_ID` | OAuth client ID | `rise-cli` | `custom-client` |

### Database

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `DATABASE_URL` | PostgreSQL connection string | See config | `postgresql://user:pass@localhost/rise` |

### Kubernetes

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `KUBECONFIG` | Path to kubeconfig file | `~/.kube/config` | `/custom/path/kubeconfig` |
| `KUBE_NAMESPACE` | Default namespace | `default` | `rise-apps` |

### Advanced

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `RUST_LOG` | Logging level | `info` | `debug`, `rise=debug` |
| `CONTAINER_CLI` | Container runtime | `docker` | `podman` |

### Environment Variable Precedence

Rise loads configuration in the following order (later sources override earlier):

1. Default values in code
2. Configuration file (`config/development.yaml`)
3. Environment variables
4. Command-line arguments

### Setting Environment Variables

**Bash/Zsh:**
```bash
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
```

**Fish:**
```fish
set -x RISE_MANAGED_BUILDKIT_NETWORK_NAME rise_default
```

**PowerShell:**
```powershell
$env:RISE_MANAGED_BUILDKIT_NETWORK_NAME = "rise_default"
```

**direnv (.envrc):**
```bash
export RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
export RISE_MANAGED_BUILDKIT=true
export RUST_LOG=debug
```

**Docker Compose (.env):**
```env
RISE_MANAGED_BUILDKIT_NETWORK_NAME=rise_default
RISE_MANAGED_BUILDKIT=true
```

---

## Summary

Local development with Rise requires careful networking configuration to ensure all components can communicate properly. The key elements are:

1. **Name Resolution**: `/etc/hosts` entries for `rise.local` and `rise-registry`
2. **Registry Access**: Docker daemon configured for insecure registry
3. **BuildKit Connectivity**: Network connection via `RISE_MANAGED_BUILDKIT_NETWORK_NAME`
4. **Minikube Setup**: Insecure registry and host aliases configured

Following this guide ensures a smooth local development experience with Rise.
