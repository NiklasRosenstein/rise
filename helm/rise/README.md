# Rise Helm Chart

This Helm chart deploys the Rise application on Kubernetes, including the backend API server, controllers, and optionally Dex for OIDC authentication.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- PostgreSQL database (can be deployed as a dependency or use external database)

## Installation

### Using default values (for testing only)

```bash
helm install rise ./helm/rise
```

### Production installation with custom values

Create a `values-prod.yaml` file:

```yaml
image:
  repository: ghcr.io/niklasrosenstein/rise-backend
  tag: "v0.1.0"
  pullPolicy: IfNotPresent

# Rise configuration in TOML format
config: |
  [server]
  host = "0.0.0.0"
  port = 3000
  public_url = "https://rise.example.com"

  [auth]
  issuer = "https://dex.example.com/dex"
  client_id = "rise-backend"
  admin_users = ["admin@example.com"]

  [registry]
  type = "oci-client-auth"
  registry_url = "registry.example.com"
  namespace = "rise-apps"

  [kubernetes]
  ingress_class = "nginx"
  domain_suffix = "apps.example.com"

ingress:
  enabled: true
  className: "nginx"
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  hosts:
    - host: rise.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: rise-tls
      hosts:
        - rise.example.com

# Create a secret with sensitive values
existingSecret: "rise-secrets"

resources:
  limits:
    cpu: 2000m
    memory: 1Gi
  requests:
    cpu: 500m
    memory: 256Mi

# Optional: Enable Dex for OIDC authentication
dex:
  enabled: true
  config: |
    issuer: https://dex.example.com/dex

    storage:
      type: kubernetes
      config:
        inCluster: true

    web:
      http: 0.0.0.0:5556

    staticClients:
    - id: rise-backend
      redirectURIs:
      - 'https://rise.example.com/auth/callback'
      name: 'Rise Backend'
      secret: your-client-secret-here

    connectors:
    - type: github
      id: github
      name: GitHub
      config:
        clientID: $GITHUB_CLIENT_ID
        clientSecret: $GITHUB_CLIENT_SECRET
        redirectURI: https://dex.example.com/dex/callback
```

Create the secret:

```bash
kubectl create secret generic rise-secrets \
  --from-literal=auth-client-secret=YOUR_CLIENT_SECRET \
  --from-literal=database-password=YOUR_DB_PASSWORD \
  --from-literal=database-url=postgres://rise:YOUR_DB_PASSWORD@postgresql:5432/rise
```

Install the chart:

```bash
helm install rise ./helm/rise -f values-prod.yaml
```

## Configuration

The following table lists the configurable parameters of the Rise chart and their default values.

### Core Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of server replicas | `1` |
| `image.repository` | Image repository | `ghcr.io/niklasrosenstein/rise-backend` |
| `image.tag` | Image tag | `""` (uses Chart appVersion) |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `config` | Rise configuration in TOML format | See values.yaml |
| `existingSecret` | Name of existing secret with sensitive values | `""` |
| `ingress.enabled` | Enable ingress | `false` |

### Controllers

| Parameter | Description | Default |
|-----------|-------------|---------|
| `controllers.deployment.enabled` | Enable deployment controller | `true` |
| `controllers.deployment.replicaCount` | Number of deployment controller replicas | `1` |
| `controllers.project.enabled` | Enable project controller | `true` |
| `controllers.project.replicaCount` | Number of project controller replicas | `1` |

### Dex (Optional OIDC Provider)

| Parameter | Description | Default |
|-----------|-------------|---------|
| `dex.enabled` | Enable Dex deployment | `false` |
| `dex.replicaCount` | Number of Dex replicas | `1` |
| `dex.image.repository` | Dex image repository | `ghcr.io/dexidp/dex` |
| `dex.image.tag` | Dex image tag | `v2.40.0` |
| `dex.service.port` | Dex HTTP port | `5556` |
| `dex.service.grpcPort` | Dex gRPC port | `5557` |
| `dex.config` | Dex configuration in YAML format | See values.yaml |

### PostgreSQL Dependency

| Parameter | Description | Default |
|-----------|-------------|---------|
| `postgresql.enabled` | Enable PostgreSQL subchart | `false` |
| `postgresql.auth.username` | PostgreSQL username | `rise` |
| `postgresql.auth.database` | PostgreSQL database | `rise` |

## Components

The chart can deploy the following components:

1. **Server Deployment**: Handles API requests and user interactions (always deployed)
2. **Deployment Controller**: Manages application deployments (enabled by default)
3. **Project Controller**: Handles project lifecycle management (enabled by default)
4. **Dex**: OIDC provider for authentication (optional, disabled by default)

## Configuration Format

### Rise Configuration

The `config` parameter accepts a TOML string that is passed directly to the ConfigMap. This allows for flexible configuration without requiring Helm chart updates for new options.

Example:

```yaml
config: |
  [server]
  host = "0.0.0.0"
  port = 3000
  public_url = "https://rise.example.com"

  [auth]
  issuer = "https://dex.example.com/dex"
  client_id = "rise-backend"

  [registry]
  type = "oci-client-auth"
  registry_url = "registry.example.com"
  namespace = "rise-apps"
```

### Dex Configuration

When Dex is enabled (`dex.enabled: true`), you can provide Dex configuration in YAML format via `dex.config`:

```yaml
dex:
  enabled: true
  config: |
    issuer: https://dex.example.com/dex

    storage:
      type: kubernetes
      config:
        inCluster: true

    web:
      http: 0.0.0.0:5556

    staticClients:
    - id: rise-backend
      redirectURIs:
      - 'https://rise.example.com/auth/callback'
      name: 'Rise Backend'
      secret: your-client-secret-here

    connectors:
    - type: github
      id: github
      name: GitHub
      config:
        clientID: $GITHUB_CLIENT_ID
        clientSecret: $GITHUB_CLIENT_SECRET
        redirectURI: https://dex.example.com/dex/callback
```

## Upgrading

To upgrade an existing release:

```bash
helm upgrade rise ./helm/rise -f values-prod.yaml
```

## Uninstalling

To uninstall/delete the `rise` deployment:

```bash
helm uninstall rise
```
