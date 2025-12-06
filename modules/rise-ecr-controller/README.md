# Rise ECR Controller Terraform Module

This module creates the AWS IAM resources required for the Rise ECR controller to manage ECR repositories.

## Features

- Creates an IAM policy with least-privilege permissions for ECR operations
- Supports IAM role (for AWS deployments with IRSA or instance profiles)
- Supports IAM user with access keys (for non-AWS deployments)
- Configurable repository prefix for multi-tenant isolation
- Optional auto-remove or soft-delete (orphan tagging) modes
- Default lifecycle policy to limit image retention

## Usage

### Basic Usage (IAM Role)

```hcl
module "rise_ecr" {
  source = "./modules/rise-ecr-controller"

  name_prefix = "rise-prod"
  repo_prefix = "rise/"
  auto_remove = false  # Tag as orphaned instead of deleting

  tags = {
    Environment = "production"
  }
}
```

### With IRSA (EKS)

```hcl
module "rise_ecr" {
  source = "./modules/rise-ecr-controller"

  name_prefix            = "rise-prod"
  repo_prefix            = "rise/"
  irsa_oidc_provider_arn = module.eks.oidc_provider_arn
  irsa_namespace         = "rise-system"
  irsa_service_account   = "rise-ecr-controller"
}
```

### With IAM User (Non-AWS Deployment)

```hcl
module "rise_ecr" {
  source = "./modules/rise-ecr-controller"

  name_prefix     = "rise-prod"
  repo_prefix     = "rise/"
  create_iam_role = false
  create_iam_user = true
}

# Store credentials securely
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

## Rise Backend Configuration

After applying this module, configure the Rise backend with the ECR settings:

```toml
# config/local.toml
[registry]
type = "ecr"
region = "eu-west-1"  # From module.rise_ecr.rise_config.region
account_id = "123456789012"  # From module.rise_ecr.rise_config.account_id
repo_prefix = "rise/"  # From module.rise_ecr.rise_config.repo_prefix
role_arn = "arn:aws:iam::123456789012:role/rise-prod-ecr-controller"  # From module.rise_ecr.role_arn
auto_remove = false  # From module.rise_ecr.rise_config.auto_remove

# If using IAM user instead of role:
# access_key_id = "AKIA..."
# secret_access_key = "..."
```

## Inputs

| Name | Description | Type | Default | Required |
|------|-------------|------|---------|:--------:|
| name_prefix | Prefix for all resource names | `string` | `"rise"` | no |
| repo_prefix | Prefix for ECR repository names | `string` | `"rise/"` | no |
| tags | Tags to apply to all resources | `map(string)` | `{}` | no |
| create_iam_role | Create an IAM role for the controller | `bool` | `true` | no |
| create_iam_user | Create an IAM user with access keys | `bool` | `false` | no |
| role_assume_policy | Custom assume role policy JSON | `string` | `null` | no |
| irsa_oidc_provider_arn | OIDC provider ARN for IRSA | `string` | `null` | no |
| irsa_namespace | Kubernetes namespace for IRSA | `string` | `"rise-system"` | no |
| irsa_service_account | Kubernetes service account for IRSA | `string` | `"rise-ecr-controller"` | no |
| auto_remove | Delete repos on project deletion | `bool` | `false` | no |
| image_tag_mutability | Tag mutability for repositories | `string` | `"MUTABLE"` | no |
| scan_on_push | Enable image scanning on push | `bool` | `true` | no |
| encryption_type | Encryption type (AES256 or KMS) | `string` | `"AES256"` | no |
| kms_key_arn | KMS key ARN for encryption | `string` | `null` | no |
| lifecycle_policy | Custom ECR lifecycle policy JSON | `string` | `null` | no |
| max_image_count | Max images to retain per repository | `number` | `100` | no |

## Outputs

| Name | Description |
|------|-------------|
| role_arn | ARN of the IAM role |
| role_name | Name of the IAM role |
| user_arn | ARN of the IAM user |
| user_name | Name of the IAM user |
| access_key_id | Access key ID (sensitive) |
| secret_access_key | Secret access key (sensitive) |
| policy_arn | ARN of the IAM policy |
| policy_document | The IAM policy document JSON |
| rise_config | Configuration values for Rise backend |
| lifecycle_policy | ECR lifecycle policy JSON |

## IAM Permissions

The module creates an IAM policy with the following permissions:

- `ecr:GetAuthorizationToken` - Required for any ECR operation
- `ecr:DescribeRepositories`, `ecr:ListTagsForResource` - For discovery
- `ecr:CreateRepository`, `ecr:TagResource`, `ecr:PutImageScanningConfiguration`, `ecr:PutImageTagMutability`, `ecr:PutLifecyclePolicy` - For creating repos
- `ecr:DeleteRepository`, `ecr:BatchDeleteImage` - Only if `auto_remove = true`
- KMS permissions - Only if using KMS encryption

All repository-level permissions are scoped to `${repo_prefix}*`.
