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

# Feature flags

variable "enable_ecr" {
  description = "Enable ECR permissions for the Rise backend. Set to true if using ECR for container registry."
  type        = bool
  default     = true
}

variable "enable_rds" {
  description = "Enable RDS permissions for the Rise backend. Set to true if using the AWS RDS extension."
  type        = bool
  default     = false
}

variable "create_rds_service_linked_role" {
  description = "Create the RDS service-linked role. Only needed once per AWS account. Set to false if the role already exists."
  type        = bool
  default     = true
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

variable "enable_kms" {
  description = "Enable KMS encryption for ECR repositories. If true, a KMS key will be automatically created. If false, AES256 encryption is used."
  type        = bool
  default     = false
}

# Lifecycle policies

variable "max_image_count" {
  description = "Maximum number of images to retain per repository"
  type        = number
  default     = 100
}
