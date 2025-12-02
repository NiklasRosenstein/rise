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

### JFrog Artifactory

Enterprise registry with static or helper-based auth.

**Static credentials:**
```toml
[registry]
type = "artifactory"
base_url = "https://company.jfrog.io"
repository = "docker-local"
username = "user"
password = "pass"
```

**Docker credential helper:**
```toml
[registry]
type = "artifactory"
base_url = "https://company.jfrog.io"
repository = "docker-local"
use_credential_helper = true
```

Requires `docker login` to have been run previously.

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
- Artifactory: Use project-specific access tokens

**2. Credential Lifespan**

- **ECR**: 12-hour tokens (AWS enforced)
- **Artifactory**: Credentials don't expire (unless using temporary tokens)

**Risk:** Stolen credentials remain valid for their full lifespan.

**Mitigation:**
- Rotate Artifactory credentials regularly
- Monitor registry push logs for unauthorized activity
- Consider implementing token refresh before expiry

**3. Credential Storage in Transit**

Credentials are returned over HTTPS (in production). In development (localhost), they're transmitted over HTTP.

**Risk:** Local network interception in development.

**Mitigation:**
- Always use HTTPS in production
- Consider using TLS even in local development

**4. Backend Permissions**

The backend needs full registry access to generate credentials.

- **ECR**: Backend's IAM role needs `ecr:GetAuthorizationToken`
- **Artifactory**: Backend needs admin credentials or token generation permissions

**Risk:** Backend compromise exposes full registry access.

**Mitigation:**
- Rotate backend credentials regularly
- Use least-privilege IAM policies
- Audit backend access logs
- Consider credential vaulting (HashiCorp Vault, AWS Secrets Manager)

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

**Artifactory:**
```toml
[registry]
type = "artifactory"
base_url = "https://company.jfrog.io"
repository = "docker-local"
# Credentials via environment variables, not config file
```

Set via:
```bash
export RISE_REGISTRY__USERNAME="service-account"
export RISE_REGISTRY__PASSWORD="from-vault"
```

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
