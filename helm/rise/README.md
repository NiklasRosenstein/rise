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

# Inject sensitive configuration via envFrom
envFrom:
  - secretRef:
      name: rise-secrets

# Server container resources
server:
  resources:
    limits:
      cpu: 2000m
      memory: 1Gi
    requests:
      cpu: 500m
      memory: 256Mi

# Controllers run as sidecar containers
controllers:
  deployment:
    enabled: true
    type: "deployment-kubernetes"
  project:
    enabled: true
  ecr:
    enabled: true

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

Create the secret with required environment variables:

```bash
kubectl create secret generic rise-secrets \
  --from-literal=DATABASE_URL=postgres://rise:YOUR_DB_PASSWORD@postgresql:5432/rise \
  --from-literal=RISE_AUTH__CLIENT_SECRET=YOUR_CLIENT_SECRET
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
| `envFrom` | List of sources to populate environment variables (required for secrets) | `[]` |
| `ingress.enabled` | Enable ingress | `false` |

### Server

| Parameter | Description | Default |
|-----------|-------------|---------|
| `server.resources` | Server container resource requests/limits | See values.yaml |

### Controllers

All controllers run as sidecar containers in the main deployment pod.

| Parameter | Description | Default |
|-----------|-------------|---------|
| `controllers.deployment.enabled` | Enable deployment controller | `true` |
| `controllers.deployment.type` | Controller type (`deployment-kubernetes` or `deployment-docker`) | `deployment-kubernetes` |
| `controllers.deployment.resources` | Deployment controller container resources | See values.yaml |
| `controllers.project.enabled` | Enable project controller | `true` |
| `controllers.project.resources` | Project controller container resources | See values.yaml |
| `controllers.ecr.enabled` | Enable ECR controller | `true` |
| `controllers.ecr.resources` | ECR controller container resources | See values.yaml |

### Dex (Optional OIDC Provider)

| Parameter | Description | Default |
|-----------|-------------|---------|
| `dex.enabled` | Enable Dex deployment | `false` |
| `dex.replicaCount` | Number of Dex replicas | `1` |
| `dex.image.repository` | Dex image repository | `ghcr.io/dexidp/dex` |
| `dex.image.tag` | Dex image tag | `v2.40.0` |
| `dex.service.port` | Dex HTTP port | `5556` |
| `dex.service.grpcPort` | Dex gRPC port | `5557` |
| `dex.issuerUrl` | OIDC issuer URL injected into Dex config | `http://dex:5556/dex` |
| `dex.config` | Dex configuration in YAML format (empty = use default) | `""` |

### PostgreSQL Dependency

| Parameter | Description | Default |
|-----------|-------------|---------|
| `postgresql.enabled` | Enable PostgreSQL subchart | `false` |
| `postgresql.auth.username` | PostgreSQL username | `rise` |
| `postgresql.auth.password` | PostgreSQL password | `rise123` |
| `postgresql.auth.database` | PostgreSQL database | `rise` |

**Note:** When `postgresql.enabled: true`, the chart automatically injects the `DATABASE_URL` environment variable into all containers using the configured credentials. You don't need to manually configure it via `envFrom`.

## Architecture

The chart uses a multi-container pod architecture where all Rise components run as containers in a single deployment:

1. **Server Container**: Handles API requests and user interactions (always present)
2. **Project Controller Container**: Handles project lifecycle management (optional, enabled by default)
3. **ECR Controller Container**: Manages ECR repository credentials (optional, enabled by default)
4. **Deployment Controller Container**: Manages application deployments to Kubernetes or Docker (optional, enabled by default)

All containers share the same configuration and secrets, reducing resource overhead and simplifying management.

### Separate Deployments

The chart also supports deploying these components separately:

- **Dex**: OIDC provider for authentication (optional, disabled by default, runs as a separate deployment)

## Security Considerations

### RBAC and Namespace Management

When the Kubernetes deployment controller is enabled (`controllers.deployment.type: deployment-kubernetes`), Rise requires cluster-wide permissions via ClusterRole and ClusterRoleBinding to:

- Create, manage, and delete namespaces
- Deploy applications (Deployments, Services, Ingresses) within those namespaces

**Important Security Notes:**

1. **Namespace Isolation**: Kubernetes RBAC does not support wildcard patterns for namespace names in ClusterRoles. Rise requires cluster-wide permissions to manage namespaces dynamically.

2. **Recommended Practices**:
   - Configure `namespace_format` with a consistent prefix (e.g., `rise-{project_name}`)
   - Use admission controllers (OPA/Gatekeeper) to enforce namespace naming policies
   - Implement network policies to isolate Rise-managed namespaces
   - Enable audit logging to monitor namespace creation and resource access
   - Consider using namespace quotas to limit resource consumption

3. **Example Enforcement with OPA Gatekeeper**:
   ```yaml
   # Ensure all Rise-created namespaces follow the naming pattern
   apiVersion: constraints.gatekeeper.sh/v1beta1
   kind: K8sRequiredLabels
   metadata:
     name: rise-namespace-naming
   spec:
     match:
       kinds:
       - apiGroups: [""]
         kinds: ["Namespace"]
     parameters:
       labels:
         - key: "app.kubernetes.io/managed-by"
           allowedRegex: "^rise$"
   ```

4. **Namespace Format Configuration**: Always align your `namespace_format` configuration in the Rise config with your organization's namespace policies.

## Configuration Format

### Rise Configuration

The `config` parameter accepts a TOML string that is passed directly to the ConfigMap. This allows for flexible configuration without requiring Helm chart updates for new options.

**Sensitive Values:** For sensitive configuration like secrets and passwords, use environment variables via `envFrom` instead of hardcoding in TOML. See [Environment Variables from Secrets/ConfigMaps](#environment-variables-from-secretsconfigmaps) below.

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
  client_secret = ""  # Placeholder - override with RISE_AUTH__CLIENT_SECRET env var

  [registry]
  type = "oci-client-auth"
  registry_url = "registry.example.com"
  namespace = "rise-apps"
```

**Note:** Required fields like `client_secret` must have a placeholder value (e.g., `""`) in the TOML, even if you override them with environment variables.

### Dex Configuration

When Dex is enabled (`dex.enabled: true`), the chart provides sensible defaults for development and testing.

#### Using the Default Configuration

By default, `dex.config` is empty and the chart uses a pre-configured development setup that includes:
- In-memory storage (data is lost on pod restart)
- Static password authentication with test users (admin@example.com and test@example.com, both with password "password")
- Configured redirect URIs for local development

To use the default config, simply set the issuer URL:

```yaml
dex:
  enabled: true
  issuerUrl: "https://dex.example.com/dex"
```

The `issuerUrl` will be automatically injected into the Dex configuration, replacing the default `http://localhost:5556/dex`.

#### Custom Configuration

For production deployments, you can provide a complete custom configuration via `dex.config`:

```yaml
dex:
  enabled: true
  issuerUrl: "https://dex.example.com/dex"
  config: |
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

**Note:** The `issuerUrl` is automatically injected as the `issuer:` field in the Dex config, so you don't need to specify it in the config YAML.

### Environment Variables from Secrets/ConfigMaps

The `envFrom` parameter allows you to inject environment variables from Secrets and ConfigMaps into all Rise containers (server and controllers). This is useful for:

- Injecting sensitive configuration that shouldn't be in the TOML config
- Overriding specific configuration values
- Managing environment-specific settings

#### How Environment Variable Overrides Work

Rise uses a layered configuration system where environment variables have the **highest priority** and override TOML values:

1. Load TOML config from ConfigMap
2. Apply environment variables (prefix: `RISE_`, separator: `__`)
3. Environment variables override TOML values

**Important:** For **required fields**, you must include a placeholder value in the TOML config, even if you plan to override it with an environment variable. Use an empty string (`""`) as the placeholder.

**Example:**

```yaml
# values.yaml
config: |
  [auth]
  issuer = "http://dex:5556/dex"
  client_id = "rise-backend"
  client_secret = ""  # Required placeholder - will be overridden by env var

envFrom:
  - secretRef:
      name: rise-secrets
```

```bash
# Create secret with actual value
kubectl create secret generic rise-secrets \
  --from-literal=RISE_AUTH__CLIENT_SECRET=your-actual-secret
```

The final configuration will use `client_secret = "your-actual-secret"` from the environment variable.

**Environment Variable Format:**
- TOML: `[section]` → `key = "value"`
- Env Var: `RISE_SECTION__KEY=value`
- Example: `[auth]` → `client_secret = "..."` becomes `RISE_AUTH__CLIENT_SECRET=...`

**Example: Using a Secret for AWS credentials**

Create a secret with your AWS credentials:

```bash
kubectl create secret generic rise-aws-creds \
  --from-literal=AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE \
  --from-literal=AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
  --from-literal=AWS_REGION=us-east-1
```

Configure the chart to use this secret:

```yaml
envFrom:
  - secretRef:
      name: rise-aws-creds
```

**Example: Using multiple sources**

You can combine multiple Secrets and ConfigMaps:

```yaml
envFrom:
  # AWS credentials from secret
  - secretRef:
      name: rise-aws-creds
  # Additional environment-specific config
  - configMapRef:
      name: rise-env-config
  # Override specific settings
  - secretRef:
      name: rise-overrides
```

**Note:** Environment variables set via `envFrom` can override TOML configuration values. Rise follows standard environment variable precedence where `RISE_SECTION__KEY` maps to `[section] key` in TOML.

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
