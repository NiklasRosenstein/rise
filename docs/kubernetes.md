# Kubernetes Deployment Backend

The Kubernetes deployment backend deploys applications to Kubernetes clusters using Deployments, Services, and Ingresses.

## Overview

The Kubernetes controller manages application deployments on Kubernetes by:
- Creating namespace-scoped resources for each project
- Deploying applications as Deployments with rolling updates
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

# Ingress URL template for production (default) deployment group
# Supports both subdomain and sub-path routing (must contain {project_name})
production_ingress_url_template = "{project_name}.apps.rise.local"

# Optional: Ingress URL template for staging (non-default) deployment groups
# Must contain both {project_name} and {deployment_group} placeholders
staging_ingress_url_template = "{project_name}-{deployment_group}.preview.rise.local"

# Or for sub-path routing:
# production_ingress_url_template = "rise.local/{project_name}"
# staging_ingress_url_template = "rise.local/{project_name}/{deployment_group}"

# Namespace format (must contain {project_name})
namespace_format = "rise-{project_name}"

# Custom domain TLS mode
# - "per-domain": Each custom domain gets its own tls-{domain} secret (for cert-manager)
# - "shared": All custom domains share ingress_tls_secret_name
custom_domain_tls_mode = "per-domain"  # Default

# Annotations for custom domain ingresses (e.g., cert-manager integration)
[kubernetes.custom_domain_ingress_annotations]
"cert-manager.io/cluster-issuer" = "letsencrypt-prod"
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
| Deployment | One per deployment | Runs application pods |
| Service | One per deployment group | Routes traffic to active deployment |
| Ingress | One per deployment group | Exposes HTTP/HTTPS endpoints |
| Secret | One per project | Stores image pull credentials |

### Naming Scheme

Resources follow consistent naming patterns:

| Resource | Pattern | Example |
|----------|---------|---------|
| Namespace | `rise-{project}` | `rise-my-app` |
| Deployment | `{project}-{deployment_id}` | `my-app-20251207-143022` |
| Service | `{escaped_group}` | `default`, `mr--26` |
| Ingress | `{escaped_group}` | `default`, `mr--26` |
| Secret | `rise-registry-creds` | `rise-registry-creds` |

**Character escaping**: Deployment group names containing invalid Kubernetes characters (e.g., `/`, `@`) are escaped with `--`. For example, `mr/26` becomes `mr--26`.

### Deployment Groups and URLs

Each deployment group gets its own Service and Ingress with a unique URL:

| Group | URL Pattern | Example (Subdomain) | Example (Sub-path) |
|-------|-------------|---------------------|-------------------|
| `default` | `production_ingress_url_template` | `my-app.apps.rise.local` | `rise.local/my-app` |
| Custom groups | `staging_ingress_url_template` | `my-app-mr--26.preview.rise.local` | `rise.local/my-app/mr--26` |

### Sub-path vs Subdomain Routing

Rise supports two Ingress routing modes configured globally via URL templates:

**Subdomain Routing** (traditional approach):
- Production: `{project_name}.apps.rise.local`
- Staging: `{project_name}-{deployment_group}.preview.rise.local`
- Each project gets a unique subdomain
- Ingress path: `/` (Prefix type)
- No path rewriting needed

**Sub-path Routing** (shared domain):
- Production: `rise.local/{project_name}`
- Staging: `rise.local/{project_name}/{deployment_group}`
- All projects share the same domain with different paths
- Ingress path: `/{project}(/|$)(.*)` (ImplementationSpecific type with regex)
- Nginx automatically rewrites paths

#### Path Rewriting

For sub-path routing, Nginx automatically rewrites paths so your application receives requests at `/` while preserving the original path prefix:

- **Client request**: `GET https://rise.local/myapp/api/users`
- **Application receives**: `GET /api/users`
- **Headers added**: `X-Forwarded-Prefix: /myapp`

The controller uses the built-in `nginx.ingress.kubernetes.io/x-forwarded-prefix` annotation to add this header. Configure your application to use the `X-Forwarded-Prefix` header when generating URLs to ensure links and assets work correctly.

**Example configuration**:
```toml
[kubernetes]
production_ingress_url_template = "rise.local/{project_name}"
staging_ingress_url_template = "rise.local/{project_name}/{deployment_group}"
auth_backend_url = "http://rise-backend.default.svc.cluster.local:3000"
auth_signin_url = "https://rise.local"
```

### Private Project Authentication

The Kubernetes controller implements ingress-level authentication for private projects using Nginx auth annotations and Rise-issued JWTs.

#### Overview

- **Public projects**: Accessible without authentication
- **Private projects**: Require user authentication AND project access authorization
- **Authentication method**: OAuth2 via configured identity provider (Dex)
- **Token security**: Rise-issued JWTs scoped to specific projects
- **Cookie isolation**: Separate cookies prevent projects from accessing Rise APIs

#### Configuration

Private project authentication requires JWT signing configuration:

```toml
[server]
# JWT signing secret for ingress authentication (base64-encoded, min 32 bytes)
# Generate with: openssl rand -base64 32
# REQUIRED: The backend will fail to start without this
jwt_signing_secret = "YOUR_BASE64_SECRET_HERE"

# Optional: JWT claims to include from IdP token (default shown)
jwt_claims = ["sub", "email", "name"]

# Cookie settings for subdomain sharing
cookie_domain = ".rise.local"  # Allows cookies to work across *.rise.local
cookie_secure = false          # Set to false for local development (HTTP)
```

```toml
[kubernetes]
# Internal cluster URL for Nginx auth subrequests
auth_backend_url = "http://rise-backend.default.svc.cluster.local:3000"

# Public backend URL for browser redirects during authentication
auth_signin_url = "http://rise.local"  # Use http:// for local development
```

**Generate JWT signing secret**:
```bash
openssl rand -base64 32
```

#### Authentication Flow

When a user visits a private project, the following flow occurs:

```
User → myapp.apps.rise.local (private)
  ↓
Nginx calls GET /api/v1/auth/ingress?project=myapp
  - 🍪 NO COOKIE or invalid JWT
  ↓ Returns 401 Unauthorized
  ↓
Nginx redirects to /api/v1/auth/signin?project=myapp&redirect=http://myapp.apps.rise.local
  ↓
GET /api/v1/auth/signin (Pre-Auth Page):
  - Renders auth-signin.html.tera
  - Shows: "Project 'myapp' is private. Sign in to access."
  - Button: "Sign In" → /api/v1/auth/signin/start?project=myapp&redirect=...
  ↓
User clicks "Sign In" button
  ↓
GET /api/v1/auth/signin/start (OAuth Start):
  - Stores project_name='myapp' in OAuth2State (PKCE state)
  - Redirects to Dex IdP authorize endpoint
  ↓
User completes OAuth at Dex
  ↓
Dex redirects to /api/v1/auth/callback?code=xyz&state=abc
  ↓
GET /api/v1/auth/callback (Token Exchange):
  - Retrieve OAuth2State (includes project_name='myapp' for UI context only)
  - Exchange code for IdP tokens
  - Validate IdP JWT
  - Extract claims (sub, email, name) and expiry
  - Issue Rise JWT with user claims (NOT project-scoped!)
  - 🍪 SET COOKIE: _rise_ingress = <Rise JWT>
       (Domain: .rise.local, HttpOnly, Secure=false, SameSite=Lax)
  - Renders auth-success.html.tera
  - Shows: "Authentication successful! Redirecting in 3s..."
  - JavaScript auto-redirects to http://myapp.apps.rise.local
  ↓
After 3 seconds, browser redirects to http://myapp.apps.rise.local
  ↓
Nginx calls GET /api/v1/auth/ingress?project=myapp
  - 🍪 READS COOKIE: _rise_ingress
  - Verifies Rise JWT signature (HS256)
  - Validates expiry
  - Checks user has project access via database query (NOT JWT claim!)
  ↓ Returns 200 OK + headers (X-Auth-Request-Email, X-Auth-Request-User)
  ↓
Nginx serves app
  - 🍪 Rise JWT cookie is sent to app (but app cannot decode it - HttpOnly)
  - App does NOT have access to Rise APIs (different cookie name)
```

#### JWT Structure

Rise issues symmetric HS256 JWTs with the following claims:

```json
{
  "sub": "user-id-from-idp",
  "email": "user@example.com",
  "name": "User Name",
  "iat": 1234567890,
  "exp": 1234571490,
  "iss": "http://rise.local",
  "aud": "rise-ingress"
}
```

**Key features**:
- **NOT project-scoped**: JWTs do NOT contain a project claim because the cookie is set at `rise.local` domain and shared across all `*.apps.rise.local` subdomains. Project access is validated separately in the ingress auth handler by checking database permissions.
- **Configurable claims**: Include only necessary user information
- **Expiry matching**: Token expiration matches IdP token (typically 1 hour)
- **Symmetric signing**: HS256 with shared secret for fast validation

#### Cookie Security

Two separate cookies are used for different purposes:

| Cookie | Purpose | Contents | Access |
|--------|---------|----------|--------|
| `_rise_session` | Rise API authentication | IdP JWT | Frontend JavaScript |
| `_rise_ingress` | Project access authentication | Rise JWT | HttpOnly (no JS access) |

**Security attributes**:
- `HttpOnly`: Prevents JavaScript access (XSS protection)
- `Secure`: HTTPS-only transmission
- `SameSite=Lax`: CSRF protection while allowing navigation
- `Domain`: Shared across subdomains (e.g., `.rise.local`)
- `Max-Age`: Matches JWT expiration

#### Access Control

For private projects, the ingress auth endpoint validates:

1. **JWT validity**: Signature, expiration, issuer, audience
2. **User permissions**: Database query to check if user is owner or team member

Access check logic:
```rust
// User can access if:
// - User is the project owner (owner_user_id), OR
// - User is a member of the team that owns the project (owner_team_id)
//
// NOTE: JWTs are NOT project-scoped - the same JWT can be used across all projects
// because the cookie is set at rise.local domain level and shared across *.apps.rise.local
```

#### Ingress Annotations

For private projects, the controller adds these Nginx annotations:

```yaml
annotations:
  nginx.ingress.kubernetes.io/auth-url: "http://rise-backend.default.svc.cluster.local:3000/api/v1/auth/ingress?project=myapp"
  nginx.ingress.kubernetes.io/auth-signin: "http://rise.local/api/v1/auth/signin?project=myapp&redirect=$escaped_request_uri"
  nginx.ingress.kubernetes.io/auth-response-headers: "X-Auth-Request-Email,X-Auth-Request-User"
```

**How it works**:
- `auth-url`: Nginx calls this endpoint for every request to validate authentication
  - Returns 2xx (200): Access granted
  - Returns 401/403: Access denied, redirect to auth-signin
  - Returns 5xx or unreachable: **Access denied (fail-closed)** - ensures security even if auth service is misconfigured or down
- `auth-signin`: Where to redirect unauthenticated users
- `auth-response-headers`: Headers to pass from auth response to the application

The application receives authenticated requests with these additional headers:
- `X-Auth-Request-Email`: User's email address
- `X-Auth-Request-User`: User's ID

#### Troubleshooting Authentication

**Infinite redirect loop**:
- Check `cookie_domain` matches your domain structure
- Verify cookies are being set (check browser DevTools → Application → Cookies)
- Ensure `cookie_secure` is `false` for HTTP development environments

**Browser always redirects HTTP to HTTPS**:
- Some TLDs (e.g., `.dev`) are on the HSTS preload list and browsers will always force HTTPS
- Use `.local` TLD for local development to avoid HSTS issues
- The default configuration uses `rise.local` which works correctly with HTTP
- If you must use a different TLD, check if it's on the HSTS preload list at https://hstspreload.org/

**"Access denied" or 403 Forbidden error**:
- User is authenticated but not authorized for this project
- Check project ownership: `rise project show <project-name>`
- Add user to project's team if needed

**"No session cookie" error**:
- Cookie expired or not set
- Cookie domain mismatch
- Browser blocking third-party cookies
- Check `cookie_domain` configuration

**Private projects accessible without authentication**:
- Check ingress controller logs for auth subrequest errors: `kubectl logs -n ingress-nginx <ingress-controller-pod>`
- Verify `auth_backend_url` in config includes the correct service URL and port
- Ensure the auth service is reachable from the ingress controller (test with `curl` from ingress pod)
- Check that ingress annotations are correctly set: `kubectl get ingress -n rise-<project> -o yaml`
- All auth endpoints are under `/api/v1` prefix (e.g., `/api/v1/auth/ingress`)

**Authentication succeeds but access denied**:
- User is authenticated but not authorized for this project
- Check project ownership: `rise project show <project-name>`
- Add user to project's team if needed

**JWT signing errors in logs**:
```
Error: Failed to initialize JWT signer: Invalid base64
```
- JWT signing secret is not valid base64
- Regenerate with: `openssl rand -base64 32`
- Ensure secret is at least 32 bytes when decoded

### Blue/Green Deployments

The controller implements blue/green deployments using Service selector updates:

1. **Deploy new Deployment**: Create new Deployment with deployment-specific labels
2. **Wait for health**: Wait until new Deployment pods are ready and pass health checks
3. **Switch traffic**: Update Service selector to point to new deployment labels
4. **Previous deployment**: Old Deployment remains but receives no traffic

This ensures zero-downtime deployments with instant rollback capability.

### Labels

All resources are labeled for management and selection:

```yaml
labels:
  rise.dev/managed-by: "rise"
  rise.dev/project: "my-app"
  rise.dev/deployment-group: "default"
  rise.dev/deployment-id: "20251207-143022"
  rise.dev/deployment-uuid: "550e8400-e29b-41d4-a716-446655440000"
```

### Custom Domains and TLS

Rise supports custom domains for projects, allowing you to serve your application at your own domain names (e.g., `app.example.com`) instead of or in addition to the default project URL pattern.

#### Overview

When custom domains are configured for a project:
- Rise creates a separate Ingress resource specifically for custom domains
- Custom domains always route to the root path (`/`) regardless of the default ingress URL pattern
- TLS certificates can be automatically provisioned using cert-manager integration

#### TLS Certificate Management

Rise provides two modes for TLS certificate management on custom domains:

**Per-Domain Mode (Recommended for cert-manager)**

When `custom_domain_tls_mode` is set to `per-domain` (the default), each custom domain gets its own TLS secret named `tls-{domain}`. This mode is designed to work with cert-manager for automatic certificate provisioning:

```yaml
deployment_controller:
  type: kubernetes
  # ... other settings ...
  
  # TLS mode - per-domain creates separate secrets for each custom domain
  custom_domain_tls_mode: "per-domain"  # Default
  
  # Annotations to apply to custom domain ingresses (for cert-manager)
  custom_domain_ingress_annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    # Or use a specific issuer per namespace:
    # cert-manager.io/issuer: "letsencrypt-prod"
```

With this configuration:
- Each custom domain (e.g., `app.example.com`) will have its own TLS secret (`tls-app.example.com`)
- cert-manager will automatically provision Let's Encrypt certificates
- Certificates are automatically renewed by cert-manager
- No manual TLS secret management required

**Shared Mode**

When `custom_domain_tls_mode` is set to `shared`, all custom domains share the same TLS secret specified by `ingress_tls_secret_name`:

```yaml
deployment_controller:
  type: kubernetes
  # ... other settings ...
  
  # Shared TLS secret for all hosts (primary + custom domains)
  ingress_tls_secret_name: "my-wildcard-cert"
  
  # Use shared mode
  custom_domain_tls_mode: "shared"
```

This mode is useful when you have a wildcard certificate or want to manage certificates externally.

#### Cert-Manager Setup

To use cert-manager with Rise custom domains:

1. **Install cert-manager in your cluster:**

```bash
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml
```

2. **Create a ClusterIssuer for Let's Encrypt:**

```yaml
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    # Let's Encrypt production server
    server: https://acme-v02.api.letsencrypt.org/directory
    email: your-email@example.com
    privateKeySecretRef:
      name: letsencrypt-prod-key
    solvers:
      - http01:
          ingress:
            class: nginx
```

3. **Configure Rise to use cert-manager:**

```yaml
deployment_controller:
  type: kubernetes
  # ... other settings ...
  
  custom_domain_tls_mode: "per-domain"
  custom_domain_ingress_annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
```

4. **Add a custom domain to your project:**

```bash
rise domain add my-project custom-domain.example.com
```

cert-manager will automatically:
- Create an ACME challenge
- Validate domain ownership
- Issue a Let's Encrypt certificate
- Store it in the `tls-custom-domain.example.com` secret
- Automatically renew certificates before expiration

#### DNS Configuration

For custom domains to work, you must configure DNS records to point to your Kubernetes ingress:

```
custom-domain.example.com.  A  <ingress-ip-address>
```

Or for CNAMEs:

```
custom-domain.example.com.  CNAME  <ingress-hostname>
```

#### Troubleshooting Custom Domain TLS

**Certificate not being issued:**
- Check cert-manager logs: `kubectl logs -n cert-manager deployment/cert-manager`
- Check certificate status: `kubectl get certificate -n rise-<project>`
- Verify DNS is correctly configured and resolves to your ingress
- Check ClusterIssuer/Issuer status: `kubectl describe clusterissuer letsencrypt-prod`

**"Certificate not ready" error:**
- cert-manager is still working on the challenge - wait a few minutes
- Check challenge status: `kubectl get challenges -n rise-<project>`
- Verify ingress controller can handle ACME challenges

**Multiple certificate requests:**
- Check that `custom_domain_ingress_annotations` are correctly configured
- Verify you're not mixing cert-manager annotations in `ingress_annotations` and `custom_domain_ingress_annotations`

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

#### Configuring Image Pull Secrets

The Kubernetes controller supports three modes for managing image pull secrets:

**1. Automatic Management (with registry provider)**
- When a registry provider is configured (e.g., AWS ECR), the controller automatically creates and refreshes the `rise-registry-creds` secret in each project namespace
- Credentials are fetched from the registry provider on-demand
- Secrets are automatically refreshed every hour
- No additional configuration needed

**2. External Secret Reference**
- For static Docker registries where credentials are managed externally (e.g., manually created secrets, sealed-secrets, external-secrets operator)
- Configure the secret name in the deployment controller settings:

```yaml
deployment_controller:
  type: kubernetes
  # ... other settings ...
  image_pull_secret_name: "my-registry-secret"
```

- The controller will reference this secret name in all Deployments
- The secret must exist in each project namespace before deployments can succeed
- The controller will NOT create or manage this secret
- Useful when:
  - Using a static registry that doesn't support dynamic credential generation
  - Managing secrets through GitOps tools like sealed-secrets or external-secrets operator
  - Using a cluster-wide image pull secret that's pre-configured in all namespaces

**3. No Image Pull Secret**
- When no registry provider is configured and no `image_pull_secret_name` is set
- Deployments will not include any `imagePullSecrets` field
- Only works with public container images or when using Kubernetes cluster defaults

**Example configurations:**

Using AWS ECR (automatic):
```yaml
registry:
  type: ecr
  region: us-east-1
  account_id: "123456789012"
  # ... other ECR settings ...

deployment_controller:
  type: kubernetes
  # No image_pull_secret_name needed - automatically managed
```

Using external secret:
```yaml
registry:
  type: oci-client-auth
  registry_url: "registry.example.com"
  # ... other registry settings ...

deployment_controller:
  type: kubernetes
  # ... other settings ...
  image_pull_secret_name: "my-registry-secret"
```

For external secrets, ensure the secret exists in each namespace:
```bash
# Create secret in namespace
kubectl create secret docker-registry my-registry-secret \
  --docker-server=registry.example.com \
  --docker-username=myuser \
  --docker-password=mypassword \
  -n rise-my-app
```

### Deployment

One per deployment:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app-20251207-143022
  namespace: rise-my-app
  labels:
    rise.dev/managed-by: "rise"
    rise.dev/project: "my-app"
    rise.dev/deployment-group: "default"
    rise.dev/deployment-id: "20251207-143022"
    rise.dev/deployment-uuid: "550e8400-e29b-41d4-a716-446655440000"
spec:
  replicas: 1
  selector:
    matchLabels:
      rise.dev/project: "my-app"
      rise.dev/deployment-group: "default"
      rise.dev/deployment-id: "20251207-143022"
      rise.dev/deployment-uuid: "550e8400-e29b-41d4-a716-446655440000"
  template:
    metadata:
      labels:
        rise.dev/project: "my-app"
        rise.dev/deployment-group: "default"
        rise.dev/deployment-id: "20251207-143022"
        rise.dev/deployment-uuid: "550e8400-e29b-41d4-a716-446655440000"
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
    rise.dev/deployment-uuid: "550e8400-e29b-41d4-a716-446655440000"  # Updated on traffic switch
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
    - host: my-app.apps.rise.local
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
    resources: ["deployments", "replicasets"]
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
