variable "name_prefix" {
  description = "Prefix for all resource names (e.g., 'rise-prod', 'rise-dev')"
  type        = string
  default     = "rise"
}

variable "repo_prefix" {
  description = "Prefix for ECR repository names. Repositories will be named '{repo_prefix}{project}' (e.g., 'rise/' produces 'rise/hello')"
  type        = string
  default     = "rise/"
}

variable "tags" {
  description = "Tags to apply to all resources"
  type        = map(string)
  default     = {}
}

# Authentication method - choose one

variable "create_iam_user" {
  description = "Create an IAM user with access keys for the controller (use for non-AWS deployments)"
  type        = bool
  default     = false
}

variable "create_iam_role" {
  description = "Create an IAM role for the controller (use for AWS deployments with IRSA or instance profiles)"
  type        = bool
  default     = true
}

variable "role_assume_policy" {
  description = "Custom assume role policy document JSON. If not provided, defaults to allowing the AWS account to assume the role."
  type        = string
  default     = null
}

variable "irsa_oidc_provider_arn" {
  description = "OIDC provider ARN for IRSA (IAM Roles for Service Accounts). Required if using EKS."
  type        = string
  default     = null
}

variable "irsa_namespace" {
  description = "Kubernetes namespace where the controller runs (for IRSA)"
  type        = string
  default     = "rise-system"
}

variable "irsa_service_account" {
  description = "Kubernetes service account name for the controller (for IRSA)"
  type        = string
  default     = "rise-ecr-controller"
}

# ECR settings

variable "auto_remove" {
  description = "Whether to automatically delete ECR repositories when projects are deleted. If false, repositories are tagged as orphaned."
  type        = bool
  default     = false
}

variable "image_tag_mutability" {
  description = "The tag mutability setting for repositories created by the controller"
  type        = string
  default     = "MUTABLE"

  validation {
    condition     = contains(["MUTABLE", "IMMUTABLE"], var.image_tag_mutability)
    error_message = "image_tag_mutability must be either MUTABLE or IMMUTABLE"
  }
}

variable "scan_on_push" {
  description = "Enable image scanning on push for repositories created by the controller"
  type        = bool
  default     = true
}

variable "encryption_type" {
  description = "Encryption type for repositories (AES256 or KMS)"
  type        = string
  default     = "AES256"

  validation {
    condition     = contains(["AES256", "KMS"], var.encryption_type)
    error_message = "encryption_type must be either AES256 or KMS"
  }
}

variable "kms_key_arn" {
  description = "KMS key ARN for repository encryption (required if encryption_type is KMS)"
  type        = string
  default     = null
}

# Lifecycle policies

variable "lifecycle_policy" {
  description = "ECR lifecycle policy JSON to apply to all repositories created by the controller"
  type        = string
  default     = null
}

variable "max_image_count" {
  description = "Maximum number of images to retain per repository (creates a simple lifecycle policy if lifecycle_policy is not set)"
  type        = number
  default     = 100
}
