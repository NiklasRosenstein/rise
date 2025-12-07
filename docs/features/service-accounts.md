# Service Accounts (Workload Identity)

Service accounts enable CI/CD systems like GitLab CI and GitHub Actions to authenticate and deploy to Rise projects using OIDC (OpenID Connect) JWT tokens, without requiring user credentials.

## Overview

Service accounts use the **workload identity** pattern:
1. Your CI/CD system generates a JWT token with claims about the job (project path, branch, etc.)
2. Rise validates the JWT signature against the OIDC issuer (GitLab, GitHub, etc.)
3. Rise matches the token claims against configured service account requirements
4. If matched, the service account can deploy to its associated project

**Key security features**:
- No long-lived credentials - tokens are short-lived and automatically rotated
- Claim-based authorization - only jobs matching specific criteria can deploy
- Collision detection - prevents ambiguous claim configurations
- Project-scoped - service accounts can only deploy to their assigned project

## Quick Start

### GitLab CI

Create service account:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo \
  --claim ref_protected=true
```

Add to `.gitlab-ci.yml`:
```yaml
deploy:
  stage: deploy
  id_tokens:
    RISE_TOKEN:
      aud: rise-project-my-project
  script:
    - rise deployment create my-project --image $CI_REGISTRY_IMAGE:$CI_COMMIT_TAG
  only:
    - tags
```

### GitHub Actions

Create service account:
```bash
rise sa create my-app \
  --issuer https://token.actions.githubusercontent.com \
  --claim aud=rise-project-my-app \
  --claim repository=myorg/my-app
```

Add to `.github/workflows/deploy.yml`:
```yaml
name: Deploy
on:
  push:
    branches: [main]

permissions:
  id-token: write
  contents: read

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Get OIDC token
        run: |
          TOKEN=$(curl -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
                       "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=rise-project-my-app" | jq -r .value)
          echo "RISE_TOKEN=$TOKEN" >> $GITHUB_ENV
      - name: Deploy
        run: rise deployment create my-app --image ghcr.io/myorg/my-app:$GITHUB_SHA
```

## Creating Service Accounts

### Command

```bash
rise service-account create <project> \
  --issuer <issuer-url> \
  --claim <key>=<value> \
  [--claim <key>=<value> ...]
```

**Aliases**: `rise sa create`, `rise sa c`, `rise sa new`

### Requirements

⚠️ Every service account must specify:

1. **The `aud` (audience) claim** - Uniquely identifies the service account
2. **At least one additional claim** - For authorization (e.g., `project_path`, `ref_protected`)

**Recommended `aud` format**: `rise-project-{project-name}`

### Why `aud` is Required

The `aud` claim prevents collisions where multiple service accounts match the same token:

Without `aud`:
- Service Account A: `{project_path=myorg/repo}`
- Service Account B: `{project_path=myorg/repo, ref_protected=true}`

A JWT with `{project_path=myorg/repo, ref_protected=true}` would match **both**!

With `aud`, each service account is unique even if other claims overlap.

## Common Use Cases

**Protected branches only** (production):
```bash
rise sa create prod \
  --issuer https://gitlab.com \
  --claim aud=rise-project-prod \
  --claim project_path=myorg/app \
  --claim ref_protected=true
```

**Specific branch** (staging):
```bash
rise sa create staging \
  --issuer https://gitlab.com \
  --claim aud=rise-project-staging \
  --claim project_path=myorg/app \
  --claim ref=refs/heads/staging
```

**Deploy from tags** (releases):
```bash
rise sa create releases \
  --issuer https://gitlab.com \
  --claim aud=rise-project-releases \
  --claim project_path=myorg/app \
  --claim ref_type=tag
```

## Managing Service Accounts

### List

```bash
rise sa list <project>
```

### Show Details

```bash
rise sa show <project> <service-account-id>
```

### Delete

```bash
rise sa delete <project> <service-account-id>
```

⚠️ **Warning**: Deleting a service account will prevent CI/CD pipelines from deploying until a new one is created.

## Collision Detection

Rise detects when multiple service accounts match the same JWT token and rejects authentication:

```
Error 409 Conflict: Multiple service accounts (2) matched this token.
This indicates ambiguous claim configuration.
```

**To fix**:
1. List service accounts: `rise sa list <project>`
2. Make claims more specific
3. Ensure unique `aud` values

**Example fix**:

Before (collision):
```bash
# Service Account 1
--claim aud=gitlab-ci --claim project_path=myorg/app

# Service Account 2 (overlaps with SA1)
--claim aud=gitlab-ci --claim project_path=myorg/app --claim ref_protected=true
```

After (unique):
```bash
# Unprotected branches
--claim aud=rise-project-app-dev --claim project_path=myorg/app --claim ref_protected=false

# Protected branches
--claim aud=rise-project-app-prod --claim project_path=myorg/app --claim ref_protected=true
```

## Available Claims

### GitLab CI

| Claim | Description | Example |
|-------|-------------|---------|
| `project_path` | Full path to project | `myorg/myrepo` |
| `ref` | Git ref being deployed | `refs/heads/main` |
| `ref_type` | Type of ref | `branch`, `tag` |
| `ref_protected` | Whether ref is protected | `true`, `false` |
| `environment` | Environment name | `production` |
| `pipeline_source` | What triggered pipeline | `push`, `web` |

**Documentation**: [GitLab CI/CD ID tokens](https://docs.gitlab.com/ee/ci/secrets/id_token_authentication.html)

### GitHub Actions

| Claim | Description | Example |
|-------|-------------|---------|
| `repository` | Repository full name | `myorg/my-app` |
| `ref` | Git ref | `refs/heads/main` |
| `workflow` | Workflow name | `Deploy` |
| `environment` | Environment name | `production` |
| `actor` | User who triggered | `username` |

**Documentation**: [GitHub OIDC tokens](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/about-security-hardening-with-openid-connect)

## Permissions

Service accounts have **limited permissions**:

✅ **Allowed**:
- Create, view, list, stop, rollback deployments

❌ **Not Allowed**:
- Create/modify/delete projects
- Join teams
- Create service accounts

## Security Best Practices

1. **Use specific claims**
   - ❌ Bad: `--claim project_path=myorg/*`
   - ✅ Good: `--claim project_path=myorg/specific-repo`

2. **Include project in `aud`**
   - ❌ Bad: `--claim aud=gitlab-ci`
   - ✅ Good: `--claim aud=rise-project-backend`

3. **Add `ref_protected` for production**
   ```bash
   rise sa create prod \
     --claim aud=rise-project-prod \
     --claim project_path=myorg/app \
     --claim ref_protected=true
   ```

4. **Use unique `aud` for each service account**

## Troubleshooting

### "The 'aud' claim is required"

Add `--claim aud=<unique-value>`:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo
```

### "At least one claim in addition to 'aud' is required"

Add authorization claims:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo  # Required
```

### "Multiple service accounts matched this token"

Make claims unique. See [Collision Detection](#collision-detection).

### "No service account matched the token claims"

**Debug steps**:
1. **Check token claims** - CI/CD systems usually log available claims
2. **Verify exact match** - Claims are case-sensitive
3. **Check issuer URL** - Must match exactly (no trailing slash)
4. **Ensure all claims present** - ALL service account claims must be in the token

### "403 Forbidden"

Service accounts can only deploy, not manage projects. Use a regular user account for project operations.

## Next Steps

- **GitLab CI setup**: See [Quick Start](#quick-start)
- **GitHub Actions setup**: See [Quick Start](#quick-start)
- **Learn about deployments**: See [Deployments](../core-concepts/deployments.md)
- **CLI reference**: See [CLI Basics](../getting-started/cli-basics.md)
