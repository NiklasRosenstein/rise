# Authentication

Rise uses JWT tokens issued by Dex OAuth2/OIDC provider for user authentication and service accounts for CI/CD workload identity.

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
