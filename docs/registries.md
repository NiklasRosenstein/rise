# Container Registries

Rise generates temporary credentials for pushing container images to registries. The backend acts as a credential broker, abstracting provider-specific authentication.

## Supported Providers

### AWS ECR

Amazon Elastic Container Registry with scoped credentials via STS AssumeRole.

**Configuration:**
```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
repo_prefix = "rise/"
role_arn = "arn:aws:iam::123456789012:role/rise-backend"
push_role_arn = "arn:aws:iam::123456789012:role/rise-backend-ecr-push"
auto_remove = false  # Tag as orphaned instead of deleting
```

**How it works:**
1. Backend assumes `push_role_arn` with inline session policy scoped to specific project
2. Backend calls AWS `GetAuthorizationToken` API with scoped credentials
3. Returns credentials valid for 12 hours, scoped to single project repository
4. CLI uses credentials to push images

**Image path**: `{account}.dkr.ecr.{region}.amazonaws.com/{repo_prefix}{project}:{tag}`

Example: `123456789012.dkr.ecr.us-east-1.amazonaws.com/rise/my-app:latest`

### Docker Registry

Works with any Docker-compatible registry (Docker Hub, Harbor, Quay, local registries).

**Configuration:**
```toml
[registry]
type = "docker"
registry_url = "localhost:5000"
namespace = "rise-apps"
```

**How it works:**
1. Backend returns registry URL to CLI
2. CLI uses existing Docker credentials from `~/.docker/config.json`
3. No credential generation - relies on pre-authentication via `docker login`

**Common use cases:**
- **Local development**: docker-compose registry (port 5000)
- **Docker Hub**: `registry_url = "docker.io"`, `namespace = "myorg"`
- **Harbor**: `registry_url = "harbor.company.com"`, `namespace = "project"`

## Local Development Registry

For local development, Rise includes a Docker registry in `docker-compose`:

```yaml
registry:
  image: registry:2
  ports:
    - "5000:5000"
  volumes:
    - registry_data:/var/lib/registry
```

**Start:**
```bash
mise backend:deps  # Starts all services including registry
```

**Access:**
- Registry API: http://localhost:5000
- Registry UI: http://localhost:5001 (browse images)

**Usage:**
```bash
# List repositories
curl http://localhost:5000/v2/_catalog

# List tags
curl http://localhost:5000/v2/my-app/tags/list

# Deploy (automatically uses local registry)
rise deployment create my-app
```

**⚠️ Production Warning**: Local registry uses HTTP, has no auth, and uses Docker volumes. For production, use AWS ECR, GCR, or similar.

## AWS ECR Production Setup

### Architecture: Two-Role Pattern

**Controller Role (`rise-backend`)**:
- Create/delete ECR repositories
- Tag repositories (managed, orphaned)
- Configure repository settings
- Assume the push role

**Push Role (`rise-backend-ecr-push`)**:
- Push/pull images to ECR (under `rise/*` prefix)
- Used by backend to generate scoped credentials for CLI

**Why two roles?**
- Separation: Controller manages infrastructure, push handles images
- Least privilege: Scoped credentials limited to single repository
- Temporary: 12-hour max lifetime, can't delete repositories

### Terraform Module

Use the provided `modules/rise-aws` module:

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name        = "rise-backend"
  repo_prefix = "rise/"
  auto_remove = false

  tags = {
    Environment = "production"
    ManagedBy   = "terraform"
  }
}

output "rise_ecr_config" {
  value = module.rise_ecr.rise_config
}
```

Apply:
```bash
cd terraform
terraform init
terraform apply
terraform output rise_ecr_config
```

### With EKS + IRSA

Configure module for IRSA:

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name                   = "rise-backend"
  repo_prefix            = "rise/"
  irsa_oidc_provider_arn = module.eks.oidc_provider_arn
  irsa_namespace         = "rise-system"
  irsa_service_account   = "rise-backend"
}
```

Helm values:
```yaml
serviceAccount:
  create: true
  iamRoleArn: "arn:aws:iam::123456789012:role/rise-backend"

config:
  registry:
    type: "ecr"
    region: "us-east-1"
    account_id: "123456789012"
    repo_prefix: "rise/"
    role_arn: "arn:aws:iam::123456789012:role/rise-backend"
    push_role_arn: "arn:aws:iam::123456789012:role/rise-backend-ecr-push"
    # NO static credentials with IRSA
```

### With IAM User (Non-AWS)

For running Rise outside AWS:

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name            = "rise-backend"
  repo_prefix     = "rise/"
  create_iam_role = false
  create_iam_user = true
}

# Store credentials securely
resource "aws_secretsmanager_secret_version" "rise_ecr_creds" {
  secret_id = aws_secretsmanager_secret.rise_ecr_creds.id
  secret_string = jsonencode({
    access_key_id     = module.rise_ecr.access_key_id
    secret_access_key = module.rise_ecr.secret_access_key
  })
}
```

## Configuration

Registry configuration is in `rise-backend/config/`:

**Precedence** (highest to lowest):
1. Local config (`local.toml`)
2. Environment-specific config (`production.toml`)
3. Default config (`default.toml`)
4. Environment variable substitution (`${VAR}`)

**Environment variables:**
```bash
export RISE_REGISTRY__TYPE="ecr"
export RISE_REGISTRY__REGION="us-east-1"
export RISE_REGISTRY__ACCOUNT_ID="123456789012"
export RISE_REGISTRY__REPO_PREFIX="rise/"
export RISE_REGISTRY__ROLE_ARN="arn:aws:iam::123456789012:role/rise-backend"
export RISE_REGISTRY__PUSH_ROLE_ARN="arn:aws:iam::123456789012:role/rise-backend-ecr-push"
```

## API Endpoint

Request credentials for a project:

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

## Security

**ECR Credential Scope**:
- Scoped to specific project using STS AssumeRole with inline session policies
- Credentials for `my-app` can only push to `{repo_prefix}my-app*`
- 12-hour lifespan (AWS enforced)

**Docker**:
- No credential scoping
- Use registry-specific access controls

**Best Practices**:
1. Use IAM roles (ECR): Avoid static credentials
2. Enable HTTPS: Always use TLS in production
3. Monitor access: Track credential requests and usage
4. Rotate credentials: For Docker registries, rotate regularly
5. Least privilege: Scope credentials to minimum permissions

## Troubleshooting

**"Access Denied" when pushing (ECR)**:
1. Verify controller role can assume push role
2. Check push role permissions
3. Ensure repository exists with correct prefix
4. Verify STS session policy scope

**"Connection refused" to registry (Docker)**:
```bash
docker-compose ps registry
docker-compose logs registry
docker-compose restart registry
```

**Images not persisting (Docker)**:
```bash
docker volume ls | grep registry
docker-compose down -v  # Removes volumes!
```

## Extending Registry Support

To add a new registry provider:
1. Implement `RegistryProvider` trait in `rise-backend/src/registry/providers/`
2. Add provider to `RegistryConfig` enum
3. Register provider in `create_registry_provider()`

Potential future providers: JFrog Artifactory, GCR, ACR, GHCR, Quay.io
