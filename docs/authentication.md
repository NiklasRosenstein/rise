# Authentication

Rise uses JWT tokens for user authentication and service accounts for CI/CD workload identity.

## Overview

- **User Authentication**: Rise issues its own HS256 JWTs after validating IdP (Dex) tokens
- **Token Flow**: IdP auth → Rise validates IdP token → Rise issues JWT → Client receives Rise JWT
- **Service Accounts**: CI/CD systems authenticate using OIDC JWT tokens from external issuers

The IdP (Dex) tokens are used internally for group synchronization but are not exposed to users. All user-facing authentication (CLI and UI) uses Rise-issued JWTs.

## User Authentication

### Browser Flow (Default, Recommended)

OAuth2 authorization code flow with PKCE:

```bash
rise login
```

CLI starts local HTTP server (ports 8765-8767), opens browser to Dex, exchanges auth code for JWT token.

### Device Flow (Not Compatible with Dex)

⚠️ Dex's device flow doesn't follow RFC 8628. Use browser flow instead.

### Token Storage

Tokens stored in `~/.config/rise/config.json` (plain JSON; OS-native secure storage planned).

### Backend URL

```bash
rise login --url https://rise.example.com
```

### API Usage

Protected endpoints require `Authorization: Bearer <token>` header (401 if missing/invalid).

### Authentication Endpoints

**Public**: `POST /api/v1/auth/code/exchange` - Exchange auth code for JWT

**Protected**: `GET /api/v1/users/me`, `POST /users/lookup`

## Service Accounts (Workload Identity)

CI/CD systems (GitLab CI, GitHub Actions) authenticate using OIDC JWT tokens.

**Process**: CI generates JWT → Rise validates signature against OIDC issuer → Matches claims → Deploys if matched

**Security**: Short-lived tokens, claim-based authorization, project-scoped access

### Quick Start

**GitLab CI:**
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

**GitHub Actions:**
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

### Creating Service Accounts

```bash
rise sa create <project> \
  --issuer <issuer-url> \
  --claim aud=<value> \
  --claim <key>=<value>
```

**Requirements**: Must specify `aud` claim + at least one additional claim for authorization.

### Common Use Cases

**Protected branches only (production):**
```bash
rise sa create prod \
  --issuer https://gitlab.com \
  --claim aud=rise-project-prod \
  --claim project_path=myorg/app \
  --claim ref_protected=true
```

**Specific branch (staging):**
```bash
rise sa create staging \
  --issuer https://gitlab.com \
  --claim aud=rise-project-staging \
  --claim project_path=myorg/app \
  --claim ref=refs/heads/staging
```

**Deploy from tags (releases):**
```bash
rise sa create releases \
  --issuer https://gitlab.com \
  --claim aud=rise-project-releases \
  --claim project_path=myorg/app \
  --claim ref_type=tag
```

### Available Claims

**GitLab CI**: `project_path`, `ref`, `ref_type`, `ref_protected`, `environment`, `pipeline_source` - [Docs](https://docs.gitlab.com/ee/ci/secrets/id_token_authentication.html)

**GitHub Actions**: `repository`, `ref`, `workflow`, `environment`, `actor` - [Docs](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/about-security-hardening-with-openid-connect)

### Wildcard Patterns in Claims

Service account claims support glob-style wildcard patterns using `*` to match multiple values. This is particularly useful for monorepo CI scenarios where you deploy per-branch apps with dynamic environment names.

**Syntax:**
- `*` matches any sequence of characters (including empty string)
- Unlike filesystem globs, wildcards match across any characters (including `/` and `-`)
- Use exact matching when no wildcard is present (backward compatible)

**Important:** 
- The wildcard `*` matches partial words. For example, `app*` will match both `app-staging` (intended) and `application` (which includes "app" at the start)
- Since `*` can match an empty string, `app*` will also match `app` exactly
- Pattern `app-*` requires the dash, so it matches `app-staging` but NOT `app`
- Design your patterns carefully to avoid unintended matches

**Examples:**

**Match all environments starting with "app":**
```bash
rise sa create my-app \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-app \
  --claim project_path=myorg/myrepo \
  --claim environment=app*
```
Matches: `app` (wildcard matches empty string), `app-mr/6`, `app-staging`  
Also matches: `application`, `app_test` (wildcard matches any continuation)

**Match all production environments:**
```bash
rise sa create prod-services \
  --issuer https://gitlab.com \
  --claim aud=rise-project-prod \
  --claim project_path=myorg/myrepo \
  --claim environment=*-prod
```
Matches: `api-prod`, `web-prod`, `my-service-prod`, etc.

**Match specific pattern with multiple wildcards:**
```bash
rise sa create test-environments \
  --issuer https://gitlab.com \
  --claim aud=rise-project-test \
  --claim project_path=myorg/myrepo \
  --claim environment=app-*-test
```
Matches: `app-staging-test`, `app-mr/6-test`, etc.

**Common patterns for monorepo CI:**
```bash
# Match all merge request environments: app-mr/1, app-mr/2, etc.
--claim environment=app-mr/*

# Match all branch deployments: app-feature-*, app-hotfix-*, etc.
--claim environment=app-*

# Match specific repository branches: myorg/repo/branch-*
--claim ref=refs/heads/feature/*
```

**Note:** You can mix exact and wildcard claims in the same service account. All claims must match for authentication to succeed.

### Managing Service Accounts

```bash
rise sa list <project>
rise sa show <project> <service-account-id>
rise sa delete <project> <service-account-id>
```

**Permissions**: Can create/view/list/stop/rollback deployments. Cannot manage projects/teams/service accounts.

## Troubleshooting

### User Authentication

- **"Failed to start local callback server"**: Ports 8765-8767 in use
- **"Code exchange failed"**: Check backend/Dex logs
- **Token expired**: Run `rise login`

### Service Accounts

- **"The 'aud' claim is required"**: Add `--claim aud=<value>`
- **"At least one additional claim required"**: Add authorization claims (e.g., `project_path`)
- **"Multiple service accounts matched"**: Make claims more specific to avoid ambiguity
- **"No service account matched"**: Check token claims (case-sensitive), verify issuer URL (no trailing slash), ensure ALL claims present
- **"403 Forbidden"**: Service accounts can only deploy, not manage projects
