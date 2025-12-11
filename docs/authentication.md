# Authentication

Rise uses JWT tokens issued by Dex OAuth2/OIDC provider for user authentication and service accounts for CI/CD workload identity.

## User Authentication

### Browser Flow (Default, Recommended)

OAuth2 authorization code flow with PKCE (Proof Key for Code Exchange):

```bash
rise login
```

Or explicitly:

```bash
rise login --browser
```

**How it works:**
1. CLI starts a local HTTP server on ports 8765-8767 to receive the OAuth callback
2. Opens browser to Dex authentication page
3. User authenticates with Dex (username/password or other configured methods)
4. Dex redirects to `http://localhost:8765/callback` with authorization code
5. CLI exchanges code with backend at `/auth/code/exchange`
6. Backend validates code with Dex using client credentials and PKCE verifier
7. CLI receives and stores JWT token

**Advantages:**
- Standard OAuth2 flow (RFC 6749 + RFC 7636)
- More secure than password grant (credentials never pass through CLI)
- Fast and user-friendly
- Works reliably with Dex

### Device Flow (Not Compatible with Dex)

OAuth2 device authorization flow:

```bash
rise login --device
```

**⚠️ Warning:** Dex's device flow implementation doesn't follow RFC 8628 properly. It uses a hybrid approach that redirects the browser with an authorization code instead of returning the token via polling, which is incompatible with pure CLI implementations.

**Status:** Not recommended with Dex. Use the browser flow instead.

### Token Storage

Tokens are stored in `~/.config/rise/config.json`:

```json
{
  "backend_url": "http://localhost:3000",
  "token": "eyJhbG..."
}
```

**Security Note:** Tokens are currently stored in plain JSON. Future enhancement planned to use OS-native secure storage (macOS Keychain, Linux libsecret, Windows Credential Manager).

### Backend URL

You can authenticate with a different backend:

```bash
rise login --url https://rise.example.com
```

The URL is saved and used for subsequent commands.

### API Usage

All protected endpoints require `Authorization: Bearer <token>`:

```bash
curl http://localhost:3000/projects \
  -H "Authorization: Bearer YOUR_TOKEN"
```

**Responses:**
- Without token: `401 Unauthorized`
- Invalid/expired token: `401 Unauthorized`
- Valid token: Success response

### Authentication Endpoints

**Public Endpoints (No Authentication Required)**

- `POST /auth/code/exchange` - Exchange authorization code for JWT token
  ```json
  {
    "code": "authorization_code_from_dex",
    "code_verifier": "pkce_verifier",
    "redirect_uri": "http://localhost:8765/callback"
  }
  ```

  Response:
  ```json
  {
    "token": "eyJhbG..."
  }
  ```

**Protected Endpoints (Authentication Required)**

- `GET /me` - Get current user information
  ```json
  {
    "id": "user-uuid",
    "email": "user@example.com"
  }
  ```

- `POST /users/lookup` - Lookup users by email addresses
  ```json
  {
    "emails": ["user1@example.com", "user2@example.com"]
  }
  ```

## Service Accounts (Workload Identity)

Service accounts enable CI/CD systems like GitLab CI and GitHub Actions to authenticate and deploy to Rise projects using OIDC (OpenID Connect) JWT tokens, without requiring user credentials.

### Overview

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
rise service-account create <project> \
  --issuer <issuer-url> \
  --claim <key>=<value> \
  [--claim <key>=<value> ...]
```

**Aliases**: `rise sa create`, `rise sa c`, `rise sa new`

**Requirements:**
⚠️ Every service account must specify:
1. **The `aud` (audience) claim** - Uniquely identifies the service account
2. **At least one additional claim** - For authorization (e.g., `project_path`, `ref_protected`)

**Recommended `aud` format**: `rise-project-{project-name}`

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

**GitLab CI:**
- `project_path` - Full path to project (`myorg/myrepo`)
- `ref` - Git ref being deployed (`refs/heads/main`)
- `ref_type` - Type of ref (`branch`, `tag`)
- `ref_protected` - Whether ref is protected (`true`, `false`)
- `environment` - Environment name (`production`)
- `pipeline_source` - What triggered pipeline (`push`, `web`)

[GitLab CI/CD ID tokens](https://docs.gitlab.com/ee/ci/secrets/id_token_authentication.html)

**GitHub Actions:**
- `repository` - Repository full name (`myorg/my-app`)
- `ref` - Git ref (`refs/heads/main`)
- `workflow` - Workflow name (`Deploy`)
- `environment` - Environment name (`production`)
- `actor` - User who triggered (`username`)

[GitHub OIDC tokens](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/about-security-hardening-with-openid-connect)

### Managing Service Accounts

```bash
# List
rise sa list <project>

# Show details
rise sa show <project> <service-account-id>

# Delete
rise sa delete <project> <service-account-id>
```

⚠️ **Warning**: Deleting a service account will prevent CI/CD pipelines from deploying until a new one is created.

### Permissions

Service accounts have **limited permissions**:

✅ **Allowed**:
- Create, view, list, stop, rollback deployments

❌ **Not Allowed**:
- Create/modify/delete projects
- Join teams
- Create service accounts

## Troubleshooting

### User Authentication

**"Failed to start local callback server"**

The CLI tries to bind to ports 8765, 8766, and 8767. If all are in use:
1. Close applications using these ports
2. Or use device flow (if using a compatible OAuth2 provider): `rise login --device`

**"Code exchange failed"**

Common causes:
1. Backend is not running
2. Dex is not configured properly
3. Network connectivity issues

Check backend and Dex logs for details.

**Token Expired**

Tokens have an expiration time (default: 1 hour). Re-authenticate:

```bash
rise login
```

### Service Accounts

**"The 'aud' claim is required"**

Add `--claim aud=<unique-value>`:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo
```

**"At least one claim in addition to 'aud' is required"**

Add authorization claims:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo  # Required
```

**"Multiple service accounts matched this token"**

Make claims unique:
1. List service accounts: `rise sa list <project>`
2. Make claims more specific
3. Ensure unique `aud` values

Example fix:
```bash
# Unprotected branches
--claim aud=rise-project-app-dev --claim project_path=myorg/app --claim ref_protected=false

# Protected branches
--claim aud=rise-project-app-prod --claim project_path=myorg/app --claim ref_protected=true
```

**"No service account matched the token claims"**

Debug steps:
1. **Check token claims** - CI/CD systems usually log available claims
2. **Verify exact match** - Claims are case-sensitive
3. **Check issuer URL** - Must match exactly (no trailing slash)
4. **Ensure all claims present** - ALL service account claims must be in the token

**"403 Forbidden"**

Service accounts can only deploy, not manage projects. Use a regular user account for project operations.
