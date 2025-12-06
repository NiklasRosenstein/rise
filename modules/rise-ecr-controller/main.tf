locals {
  name = "${var.name_prefix}-ecr-controller"

  default_tags = {
    "rise:managed-by" = "terraform"
    "rise:component"  = "ecr-controller"
  }

  tags = merge(local.default_tags, var.tags)

  # Default lifecycle policy to limit image count
  default_lifecycle_policy = jsonencode({
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

  lifecycle_policy = var.lifecycle_policy != null ? var.lifecycle_policy : local.default_lifecycle_policy
}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  region     = data.aws_region.current.id
  account_id = data.aws_caller_identity.current.account_id
}

# -----------------------------------------------------------------------------
# IAM Policy for ECR Controller
# -----------------------------------------------------------------------------

data "aws_iam_policy_document" "ecr_controller" {
  # Allow getting authorization tokens (required for any ECR operation)
  statement {
    sid    = "GetAuthorizationToken"
    effect = "Allow"
    actions = [
      "ecr:GetAuthorizationToken"
    ]
    resources = ["*"]
  }

  # Allow listing and describing repositories (for discovery)
  statement {
    sid    = "DescribeRepositories"
    effect = "Allow"
    actions = [
      "ecr:DescribeRepositories",
      "ecr:ListTagsForResource"
    ]
    resources = ["*"]
  }

  # Allow creating repositories with the configured prefix
  statement {
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
      "arn:aws:ecr:${local.region}:${local.account_id}:repository/${var.repo_prefix}*"
    ]
  }

  # Allow deleting repositories (only if auto_remove is enabled)
  dynamic "statement" {
    for_each = var.auto_remove ? [1] : []
    content {
      sid    = "DeleteRepository"
      effect = "Allow"
      actions = [
        "ecr:DeleteRepository",
        "ecr:BatchDeleteImage"
      ]
      resources = [
        "arn:aws:ecr:${local.region}:${local.account_id}:repository/${var.repo_prefix}*"
      ]
    }
  }

  # Allow tagging repositories as orphaned (for soft delete)
  statement {
    sid    = "TagRepository"
    effect = "Allow"
    actions = [
      "ecr:TagResource",
      "ecr:UntagResource"
    ]
    resources = [
      "arn:aws:ecr:${local.region}:${local.account_id}:repository/${var.repo_prefix}*"
    ]
  }

  # KMS permissions if using KMS encryption
  dynamic "statement" {
    for_each = var.encryption_type == "KMS" && var.kms_key_arn != null ? [1] : []
    content {
      sid    = "KMSEncryption"
      effect = "Allow"
      actions = [
        "kms:Encrypt",
        "kms:Decrypt",
        "kms:GenerateDataKey*",
        "kms:DescribeKey"
      ]
      resources = [var.kms_key_arn]
    }
  }
}

resource "aws_iam_policy" "ecr_controller" {
  name        = local.name
  description = "IAM policy for Rise ECR controller to manage ECR repositories"
  policy      = data.aws_iam_policy_document.ecr_controller.json
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
  assume_role_policy = coalesce(
    var.role_assume_policy,
    var.irsa_oidc_provider_arn != null ? data.aws_iam_policy_document.assume_role_irsa[0].json : null,
    data.aws_iam_policy_document.assume_role_default.json
  )
}

resource "aws_iam_role" "ecr_controller" {
  count = var.create_iam_role ? 1 : 0

  name               = local.name
  description        = "IAM role for Rise ECR controller"
  assume_role_policy = local.assume_role_policy
  tags               = local.tags
}

resource "aws_iam_role_policy_attachment" "ecr_controller" {
  count = var.create_iam_role ? 1 : 0

  role       = aws_iam_role.ecr_controller[0].name
  policy_arn = aws_iam_policy.ecr_controller.arn
}

# -----------------------------------------------------------------------------
# IAM User (for non-AWS deployments)
# -----------------------------------------------------------------------------

resource "aws_iam_user" "ecr_controller" {
  count = var.create_iam_user ? 1 : 0

  name = local.name
  tags = local.tags
}

resource "aws_iam_user_policy_attachment" "ecr_controller" {
  count = var.create_iam_user ? 1 : 0

  user       = aws_iam_user.ecr_controller[0].name
  policy_arn = aws_iam_policy.ecr_controller.arn
}

resource "aws_iam_access_key" "ecr_controller" {
  count = var.create_iam_user ? 1 : 0

  user = aws_iam_user.ecr_controller[0].name
}
