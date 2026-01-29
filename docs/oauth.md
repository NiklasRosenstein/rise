# OAuth Extension

Rise's Generic OAuth 2.0 extension enables end-user authentication with any OAuth provider (Snowflake, Google, GitHub, custom SSO) without managing client secrets locally.

## Overview

**Key Features:**

- **Generic Provider Support**: Works with any OAuth 2.0 compliant provider
- **Multiple Flow Support**:
  - PKCE (SPAs, RFC 7636-compliant)
  - Token endpoint with client credentials (backend apps, RFC 6749-compliant)
- **Stateless OAuth Proxy**: Rise proxies OAuth flows, clients own their tokens after exchange
- **No Client Secret Exposure**: Secrets stored as encrypted environment variables on Rise
- **Standards Compliant**: RFC 6749 (OAuth 2.0) and RFC 7636 (PKCE) support

**Security Model:**

- Client secrets never leave Rise backend (both upstream OAuth and Rise client credentials)
- OAuth state tokens prevent CSRF attacks
- Authorization codes single-use with 5-minute TTL
- PKCE support for public clients (SPAs) prevents code interception attacks
- Constant-time comparison for all secret validation
- Clients manage token refresh via `/oauth/token` with `grant_type=refresh_token`

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
rise extension show my-app oauth-google --output json | jq -r '.spec.rise_client_id'
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
    `https://api.rise.dev/api/v1/projects/${CONFIG.projectName}/extensions/${CONFIG.extensionName}/oauth/authorize`
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
  const tokenUrl = `https://api.rise.dev/api/v1/projects/${CONFIG.projectName}/extensions/${CONFIG.extensionName}/oauth/token`;
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
    `https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code as string,
        client_id: process.env.OAUTH_RISE_CLIENT_ID_OAUTH_GOOGLE!,
        client_secret: process.env.OAUTH_RISE_CLIENT_SECRET_OAUTH_GOOGLE!
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
            "https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token",
            data={
                "grant_type": "authorization_code",
                "code": code,
                "client_id": os.getenv("OAUTH_RISE_CLIENT_ID_OAUTH_GOOGLE"),
                "client_secret": os.getenv("OAUTH_RISE_CLIENT_SECRET_OAUTH_GOOGLE"),
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
        .post("https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("client_id", &std::env::var("OAUTH_RISE_CLIENT_ID_OAUTH_GOOGLE").unwrap()),
            ("client_secret", &std::env::var("OAUTH_RISE_CLIENT_SECRET_OAUTH_GOOGLE").unwrap()),
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
- **Redirect URI**: Set to `https://api.{your-domain}/api/v1/oauth/callback/{project}/{extension}`

**2. Store Client Secret in Rise**

```bash
rise env set my-app OAUTH_GOOGLE_SECRET "your_client_secret_here" --secret
```

### Creating OAuth Extension

**Generic Provider:**

```bash
rise extension create my-app oauth-provider \
  --type oauth \
  --spec '{
    "provider_name": "My OAuth Provider",
    "description": "OAuth authentication for my app",
    "client_id": "your_client_id",
    "client_secret_ref": "OAUTH_PROVIDER_SECRET",
    "authorization_endpoint": "https://provider.com/oauth/authorize",
    "token_endpoint": "https://provider.com/oauth/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

### Provider-Specific Examples

**Snowflake:**

```bash
rise extension create analytics oauth-snowflake \
  --type oauth \
  --spec '{
    "provider_name": "Snowflake Production",
    "description": "Snowflake OAuth for analytics",
    "client_id": "ABC123XYZ...",
    "client_secret_ref": "OAUTH_SNOWFLAKE_SECRET",
    "authorization_endpoint": "https://myorg.snowflakecomputing.com/oauth/authorize",
    "token_endpoint": "https://myorg.snowflakecomputing.com/oauth/token-request",
    "scopes": ["refresh_token"]
  }'
```

**Google:**

```bash
rise extension create my-app oauth-google \
  --type oauth \
  --spec '{
    "provider_name": "Google",
    "description": "Sign in with Google",
    "client_id": "123456789.apps.googleusercontent.com",
    "client_secret_ref": "OAUTH_GOOGLE_SECRET",
    "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
    "token_endpoint": "https://oauth2.googleapis.com/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

**GitHub:**

```bash
rise extension create my-app oauth-github \
  --type oauth \
  --spec '{
    "provider_name": "GitHub",
    "description": "Sign in with GitHub",
    "client_id": "Iv1.abc123...",
    "client_secret_ref": "OAUTH_GITHUB_SECRET",
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
const authUrl = `https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize?code_challenge=${codeChallenge}&code_challenge_method=S256&redirect_uri=${encodeURIComponent(localCallbackUrl)}`;
window.location.href = authUrl;
```

**Token Endpoint Flow (Backend):**

```ruby
# Initiate OAuth
def login
  redirect_uri = "http://localhost:3000/oauth/callback"
  auth_url = "https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize?redirect_uri=#{CGI.escape(redirect_uri)}"
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
  'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token',
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

**"Invalid authorization_endpoint URL"**
- Ensure URL is valid HTTPS endpoint
- Check provider documentation for correct URL

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
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?code_challenge=...&code_challenge_method=S256
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?code_challenge=...&code_challenge_method=S256&redirect_uri=http://localhost:3000/callback
```

**Token Endpoint Flow (Backend):**

```
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?redirect_uri=http://localhost:3000/callback
```

**Query Parameters:**
- `code_challenge` (optional): PKCE code challenge (base64url-encoded SHA-256 hash of code_verifier)
- `code_challenge_method` (optional): `S256` (default) or `plain`
- `redirect_uri` (optional): Where to redirect after OAuth (localhost or project domain)
- `state` (optional): Application state passed through OAuth flow

### Callback Endpoint

```
GET /api/v1/oauth/callback/{project}/{extension}?code=...&state=...
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
POST /api/v1/projects/{project}/extensions/{extension}/oauth/token
Content-Type: application/x-www-form-urlencoded
```

**Request Parameters (form-urlencoded or JSON):**

**For authorization_code grant (exchange code for tokens):**
- `grant_type` (required): Must be `"authorization_code"`
- `code` (required): Authorization code from callback
- `client_id` (required): Rise client ID from environment variable `OAUTH_RISE_CLIENT_ID_{extension}`
- **Client authentication (choose ONE method - mutually exclusive):**
  - **Confidential clients (backend apps):**
    - `client_secret` (required): Rise client secret from environment variable `OAUTH_RISE_CLIENT_SECRET_{extension}`
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

**Client Credentials:**

Rise automatically generates client credentials when you create an OAuth extension:
- `OAUTH_RISE_CLIENT_ID_{extension}` - Client ID (plaintext, can be public for PKCE flows)
  - Format: `{project-name}-{extension-name}` (deterministic and predictable)
  - Example: For project `my-app` and extension `oauth-google` → `my-app-oauth-google`
- `OAUTH_RISE_CLIENT_SECRET_{extension}` - Client secret (encrypted, random, for confidential clients)

These are available as environment variables in your deployed applications.

**Rise URL Environment Variables:**

Rise injects URL environment variables into deployments for building OAuth URLs dynamically:

| Environment Variable | Purpose | Example |
|---------------------|---------|---------|
| `RISE_PUBLIC_URL` | Browser redirects (authorize endpoint) | `http://localhost:3000` |
| `RISE_API_URL` | Backend-to-backend API calls (token endpoint) | `http://host.minikube.internal:3000` |

Use these instead of hardcoded URLs:

```javascript
// Browser redirect (PKCE authorize URL)
const authUrl = `${process.env.RISE_PUBLIC_URL}/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize`;

// Backend token exchange
const tokenUrl = `${process.env.RISE_API_URL}/api/v1/projects/my-app/extensions/oauth-google/oauth/token`;
```

This separation is important in Kubernetes deployments where the internal URL (`RISE_API_URL`) differs from the public URL (`RISE_PUBLIC_URL`) that browsers can reach.

**Security:**
- Client secret validated with constant-time comparison
- PKCE code_verifier validated against code_challenge (SHA-256 hash)
- Authorization codes are single-use with 5-minute TTL
- All tokens encrypted at rest
