# OAuth Extension

Rise's Generic OAuth 2.0 extension enables end-user authentication with any OAuth provider (Snowflake, Google, GitHub, custom SSO) without managing client secrets locally.

## Overview

**Key Features:**

- **Generic Provider Support**: Works with any OAuth 2.0 compliant provider
- **Multiple Flow Support**:
  - PKCE (SPAs, RFC 7636-compliant)
  - Token endpoint with client credentials (backend apps, RFC 6749-compliant)
- **Stateless OAuth Proxy**: Rise proxies OAuth flows, clients own their tokens after exchange
- **OIDC Discovery Proxy**: Rise proxies OIDC discovery and JWKS endpoints, making apps work in local dev
- **No Client Secret Exposure**: Secrets stored as encrypted environment variables on Rise
- **Standards Compliant**: RFC 6749 (OAuth 2.0) and RFC 7636 (PKCE) support

**Security Model:**

- Client secrets never leave Rise backend (both upstream OAuth and Rise client credentials)
- OAuth state tokens prevent CSRF attacks
- Authorization codes single-use with 5-minute TTL
- PKCE support for public clients (SPAs) prevents code interception attacks
- Constant-time comparison for all secret validation
- Clients manage token refresh via `/oidc/{project}/{extension}/token` with `grant_type=refresh_token`

## OAuth Flows

Rise supports multiple OAuth flows to accommodate different application architectures:

### PKCE Flow (For SPAs)

Best for single-page applications (React, Vue, Angular) using RFC 7636 Proof Key for Code Exchange (PKCE).

**Security:** PKCE prevents authorization code interception attacks by requiring the client to prove it initiated the OAuth flow. No client secret needed (SPAs can't securely store secrets).

**Configuration:**

Rise client IDs are deterministic and follow the pattern `{project-name}-{extension-name}`. You can construct the client ID directly or fetch it from the extension:

```bash
# Option 1: Construct directly (deterministic format)
# For project "my-app" and extension "oauth-google": my-app-oauth-google

# Option 2: Fetch from extension spec (requires Rise auth token)
rise extension show oauth-google -p my-app --output json | jq -r '.spec.rise_client_id'
# Output: "my-app-oauth-google"
```

Add to your build-time configuration:

```javascript
// config.js (or environment variables)
const CONFIG = {
  apiUrl: 'https://api.rise.dev',
  projectName: 'my-app',
  extensionName: 'oauth-google',
  // Client ID is deterministic: {projectName}-{extensionName}
  get riseClientId() {
    return `${this.projectName}-${this.extensionName}`;
  }
};
```

**Usage Example:**

```bash
# Install OAuth library for PKCE helpers
npm install oauth4webapi
```

```javascript
import * as oauth from 'oauth4webapi';

// 1. Initiate OAuth login with PKCE
async function login() {
  // Generate PKCE code verifier and challenge
  const codeVerifier = oauth.generateRandomCodeVerifier();
  const codeChallenge = await oauth.calculatePKCECodeChallenge(codeVerifier);
  sessionStorage.setItem('pkce_verifier', codeVerifier);

  // Build authorization URL
  const authUrl = new URL(
    `https://api.rise.dev/oidc/${CONFIG.projectName}/${CONFIG.extensionName}/authorize`
  );
  authUrl.searchParams.set('code_challenge', codeChallenge);
  authUrl.searchParams.set('code_challenge_method', 'S256');

  window.location.href = authUrl;
}

// 2. After callback, exchange code for tokens
async function handleCallback() {
  const urlParams = new URLSearchParams(window.location.search);
  const code = urlParams.get('code');
  const codeVerifier = sessionStorage.getItem('pkce_verifier');

  if (!code || !codeVerifier) {
    throw new Error('Missing code or verifier');
  }

  // Exchange code for tokens
  const tokenUrl = `https://api.rise.dev/oidc/${CONFIG.projectName}/${CONFIG.extensionName}/token`;
  const response = await fetch(tokenUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'authorization_code',
      code: code,
      client_id: CONFIG.riseClientId,
      code_verifier: codeVerifier
    })
  });

  if (!response.ok) {
    const error = await response.json();
    throw new Error(`OAuth error: ${error.error}`);
  }

  const tokens = await response.json();
  // { access_token, token_type, expires_in, refresh_token, scope, id_token }

  // Store tokens securely
  localStorage.setItem('oauth_tokens', JSON.stringify(tokens));
  sessionStorage.removeItem('pkce_verifier');

  return tokens;
}
```

### Token Endpoint Flow (For Backend Apps)

Best for server-rendered applications (Ruby on Rails, Django, Express) where tokens should be handled server-side.

**Security:** Authorization code (5-min TTL, single-use) passed in query param, backend exchanges for tokens via Rise's token endpoint.

```
┌──────────────┐                                           ┌──────────────┐
│              │  1. GET /oauth/authorize                  │              │
│   Browser    │──────────────────────────────────────────>│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
                                                                   │
                  2. Generate state token                         │
                     Store in cache: { redirect_uri, PKCE }       │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│              │  3. Redirect to OAuth Provider            │              │
│   Browser    │<──────────────────────────────────────────│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
       │
       │  4. User authenticates
       v
┌──────────────┐
│    OAuth     │
│   Provider   │
└──────────────┘
       │
       │  5. Redirect to callback
       v
┌──────────────┐                                           ┌──────────────┐
│              │  6. GET /oauth/callback?code=...&state=...│              │
│   Browser    │──────────────────────────────────────────>│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
                                                                   │
                  7. Exchange upstream code for tokens            │
                     Encrypt tokens                               │
                     Generate authorization code                  │
                     Store in cache (5-min TTL, single-use)       │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│              │  8. Redirect with authorization code      │              │
│   Browser    │<──────?code=abc123────────────────────────│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
       │
       │  9. Pass code to backend
       v
┌──────────────┐
│     App      │  10. POST /oauth/token (grant_type=authorization_code)
│   Backend    │──────────────────────────────────────────>┌──────────────┐
└──────────────┘                                           │     Rise     │
                                                           │   Backend    │
                  11. Validate code (single-use)           └──────────────┘
                      Decrypt and return tokens                   │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│     App      │  12. Return OAuth tokens                  │              │
│   Backend    │<──────────────────────────────────────────│     Rise     │
└──────────────┘                                           │   Backend    │
       │                                                   └──────────────┘
       │  13. Store tokens in session (HttpOnly cookie)
       │      Client owns and manages refresh
       v
```

**Usage Examples:**

```typescript
// TypeScript (Express)
app.get('/oauth/callback', async (req, res) => {
  const { code } = req.query;

  const tokens = await fetch(
    `https://api.rise.dev/oidc/my-app/oauth-google/token`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code as string,
        client_id: process.env.OAUTH_GOOGLE_CLIENT_ID!,
        client_secret: process.env.OAUTH_GOOGLE_CLIENT_SECRET!
      })
    }
  ).then(r => r.json());

  req.session.tokens = tokens;  // Store in HttpOnly session
  res.redirect('/');
});
```

```python
# Python (FastAPI)
import httpx
from fastapi import FastAPI, Request

@app.get("/oauth/callback")
async def callback(code: str, request: Request):
    async with httpx.AsyncClient() as client:
        response = await client.post(
            "https://api.rise.dev/oidc/my-app/oauth-google/token",
            data={
                "grant_type": "authorization_code",
                "code": code,
                "client_id": os.getenv("OAUTH_GOOGLE_CLIENT_ID"),
                "client_secret": os.getenv("OAUTH_GOOGLE_CLIENT_SECRET"),
            }
        )
        tokens = response.json()

    request.session["tokens"] = tokens  # Store in session
    return RedirectResponse("/")
```

```rust
// Rust (Axum)
use axum::{extract::Query, response::Redirect};
use serde::Deserialize;

#[derive(Deserialize)]
struct Callback { code: String }

async fn oauth_callback(Query(params): Query<Callback>) -> Redirect {
    let client = reqwest::Client::new();
    let tokens: serde_json::Value = client
        .post("https://api.rise.dev/oidc/my-app/oauth-google/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("client_id", &std::env::var("OAUTH_GOOGLE_CLIENT_ID").unwrap()),
            ("client_secret", &std::env::var("OAUTH_GOOGLE_CLIENT_SECRET").unwrap()),
        ])
        .send()
        .await.unwrap()
        .json()
        .await.unwrap();

    // Store tokens in session (implementation depends on session middleware)
    Redirect::to("/")
}
```

## Configuration

### Prerequisites

**1. Register OAuth Application with Provider**

Obtain client credentials from your OAuth provider:
- **Client ID**: Public identifier
- **Client Secret**: Secret key (never expose in frontend)
- **Redirect URI**: Set to `https://api.{your-domain}/oidc/{project}/{extension}/callback`

**2. Determine Provider Type**

Rise supports two types of OAuth providers:

| Provider Type | issuer_url | Endpoints |
|--------------|-----------|-----------|
| **OIDC-compliant** (Google, Dex, Auth0) | Required | Auto-discovered via `{issuer_url}/.well-known/openid-configuration` |
| **Non-OIDC** (GitHub, Snowflake) | Required | Must set `authorization_endpoint` and `token_endpoint` manually |

**3. Store Client Secret in Rise**

There are two ways to store the OAuth provider's client secret:

**Option A: Encrypted in Extension Spec (Recommended)**

Encrypt the secret and store it directly in the extension spec:

```bash
# Encrypt the secret
ENCRYPTED=$(rise encrypt "your_client_secret_here")

# Use in extension spec (assuming rise.toml has project = "my-app")
rise extension create oauth-provider \
  --type oauth \
  --spec '{
    "provider_name": "My OAuth Provider",
    "client_id": "your_client_id",
    "client_secret_encrypted": "'"$ENCRYPTED"'",
    "issuer_url": "https://accounts.google.com",
    "scopes": ["openid", "email", "profile"]
  }'

# Or specify project explicitly with -p flag:
# rise extension create oauth-provider -p my-app --type oauth --spec '{...}'
```

Or encrypt via stdin:

```bash
echo "your_client_secret_here" | rise encrypt
```

The `rise encrypt` command is rate-limited to 100 requests per hour per user.

**Option B: Environment Variable Reference (Legacy)**

Store the secret as an encrypted environment variable and reference it:

```bash
# Store as environment variable
rise env set my-app OAUTH_GOOGLE_SECRET "your_client_secret_here" --secret

# Reference in extension spec
rise extension create oauth-provider -p my-app \
  --type oauth \
  --spec '{
    "provider_name": "My OAuth Provider",
    "client_id": "your_client_id",
    "client_secret_ref": "OAUTH_GOOGLE_SECRET",
    "issuer_url": "https://accounts.google.com",
    "scopes": ["openid", "email", "profile"]
  }'
```

**Which should you use?**

- **New extensions**: Use `client_secret_encrypted` (Option A) for simpler configuration
- **Existing extensions**: Continue using `client_secret_ref` (Option B) - it will be supported indefinitely
- **Migration**: Optional - both patterns work identically

### Creating OAuth Extension

**OIDC-Compliant Provider (Google, Dex, Auth0):**

For OIDC-compliant providers, only `issuer_url` is needed - endpoints are auto-discovered:

```bash
# Encrypt the client secret
ENCRYPTED=$(rise encrypt "your_client_secret_here")

# Create extension - endpoints auto-discovered via OIDC
rise extension create oauth-google -p my-app \
  --type oauth \
  --spec '{
    "provider_name": "Google",
    "description": "Sign in with Google",
    "client_id": "123456789.apps.googleusercontent.com",
    "client_secret_encrypted": "'"$ENCRYPTED"'",
    "issuer_url": "https://accounts.google.com",
    "scopes": ["openid", "email", "profile"]
  }'
```

**Non-OIDC Provider (GitHub, Snowflake):**

For non-OIDC providers, also set `authorization_endpoint` and `token_endpoint`:

```bash
# Encrypt the client secret
ENCRYPTED=$(rise encrypt "your_client_secret_here")

# Create extension with manual endpoints
rise extension create oauth-github -p my-app \
  --type oauth \
  --spec '{
    "provider_name": "GitHub",
    "description": "Sign in with GitHub",
    "client_id": "Iv1.abc123...",
    "client_secret_encrypted": "'"$ENCRYPTED"'",
    "issuer_url": "https://github.com",
    "authorization_endpoint": "https://github.com/login/oauth/authorize",
    "token_endpoint": "https://github.com/login/oauth/access_token",
    "scopes": ["read:user", "user:email"]
  }'
```

### Provider-Specific Examples

**Google (OIDC-compliant):**

```bash
rise extension create oauth-google -p my-app \
  --type oauth \
  --spec '{
    "provider_name": "Google",
    "description": "Sign in with Google",
    "client_id": "123456789.apps.googleusercontent.com",
    "client_secret_ref": "OAUTH_GOOGLE_SECRET",
    "issuer_url": "https://accounts.google.com",
    "scopes": ["openid", "email", "profile"]
  }'
```

**Snowflake (non-OIDC):**

```bash
rise extension create oauth-snowflake -p analytics \
  --type oauth \
  --spec '{
    "provider_name": "Snowflake Production",
    "description": "Snowflake OAuth for analytics",
    "client_id": "ABC123XYZ...",
    "client_secret_ref": "OAUTH_SNOWFLAKE_SECRET",
    "issuer_url": "https://myorg.snowflakecomputing.com",
    "authorization_endpoint": "https://myorg.snowflakecomputing.com/oauth/authorize",
    "token_endpoint": "https://myorg.snowflakecomputing.com/oauth/token-request",
    "scopes": ["refresh_token"]
  }'
```

**GitHub (non-OIDC):**

```bash
rise extension create oauth-github -p my-app \
  --type oauth \
  --spec '{
    "provider_name": "GitHub",
    "description": "Sign in with GitHub",
    "client_id": "Iv1.abc123...",
    "client_secret_ref": "OAUTH_GITHUB_SECRET",
    "issuer_url": "https://github.com",
    "authorization_endpoint": "https://github.com/login/oauth/authorize",
    "token_endpoint": "https://github.com/login/oauth/access_token",
    "scopes": ["read:user", "user:email"]
  }'
```

## Local Development

For local development, pass a `redirect_uri` parameter to redirect back to localhost:

**PKCE Flow (SPA):**

```javascript
// Generate PKCE verifier and challenge
const codeVerifier = generateRandomString(128);
const codeChallenge = await sha256Base64Url(codeVerifier);
sessionStorage.setItem('pkce_verifier', codeVerifier);

// Initiate OAuth with PKCE
const localCallbackUrl = 'http://localhost:3000/oauth/callback';
const authUrl = `https://api.rise.dev/oidc/my-app/oauth-google/authorize?code_challenge=${codeChallenge}&code_challenge_method=S256&redirect_uri=${encodeURIComponent(localCallbackUrl)}`;
window.location.href = authUrl;
```

**Token Endpoint Flow (Backend):**

```ruby
# Initiate OAuth
def login
  redirect_uri = "http://localhost:3000/oauth/callback"
  auth_url = "https://api.rise.dev/oidc/my-app/oauth-google/authorize?redirect_uri=#{CGI.escape(redirect_uri)}"
  redirect_to auth_url
end
```

**Redirect URI Validation:**

Rise only allows redirects to:
- Localhost URLs (any port) - for local development
- Project domain (e.g., `https://my-app.rise.dev`) - for production

## Security Considerations

### PKCE Flow Security

**Why PKCE for SPAs?**

PKCE (Proof Key for Code Exchange, RFC 7636) provides additional security for public clients:

1. **Code Interception Protection**: Prevents attackers from stealing authorization codes
2. **No Client Secret Needed**: SPAs can't securely store secrets - PKCE solves this
3. **Standards-Based**: Works with any RFC 7636-compliant OAuth provider
4. **Code Verifier Challenge**: Client proves it initiated the flow by providing the verifier

### Token Endpoint Flow Security

**Why authorization codes for backend apps?**

Authorization code flow with client credentials provides security for confidential clients:

1. **Short-lived codes**: 5-minute TTL reduces exposure window
2. **Single-use**: Codes invalidated immediately after exchange
3. **Client authentication**: Backend proves identity with client_secret
4. **Backend-only**: Real tokens never touch browser
5. **HttpOnly cookies**: Can store tokens in cookies inaccessible to JavaScript (XSS protection)

### Token Storage

**Rise Platform:**
- Client secrets: Encrypted environment variables (AES-GCM or AWS KMS)
- OAuth state: In-memory cache (10-minute TTL)
- Authorization codes: In-memory cache with encrypted tokens (5-minute TTL, single-use)

**Application (after token exchange, clients own their tokens):**
- **SPAs (PKCE Flow)**:
  - Memory (best security, lost on refresh)
  - localStorage (persistent, vulnerable to XSS)
  - Never use cookies (sent with all requests, CSRF risk)

- **Backend Apps (Token Endpoint Flow)**:
  - HttpOnly cookies (best security, XSS-safe)
  - Backend session store (Redis, database)
  - Never expose to frontend

### CSRF Protection

All OAuth flows include CSRF protection via state tokens:

1. Rise generates random state token
2. Stores in cache with flow context
3. Passes to OAuth provider
4. Validates on callback
5. Rejects mismatched/expired states

### Token Refresh

Clients manage their own token refresh by calling the `/oauth/token` endpoint:

```javascript
const response = await fetch(
  'https://api.rise.dev/oidc/my-app/oauth-google/token',
  {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'refresh_token',
      refresh_token: storedRefreshToken,
      client_id: clientId,
      client_secret: clientSecret  // or omit for PKCE flows
    })
  }
);
const newTokens = await response.json();
```

Rise proxies the refresh request to the upstream OAuth provider and returns fresh tokens.

## Troubleshooting

**"Environment variable 'OAUTH_XXX_SECRET' not found"**
- Store client secret: `rise env set <project> OAUTH_XXX_SECRET "secret" --secret`

**"Failed to resolve OAuth endpoints"** or **"No authorization_endpoint in spec or OIDC discovery"**
- For OIDC-compliant providers: Ensure `issuer_url` is correct and provider supports OIDC discovery
- For non-OIDC providers (GitHub, Snowflake): Set `authorization_endpoint` and `token_endpoint` manually
- Test OIDC discovery: `curl {issuer_url}/.well-known/openid-configuration`

**"Invalid issuer_url URL"**
- Ensure URL is valid HTTPS endpoint
- Don't include trailing slash or paths (e.g., `https://accounts.google.com`, not `https://accounts.google.com/`)

**"Token exchange failed with status 400"**
- Verify `client_id` and `client_secret_ref` are correct
- Check redirect URI matches OAuth provider configuration
- Review OAuth provider logs for specific error

**"No cached state found for state token"**
- State token expired (10-minute TTL)
- Restart OAuth flow from beginning

**"Invalid or expired authorization code"**
- Authorization code already used (single-use)
- Authorization code expired (5-minute TTL)
- Request new authorization

## API Reference

### Authorization Endpoint

**PKCE Flow (SPA):**

```
GET /oidc/{project}/{extension}/authorize?code_challenge=...&code_challenge_method=S256
GET /oidc/{project}/{extension}/authorize?code_challenge=...&code_challenge_method=S256&redirect_uri=http://localhost:3000/callback
```

**Token Endpoint Flow (Backend):**

```
GET /oidc/{project}/{extension}/authorize
GET /oidc/{project}/{extension}/authorize?redirect_uri=http://localhost:3000/callback
```

**Query Parameters:**
- `code_challenge` (optional): PKCE code challenge (base64url-encoded SHA-256 hash of code_verifier)
- `code_challenge_method` (optional): `S256` (default) or `plain`
- `redirect_uri` (optional): Where to redirect after OAuth (localhost or project domain)
- `state` (optional): Application state passed through OAuth flow

### Callback Endpoint

```
GET /oidc/{project}/{extension}/callback?code=...&state=...
```

**Response:**
```
HTTP/1.1 302 Found
Location: https://my-app.rise.dev/callback?code=abc123...
```

The `code` parameter is an authorization code that can be exchanged for tokens at the token endpoint.

### Token Endpoint (RFC 6749-compliant)

**Recommended:** This is the standards-compliant OAuth 2.0 token endpoint.

```
POST /oidc/{project}/{extension}/token
Content-Type: application/x-www-form-urlencoded
```

**Request Parameters (form-urlencoded or JSON):**

**For authorization_code grant (exchange code for tokens):**
- `grant_type` (required): Must be `"authorization_code"`
- `code` (required): Authorization code from callback
- `client_id` (required): Rise client ID from environment variable `{EXTENSION}_CLIENT_ID`
- **Client authentication (choose ONE method - mutually exclusive):**
  - **Confidential clients (backend apps):**
    - `client_secret` (required): Rise client secret from environment variable `{EXTENSION}_CLIENT_SECRET`
  - **Public clients (SPAs with PKCE):**
    - `code_verifier` (required): PKCE code verifier (proves client initiated the flow)
  - **Note:** Providing both `client_secret` and `code_verifier` will result in an `invalid_request` error

**For refresh_token grant (refresh access token):**
- `grant_type` (required): Must be `"refresh_token"`
- `refresh_token` (required): Refresh token from previous token response
- `client_id` (required): Rise client ID
- `client_secret` (required): Rise client secret (confidential clients)

**Response (RFC 6749 format):**
```json
{
  "access_token": "eyJhbGc...",
  "token_type": "Bearer",
  "expires_in": 3600,  // Seconds from now (not timestamp)
  "refresh_token": "eyJhbGc...",  // Optional
  "scope": "email profile",  // Optional, space-delimited
  "id_token": "eyJhbGc..."  // Optional, OIDC
}
```

**Error Response (RFC 6749 format):**
```json
{
  "error": "invalid_grant",
  "error_description": "Invalid or expired authorization code"
}
```

**Error Codes:**
- `invalid_request` (400): Missing or invalid parameters, or both `client_secret` and `code_verifier` provided
- `invalid_client` (401): Invalid client_id or client_secret
- `invalid_grant` (400): Invalid/expired code, or PKCE validation failed
- `unsupported_grant_type` (400): Unknown grant_type
- `server_error` (500): Internal server error

### OIDC Discovery Endpoint

Rise provides an OIDC-compliant discovery endpoint that proxies from the upstream provider and rewrites URLs to point to Rise's OIDC proxy.

```
GET /oidc/{project}/{extension}/.well-known/openid-configuration
```

**Response:** Standard OIDC discovery document with Rise URLs:

```json
{
  "issuer": "https://api.rise.dev/oidc/my-app/oauth-google",
  "authorization_endpoint": "https://api.rise.dev/oidc/my-app/oauth-google/authorize",
  "token_endpoint": "https://api.rise.dev/oidc/my-app/oauth-google/token",
  "jwks_uri": "https://api.rise.dev/oidc/my-app/oauth-google/jwks",
  "...": "other fields from upstream provider"
}
```

### JWKS Endpoint

Rise proxies the JWKS (JSON Web Key Set) from the upstream OAuth provider.

```
GET /oidc/{project}/{extension}/jwks
```

**Response:** JWKS from upstream provider for id_token signature validation.

**Client Credentials:**

Rise automatically generates client credentials when you create an OAuth extension:
- `{EXTENSION}_CLIENT_ID` - Client ID (plaintext, can be public for PKCE flows)
  - Format: `{project-name}-{extension-name}` (deterministic and predictable)
  - Example: For project `my-app` and extension `oauth-google` → `my-app-oauth-google`
  - Env var: `OAUTH_GOOGLE_CLIENT_ID` (for extension named `oauth-google`)
- `{EXTENSION}_CLIENT_SECRET` - Client secret (encrypted, random, for confidential clients)
  - Env var: `OAUTH_GOOGLE_CLIENT_SECRET` (for extension named `oauth-google`)
- `{EXTENSION}_ISSUER` - Rise OIDC proxy URL for id_token validation via JWKS discovery
  - Env var: `OAUTH_GOOGLE_ISSUER` (for extension named `oauth-google`)
  - Points to Rise's OIDC proxy: `{RISE_PUBLIC_URL}/oidc/{project}/{extension}`
  - Proxies OIDC discovery and JWKS from upstream provider with URLs rewritten to Rise endpoints

These are available as environment variables in your deployed applications. The extension name is normalized to uppercase with hyphens replaced by underscores.

**Rise URL Environment Variables:**

Rise injects URL environment variables into deployments for building OAuth URLs dynamically:

| Environment Variable | Purpose | Example |
|---------------------|---------|---------|
| `RISE_ISSUER` | Rise server URL (base URL for all Rise endpoints) | `http://localhost:3000` |

Use this for all Rise endpoints:

```javascript
// Browser redirect (PKCE authorize URL)
const authUrl = `${process.env.RISE_ISSUER}/oidc/my-app/oauth-google/authorize`;

// Backend token exchange
const tokenUrl = `${process.env.RISE_ISSUER}/oidc/my-app/oauth-google/token`;

// OpenID configuration (for JWT validation)
const configUrl = `${process.env.RISE_ISSUER}/.well-known/openid-configuration`;
```

**Security:**
- Client secret validated with constant-time comparison
- PKCE code_verifier validated against code_challenge (SHA-256 hash)
- Authorization codes are single-use with 5-minute TTL
- All tokens encrypted at rest
