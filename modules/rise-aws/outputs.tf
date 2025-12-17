# -----------------------------------------------------------------------------
# IAM Role outputs
# -----------------------------------------------------------------------------

output "role_arn" {
  description = "ARN of the IAM role for the Rise backend"
  value       = aws_iam_role.backend.arn
}

output "role_name" {
  description = "Name of the IAM role for the Rise backend"
  value       = aws_iam_role.backend.name
}

# -----------------------------------------------------------------------------
# IAM User outputs (for non-AWS deployments)
# -----------------------------------------------------------------------------

output "user_arn" {
  description = "ARN of the IAM user for the Rise backend"
  value       = var.create_iam_user ? aws_iam_user.backend[0].arn : null
}

output "user_name" {
  description = "Name of the IAM user for the Rise backend"
  value       = var.create_iam_user ? aws_iam_user.backend[0].name : null
}

output "access_key_id" {
  description = "Access key ID for the Rise backend IAM user"
  value       = var.create_iam_user ? aws_iam_access_key.backend[0].id : null
  sensitive   = true
}

output "secret_access_key" {
  description = "Secret access key for the Rise backend IAM user"
  value       = var.create_iam_user ? aws_iam_access_key.backend[0].secret : null
  sensitive   = true
}

# -----------------------------------------------------------------------------
# Push Role outputs
# -----------------------------------------------------------------------------

output "push_role_arn" {
  description = "ARN of the IAM role for push operations (null if ECR not enabled)"
  value       = var.enable_ecr ? aws_iam_role.push_role[0].arn : null
}

output "push_role_name" {
  description = "Name of the IAM role for push operations (null if ECR not enabled)"
  value       = var.enable_ecr ? aws_iam_role.push_role[0].name : null
}

# -----------------------------------------------------------------------------
# Policy outputs
# -----------------------------------------------------------------------------

output "controller_policy_arn" {
  description = "ARN of the IAM policy for the Rise backend"
  value       = aws_iam_policy.backend.arn
}

output "push_policy_arn" {
  description = "ARN of the IAM policy for push operations (null if ECR not enabled)"
  value       = var.enable_ecr ? aws_iam_policy.push_role[0].arn : null
}

output "policy_document" {
  description = "The IAM policy document JSON for the Rise backend"
  value       = data.aws_iam_policy_document.backend.json
}

# -----------------------------------------------------------------------------
# KMS Key outputs
# -----------------------------------------------------------------------------

output "kms_key_arn" {
  description = "ARN of the KMS key for ECR encryption (null if KMS not enabled)"
  value       = var.enable_kms ? aws_kms_key.ecr[0].arn : null
}

output "kms_key_id" {
  description = "ID of the KMS key for ECR encryption (null if KMS not enabled)"
  value       = var.enable_kms ? aws_kms_key.ecr[0].key_id : null
}

# -----------------------------------------------------------------------------
# Configuration outputs (for Rise backend config)
# -----------------------------------------------------------------------------

output "rise_config" {
  description = "Configuration values for the Rise backend"
  value = {
    region        = local.region
    account_id    = local.account_id
    repo_prefix   = local.repo_prefix
    role_arn      = aws_iam_role.backend.arn
    push_role_arn = var.enable_ecr ? aws_iam_role.push_role[0].arn : null
  }
}

output "lifecycle_policy" {
  description = "The ECR lifecycle policy JSON that will be applied to repositories"
  value       = local.lifecycle_policy
}

# -----------------------------------------------------------------------------
# RDS outputs
# -----------------------------------------------------------------------------

output "rds_security_group_id" {
  description = "ID of the RDS security group (null if RDS not enabled or VPC not specified)"
  value       = var.enable_rds && var.rds_vpc_id != null ? aws_security_group.rds[0].id : null
}

output "rds_security_group_name" {
  description = "Name of the RDS security group (null if RDS not enabled or VPC not specified)"
  value       = var.enable_rds && var.rds_vpc_id != null ? aws_security_group.rds[0].name : null
}

output "rds_subnet_group_name" {
  description = "Name of the RDS DB subnet group (null if RDS not enabled or subnets not specified)"
  value       = var.enable_rds && var.rds_vpc_id != null && length(var.rds_subnet_ids) > 0 ? aws_db_subnet_group.rds[0].name : null
}

output "rds_subnet_group_arn" {
  description = "ARN of the RDS DB subnet group (null if RDS not enabled or subnets not specified)"
  value       = var.enable_rds && var.rds_vpc_id != null && length(var.rds_subnet_ids) > 0 ? aws_db_subnet_group.rds[0].arn : null
}
