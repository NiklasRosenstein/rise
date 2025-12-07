# Rise Backend Helm Chart

This Helm chart deploys the Rise Backend application on Kubernetes.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- PostgreSQL database (can be deployed as a dependency or use external database)

## Installation

### Using default values (for testing only)

```bash
helm install rise-backend ./helm/rise-backend
```

### Production installation with custom values

Create a `values-prod.yaml` file:

```yaml
image:
  repository: ghcr.io/yourusername/rise-backend
  tag: "v0.1.0"
  pullPolicy: IfNotPresent

config:
  server:
    publicUrl: "https://rise.example.com"

  auth:
    issuer: "https://dex.example.com/dex"
    clientId: "rise-backend"
    adminUsers:
      - admin@example.com

  database:
    host: "postgresql.database.svc.cluster.local"
    port: 5432
    name: "rise"
    user: "rise"

  registry:
    type: "oci-client-auth"
    registryUrl: "registry.example.com"
    namespace: "rise-apps"

  kubernetes:
    enabled: true
    ingressClass: "nginx"
    domainSuffix: "apps.example.com"

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
helm install rise-backend ./helm/rise-backend -f values-prod.yaml
```

## Configuration

The following table lists the configurable parameters of the Rise Backend chart and their default values.

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of server replicas | `1` |
| `image.repository` | Image repository | `ghcr.io/yourusername/rise-backend` |
| `image.tag` | Image tag | `""` (uses Chart appVersion) |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `config.server.publicUrl` | Public URL for the backend | `http://rise.example.com` |
| `config.auth.issuer` | OIDC issuer URL | `http://dex:5556/dex` |
| `config.auth.clientId` | OAuth client ID | `rise-backend` |
| `config.auth.adminUsers` | List of admin user emails | `[]` |
| `config.database.host` | PostgreSQL host | `postgresql` |
| `config.database.port` | PostgreSQL port | `5432` |
| `config.database.name` | PostgreSQL database name | `rise` |
| `config.database.user` | PostgreSQL user | `rise` |
| `config.registry.type` | Registry type | `oci-client-auth` |
| `config.registry.registryUrl` | Container registry URL | `registry.example.com` |
| `config.registry.namespace` | Registry namespace | `rise-apps` |
| `config.kubernetes.enabled` | Enable Kubernetes controller | `false` |
| `existingSecret` | Name of existing secret with sensitive values | `""` |
| `controllers.deployment.enabled` | Enable deployment controller | `true` |
| `controllers.project.enabled` | Enable project controller | `true` |
| `ingress.enabled` | Enable ingress | `false` |

## Components

The chart deploys the following components:

1. **Server Deployment**: Handles API requests and user interactions
2. **Deployment Controller**: Manages application deployments (when using Kubernetes controller)
3. **Project Controller**: Handles project lifecycle management

## Upgrading

To upgrade an existing release:

```bash
helm upgrade rise-backend ./helm/rise-backend -f values-prod.yaml
```

## Uninstalling

To uninstall/delete the `rise-backend` deployment:

```bash
helm uninstall rise-backend
```
