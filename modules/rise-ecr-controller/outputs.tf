# -----------------------------------------------------------------------------
# IAM Role outputs
# -----------------------------------------------------------------------------

output "role_arn" {
  description = "ARN of the IAM role for the ECR controller"
  value       = var.create_iam_role ? aws_iam_role.ecr_controller[0].arn : null
}

output "role_name" {
  description = "Name of the IAM role for the ECR controller"
  value       = var.create_iam_role ? aws_iam_role.ecr_controller[0].name : null
}

# -----------------------------------------------------------------------------
# IAM User outputs (for non-AWS deployments)
# -----------------------------------------------------------------------------

output "user_arn" {
  description = "ARN of the IAM user for the ECR controller"
  value       = var.create_iam_user ? aws_iam_user.ecr_controller[0].arn : null
}

output "user_name" {
  description = "Name of the IAM user for the ECR controller"
  value       = var.create_iam_user ? aws_iam_user.ecr_controller[0].name : null
}

output "access_key_id" {
  description = "Access key ID for the ECR controller IAM user"
  value       = var.create_iam_user ? aws_iam_access_key.ecr_controller[0].id : null
  sensitive   = true
}

output "secret_access_key" {
  description = "Secret access key for the ECR controller IAM user"
  value       = var.create_iam_user ? aws_iam_access_key.ecr_controller[0].secret : null
  sensitive   = true
}

# -----------------------------------------------------------------------------
# Push Role outputs
# -----------------------------------------------------------------------------

output "push_role_arn" {
  description = "ARN of the IAM role for push operations"
  value       = var.create_push_role ? aws_iam_role.push_role[0].arn : null
}

output "push_role_name" {
  description = "Name of the IAM role for push operations"
  value       = var.create_push_role ? aws_iam_role.push_role[0].name : null
}

# -----------------------------------------------------------------------------
# Policy outputs
# -----------------------------------------------------------------------------

output "controller_policy_arn" {
  description = "ARN of the IAM policy for the ECR controller"
  value       = aws_iam_policy.ecr_controller.arn
}

output "push_policy_arn" {
  description = "ARN of the IAM policy for push operations"
  value       = var.create_push_role ? aws_iam_policy.push_role[0].arn : null
}

output "policy_document" {
  description = "The IAM policy document JSON for the controller"
  value       = data.aws_iam_policy_document.ecr_controller.json
}

# -----------------------------------------------------------------------------
# Configuration outputs (for Rise backend config)
# -----------------------------------------------------------------------------

output "rise_config" {
  description = "Configuration values for the Rise backend ECR settings"
  value = {
    region        = local.region
    account_id    = local.account_id
    repo_prefix   = var.repo_prefix
    role_arn      = var.create_iam_role ? aws_iam_role.ecr_controller[0].arn : null
    push_role_arn = var.create_push_role ? aws_iam_role.push_role[0].arn : null
    auto_remove   = var.auto_remove
  }
}

output "lifecycle_policy" {
  description = "The ECR lifecycle policy JSON that will be applied to repositories"
  value       = local.lifecycle_policy
}
