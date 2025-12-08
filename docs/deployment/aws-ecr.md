# AWS ECR Deployment

This guide explains how to deploy Rise with AWS Elastic Container Registry (ECR) for production use.

## Overview

AWS ECR provides a managed Docker registry with:
- **Automatic image scanning** for vulnerabilities
- **Encryption at rest** with AES-256 or KMS
- **IAM-based access control** for fine-grained permissions
- **Private repositories** within your AWS account
- **High availability** across multiple availability zones

Rise includes a Terraform module (`modules/rise-aws`) that provisions all required AWS resources for ECR integration.

## Architecture

The Rise ECR integration uses a two-role architecture for security and least privilege:

### Controller Role (`rise-backend`)

**Purpose**: Allows the Rise ECR controller to manage repository lifecycle.

**Permissions**:
- Create and delete ECR repositories
- Tag repositories (e.g., `managed`, `orphaned`)
- List and describe repositories
- Configure repository settings (scanning, lifecycle policies)
- **Assume the push role** to generate scoped credentials

**Used by**: The `backend-ecr` controller process

### Push Role (`rise-ecr-push`)

**Purpose**: Provides scoped credentials for pushing images to specific repositories.

**Permissions**:
- Push images to ECR repositories (under `rise/*` prefix)
- Pull images from ECR repositories
- Get authorization tokens for Docker login

**Used by**: The Rise backend (via STS AssumeRole) to generate temporary credentials for the CLI

### Why Two Roles?

**Separation of concerns**:
- **Controller role**: Manages infrastructure (repositories)
- **Push role**: Handles image operations (push/pull)

**Least privilege**:
- The backend generates **scoped** push credentials that are limited to a single repository
- Credentials are temporary (12 hours max)
- Even if credentials leak, they can't delete repositories or access other projects

**Example credential flow**:
1. User runs `rise deployment create my-app`
2. CLI requests credentials from backend
3. Backend assumes push role with inline session policy scoped to `rise/my-app`
4. Backend returns temporary credentials valid for 12 hours
5. CLI pushes image using scoped credentials

## Terraform Module Setup

### What the Module Creates

The `rise-aws` Terraform module provisions:

1. **ECR Controller IAM Role**:
   - Manages repository lifecycle
   - Can assume the push role
   - Scoped to repositories with prefix `rise/*`

2. **ECR Push IAM Role**:
   - Used for generating scoped push credentials
   - Trust policy allows controller role to assume it

3. **IAM Policies**:
   - Separate policies for controller and push permissions
   - Minimal required permissions following least privilege

4. **IRSA Configuration** (optional):
   - For future Kubernetes controller deployment on EKS
   - Trust policy for OIDC provider

5. **Lifecycle Policy** (default):
   - Keeps last 100 images per repository
   - Configurable via `max_image_count`

### Using the Module

#### Basic Usage

Create a `terraform/rise-ecr.tf` file:

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name_prefix = "rise"
  repo_prefix = "rise/"
  auto_remove = false  # Tag as orphaned instead of deleting

  tags = {
    Environment = "production"
    ManagedBy   = "terraform"
  }
}

# Output configuration for Rise backend
output "rise_ecr_config" {
  value = module.rise_ecr.rise_config
  description = "Configuration values to add to Rise backend config"
}
```

Apply the module:

```bash
cd terraform
terraform init
terraform plan
terraform apply
```

#### With IRSA for EKS

If you plan to run the Rise backend on EKS with IRSA:

**1. Configure Terraform module with IRSA settings:**

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name_prefix            = "rise"
  repo_prefix            = "rise/"
  irsa_oidc_provider_arn = module.eks.oidc_provider_arn
  irsa_namespace         = "rise-system"
  irsa_service_account   = "rise-backend"
}

# Output the role ARN for Helm values
output "rise_ecr_role_arn" {
  value = module.rise_ecr.role_arn
  description = "IAM role ARN for IRSA annotation"
}
```

This configures the trust policy to allow the Kubernetes service account to assume the role.

**2. Configure Helm chart with IRSA annotation:**

```yaml
# values-production.yaml
serviceAccount:
  create: true
  # Automatically adds eks.amazonaws.com/role-arn annotation
  iamRoleArn: "arn:aws:iam::123456789012:role/rise-backend"

# Backend config doesn't need static credentials with IRSA
config:
  registry:
    type: "ecr"
    region: "us-east-1"
    account_id: "123456789012"
    repo_prefix: "rise/"
    role_arn: "arn:aws:iam::123456789012:role/rise-backend"
    push_role_arn: "arn:aws:iam::123456789012:role/rise-ecr-push"
    # NO access_key_id or secret_access_key needed with IRSA
```

Deploy with Helm:

```bash
helm upgrade --install rise ./helm/rise \
  -f values-production.yaml \
  --namespace rise-system \
  --create-namespace
```

#### With IAM User (Non-AWS Deployment)

If running Rise outside AWS (e.g., on-premises, other cloud):

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name_prefix     = "rise"
  repo_prefix     = "rise/"
  create_iam_role = false
  create_iam_user = true
}

# Store credentials in AWS Secrets Manager
resource "aws_secretsmanager_secret" "rise_ecr_creds" {
  name = "rise/ecr-controller-credentials"
}

resource "aws_secretsmanager_secret_version" "rise_ecr_creds" {
  secret_id = aws_secretsmanager_secret.rise_ecr_creds.id
  secret_string = jsonencode({
    access_key_id     = module.rise_ecr.access_key_id
    secret_access_key = module.rise_ecr.secret_access_key
  })
}
```

**Important**: Store IAM user credentials securely. Never commit them to version control.

## Rise Backend Configuration

After applying the Terraform module, configure the Rise backend to use ECR.

### Get Configuration Values

The module outputs a `rise_config` object with all required values:

```bash
terraform output rise_ecr_config
```

Example output:
```hcl
{
  account_id    = "123456789012"
  auto_remove   = false
  region        = "us-east-1"
  repo_prefix   = "rise/"
}
```

### Configure Backend

Edit `rise-backend/config/production.toml`:

```toml
[registry]
type = "ecr"
region = "us-east-1"                    # From Terraform output
account_id = "123456789012"              # From Terraform output
repo_prefix = "rise/"                    # From Terraform output
role_arn = "arn:aws:iam::123456789012:role/rise-backend"      # From module.rise_ecr.role_arn
push_role_arn = "arn:aws:iam::123456789012:role/rise-ecr-push"       # From module.rise_ecr.push_role_arn
auto_remove = false                      # From Terraform output

# Optional: If using IAM user instead of role
# access_key_id = "AKIA..."
# secret_access_key = "..."
```

### Environment Variables (Alternative)

You can also configure via environment variables:

```bash
export RISE_REGISTRY__TYPE="ecr"
export RISE_REGISTRY__REGION="us-east-1"
export RISE_REGISTRY__ACCOUNT_ID="123456789012"
export RISE_REGISTRY__REPO_PREFIX="rise/"
export RISE_REGISTRY__ROLE_ARN="arn:aws:iam::123456789012:role/rise-backend"
export RISE_REGISTRY__PUSH_ROLE_ARN="arn:aws:iam::123456789012:role/rise-ecr-push"
```

## Credential Flow

Here's how credentials work in the ECR integration:

1. **Rise CLI** requests credentials for pushing to `my-app`
2. **Rise Backend** (with controller role) assumes push role with inline session policy:
   ```json
   {
     "Version": "2012-10-17",
     "Statement": [{
       "Effect": "Allow",
       "Action": ["ecr:PutImage", "ecr:InitiateLayerUpload", ...],
       "Resource": "arn:aws:ecr:us-east-1:123456789012:repository/rise/my-app"
     }]
   }
   ```
3. **AWS STS** returns temporary credentials (12-hour max) scoped to `rise/my-app`
4. **Rise Backend** returns credentials to CLI
5. **Rise CLI** uses credentials to push image to `rise/my-app` only

**Key benefits**:
- Credentials are **temporary** (12 hours max)
- Credentials are **scoped** to a single repository
- No long-lived credentials on developer machines
- Credentials can't be used to access other projects

## Troubleshooting

### "Access Denied" when pushing images

**Cause**: Controller role can't assume push role, or push role permissions are insufficient.

**Solution**:
1. Verify trust policy on push role allows controller role:
   ```bash
   aws iam get-role --role-name rise-ecr-push --query 'Role.AssumeRolePolicyDocument'
   ```

2. Verify controller role has `sts:AssumeRole` on push role:
   ```bash
   aws iam simulate-principal-policy \
     --policy-source-arn arn:aws:iam::123456789012:role/rise-backend \
     --action-names sts:AssumeRole \
     --resource-arns arn:aws:iam::123456789012:role/rise-ecr-push
   ```

### "Repository does not exist"

**Cause**: ECR controller hasn't created the repository yet.

**Solution**:
1. Check ECR controller logs:
   ```bash
   overmind connect backend-ecr
   ```

2. Verify controller has permissions to create repositories:
   ```bash
   aws ecr describe-repositories --repository-names rise/my-app
   ```

3. Manually trigger repository creation (development):
   ```bash
   aws ecr create-repository --repository-name rise/my-app
   ```

### Images not being cleaned up

**Cause**: Lifecycle policy not applied or auto_remove disabled.

**Solution**:
1. Check lifecycle policy:
   ```bash
   aws ecr get-lifecycle-policy --repository-name rise/my-app
   ```

2. Verify `auto_remove` setting in backend config

## Module Configuration Options

Key Terraform module inputs:

| Input | Description | Default |
|-------|-------------|---------|
| `name_prefix` | Prefix for IAM roles/policies | `"rise"` |
| `repo_prefix` | Prefix for ECR repositories | `"rise/"` |
| `auto_remove` | Delete repos on project deletion | `false` |
| `max_image_count` | Max images per repository | `100` |
| `scan_on_push` | Enable vulnerability scanning | `true` |
| `encryption_type` | AES256 or KMS | `"AES256"` |
| `irsa_oidc_provider_arn` | EKS OIDC provider for IRSA | `null` |

See [modules/rise-aws/README.md](../../modules/rise-aws/README.md) for full documentation.

## Security Best Practices

1. **Use IRSA for EKS**: Avoid long-lived credentials when running on EKS
2. **Enable encryption**: Use KMS encryption for sensitive images
3. **Enable scanning**: Scan images for vulnerabilities on push
4. **Rotate IAM user keys**: If using IAM user, rotate keys regularly
5. **Monitor access**: Use CloudTrail to audit ECR API calls
6. **Limit repository prefix**: Use unique prefix per environment (e.g., `rise-prod/`, `rise-staging/`)

## Cost Considerations

AWS ECR pricing:
- **Storage**: $0.10 per GB-month
- **Data transfer**: Standard AWS rates
- **Image scanning**: $0.09 per image scan (optional)

**Recommendations**:
- Use lifecycle policies to limit image retention
- Clean up unused repositories
- Use compression for smaller image sizes

## Next Steps

- **Configure registry**: See [Container Registry](../features/registry.md)
- **Deploy production**: See [Production Setup](./production.md)
- **Service accounts**: See [Service Accounts](../features/service-accounts.md)
