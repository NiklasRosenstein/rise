# OAuth Extension

Rise's Generic OAuth 2.0 extension enables end-user authentication with any OAuth provider (Snowflake, Google, GitHub, custom SSO) without managing client secrets locally.

## Overview

**Key Features:**

- **Generic Provider Support**: Works with any OAuth 2.0 compliant provider
- **Multiple Flow Support**:
  - PKCE (SPAs, RFC 7636-compliant)
  - Token endpoint with client credentials (backend apps, RFC 6749-compliant)
- **Secure Token Storage**: Encrypted user tokens with automatic refresh
- **Session Management**: Browser cookie-based session tracking
- **No Client Secret Exposure**: Secrets stored as encrypted environment variables on Rise
- **Standards Compliant**: RFC 6749 (OAuth 2.0) and RFC 7636 (PKCE) support

**Security Model:**

- Client secrets never leave Rise backend (both upstream OAuth and Rise client credentials)
- User tokens encrypted at rest in database
- OAuth state tokens prevent CSRF attacks
- Authorization codes single-use with 5-minute TTL
- Session-based token caching per user
- PKCE support for public clients (SPAs) prevents code interception attacks
- Constant-time comparison for all secret validation

## OAuth Flows

Rise supports multiple OAuth flows to accommodate different application architectures:

### PKCE Flow (For SPAs)

Best for single-page applications (React, Vue, Angular) using RFC 7636 Proof Key for Code Exchange (PKCE).

**Security:** PKCE prevents authorization code interception attacks by requiring the client to prove it initiated the OAuth flow. No client secret needed (SPAs can't securely store secrets).

**Usage Example:**

```javascript
// 1. Generate PKCE verifier and challenge
function generateRandomString(length) {
  const charset = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';
  const randomValues = new Uint8Array(length);
  crypto.getRandomValues(randomValues);
  return Array.from(randomValues)
    .map(v => charset[v % charset.length])
    .join('');
}

async function sha256(plain) {
  const encoder = new TextEncoder();
  const data = encoder.encode(plain);
  return await crypto.subtle.digest('SHA-256', data);
}

function base64urlEncode(arrayBuffer) {
  const bytes = new Uint8Array(arrayBuffer);
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=/g, '');
}

async function generatePKCE() {
  const codeVerifier = generateRandomString(128);
  const hashed = await sha256(codeVerifier);
  const codeChallenge = base64urlEncode(hashed);
  return { codeVerifier, codeChallenge };
}

// 2. Initiate OAuth login with PKCE
async function login() {
  const { codeVerifier, codeChallenge } = await generatePKCE();

  // Store verifier for later use
  sessionStorage.setItem('pkce_verifier', codeVerifier);

  // Start OAuth flow with code_challenge
  const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize' +
    `?code_challenge=${codeChallenge}&code_challenge_method=S256`;

  window.location.href = authUrl;
}

// 3. After callback, exchange code for tokens
async function handleCallback() {
  const urlParams = new URLSearchParams(window.location.search);
  const code = urlParams.get('code');  // Authorization code from callback
  const verifier = sessionStorage.getItem('pkce_verifier');

  if (!code || !verifier) {
    throw new Error('Missing code or verifier');
  }

  // Exchange code for tokens using PKCE
  const response = await fetch(
    'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: 'OAUTH_RISE_CLIENT_ID_oauth-google',  // Can be public
        code_verifier: verifier
      })
    }
  );

  if (!response.ok) {
    const error = await response.json();
    throw new Error(`OAuth error: ${error.error} - ${error.error_description}`);
  }

  const tokens = await response.json();
  // tokens = {
  //   access_token: "...",
  //   token_type: "Bearer",
  //   expires_in: 3600,  // seconds
  //   refresh_token: "...",
  //   scope: "email profile",
  //   id_token: "..."
  // }

  // Store tokens
  localStorage.setItem('oauth_tokens', JSON.stringify(tokens));

  // Clean up
  sessionStorage.removeItem('pkce_verifier');
  window.history.replaceState(null, '', window.location.pathname);

  return tokens;
}
```

### Token Endpoint Flow (For Backend Apps)

Best for server-rendered applications (Ruby on Rails, Django, Express) where tokens should be handled server-side.

**Security:** Temporary exchange token (5-min TTL, single-use) passed in query param, backend exchanges for real tokens securely.

```
┌──────────────┐                                           ┌──────────────┐
│              │  1. GET /oauth/authorize?flow=exchange    │              │
│   Browser    │──────────────────────────────────────────>│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
                                                                   │
                  2. Generate state token                         │
                     Store in cache:                              │
                     { redirect_uri, session_id, flow: Exchange } │
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
                  7. Exchange code for tokens                     │
                     Encrypt + store in database                  │
                     Generate exchange token                      │
                     Store in cache (5-min TTL, single-use)       │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│              │  8. Redirect with exchange token          │              │
│   Browser    │<──────?exchange_token=abc123──────────────│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
       │
       │  9. Pass exchange token to backend
       v
┌──────────────┐
│     App      │  10. POST to Rise with exchange token
│   Backend    │──────────────────────────────────────────>┌──────────────┐
└──────────────┘                                           │     Rise     │
                                                           │   Backend    │
                  11. Validate exchange token (single-use) └──────────────┘
                      Invalidate immediately                      │
                      Retrieve + decrypt tokens                   │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│     App      │  12. Return OAuth credentials             │              │
│   Backend    │<──────────────────────────────────────────│     Rise     │
└──────────────┘                                           │   Backend    │
       │                                                   └──────────────┘
       │  13. Store in session (HttpOnly cookie)
       │      or backend database
       v
```

**Usage Example (RFC 6749-compliant with client credentials):**

```ruby
# Rails controller
class OAuthController < ApplicationController
  def callback
    # Extract authorization code from query params
    code = params[:code]  # This is the authorization code, not an exchange token

    # Get Rise client credentials from environment
    client_id = ENV['OAUTH_RISE_CLIENT_ID_oauth-google']
    client_secret = ENV['OAUTH_RISE_CLIENT_SECRET_oauth-google']

    # Exchange authorization code for tokens using RFC 6749-compliant endpoint
    response = HTTParty.post(
      "https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token",
      body: {
        grant_type: 'authorization_code',
        code: code,
        client_id: client_id,
        client_secret: client_secret
      }
    )

    if response.code != 200
      error = JSON.parse(response.body)
      Rails.logger.error("OAuth token exchange failed: #{error['error']} - #{error['error_description']}")
      redirect_to root_path, alert: 'Authentication failed'
      return
    end

    tokens = JSON.parse(response.body)
    # tokens = {
    #   "access_token" => "...",
    #   "token_type" => "Bearer",
    #   "expires_in" => 3600,  # seconds from now
    #   "refresh_token" => "...",
    #   "scope" => "email profile",
    #   "id_token" => "..."
    # }

    # Store in session (HttpOnly cookie)
    session[:oauth_access_token] = tokens['access_token']
    session[:oauth_expires_at] = Time.now + tokens['expires_in'].seconds
    session[:oauth_refresh_token] = tokens['refresh_token']

    # Redirect to app
    redirect_to root_path
  end
end
```

```javascript
// Express/Node.js
app.get('/oauth/callback', async (req, res) => {
  const { code } = req.query;  // Authorization code from callback

  // Get Rise client credentials from environment
  const clientId = process.env.OAUTH_RISE_CLIENT_ID_oauth_google;
  const clientSecret = process.env.OAUTH_RISE_CLIENT_SECRET_oauth_google;

  // Exchange authorization code for tokens
  const response = await fetch(
    'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: clientId,
        client_secret: clientSecret
      })
    }
  );

  if (!response.ok) {
    const error = await response.json();
    console.error(`OAuth token exchange failed: ${error.error} - ${error.error_description}`);
    return res.redirect('/?error=auth_failed');
  }

  const tokens = await response.json();
  // tokens = {
  //   access_token: "...",
  //   token_type: "Bearer",
  //   expires_in: 3600,  // seconds from now
  //   refresh_token: "...",
  //   scope: "email profile",
  //   id_token: "..."
  // }

  // Store in session
  req.session.oauthAccessToken = tokens.access_token;
  req.session.oauthExpiresAt = new Date(Date.now() + tokens.expires_in * 1000);
  req.session.oauthRefreshToken = tokens.refresh_token;

  res.redirect('/');
});
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
- User tokens: Encrypted at rest in PostgreSQL
- OAuth state: In-memory cache (10-minute TTL)
- Exchange tokens: In-memory cache (5-minute TTL)

**Application:**
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
2. Stores in cache with session context
3. Passes to OAuth provider
4. Validates on callback
5. Rejects mismatched/expired states

### Token Refresh

Rise automatically refreshes expired tokens when `refresh_token` is available:

1. Check `expires_at` before using token
2. If expired, use `refresh_token` to get new `access_token`
3. Update database with new tokens
4. Return fresh credentials

**Background job** (future enhancement): Proactively refresh tokens before expiration.

### Token Cleanup

**Inactive token cleanup** (future enhancement):

Tokens unused for 30+ days automatically deleted to reduce attack surface.

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

**"Invalid exchange token"**
- Exchange token already used (single-use)
- Exchange token expired (5-minute TTL)
- Request new authorization

**"Could not find user OAuth token"**
- Session cookie missing or expired
- User has not completed OAuth flow
- Token may have been cleaned up due to inactivity

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
- **Confidential clients (backend apps):**
  - `client_secret` (required): Rise client secret from environment variable `OAUTH_RISE_CLIENT_SECRET_{extension}`
- **Public clients (SPAs with PKCE):**
  - `code_verifier` (required): PKCE code verifier (proves client initiated the flow)

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
- `invalid_request` (400): Missing or invalid parameters
- `invalid_client` (401): Invalid client_id or client_secret
- `invalid_grant` (400): Invalid/expired code, or PKCE validation failed
- `unsupported_grant_type` (400): Unknown grant_type
- `server_error` (500): Internal server error

**Client Credentials:**

Rise automatically generates client credentials when you create an OAuth extension:
- `OAUTH_RISE_CLIENT_ID_{extension}` - Client ID (plaintext, can be public for PKCE flows)
- `OAUTH_RISE_CLIENT_SECRET_{extension}` - Client secret (encrypted, for confidential clients)

These are available as environment variables in your deployed applications.

**Security:**
- Client secret validated with constant-time comparison
- PKCE code_verifier validated against code_challenge (SHA-256 hash)
- Authorization codes are single-use with 5-minute TTL
- All tokens encrypted at rest

### Exchange Credentials Endpoint (DEPRECATED)

**⚠️ DEPRECATED:** This endpoint is deprecated in favor of the standards-compliant `/token` endpoint above. It is kept for backwards compatibility but may be removed in a future major version.

```
GET /api/v1/projects/{project}/extensions/{extension}/oauth/exchange?exchange_token=...
```

**Query Parameters:**
- `exchange_token` (required): Temporary exchange token from callback

**Response:**
```json
{
  "access_token": "eyJhbGc...",
  "token_type": "Bearer",
  "expires_at": "2025-12-19T12:00:00Z",  // Note: timestamp, not expires_in
  "refresh_token": "eyJhbGc..." // optional
}
```

**Error Responses:**
- `400 Bad Request`: Missing or invalid exchange token
- `404 Not Found`: Token expired, already used, or never existed

**Migration Guide:** Replace GET `/oauth/exchange` with POST `/oauth/token` using `grant_type=authorization_code` and client credentials.
