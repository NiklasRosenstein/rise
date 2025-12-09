variable "name" {
  description = "Name for the IAM roles and policies (e.g., 'rise-backend', 'rise-prod-backend'). Also used as ECR repository prefix."
  type        = string
  default     = "rise-backend"
}

variable "tags" {
  description = "Tags to apply to all resources"
  type        = map(string)
  default     = {}
}

# Authentication method - choose one

variable "create_iam_user" {
  description = "Create an IAM user with access keys for the Rise backend (use for non-AWS deployments)"
  type        = bool
  default     = false
}

variable "irsa_oidc_provider_arn" {
  description = "OIDC provider ARN for IRSA (IAM Roles for Service Accounts). Required if using EKS."
  type        = string
  default     = null
}

variable "irsa_namespace" {
  description = "Kubernetes namespace where the Rise backend runs (for IRSA)"
  type        = string
  default     = "rise-system"
}

variable "irsa_service_account" {
  description = "Kubernetes service account name for the Rise backend (for IRSA)"
  type        = string
  default     = "rise-backend"
}

# ECR settings

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
  description = "Encryption type for repositories (AES256 or KMS). If KMS, a KMS key will be automatically created."
  type        = string
  default     = "AES256"

  validation {
    condition     = contains(["AES256", "KMS"], var.encryption_type)
    error_message = "encryption_type must be either AES256 or KMS"
  }
}

# Lifecycle policies

variable "max_image_count" {
  description = "Maximum number of images to retain per repository"
  type        = number
  default     = 100
}
