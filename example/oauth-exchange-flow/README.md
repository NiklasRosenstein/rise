# OAuth Token Endpoint Flow Example

This example demonstrates the **RFC 6749-compliant token endpoint flow** for server-rendered applications. The backend exchanges an authorization code for OAuth credentials using the `/oidc/{project}/{extension}/token` endpoint, enabling secure server-side token storage with HttpOnly cookies.

## Prerequisites

1. **Rise backend running** at `http://localhost:3000`
2. **Dex running** at `http://localhost:5556` (via `docker-compose up`)
3. **OAuth extension created** in Rise
4. **Node.js** installed (v14 or higher)

## Setup

### 1. Create the OAuth Extension

```bash
# Create a project (if you haven't already)
rise project create oauth-demo --visibility public

# Encrypt the Dex client secret
ENCRYPTED=$(rise encrypt "rise-backend-secret")

# Create the OAuth extension with encrypted secret
rise extension create oauth-dex -p oauth-demo \
  --type oauth \
  --spec '{
    "provider_name": "Dex (Local Dev)",
    "description": "Local Dex OIDC provider for development",
    "client_id": "rise-backend",
    "client_secret_encrypted": "'"$ENCRYPTED"'",
    "authorization_endpoint": "http://localhost:5556/dex/auth",
    "token_endpoint": "http://localhost:5556/dex/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

The extension will automatically generate Rise client credentials (`OAUTH_DEX_CLIENT_ID` and `OAUTH_DEX_CLIENT_SECRET`) that your backend uses to exchange authorization codes for tokens.

### 2. Update Dex Configuration

Add the OAuth callback URL to Dex's redirect URIs in `dev/dex/config.yaml`:

```yaml
staticClients:
- id: rise-backend
  redirectURIs:
  # ... existing URIs ...
  # OAuth extension callback
  - http://localhost:3000/oidc/oauth-demo/oauth-dex/callback
  name: 'Rise Backend'
  secret: rise-backend-secret
```

Restart Dex:

```bash
docker-compose restart dex
```

### 3. Install Dependencies and Run

```bash
cd example/oauth-exchange-flow

# Install dependencies
npm install

# Run the application
npm start
```

The app will be available at `http://localhost:8080`.

## How It Works

1. **User clicks "Login"**: Server redirects to Rise OAuth authorization endpoint (`/oidc/{project}/{extension}/authorize`)
2. **Rise redirects to Dex**: User authenticates (username: `admin@example.com`, password: `password`)
3. **Dex redirects to Rise callback**: With authorization code (`/oidc/{project}/{extension}/callback`)
4. **Rise exchanges code for tokens**: Calls Dex token endpoint
5. **Rise redirects to app callback**: With authorization code in query param (`?code=...`)
6. **App backend exchanges code**: Calls Rise token endpoint (`/oidc/{project}/{extension}/token`) with client credentials
7. **Rise returns OAuth tokens**: Access token, refresh token, etc.
8. **App stores in session**: HttpOnly cookie (XSS-safe)

## Security Features

- **Authorization code**: Single-use, 5-minute TTL
- **Client authentication**: Backend proves identity with client_secret
- **HttpOnly cookies**: Tokens inaccessible to JavaScript (XSS protection)
- **Server-side storage**: OAuth tokens never exposed to browser
- **CSRF protection**: State parameter validated
- **No token leakage**: Tokens never in URL, browser history, or server logs

## Deploying to Rise

To deploy this example to Rise:

```bash
cd example/oauth-exchange-flow

# Deploy the application
rise deployment create oauth-demo
```

When deployed:
- The app listens on port 8080 (Rise's default)
- OAuth callbacks will redirect to your deployed app's domain
- Rise automatically injects these environment variables:
  - `RISE_PUBLIC_URL`: Your Rise server URL (for both browser redirects and API calls)
  - `OAUTH_DEX_CLIENT_ID`: Rise client ID for token exchange
  - `OAUTH_DEX_CLIENT_SECRET`: Rise client secret for token exchange
- Set these environment variables manually:
  - `PROJECT_NAME`: `oauth-demo` (or your project name)
  - `EXTENSION_NAME`: `oauth-dex` (or your extension name)
  - `SESSION_SECRET`: Generate with `openssl rand -base64 32`

## Environment Variables

Configure the application using environment variables:

- `PORT`: Server port (default: `8080`)
- `RISE_PUBLIC_URL`: Rise server URL (default: `http://localhost:3000`)
- `PROJECT_NAME`: Rise project name (default: `oauth-demo`)
- `EXTENSION_NAME`: OAuth extension name (default: `oauth-dex`)
- `SESSION_SECRET`: Secret for session encryption (change in production!)
- `OAUTH_DEX_CLIENT_ID`: Rise client ID (auto-injected when deployed)
- `OAUTH_DEX_CLIENT_SECRET`: Rise client secret (auto-injected when deployed)

## Testing

1. Visit `http://localhost:8080`
2. Click "Login with OAuth"
3. Login to Dex with:
   - Email: `admin@example.com`
   - Password: `password`
4. You'll be redirected back with your session authenticated
5. Click "Test Protected API" to verify the tokens work

## Comparing OAuth Flows

| Aspect | PKCE Flow (SPAs) | Token Endpoint Flow (Backend) |
|--------|------------------|-------------------------------|
| Best for | React, Vue, Angular | Rails, Django, Express |
| Client type | Public client | Confidential client |
| Authentication | PKCE code_verifier | client_secret |
| Token delivery | Redirect with `code` → client exchanges | Redirect with `code` → backend exchanges |
| Token storage | localStorage, sessionStorage | HttpOnly cookies, backend session |
| XSS protection | Vulnerable if in localStorage | Protected with HttpOnly cookies |
| Complexity | Simple | Slightly more complex |

## Troubleshooting

**"Extension not found"**: Create the OAuth extension first

**"Invalid or expired authorization code"**: Authorization code has 5-minute TTL, restart flow

**"Session not persisting"**: Check that cookies are enabled in browser

**"CORS errors"**: Ensure `RISE_API_URL` matches the actual API URL

**"invalid_client error"**: Verify client_id and client_secret are correct

## Default Dex Credentials

- **Email**: `admin@example.com`
- **Password**: `password`
