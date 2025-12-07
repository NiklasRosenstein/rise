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

### GitLab CI Example

```bash
# Create service account for GitLab CI
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo \
  --claim ref_protected=true

# Output:
#   Created service account:
#     ID:         a1b2c3d4-...
#     Email:      my-project+1@sa.rise.local
#     Project:    my-project
#     Issuer URL: https://gitlab.com
#     Claims:
#       aud: rise-project-my-project
#       project_path: myorg/myrepo
#       ref_protected: true
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
  environment:
    name: production
```

## Creating Service Accounts

### CLI Command

```bash
rise service-account create <project> \
  --issuer <issuer-url> \
  --claim <key>=<value> \
  [--claim <key>=<value> ...]
```

**Aliases**: `rise sa create`, `rise sa c`, `rise sa new`

### Claim Requirements

⚠️ **Important**: Every service account must specify:

1. **The `aud` (audience) claim** - Uniquely identifies the service account
2. **At least one additional claim** - For authorization (e.g., `project_path`, `ref_protected`)

**Recommended `aud` format**: `rise-project-{project-name}`

This convention helps identify which Rise project the service account belongs to, though any unique value is acceptable.

### Examples

**Basic GitLab CI service account**:
```bash
rise sa create backend \
  --issuer https://gitlab.com \
  --claim aud=rise-project-backend \
  --claim project_path=myorg/backend
```

**Protected branches only**:
```bash
rise sa create production \
  --issuer https://gitlab.com \
  --claim aud=rise-project-production \
  --claim project_path=myorg/app \
  --claim ref_protected=true
```

**Specific branch**:
```bash
rise sa create staging \
  --issuer https://gitlab.com \
  --claim aud=rise-project-staging \
  --claim project_path=myorg/app \
  --claim ref=refs/heads/staging
```

**GitHub Actions**:
```bash
rise sa create my-app \
  --issuer https://token.actions.githubusercontent.com \
  --claim aud=rise-project-my-app \
  --claim repository=myorg/my-app
```

### Why `aud` is Required

The `aud` (audience) claim serves as a unique identifier for each service account:

1. **Prevents collisions** - Ensures each service account has a distinct target audience
2. **JWT best practice** - Standard claim for identifying the intended recipient
3. **Clear intent** - Makes it explicit which service account should handle the token

Without `aud`, two service accounts could have overlapping claims that match the same JWT token:
- Service Account A: `{project_path=myorg/repo}`
- Service Account B: `{project_path=myorg/repo, ref_protected=true}`

A JWT with `{project_path=myorg/repo, ref_protected=true, other_claim=value}` would match both!

## Managing Service Accounts

### List Service Accounts

```bash
rise sa list <project>
```

Example output:
```
┌──────────────────────────────────────┬─────────────────────────┬─────────────────────────┬──────────────────────────────────┐
│ ID                                   │ EMAIL                   │ ISSUER URL              │ CLAIMS                           │
├──────────────────────────────────────┼─────────────────────────┼─────────────────────────┼──────────────────────────────────┤
│ a1b2c3d4-...                         │ my-project+1@sa....     │ https://gitlab.com      │ aud=rise-project-my-project,     │
│                                      │                         │                         │ project_path=myorg/myrepo        │
└──────────────────────────────────────┴─────────────────────────┴─────────────────────────┴──────────────────────────────────┘
```

### Show Service Account Details

```bash
rise sa show <project> <service-account-id>
```

### Delete Service Account

```bash
rise sa delete <project> <service-account-id>
```

⚠️ **Warning**: Deleting a service account will prevent CI/CD pipelines from deploying until a new one is created.

## Collision Detection

Rise detects when multiple service accounts match the same JWT token and rejects authentication with an error:

```
Error 409 Conflict: Multiple service accounts (2) matched this token.
This indicates ambiguous claim configuration.
Each service account must have unique claim requirements.
```

This error indicates overlapping claim requirements. To fix:

1. **Review your service accounts**: `rise sa list <project>`
2. **Make claims more specific**: Add additional claims to disambiguate
3. **Use unique `aud` values**: Ensure each service account has a distinct audience

### Example: Fixing Collision

**Problem** - Two service accounts with overlapping claims:
```bash
# Service Account 1 (too broad)
--claim aud=gitlab-ci --claim project_path=myorg/app

# Service Account 2 (subset of SA1)
--claim aud=gitlab-ci --claim project_path=myorg/app --claim ref_protected=true
```

**Solution** - Make each service account unique:
```bash
# Service Account 1 (unprotected branches)
--claim aud=rise-project-app-dev --claim project_path=myorg/app --claim ref_protected=false

# Service Account 2 (protected branches)
--claim aud=rise-project-app-prod --claim project_path=myorg/app --claim ref_protected=true
```

## GitLab CI Integration

### Setup

1. Create service account with GitLab issuer:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo
```

2. Configure `.gitlab-ci.yml`:
```yaml
deploy:
  stage: deploy
  id_tokens:
    RISE_TOKEN:
      aud: rise-project-my-project  # Must match service account aud
  script:
    - curl -fsSL https://rise.example.com/install.sh | bash
    - rise deployment create my-project --image $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA
  environment:
    name: production
```

### Available GitLab Claims

Common claims from GitLab CI JWT tokens:

| Claim | Description | Example |
|-------|-------------|---------|
| `project_path` | Full path to project | `myorg/myrepo` |
| `ref` | Git ref being deployed | `refs/heads/main` |
| `ref_type` | Type of ref | `branch`, `tag` |
| `ref_protected` | Whether ref is protected | `true`, `false` |
| `environment` | Environment name | `production` |
| `pipeline_source` | What triggered pipeline | `push`, `web`, `merge_request_event` |

**Documentation**: [GitLab CI/CD ID tokens](https://docs.gitlab.com/ee/ci/secrets/id_token_authentication.html)

### Examples

**Deploy only from protected branches**:
```bash
rise sa create prod \
  --issuer https://gitlab.com \
  --claim aud=rise-project-prod \
  --claim project_path=myorg/app \
  --claim ref_protected=true
```

**Deploy only from main branch**:
```bash
rise sa create main \
  --issuer https://gitlab.com \
  --claim aud=rise-project-main \
  --claim project_path=myorg/app \
  --claim ref=refs/heads/main
```

**Deploy only from tags**:
```bash
rise sa create releases \
  --issuer https://gitlab.com \
  --claim aud=rise-project-releases \
  --claim project_path=myorg/app \
  --claim ref_type=tag
```

## GitHub Actions Integration

### Setup

1. Create service account with GitHub issuer:
```bash
rise sa create my-app \
  --issuer https://token.actions.githubusercontent.com \
  --claim aud=rise-project-my-app \
  --claim repository=myorg/my-app
```

2. Configure workflow (`.github/workflows/deploy.yml`):
```yaml
name: Deploy to Rise
on:
  push:
    branches: [main]

permissions:
  id-token: write  # Required for OIDC
  contents: read

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Get GitHub OIDC token
        id: oidc
        run: |
          TOKEN=$(curl -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
                       "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=rise-project-my-app" | jq -r .value)
          echo "::add-mask::$TOKEN"
          echo "RISE_TOKEN=$TOKEN" >> $GITHUB_ENV

      - name: Deploy
        run: |
          curl -fsSL https://rise.example.com/install.sh | bash
          rise deployment create my-app --image ghcr.io/myorg/my-app:$GITHUB_SHA
```

### Available GitHub Claims

| Claim | Description | Example |
|-------|-------------|---------|
| `repository` | Repository full name | `myorg/my-app` |
| `ref` | Git ref | `refs/heads/main` |
| `workflow` | Workflow name | `Deploy` |
| `environment` | Environment name | `production` |
| `actor` | User who triggered | `username` |

**Documentation**: [GitHub OIDC tokens](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/about-security-hardening-with-openid-connect)

## Permissions

Service accounts have **limited permissions** compared to regular users:

✅ **Allowed**:
- Create deployments
- View deployment status
- List deployments
- Stop deployments
- Rollback deployments

❌ **Not Allowed**:
- Create, modify, or delete projects
- Join teams
- Create other service accounts
- Modify service account settings

## Security Considerations

### Claim Design Best Practices

1. **Use specific claims rather than broad ones**
   - ❌ Bad: `--claim project_path=myorg/*`
   - ✅ Good: `--claim project_path=myorg/specific-repo`

2. **Include project-specific identifiers in `aud`**
   - ❌ Bad: `--claim aud=gitlab-ci`
   - ✅ Good: `--claim aud=rise-project-backend`

3. **Consider adding `ref_protected` for production deployments**
   ```bash
   rise sa create prod \
     --claim aud=rise-project-prod \
     --claim project_path=myorg/app \
     --claim ref_protected=true
   ```

4. **Use unique `aud` for each service account**
   - Even if other claims overlap, `aud` ensures uniqueness

### Token Lifetime

- OIDC tokens from GitLab/GitHub are short-lived (typically 5-60 minutes)
- Tokens are automatically rotated by the CI/CD system
- No long-lived credentials to manage or rotate manually

### Monitoring

Service account authentication events are logged with:
- Service account ID
- Project ID
- Timestamp
- Matched claims

Check logs for suspicious authentication patterns:
```bash
# Example: View recent service account authentications
journalctl -u rise-backend | grep "Service account authenticated"
```

## API Reference

### Create Service Account

```
POST /projects/{project_name}/workload-identities
Authorization: Bearer <user-token>
```

**Request body**:
```json
{
  "issuer_url": "https://gitlab.com",
  "claims": {
    "aud": "rise-project-my-app",
    "project_path": "myorg/my-app",
    "ref_protected": "true"
  }
}
```

**Response** (201 Created):
```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "email": "my-app+1@sa.rise.local",
  "project_name": "my-app",
  "issuer_url": "https://gitlab.com",
  "claims": {
    "aud": "rise-project-my-app",
    "project_path": "myorg/my-app",
    "ref_protected": "true"
  },
  "created_at": "2025-12-06T10:30:00Z"
}
```

**Error responses**:
- `400 Bad Request` - Missing `aud` claim or invalid claims
- `404 Not Found` - Project not found
- `403 Forbidden` - User cannot manage service accounts for this project

### List Service Accounts

```
GET /projects/{project_name}/workload-identities
Authorization: Bearer <user-token>
```

**Response** (200 OK):
```json
{
  "workload_identities": [
    {
      "id": "a1b2c3d4-...",
      "email": "my-app+1@sa.rise.local",
      "project_name": "my-app",
      "issuer_url": "https://gitlab.com",
      "claims": {...},
      "created_at": "2025-12-06T10:30:00Z"
    }
  ]
}
```

### Get Service Account

```
GET /projects/{project_name}/workload-identities/{sa_id}
Authorization: Bearer <user-token>
```

### Delete Service Account

```
DELETE /projects/{project_name}/workload-identities/{sa_id}
Authorization: Bearer <user-token>
```

**Response**: `204 No Content`

## Troubleshooting

### Error: "The 'aud' claim is required for service accounts"

**Cause**: Trying to create a service account without the `aud` claim.

**Solution**: Add `--claim aud=<unique-value>` to your create command:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo
```

### Error: "At least one claim in addition to 'aud' is required"

**Cause**: Only providing the `aud` claim without any authorization claims.

**Solution**: Add at least one claim for authorization:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo  # Additional claim
```

### Error: "Multiple service accounts matched this token"

**Cause**: Multiple service accounts have overlapping claim requirements.

**Solution**: Make each service account's claims unique:

1. List existing service accounts: `rise sa list <project>`
2. Identify overlapping claims
3. Add distinguishing claims or use different `aud` values

See [Collision Detection](#collision-detection) for detailed examples.

### Error: "No service account matched the provided token claims"

**Cause**: The JWT token from your CI/CD doesn't match any service account's claim requirements.

**Debug steps**:

1. **Check the token claims** - Most CI/CD systems log available claims

   GitLab: Check job logs for `CI_JOB_JWT` claims

   GitHub: Use `echo $ACTIONS_ID_TOKEN_REQUEST_URL`

2. **Verify claim values** - Claims must match exactly
   ```bash
   # Example: GitLab project_path
   Token claim:  project_path=myorg/my-repo
   SA requires:  project_path=myorg/my-repo  ✅ Match

   Token claim:  project_path=myorg/my-repo
   SA requires:  project_path=MyOrg/my-repo  ❌ Case mismatch
   ```

3. **Check issuer URL** - Must match exactly
   ```bash
   # Correct
   --issuer https://gitlab.com

   # Wrong
   --issuer https://gitlab.com/  # Trailing slash
   ```

4. **Review all required claims** - ALL service account claims must be present in the token

### Deployment fails with 403 Forbidden

**Cause**: Service account authenticated successfully but lacks permission for the operation.

**Allowed operations**:
- Create, view, list, stop, rollback deployments

**Not allowed**:
- Modify projects, join teams, create service accounts

**Solution**: Service accounts are intentionally limited. Use a regular user account for project management operations.
