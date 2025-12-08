# Container Registry Integration

Rise generates temporary credentials for pushing container images to registries. The backend acts as a credential broker, abstracting provider-specific authentication.

## Supported Providers

### AWS ECR

Amazon Elastic Container Registry with scoped credentials via STS AssumeRole.

**Configuration** (`rise-backend/config/production.toml`):
```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
repo_prefix = "rise/"
role_arn = "arn:aws:iam::123456789012:role/rise-backend"
push_role_arn = "arn:aws:iam::123456789012:role/rise-ecr-push"
```

**How it works**:
1. Backend assumes `push_role_arn` with inline session policy scoped to specific project
2. Backend calls AWS `GetAuthorizationToken` API with scoped credentials
3. Returns credentials valid for 12 hours, scoped to single project repository
4. CLI uses credentials to push images

**Image path**: `{account}.dkr.ecr.{region}.amazonaws.com/{repo_prefix}{project}:{tag}`

Example: `123456789012.dkr.ecr.us-east-1.amazonaws.com/rise/my-app:latest`

**Setup**: See [AWS ECR Deployment](../deployment/aws-ecr.md) for complete setup with Terraform module.

### Docker Registry

Works with any Docker-compatible registry (Docker Hub, Harbor, Quay, local registries).

**Configuration**:
```toml
[registry]
type = "docker"
registry_url = "localhost:5000"
namespace = "rise-apps"
```

**How it works**:
1. Backend returns registry URL to CLI
2. CLI uses existing Docker credentials from `~/.docker/config.json`
3. No credential generation - relies on pre-authentication via `docker login`

**Common use cases**:
- **Local development**: docker-compose registry (see [Docker Local](../deployment/docker-local.md))
- **Docker Hub**: `registry_url = "docker.io"`, `namespace = "myorg"`
- **Harbor**: `registry_url = "harbor.company.com"`, `namespace = "project"`

## Configuration

Registry configuration is in `rise-backend/config/`:

- **`default.toml`**: Default settings
- **`production.toml`**: Production overrides (loaded when `RUN_MODE=production`)
- **`local.toml`**: Local overrides (gitignored)

**Precedence** (highest to lowest):
1. Environment variables (`RISE_REGISTRY__TYPE=docker`)
2. Local config (`local.toml`)
3. Environment-specific config (`production.toml`)
4. Default config (`default.toml`)

**Environment variables**:
```bash
export RISE_REGISTRY__TYPE="ecr"
export RISE_REGISTRY__REGION="us-east-1"
export RISE_REGISTRY__ACCOUNT_ID="123456789012"
export RISE_REGISTRY__PUSH_ROLE_ARN="arn:aws:iam::123456789012:role/rise-ecr-push"
```

## API Endpoint

Request credentials for a project:

```bash
GET /registry/credentials?project=my-app
Authorization: Bearer <jwt-token>
```

**Response**:
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

## Security

### Credential Scope

**ECR**: Credentials scoped to specific project using STS AssumeRole with inline session policies. Credentials for `my-app` can **only** push to `{repo_prefix}my-app*`.

**Docker**: No credential scoping. Use registry-specific access controls.

### Credential Lifespan

- **ECR**: 12 hours (AWS enforced)
- **Docker**: No credential generation (uses existing Docker auth)

**Mitigation**:
- Monitor for unusual push activity
- Use HTTPS in production
- Audit backend access logs

### Backend Permissions

**ECR**: Backend IAM role needs `sts:AssumeRole` on `push_role_arn`

**Docker**: Backend only provides registry URL

### Production Best Practices

1. **Use IAM roles** (ECR): Avoid static credentials
2. **Enable HTTPS**: Always use TLS in production
3. **Monitor access**: Track credential requests and usage
4. **Rotate credentials**: For Docker registries requiring auth, rotate regularly
5. **Least privilege**: Scope credentials to minimum required permissions

## Extending Registry Support

To add a new registry provider:

1. Implement `RegistryProvider` trait in `rise-backend/src/registry/providers/`
2. Add provider to `RegistryConfig` enum
3. Register provider in `create_registry_provider()`

Potential future providers:
- JFrog Artifactory
- Google Container Registry (GCR)
- Azure Container Registry (ACR)
- GitHub Container Registry (GHCR)
- Quay.io

## Next Steps

- **Setup AWS ECR**: See [AWS ECR Deployment](../deployment/aws-ecr.md)
- **Local development**: See [Docker (Local)](../deployment/docker-local.md)
- **Deploy an app**: See [CLI Basics](../getting-started/cli-basics.md)
