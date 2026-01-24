locals {
  name        = var.name
  repo_prefix = "${var.name}/"

  # KMS key alias name - defaults to just the name, but can be overridden for backwards compatibility
  kms_key_alias = var.kms_key_alias != null ? var.kms_key_alias : var.name

  default_tags = {
    "rise:managed-by" = "terraform"
    "rise:component"  = "backend"
  }

  tags = merge(local.default_tags, var.tags)

  # Default lifecycle policy to limit image count
  lifecycle_policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Keep only the last ${var.max_image_count} images"
      selection = {
        tagStatus   = "any"
        countType   = "imageCountMoreThan"
        countNumber = var.max_image_count
      }
      action = {
        type = "expire"
      }
    }]
  })
}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  region     = data.aws_region.current.id
  account_id = data.aws_caller_identity.current.account_id
}

# -----------------------------------------------------------------------------
# KMS Key (for encryption)
# -----------------------------------------------------------------------------

resource "aws_kms_key" "ecr" {
  count = var.enable_ecr && var.enable_kms ? 1 : 0

  description             = "KMS key for Rise encryption"
  deletion_window_in_days = 30
  enable_key_rotation     = true
  tags                    = local.tags
}

resource "aws_kms_alias" "ecr" {
  count = var.enable_ecr && var.enable_kms ? 1 : 0

  name          = "alias/${local.kms_key_alias}"
  target_key_id = aws_kms_key.ecr[0].key_id
}

# -----------------------------------------------------------------------------
# IAM Policy for ECR Controller
# -----------------------------------------------------------------------------

data "aws_iam_policy_document" "backend" {
  # ECR permissions (if enabled)
  dynamic "statement" {
    for_each = var.enable_ecr ? [1] : []
    content {
      sid    = "GetAuthorizationToken"
      effect = "Allow"
      actions = [
        "ecr:GetAuthorizationToken"
      ]
      resources = ["*"]
    }
  }

  dynamic "statement" {
    for_each = var.enable_ecr ? [1] : []
    content {
      sid    = "DescribeRepositories"
      effect = "Allow"
      actions = [
        "ecr:DescribeRepositories",
        "ecr:ListTagsForResource"
      ]
      resources = ["*"]
    }
  }

  dynamic "statement" {
    for_each = var.enable_ecr ? [1] : []
    content {
      sid    = "CreateRepository"
      effect = "Allow"
      actions = [
        "ecr:CreateRepository",
        "ecr:TagResource",
        "ecr:PutImageScanningConfiguration",
        "ecr:PutImageTagMutability",
        "ecr:PutLifecyclePolicy"
      ]
      resources = [
        "arn:aws:ecr:${local.region}:${local.account_id}:repository/${local.repo_prefix}*"
      ]
    }
  }

  dynamic "statement" {
    for_each = var.enable_ecr ? [1] : []
    content {
      sid    = "DeleteRepository"
      effect = "Allow"
      actions = [
        "ecr:DeleteRepository",
        "ecr:BatchDeleteImage"
      ]
      resources = [
        "arn:aws:ecr:${local.region}:${local.account_id}:repository/${local.repo_prefix}*"
      ]
    }
  }

  dynamic "statement" {
    for_each = var.enable_ecr ? [1] : []
    content {
      sid    = "TagRepository"
      effect = "Allow"
      actions = [
        "ecr:TagResource",
        "ecr:UntagResource"
      ]
      resources = [
        "arn:aws:ecr:${local.region}:${local.account_id}:repository/${local.repo_prefix}*"
      ]
    }
  }

  # KMS permissions if using KMS encryption
  dynamic "statement" {
    for_each = var.enable_ecr && var.enable_kms ? [1] : []
    content {
      sid    = "KMSEncryption"
      effect = "Allow"
      actions = [
        "kms:Encrypt",
        "kms:Decrypt",
        "kms:GenerateDataKey*",
        "kms:DescribeKey"
      ]
      resources = [aws_kms_key.ecr[0].arn]
    }
  }

  # RDS permissions for managing database instances (if enabled)
  dynamic "statement" {
    for_each = var.enable_rds ? [1] : []
    content {
      sid    = "ManageRDSInstances"
      effect = "Allow"
      actions = [
        "rds:CreateDBInstance",
        "rds:DeleteDBInstance",
        "rds:DescribeDBInstances",
        "rds:ModifyDBInstance",
        "rds:ListTagsForResource",
        "rds:AddTagsToResource",
        "rds:RemoveTagsFromResource"
      ]
      resources = [
        "arn:aws:rds:${local.region}:${local.account_id}:db:${var.name}-*",
        "arn:aws:rds:${local.region}:${local.account_id}:subgrp:${var.name}-*"
      ]
    }
  }

  # RDS subnet groups (needed for VPC placement)
  dynamic "statement" {
    for_each = var.enable_rds ? [1] : []
    content {
      sid    = "ManageRDSSubnetGroups"
      effect = "Allow"
      actions = [
        "rds:CreateDBSubnetGroup",
        "rds:DeleteDBSubnetGroup",
        "rds:DescribeDBSubnetGroups"
      ]
      resources = [
        "arn:aws:rds:${local.region}:${local.account_id}:subgrp:${var.name}-*"
      ]
    }
  }

  # S3 permissions for managing buckets (if enabled)
  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ListS3Buckets"
      effect = "Allow"
      actions = [
        "s3:ListAllMyBuckets",
        "s3:GetBucketLocation"
      ]
      resources = ["*"]
    }
  }

  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ManageS3Buckets"
      effect = "Allow"
      actions = [
        "s3:CreateBucket",
        "s3:DeleteBucket",
        "s3:GetBucketVersioning",
        "s3:PutBucketVersioning",
        "s3:GetBucketLifecycleConfiguration",
        "s3:PutBucketLifecycleConfiguration",
        "s3:DeleteBucketLifecycleConfiguration",
        "s3:GetBucketCors",
        "s3:PutBucketCors",
        "s3:DeleteBucketCors",
        "s3:GetBucketPublicAccessBlock",
        "s3:PutBucketPublicAccessBlock",
        "s3:GetEncryptionConfiguration",
        "s3:PutEncryptionConfiguration",
        "s3:GetBucketTagging",
        "s3:PutBucketTagging"
      ]
      resources = [
        "arn:aws:s3:::${var.name}-*"
      ]
    }
  }

  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ManageS3Objects"
      effect = "Allow"
      actions = [
        "s3:ListBucket",
        "s3:ListBucketVersions",
        "s3:GetObject",
        "s3:GetObjectVersion",
        "s3:DeleteObject",
        "s3:DeleteObjectVersion"
      ]
      resources = [
        "arn:aws:s3:::${var.name}-*",
        "arn:aws:s3:::${var.name}-*/*"
      ]
    }
  }

  # IAM permissions for managing users and access keys (if S3 enabled)
  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ManageIAMUsers"
      effect = "Allow"
      actions = [
        "iam:CreateUser",
        "iam:DeleteUser",
        "iam:GetUser",
        "iam:ListAccessKeys",
        "iam:TagUser",
        "iam:UntagUser"
      ]
      resources = [
        "arn:aws:iam::${local.account_id}:user/rise-s3-*"
      ]
    }
  }

  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ManageIAMAccessKeys"
      effect = "Allow"
      actions = [
        "iam:CreateAccessKey",
        "iam:DeleteAccessKey",
        "iam:UpdateAccessKey"
      ]
      resources = [
        "arn:aws:iam::${local.account_id}:user/rise-s3-*"
      ]
    }
  }

  dynamic "statement" {
    for_each = var.enable_s3 ? [1] : []
    content {
      sid    = "ManageIAMUserPolicies"
      effect = "Allow"
      actions = [
        "iam:PutUserPolicy",
        "iam:DeleteUserPolicy",
        "iam:GetUserPolicy",
        "iam:ListUserPolicies"
      ]
      resources = [
        "arn:aws:iam::${local.account_id}:user/rise-s3-*"
      ]
    }
  }
}

resource "aws_iam_policy" "backend" {
  name        = local.name
  description = "IAM policy for Rise backend to manage ECR repositories, RDS instances, S3 buckets, and IAM users"
  policy      = data.aws_iam_policy_document.backend.json
  tags        = local.tags
}

# -----------------------------------------------------------------------------
# IAM Role (for AWS deployments)
# -----------------------------------------------------------------------------

# Default assume role policy - allows account to assume
data "aws_iam_policy_document" "assume_role_default" {
  statement {
    effect = "Allow"
    principals {
      type        = "AWS"
      identifiers = ["arn:aws:iam::${local.account_id}:root"]
    }
    actions = ["sts:AssumeRole"]
  }
}

# IRSA assume role policy - for EKS service accounts
data "aws_iam_policy_document" "assume_role_irsa" {
  count = var.irsa_oidc_provider_arn != null ? 1 : 0

  statement {
    effect = "Allow"
    principals {
      type        = "Federated"
      identifiers = [var.irsa_oidc_provider_arn]
    }
    actions = ["sts:AssumeRoleWithWebIdentity"]
    condition {
      test     = "StringEquals"
      variable = "${replace(var.irsa_oidc_provider_arn, "/^arn:aws:iam::[0-9]+:oidc-provider\\//", "")}:sub"
      values   = ["system:serviceaccount:${var.irsa_namespace}:${var.irsa_service_account}"]
    }
    condition {
      test     = "StringEquals"
      variable = "${replace(var.irsa_oidc_provider_arn, "/^arn:aws:iam::[0-9]+:oidc-provider\\//", "")}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

locals {
  # Use IRSA policy if OIDC provider is configured, otherwise use default
  assume_role_policy = var.irsa_oidc_provider_arn != null ? data.aws_iam_policy_document.assume_role_irsa[0].json : data.aws_iam_policy_document.assume_role_default.json
}

resource "aws_iam_role" "backend" {
  name               = local.name
  description        = "IAM role for Rise backend"
  assume_role_policy = local.assume_role_policy
  tags               = local.tags
}

resource "aws_iam_role_policy_attachment" "backend" {
  role       = aws_iam_role.backend.name
  policy_arn = aws_iam_policy.backend.arn
}

# -----------------------------------------------------------------------------
# IAM User (for non-AWS deployments)
# -----------------------------------------------------------------------------

resource "aws_iam_user" "backend" {
  count = var.create_iam_user ? 1 : 0

  name = local.name
  tags = local.tags
}

resource "aws_iam_user_policy_attachment" "backend" {
  count = var.create_iam_user ? 1 : 0

  user       = aws_iam_user.backend[0].name
  policy_arn = aws_iam_policy.backend.arn
}

resource "aws_iam_access_key" "backend" {
  count = var.create_iam_user ? 1 : 0

  user = aws_iam_user.backend[0].name
}

# -----------------------------------------------------------------------------
# Push Role - for scoped image push credentials (if ECR enabled)
# -----------------------------------------------------------------------------
# This role is assumed by the Rise backend to generate temporary credentials
# for clients to push images. The credentials are scoped per-repository using
# an inline session policy during AssumeRole.

data "aws_iam_policy_document" "push_role" {
  count = var.enable_ecr ? 1 : 0

  # Allow getting authorization tokens (required for docker login)
  statement {
    sid    = "GetAuthorizationToken"
    effect = "Allow"
    actions = [
      "ecr:GetAuthorizationToken"
    ]
    resources = ["*"]
  }

  # Allow all image push operations on repositories with our prefix
  # The actual scoping to specific repos happens via session policy during AssumeRole
  statement {
    sid    = "PushImages"
    effect = "Allow"
    actions = [
      "ecr:BatchCheckLayerAvailability",
      "ecr:InitiateLayerUpload",
      "ecr:UploadLayerPart",
      "ecr:CompleteLayerUpload",
      "ecr:PutImage",
      "ecr:BatchGetImage",
      "ecr:GetDownloadUrlForLayer"
    ]
    resources = [
      "arn:aws:ecr:${local.region}:${local.account_id}:repository/${local.repo_prefix}*"
    ]
  }
}

resource "aws_iam_policy" "push_role" {
  count = var.enable_ecr ? 1 : 0

  name        = "${var.name}-ecr-push"
  description = "IAM policy for Rise ECR push operations"
  policy      = data.aws_iam_policy_document.push_role[0].json
  tags        = local.tags
}

# The push role can be assumed by the controller role or user
data "aws_iam_policy_document" "push_role_assume" {
  count = var.enable_ecr ? 1 : 0

  # Allow the controller role to assume the push role
  statement {
    effect = "Allow"
    principals {
      type        = "AWS"
      identifiers = [aws_iam_role.backend.arn]
    }
    actions = ["sts:AssumeRole"]
  }

  # Allow the controller user to assume the push role
  dynamic "statement" {
    for_each = var.create_iam_user ? [1] : []
    content {
      effect = "Allow"
      principals {
        type        = "AWS"
        identifiers = [aws_iam_user.backend[0].arn]
      }
      actions = ["sts:AssumeRole"]
    }
  }
}

resource "aws_iam_role" "push_role" {
  count = var.enable_ecr ? 1 : 0

  name               = "${var.name}-ecr-push"
  description        = "IAM role for Rise ECR push operations (assumed to generate scoped credentials)"
  assume_role_policy = data.aws_iam_policy_document.push_role_assume[0].json
  tags               = local.tags
}

resource "aws_iam_role_policy_attachment" "push_role" {
  count = var.enable_ecr ? 1 : 0

  role       = aws_iam_role.push_role[0].name
  policy_arn = aws_iam_policy.push_role[0].arn
}

# The controller also needs permission to assume the push role
data "aws_iam_policy_document" "assume_push_role" {
  count = var.enable_ecr ? 1 : 0

  statement {
    sid    = "AssumePushRole"
    effect = "Allow"
    actions = [
      "sts:AssumeRole"
    ]
    resources = [aws_iam_role.push_role[0].arn]
  }
}

resource "aws_iam_policy" "assume_push_role" {
  count = var.enable_ecr ? 1 : 0

  name        = "${var.name}-ecr-assume-push"
  description = "Allow assuming the ECR push role"
  policy      = data.aws_iam_policy_document.assume_push_role[0].json
  tags        = local.tags
}

resource "aws_iam_role_policy_attachment" "controller_assume_push" {
  count = var.enable_ecr ? 1 : 0

  role       = aws_iam_role.backend.name
  policy_arn = aws_iam_policy.assume_push_role[0].arn
}

resource "aws_iam_user_policy_attachment" "controller_assume_push" {
  count = var.create_iam_user && var.enable_ecr ? 1 : 0

  user       = aws_iam_user.backend[0].name
  policy_arn = aws_iam_policy.assume_push_role[0].arn
}

# -----------------------------------------------------------------------------
# RDS Service-Linked Role
# -----------------------------------------------------------------------------
# This role is required for RDS to manage resources on your behalf.
# It only needs to be created once per AWS account.

resource "aws_iam_service_linked_role" "rds" {
  count = var.enable_rds && var.create_rds_service_linked_role ? 1 : 0

  aws_service_name = "rds.amazonaws.com"
  description      = "Service-linked role for Amazon RDS"
}

# -----------------------------------------------------------------------------
# RDS DB Subnet Group
# -----------------------------------------------------------------------------
# Subnet group defines which subnets RDS instances can be placed in

resource "aws_db_subnet_group" "rds" {
  count = var.enable_rds && var.rds_vpc_id != null && length(var.rds_subnet_ids) > 0 ? 1 : 0

  name_prefix = "${var.name}-rds-"
  description = "DB subnet group for Rise RDS instances"
  subnet_ids  = var.rds_subnet_ids

  tags = merge(local.tags, {
    Name = "${var.name}-rds"
  })
}

# -----------------------------------------------------------------------------
# RDS Security Group
# -----------------------------------------------------------------------------
# Security group for RDS instances, allowing access from specified security groups

resource "aws_security_group" "rds" {
  count = var.enable_rds && var.rds_vpc_id != null ? 1 : 0

  name_prefix = "${var.name}-rds-"
  description = "Security group for Rise RDS instances"
  vpc_id      = var.rds_vpc_id

  tags = merge(local.tags, {
    Name = "${var.name}-rds"
  })
}

# Allow ingress from specified security groups on PostgreSQL port
resource "aws_vpc_security_group_ingress_rule" "rds_from_allowed_sgs" {
  count = var.enable_rds && var.rds_vpc_id != null ? length(var.rds_allowed_security_groups) : 0

  security_group_id            = aws_security_group.rds[0].id
  referenced_security_group_id = var.rds_allowed_security_groups[count.index]
  from_port                    = 5432
  to_port                      = 5432
  ip_protocol                  = "tcp"
  description                  = "PostgreSQL access from allowed security group"
}

# Allow ingress from specified CIDR blocks on PostgreSQL port
resource "aws_vpc_security_group_ingress_rule" "rds_from_allowed_cidrs" {
  count = var.enable_rds && var.rds_vpc_id != null ? length(var.rds_allowed_cidr_blocks) : 0

  security_group_id = aws_security_group.rds[0].id
  cidr_ipv4         = var.rds_allowed_cidr_blocks[count.index]
  from_port         = 5432
  to_port           = 5432
  ip_protocol       = "tcp"
  description       = "PostgreSQL access from allowed CIDR block"
}

# Allow all egress (RDS instances need to reach out for updates, etc.)
resource "aws_vpc_security_group_egress_rule" "rds_egress" {
  count = var.enable_rds && var.rds_vpc_id != null ? 1 : 0

  security_group_id = aws_security_group.rds[0].id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  description       = "Allow all outbound traffic"
}
