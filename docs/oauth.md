# OAuth Extension

Rise's Generic OAuth 2.0 extension enables end-user authentication with any OAuth provider (Snowflake, Google, GitHub, custom SSO) without managing client secrets locally.

## Overview

**Key Features:**

- **Generic Provider Support**: Works with any OAuth 2.0 compliant provider
- **Dual Flow Support**: Fragment-based (SPAs) and exchange token (backend apps)
- **Secure Token Storage**: Encrypted user tokens with automatic refresh
- **Session Management**: Browser cookie-based session tracking
- **No Client Secret Exposure**: Secrets stored as encrypted environment variables on Rise

**Security Model:**

- Client secrets never leave Rise backend
- User tokens encrypted at rest in database
- OAuth state tokens prevent CSRF attacks
- Exchange tokens single-use with 5-minute TTL
- Session-based token caching per user

## OAuth Flows

Rise supports two OAuth flows to accommodate different application architectures:

### Fragment Flow (Default - For SPAs)

Best for single-page applications (React, Vue, Angular) where tokens are handled in JavaScript.

**Security:** Tokens delivered in URL fragment (`#`) which is never sent to server - no server logs, no Referer leakage.

```
┌──────────────┐                                           ┌──────────────┐
│              │  1. GET /oauth/authorize?flow=fragment    │              │
│   Browser    │──────────────────────────────────────────>│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
                                                                   │
                  2. Compute redirect_uri:                        │
                     https://api.{domain}/oauth/callback/         │
                       {project}/{extension}                      │
                                                                   │
                  3. Generate state token (CSRF)                  │
                     Store in cache (10-min TTL):                 │
                     { redirect_uri, session_id, flow: Fragment } │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│              │  4. Redirect to OAuth Provider            │              │
│   Browser    │<──────────────────────────────────────────│     Rise     │
│              │     with state token                      │   Backend    │
└──────────────┘                                           └──────────────┘
       │
       │  5. User authenticates
       │
       v
┌──────────────┐
│    OAuth     │
│   Provider   │
└──────────────┘
       │
       │  6. Redirect to callback URL
       │     with code + state
       v
┌──────────────┐                                           ┌──────────────┐
│              │  7. GET /oauth/callback?code=...&state=...│              │
│   Browser    │──────────────────────────────────────────>│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
                                                                   │
                  8. Validate state (CSRF check)                  │
                     Exchange code for tokens                     │
                     Encrypt + store in database                  │
                                                                   │
┌──────────────┐                                           ┌──────────────┐
│              │  9. Redirect to app with tokens in        │              │
│   Browser    │<─────fragment (#access_token=...)─────────│     Rise     │
│              │                                           │   Backend    │
└──────────────┘                                           └──────────────┘
       │
       │  10. JavaScript extracts tokens from URL fragment
       │      Store in memory/localStorage
       v
```

**Usage Example:**

```javascript
// Initiate OAuth login
function login() {
  const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize';
  window.location.href = authUrl;  // flow=fragment is default
}

// Extract tokens after redirect
function extractTokens() {
  const fragment = window.location.hash.substring(1);
  const params = new URLSearchParams(fragment);

  const tokens = {
    accessToken: params.get('access_token'),
    tokenType: params.get('token_type'),
    expiresAt: params.get('expires_at'),
    idToken: params.get('id_token'),
    refreshToken: params.get('refresh_token')
  };

  // Store and use tokens
  localStorage.setItem('oauth_tokens', JSON.stringify(tokens));

  // Clear fragment from URL
  window.history.replaceState(null, '', window.location.pathname);

  return tokens;
}
```

### Exchange Flow (For Backend Apps)

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

**Usage Example:**

```ruby
# Rails controller
class OAuthController < ApplicationController
  def callback
    # Extract exchange token from query params
    exchange_token = params[:exchange_token]

    # Exchange for real tokens
    response = HTTParty.get(
      "https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/exchange",
      query: { exchange_token: exchange_token }
    )

    credentials = JSON.parse(response.body)

    # Store in session (HttpOnly cookie)
    session[:oauth_access_token] = credentials['access_token']
    session[:oauth_expires_at] = credentials['expires_at']

    # Redirect to app
    redirect_to root_path
  end
end
```

```javascript
// Express/Node.js
app.get('/oauth/callback', async (req, res) => {
  const { exchange_token } = req.query;

  // Exchange for real tokens
  const response = await fetch(
    `https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/exchange?exchange_token=${exchange_token}`
  );

  const credentials = await response.json();

  // Store in session
  req.session.oauthAccessToken = credentials.access_token;
  req.session.oauthExpiresAt = credentials.expires_at;

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
    "scopes": ["session:role:ANALYST", "refresh_token"]
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

**Fragment Flow:**

```javascript
const localCallbackUrl = 'http://localhost:3000/oauth/callback';
const authUrl = `https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize?redirect_uri=${encodeURIComponent(localCallbackUrl)}`;
window.location.href = authUrl;
```

**Exchange Flow:**

```ruby
# Initiate OAuth
def login
  redirect_uri = "http://localhost:3000/oauth/callback"
  auth_url = "https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-google/oauth/authorize?flow=exchange&redirect_uri=#{CGI.escape(redirect_uri)}"
  redirect_to auth_url
end
```

**Redirect URI Validation:**

Rise only allows redirects to:
- Localhost URLs (any port) - for local development
- Project domain (e.g., `https://my-app.rise.dev`) - for production

## Security Considerations

### Fragment vs Query Parameters

**Why fragments for default flow?**

URL fragments (`#token=...`) offer superior security over query parameters (`?token=...`):

| Security Aspect | Fragment (`#`) | Query Parameter (`?`) |
|----------------|----------------|----------------------|
| Server logs | ✅ Never logged | ❌ Appears in logs |
| Referer header | ✅ Not sent | ❌ Leaked to third parties |
| Browser history | ✅ Not saved | ❌ Saved in history |
| Server access | ✅ JavaScript-only | ❌ Backend can read |

**Example vulnerability with query params:**

```
User clicks external link from app at:
https://my-app.rise.dev/dashboard?access_token=secret123

Browser sends Referer header to external site:
Referer: https://my-app.rise.dev/dashboard?access_token=secret123
                                           ^^^^^^^^^^^^^^^^^^^^^^^^^
                                           Token leaked!
```

With fragments, only the path is leaked:
```
https://my-app.rise.dev/dashboard#access_token=secret123

Browser sends:
Referer: https://my-app.rise.dev/dashboard
         (fragment never included)
```

### Exchange Token Flow Security

**Why exchange tokens for backend apps?**

Exchange tokens provide additional security for server-rendered applications:

1. **Short-lived**: 5-minute TTL reduces exposure window
2. **Single-use**: Invalidated immediately after exchange
3. **Backend-only**: Real tokens never touch browser
4. **HttpOnly cookies**: Can store tokens in cookies inaccessible to JavaScript (XSS protection)

**Best Practice:** Use exchange flow for server-rendered apps, fragment flow for SPAs.

### Token Storage

**Rise Platform:**
- Client secrets: Encrypted environment variables (AES-GCM or AWS KMS)
- User tokens: Encrypted at rest in PostgreSQL
- OAuth state: In-memory cache (10-minute TTL)
- Exchange tokens: In-memory cache (5-minute TTL)

**Application:**
- **SPAs (Fragment Flow)**:
  - Memory (best security, lost on refresh)
  - localStorage (persistent, vulnerable to XSS)
  - Never use cookies (sent with all requests, CSRF risk)

- **Backend Apps (Exchange Flow)**:
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

**Tokens not appearing in URL fragment:**
- Check browser console for JavaScript errors
- Verify redirect URI is correct
- Ensure OAuth provider redirecting to Rise callback URL

**"Could not find user OAuth token"**
- Session cookie missing or expired
- User has not completed OAuth flow
- Token may have been cleaned up due to inactivity

## API Reference

### Authorization Endpoint

**Fragment Flow (SPA):**

```
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?flow=fragment
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?redirect_uri=http://localhost:3000/callback
```

**Exchange Flow (Backend):**

```
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?flow=exchange
GET /api/v1/projects/{project}/extensions/{extension}/oauth/authorize?flow=exchange&redirect_uri=http://localhost:3000/callback
```

**Query Parameters:**
- `flow` (optional): `fragment` (default) or `exchange`
- `redirect_uri` (optional): Where to redirect after OAuth (localhost or project domain)
- `state` (optional): Application state passed through OAuth flow

### Callback Endpoint

```
GET /api/v1/oauth/callback/{project}/{extension}?code=...&state=...
```

**Fragment Flow Response:**
```
HTTP/1.1 302 Found
Location: https://my-app.rise.dev/callback#access_token=...&token_type=Bearer&expires_at=...&id_token=...
```

**Exchange Flow Response:**
```
HTTP/1.1 302 Found
Location: https://my-app.rise.dev/callback?exchange_token=abc123...
```

### Exchange Credentials Endpoint

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
  "expires_at": "2025-12-19T12:00:00Z",
  "refresh_token": "eyJhbGc..." // optional
}
```

**Error Responses:**
- `400 Bad Request`: Missing or invalid exchange token
- `404 Not Found`: Token expired, already used, or never existed
