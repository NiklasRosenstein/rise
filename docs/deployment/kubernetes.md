# Kubernetes Deployment Backend

The Kubernetes deployment backend deploys applications to Kubernetes clusters using ReplicaSets, Services, and Ingresses.

## Overview

The Kubernetes controller manages application deployments on Kubernetes by:
- Creating namespace-scoped resources for each project
- Deploying applications as ReplicaSets with rolling updates
- Managing traffic routing with Services and Ingresses
- Implementing blue/green deployments via Service selector updates
- Automatically refreshing image pull secrets for private registries

## Configuration

### TOML Configuration

```toml
[kubernetes]
# Optional: path to kubeconfig (defaults to in-cluster if not set)
kubeconfig = "/path/to/kubeconfig"

# Ingress class to use
ingress_class = "nginx"

# Domain suffix for default deployment group
domain_suffix = "apps.rise.dev"

# Optional: separate domain for non-default groups
non_default_domain_suffix = "preview.rise.dev"
```

### Environment Variables

Configure the Kubernetes backend using environment variables:

```bash
# Optional: Kubeconfig path (omit to use in-cluster config)
RISE_KUBERNETES__KUBECONFIG="/path/to/kubeconfig"

# Ingress class (required)
RISE_KUBERNETES__INGRESS_CLASS="nginx"

# Domain suffix for default group (required)
RISE_KUBERNETES__DOMAIN_SUFFIX="apps.rise.dev"

# Optional: Domain suffix for non-default groups
RISE_KUBERNETES__NON_DEFAULT_DOMAIN_SUFFIX="preview.rise.dev"
```

### Kubeconfig Options

The controller supports two authentication modes:

**In-cluster mode** (recommended for production):
- Omit `kubeconfig` setting
- Uses service account mounted at `/var/run/secrets/kubernetes.io/serviceaccount/`
- Requires RBAC permissions for the controller's service account

**External kubeconfig**:
- Set `kubeconfig` path explicitly
- Useful for development or external cluster access
- Falls back to `~/.kube/config` if path not specified

## How It Works

### Resources Managed

The Kubernetes controller creates and manages the following resources per project:

| Resource | Scope | Purpose |
|----------|-------|---------|
| Namespace | One per project | Isolates project resources |
| ReplicaSet | One per deployment | Runs application pods |
| Service | One per deployment group | Routes traffic to active deployment |
| Ingress | One per deployment group | Exposes HTTP/HTTPS endpoints |
| Secret | One per project | Stores image pull credentials |

### Naming Scheme

Resources follow consistent naming patterns:

| Resource | Pattern | Example |
|----------|---------|---------|
| Namespace | `rise-{project}` | `rise-my-app` |
| ReplicaSet | `{project}-{deployment_id}` | `my-app-20251207-143022` |
| Service | `{escaped_group}` | `default`, `mr--26` |
| Ingress | `{escaped_group}` | `default`, `mr--26` |
| Secret | `rise-registry-creds` | `rise-registry-creds` |

**Character escaping**: Deployment group names containing invalid Kubernetes characters (e.g., `/`, `@`) are escaped with `--`. For example, `mr/26` becomes `mr--26`.

### Deployment Groups and URLs

Each deployment group gets its own Service and Ingress with a unique URL:

| Group | URL Pattern | Example |
|-------|-------------|---------|
| `default` | `{project}.{domain_suffix}` | `my-app.apps.rise.dev` |
| Custom groups | `{project}-{group}.{non_default_domain_suffix}` | `my-app-mr--26.preview.rise.dev` |

### Blue/Green Deployments

The controller implements blue/green deployments using Service selector updates:

1. **Deploy new ReplicaSet**: Create new ReplicaSet with deployment-specific labels
2. **Wait for health**: Wait until new ReplicaSet pods are ready and pass health checks
3. **Switch traffic**: Update Service selector to point to new deployment labels
4. **Previous deployment**: Old ReplicaSet remains but receives no traffic

This ensures zero-downtime deployments with instant rollback capability.

### Labels

All resources are labeled for management and selection:

```yaml
labels:
  rise.dev/managed-by: "rise"
  rise.dev/project: "my-app"
  rise.dev/deployment-group: "default"
  rise.dev/deployment-id: "20251207-143022"
```

## Kubernetes Resources

### Namespace

Created once per project:

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: rise-my-app
  labels:
    rise.dev/managed-by: "rise"
    rise.dev/project: "my-app"
```

### Secret (Image Pull Credentials)

Created/refreshed automatically for private registries:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: rise-registry-creds
  namespace: rise-my-app
  annotations:
    rise.dev/last-refresh: "2025-12-07T14:30:22Z"
type: kubernetes.io/dockerconfigjson
data:
  .dockerconfigjson: <base64-encoded-docker-config>
```

**Auto-refresh**: Secrets are automatically refreshed every hour to handle short-lived credentials (e.g., ECR tokens expire after 12 hours).

### ReplicaSet

One per deployment:

```yaml
apiVersion: apps/v1
kind: ReplicaSet
metadata:
  name: my-app-20251207-143022
  namespace: rise-my-app
  labels:
    rise.dev/managed-by: "rise"
    rise.dev/project: "my-app"
    rise.dev/deployment-group: "default"
    rise.dev/deployment-id: "20251207-143022"
spec:
  replicas: 1
  selector:
    matchLabels:
      rise.dev/project: "my-app"
      rise.dev/deployment-group: "default"
      rise.dev/deployment-id: "20251207-143022"
  template:
    metadata:
      labels:
        rise.dev/project: "my-app"
        rise.dev/deployment-group: "default"
        rise.dev/deployment-id: "20251207-143022"
    spec:
      imagePullSecrets:
        - name: rise-registry-creds
      containers:
        - name: app
          image: registry.example.com/my-app@sha256:abc123...
          ports:
            - containerPort: 8080
```

### Service

One per deployment group (updated via server-side apply):

```yaml
apiVersion: v1
kind: Service
metadata:
  name: default
  namespace: rise-my-app
  labels:
    rise.dev/managed-by: "rise"
    rise.dev/project: "my-app"
spec:
  type: ClusterIP
  selector:
    rise.dev/project: "my-app"
    rise.dev/deployment-group: "default"
    rise.dev/deployment-id: "20251207-143022"  # Updated on traffic switch
  ports:
    - port: 80
      targetPort: 8080
      protocol: TCP
```

### Ingress

One per deployment group:

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: default
  namespace: rise-my-app
  labels:
    rise.dev/managed-by: "rise"
    rise.dev/project: "my-app"
  annotations:
    kubernetes.io/ingress.class: "nginx"
spec:
  rules:
    - host: my-app.apps.rise.dev
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: default
                port:
                  number: 80
```

## Running the Controller

### Starting the Controller

```bash
# Run Kubernetes deployment controller
rise backend controller deployment-kubernetes
```

The controller will:
1. Connect to Kubernetes using configured kubeconfig or in-cluster credentials
2. Start reconciliation loop for deployments in `Pushed`, `Deploying`, `Healthy`, or `Unhealthy` status
3. Start image pull secret refresh loop (runs hourly)
4. Process deployments continuously until stopped

### Required RBAC Permissions

The controller requires the following Kubernetes permissions:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: rise-controller
rules:
  - apiGroups: [""]
    resources: ["namespaces"]
    verbs: ["get", "list", "create", "update", "patch"]
  - apiGroups: [""]
    resources: ["secrets", "services"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  - apiGroups: ["apps"]
    resources: ["replicasets"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list"]
  - apiGroups: ["networking.k8s.io"]
    resources: ["ingresses"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
```

### Basic Troubleshooting

**Permission errors**:
```
Error: Forbidden (403): namespaces is forbidden
```
- Verify service account has required RBAC permissions
- Check `kubectl auth can-i` for each required verb/resource

**Connection errors**:
```
Error: Failed to connect to Kubernetes API
```
- Verify kubeconfig path is correct
- Check network connectivity to API server
- Ensure credentials are valid

**Image pull failures**:
```
Pod status: ImagePullBackOff
```
- Check secret exists: `kubectl get secret rise-registry-creds -n rise-{project}`
- Verify registry credentials are valid
- Check secret refresh logs in controller output
- Ensure image reference is correct

**Pods not becoming ready**:
- Check pod logs: `kubectl logs -n rise-{project} {pod-name}`
- Check pod events: `kubectl describe pod -n rise-{project} {pod-name}`
- Verify application listens on configured HTTP port
- Check resource limits and node capacity
