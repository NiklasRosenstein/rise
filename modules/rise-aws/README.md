# Rise AWS Terraform Module

This module creates the AWS IAM resources required for the Rise backend to manage AWS services (ECR for container registries and RDS for database instances).

## Features

- **Controller role**: Manages ECR repository lifecycle (create, delete, tag) and RDS instances
- **Push role**: Separate role for generating scoped image push credentials
- Supports IAM role (for AWS deployments with IRSA or instance profiles)
- Supports IAM user with access keys (for non-AWS deployments)
- Configurable repository prefix for multi-tenant isolation
- Optional auto-remove or soft-delete (orphan tagging) modes
- Default lifecycle policy to limit image retention
- RDS instance management for project extensions

## Architecture

The module creates two separate IAM roles with distinct permissions:

1. **Backend Role** (e.g., `rise-backend`): Used by the Rise backend to manage AWS resources
   - ECR repositories: Create/delete, tag, list for discovery
   - RDS instances: Create/delete/modify PostgreSQL instances with `rise-*` prefix
   - Security groups and subnet groups for RDS networking
   - KMS encryption (if enabled)

2. **Push Role** (e.g., `rise-backend-ecr-push`): Assumed by the backend to generate scoped credentials
   - Push images to ECR
   - The backend uses STS AssumeRole with an inline session policy to scope credentials to specific repositories

## Usage

### Basic Usage (IAM Role)

```hcl
module "rise_aws" {
  source = "./modules/rise-aws"

  name        = "rise-backend"
  repo_prefix = "rise/"
  auto_remove = false  # Tag as orphaned instead of deleting

  tags = {
    Environment = "production"
  }
}
```

### With IRSA (EKS)

```hcl
module "rise_aws" {
  source = "./modules/rise-aws"

  name                   = "rise-backend"
  repo_prefix            = "rise/"
  irsa_oidc_provider_arn = module.eks.oidc_provider_arn
  irsa_namespace         = "rise-system"
  irsa_service_account   = "rise-backend"
}
```

### With IAM User (Non-AWS Deployment)

```hcl
module "rise_aws" {
  source = "./modules/rise-aws"

  name            = "rise-backend"
  repo_prefix     = "rise/"
  create_iam_role = false
  create_iam_user = true
}

# Store credentials securely
resource "aws_secretsmanager_secret" "rise_backend_creds" {
  name = "rise/backend-credentials"
}

resource "aws_secretsmanager_secret_version" "rise_backend_creds" {
  secret_id = aws_secretsmanager_secret.rise_backend_creds.id
  secret_string = jsonencode({
    access_key_id     = module.rise_aws.access_key_id
    secret_access_key = module.rise_aws.secret_access_key
  })
}
```

## Rise Backend Configuration

After applying this module, configure the Rise backend with the ECR settings:

```toml
# config/local.toml
[registry]
type = "ecr"
region = "eu-west-1"  # From module.rise_aws.rise_config.region
account_id = "123456789012"  # From module.rise_aws.rise_config.account_id
repo_prefix = "rise/"  # From module.rise_aws.rise_config.repo_prefix
role_arn = "arn:aws:iam::123456789012:role/rise-backend"  # From module.rise_aws.role_arn
push_role_arn = "arn:aws:iam::123456789012:role/rise-backend-ecr-push"  # From module.rise_aws.push_role_arn
auto_remove = false  # From module.rise_aws.rise_config.auto_remove

# If using IAM user instead of role:
# access_key_id = "AKIA..."
# secret_access_key = "..."
```

## Inputs

| Name | Description | Type | Default | Required |
|------|-------------|------|---------|:--------:|
| name | Name for the IAM role and policy | `string` | `"rise-backend"` | no |
| repo_prefix | Prefix for ECR repository names | `string` | `"rise/"` | no |
| tags | Tags to apply to all resources | `map(string)` | `{}` | no |
| create_iam_role | Create an IAM role for the controller | `bool` | `true` | no |
| create_iam_user | Create an IAM user with access keys | `bool` | `false` | no |
| create_push_role | Create a separate push role | `bool` | `true` | no |
| push_role_assume_principals | Additional principals for push role | `list(string)` | `null` | no |
| role_assume_policy | Custom assume role policy JSON | `string` | `null` | no |
| irsa_oidc_provider_arn | OIDC provider ARN for IRSA | `string` | `null` | no |
| irsa_namespace | Kubernetes namespace for IRSA | `string` | `"rise-system"` | no |
| irsa_service_account | Kubernetes service account for IRSA | `string` | `"rise-backend"` | no |
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
| role_arn | ARN of the controller IAM role |
| role_name | Name of the controller IAM role |
| push_role_arn | ARN of the push IAM role |
| push_role_name | Name of the push IAM role |
| user_arn | ARN of the IAM user |
| user_name | Name of the IAM user |
| access_key_id | Access key ID (sensitive) |
| secret_access_key | Secret access key (sensitive) |
| controller_policy_arn | ARN of the controller IAM policy |
| push_policy_arn | ARN of the push IAM policy |
| policy_document | The controller IAM policy document JSON |
| rise_config | Configuration values for Rise backend |
| lifecycle_policy | ECR lifecycle policy JSON |

## IAM Permissions

### Controller Role

**ECR Permissions:**
- `ecr:GetAuthorizationToken` - Required for any ECR operation
- `ecr:DescribeRepositories`, `ecr:ListTagsForResource` - For discovery
- `ecr:CreateRepository`, `ecr:TagResource`, `ecr:PutImageScanningConfiguration`, `ecr:PutImageTagMutability`, `ecr:PutLifecyclePolicy` - For creating repos
- `ecr:DeleteRepository`, `ecr:BatchDeleteImage` - For deleting repos
- `sts:AssumeRole` on push role - To generate scoped credentials

**RDS Permissions:**
- `rds:CreateDBInstance`, `rds:DeleteDBInstance`, `rds:DescribeDBInstances`, `rds:ModifyDBInstance` - For managing database instances
- `rds:ListTagsForResource`, `rds:AddTagsToResource`, `rds:RemoveTagsFromResource` - For tagging
- `rds:CreateDBSubnetGroup`, `rds:DeleteDBSubnetGroup`, `rds:DescribeDBSubnetGroups` - For VPC placement
- `ec2:DescribeSecurityGroups`, `ec2:CreateSecurityGroup`, `ec2:DeleteSecurityGroup` - For network security
- `ec2:AuthorizeSecurityGroupIngress`, `ec2:RevokeSecurityGroupIngress` - For security group rules
- `ec2:DescribeVpcs`, `ec2:DescribeSubnets` - For VPC discovery

**KMS Permissions:**
- `kms:Encrypt`, `kms:Decrypt`, `kms:GenerateDataKey*`, `kms:DescribeKey` - Only if using KMS encryption

All ECR permissions are scoped to `${repo_prefix}*`. RDS permissions are scoped to `rise-*` instance names.

### Push Role

- `ecr:GetAuthorizationToken` - For docker login
- `ecr:BatchCheckLayerAvailability`, `ecr:InitiateLayerUpload`, `ecr:UploadLayerPart`, `ecr:CompleteLayerUpload`, `ecr:PutImage` - For pushing images
- `ecr:BatchGetImage`, `ecr:GetDownloadUrlForLayer` - For pulling images

All permissions are scoped to `${repo_prefix}*`.
