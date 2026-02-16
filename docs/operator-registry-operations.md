# Registry Backend Operations

This page is for platform operators maintaining Rise registry integrations.

## Overview

Rise supports multiple registry provider modes through backend configuration.  
Operators are responsible for provider selection, IAM/credentials setup, and production hardening.

## AWS ECR Production Setup

### Architecture: Two-Role Pattern

**Controller Role (`rise-backend`)**:
- Create/delete ECR repositories
- Tag repositories (managed, orphaned)
- Configure repository settings
- Assume the push role

**Push Role (`rise-backend-ecr-push`)**:
- Push/pull images to ECR (under configured prefix)
- Used by backend to generate scoped credentials for CLI workflows

### Terraform Module

Use `modules/rise-aws` to provision ECR access patterns:

```hcl
module "rise_ecr" {
  source = "../modules/rise-aws"

  name        = "rise-backend"
  repo_prefix = "rise/"
  auto_remove = false
}
```

### EKS + IRSA

For Kubernetes-based production installs, prefer IRSA over static credentials and wire the backend service account to an IAM role.

### Non-AWS Runtime

If Rise runs outside AWS, provision an IAM user/keys path and store credentials in a secure secret store.

## Docker/OCI Registry Mode

For `oci-client-auth` mode, the backend returns target registry information while clients use standard registry auth behavior.

## Backend Configuration

Registry configuration is loaded from backend config files under `config/`.

Typical precedence:
1. `local.yaml`
2. `{RISE_CONFIG_RUN_MODE}.yaml` (required)

Use environment variable substitution for secrets and environment-specific values.

## Registry Credentials API

Operator reference endpoint:

```text
GET /api/v1/registry/credentials?project=<project-name>
```

Returned credentials are provider-specific and intended for authenticated clients.

## Security Recommendations

1. Use least-privilege IAM/policy scope per project.
2. Prefer short-lived credentials and role-based access.
3. Enforce TLS for registry traffic in production.
4. Monitor credential issuance and image push activity.
5. Rotate long-lived/static credentials on a regular cadence.

## Troubleshooting (Operator-Level)

### ECR access denied

- Verify controller role can assume push role.
- Verify push-role policy scope and repo prefix alignment.
- Verify target repository exists and naming conventions match.

### Docker registry connectivity failures

- Verify registry endpoint reachability from both backend and client environments.
- Verify auth state (`docker login`) and namespace/repo permissions.

## Extending Registry Providers

To add a provider:
1. Implement the registry provider trait in backend registry provider modules.
2. Add provider configuration to registry settings.
3. Register provider selection in provider factory/bootstrap logic.
