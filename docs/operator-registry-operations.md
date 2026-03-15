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

## GitLab Container Registry

### Credential flow

Rise uses two distinct credential mechanisms for GitLab, depending on the context:

**CLI image push** — The backend mints a short-lived (~15 min) scoped JWT from GitLab's JWT auth endpoint for each push operation:

```
GET <gitlab_url>/jwt/auth?service=container_registry&scope=repository:<namespace>/<project>:push,pull
Authorization: Basic <base64(username:token)>
```

The JWT is injected directly into the container CLI's auth config file (using the `registrytoken` key), bypassing `docker login`. This keeps push credentials out of the host's persistent credential store and limits each token's scope to a single repository.

**Kubernetes image pull secrets** — When `mint_pull_secrets: true`, the controller writes a standard `kubernetes.io/dockerconfigjson` secret containing the PAT into each project's namespace. The container runtime (containerd/CRI-O) reads the PAT and handles its own JWT exchange with GitLab on each pull.

> **Note:** Pre-obtained JWTs cannot be used in Kubernetes pull secrets because containerd does not implement the `registrytoken` field in `dockerconfigjson` (its `ParseAuth` function has no support for pre-obtained bearer tokens). Providing a PAT instead allows the container runtime to perform the full token exchange itself. Follow https://github.com/containerd/containerd/pull/13032 for progress.

### IAM / token requirements

The GitLab token must have `read_registry` and `write_registry` scopes. A [Deploy Token](https://docs.gitlab.com/ee/user/project/deploy_tokens/) scoped to the group is recommended over a personal access token in production.

### Troubleshooting

**`ErrImagePull` / "access forbidden"**
- Verify `mint_pull_secrets: true` is set and the pull secret exists in the project namespace (`kubectl get secret -n rise-<project>`).
- Confirm the token has `read_registry` scope for the namespace.
- Check the token hasn't expired or been revoked.

**`"access": []` in the minted JWT**
- GitLab does not support wildcard repository scopes. The image path in the JWT scope must exactly match `<namespace>/<project>`.

**JWT auth returns non-2xx**
- Verify `gitlab_url` is reachable from the backend pod and that `username`/`token` are correct.

## Docker/OCI Registry Mode

For `oci-client-auth` mode, the backend returns target registry information while clients use standard registry auth behavior.

## Backend Configuration

Registry configuration is loaded from backend config files under `config/`.

Typical precedence:
1. `{RISE_CONFIG_RUN_MODE}.{toml,yaml,yml}` (required)
2. `local.{toml,yaml,yml}` (optional local overrides)

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
