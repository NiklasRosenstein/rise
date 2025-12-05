# Container Registry Integration

Rise generates temporary credentials for pushing container images to registries. The backend acts as a credential broker, abstracting provider-specific authentication.

## Supported Providers

### AWS ECR

Amazon Elastic Container Registry with automatic token generation.

**Configuration:**
```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
# Optional: static credentials (use IAM role in production)
access_key_id = "AKIA..."
secret_access_key = "..."
```

**How it works:**
1. Backend calls AWS `GetAuthorizationToken` API
2. Returns base64-encoded credentials valid for 12 hours
3. CLI uses credentials to `docker login` and push images

### Generic Docker Registry

Works with any Docker-compatible registry (Docker Hub, Harbor, Quay, local registries). Assumes user has authenticated via `docker login`.

**Configuration:**
```toml
[registry]
type = "docker"
registry_url = "localhost:5000"
namespace = "rise-apps"
```

**How it works:**
1. Backend returns registry URL to CLI
2. CLI uses existing Docker credentials (from `~/.docker/config.json`)
3. No credential generation - relies on pre-authentication

**Common use cases:**
- **Local development**: Use docker-compose registry (see below)
- **Docker Hub**: `registry_url = "docker.io"`, `namespace = "myorg"`
- **Harbor**: `registry_url = "harbor.company.com"`, `namespace = "project"`

## Configuration

Registry configuration is set in TOML files under `rise-backend/config/`:

- **`default.toml`**: Default settings (includes commented examples)
- **`production.toml`**: Production overrides (loaded when `RUN_MODE=production`)
- **`local.toml`**: Local overrides (optional, gitignored)

Configuration precedence (highest to lowest):
1. Environment variables (`RISE_REGISTRY__TYPE=docker`)
2. Local config file (`local.toml`)
3. Environment-specific config (`production.toml`, `development.toml`)
4. Default config (`default.toml`)

For docker-compose development, registry is configured in `production.toml`.

## Local Development with Docker Registry

The project includes a local Docker registry for development testing.

**1. Start services:**
```bash
docker-compose up -d
```

This starts:
- PostgreSQL (port 5432)
- Dex (port 5556)
- Rise backend (port 3000)
- Docker registry (port 5000)

**2. Backend is configured via `rise-backend/config/production.toml`:**
```toml
[registry]
type = "docker"
registry_url = "registry:5000"
namespace = "rise-apps"
```

**3. Test pushing to local registry:**
```bash
# Tag an image
docker tag myapp:latest localhost:5000/rise-apps/myapp:latest

# Push to local registry
docker push localhost:5000/rise-apps/myapp:latest

# Verify
curl http://localhost:5000/v2/_catalog
```

**4. CLI workflow:**
```bash
rise login
rise deployment create my-app  # Fetches registry URL from backend, pushes to localhost:5000
# Or using aliases:
rise d c my-app
```

The local registry persists data in a Docker volume (`registry_data`), so images survive restarts.

## API Endpoint

```bash
GET /registry/credentials?project=my-app
Authorization: Bearer <jwt-token>
```

Response:
```json
{
  "credentials": {
    "registry_url": "123456.dkr.ecr.us-east-1.amazonaws.com",
    "username": "AWS",
    "password": "eyJwYXlsb2FkIjoiS...",
    "expires_in": 43200
  },
  "repository": "my-app"
}
```

## Security Considerations

### ⚠️ Current Implementation Limitations

**1. Credential Scope**

The current implementation does **not** scope credentials to specific projects. When you request credentials for `project=my-app`, you receive credentials that can push to **any** repository in the configured registry.

**Risk:** A compromised token allows pushing to any repository.

**Mitigation (future):**
- ECR: Use resource tags and IAM policies to scope tokens per-project
- Docker: Use registry-specific access controls and namespacing

**2. Credential Lifespan**

- **ECR**: 12-hour tokens (AWS enforced)
- **Docker**: No credential generation - uses existing Docker auth

**Risk:** Stolen credentials remain valid for their full lifespan.

**Mitigation:**
- For ECR: Monitor for unusual push activity within 12-hour window
- For Docker: Rotate registry credentials regularly
- Monitor registry push logs for unauthorized activity

**3. Credential Storage in Transit**

Credentials are returned over HTTPS (in production). In development (localhost), they're transmitted over HTTP.

**Risk:** Local network interception in development.

**Mitigation:**
- Always use HTTPS in production
- Consider using TLS even in local development

**4. Backend Permissions**

The backend needs access to generate or provide credentials.

- **ECR**: Backend's IAM role needs `ecr:GetAuthorizationToken`
- **Docker**: Backend only provides registry URL (no credentials)

**Risk:** Backend compromise exposes registry access (ECR only).

**Mitigation:**
- For ECR: Use least-privilege IAM policies, rotate credentials regularly
- For Docker: Users must authenticate separately via `docker login`
- Audit backend access logs
- Consider credential vaulting for ECR credentials (HashiCorp Vault, AWS Secrets Manager)

### 🔒 Recommended Production Setup

**AWS ECR:**
```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
# No static credentials - use IAM role attached to ECS task/EC2 instance
```

Backend runs with IAM role having:
```json
{
  "Effect": "Allow",
  "Action": "ecr:GetAuthorizationToken",
  "Resource": "*"
}
```

**Docker Registry:**
```toml
[registry]
type = "docker"
registry_url = "registry.company.com"
namespace = "production"
```

Users authenticate via:
```bash
docker login registry.company.com
```

Backend provides registry URL; users bring their own credentials.

### 🚀 Future Improvements

**Planned:**
1. **Repository-scoped tokens** - Credentials limited to specific repositories
2. **Short-lived tokens** - 1-hour expiry with automatic refresh
3. **Audit logging** - Track who requested credentials and when
4. **Rate limiting** - Prevent credential enumeration attacks
5. **Project-based policies** - "Project X can only push to registry Y"

**Extending to other providers:**
- Google Container Registry (GCR)
- Azure Container Registry (ACR)
- GitHub Container Registry (GHCR)
- Harbor, Quay, Docker Hub

Implement `RegistryProvider` trait in `rise-backend/src/registry/providers/`.
