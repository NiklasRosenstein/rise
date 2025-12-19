# OAuth Fragment Flow Example

This example demonstrates the **fragment-based OAuth flow** for single-page applications (SPAs). Tokens are delivered in the URL fragment (`#access_token=...`) which provides better security as fragments are never sent to the server.

## Prerequisites

1. **Rise backend running** at `http://localhost:3000`
2. **Dex running** at `http://localhost:5556` (via `docker-compose up`)
3. **OAuth extension created** in Rise

## Setup

### 1. Create the OAuth Extension

First, create an OAuth extension that points to the local Dex instance:

```bash
# Create a project (if you haven't already)
rise project create oauth-demo --visibility public

# The OAuth extension needs a client secret stored as an environment variable
# For local Dex, the secret is "rise-backend-secret"
rise env set oauth-demo DEX_CLIENT_SECRET "rise-backend-secret" --secret

# Create the OAuth extension
rise extension create oauth-demo oauth-dex \
  --type oauth \
  --spec '{
    "provider_name": "Dex (Local Dev)",
    "description": "Local Dex OIDC provider for development",
    "client_id": "rise-backend",
    "client_secret_ref": "DEX_CLIENT_SECRET",
    "authorization_endpoint": "http://localhost:5556/dex/auth",
    "token_endpoint": "http://localhost:5556/dex/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

### 2. Update Dex Configuration

Add the OAuth callback URL to Dex's redirect URIs in `dev/dex/config.yaml`:

```yaml
staticClients:
- id: rise-backend
  redirectURIs:
  # ... existing URIs ...
  # OAuth extension callback
  - http://localhost:3000/api/v1/oauth/callback/oauth-demo/oauth-dex
  name: 'Rise Backend'
  secret: rise-backend-secret
```

Restart Dex after updating:

```bash
docker-compose restart dex
```

### 3. Deploy the Example

```bash
cd example/oauth-fragment-flow
rise deployment create oauth-demo
```

The app will be accessible at `https://oauth-demo.{your-domain}`.

**Note**: The app listens on port 8080 (Rise's default), as configured in `nginx.conf`.

## How It Works

1. **User clicks "Login"**: JavaScript redirects to Rise OAuth authorization endpoint
2. **Rise redirects to Dex**: User authenticates with Dex (username: `admin@example.com`, password: `password`)
3. **Dex redirects to Rise callback**: With authorization code
4. **Rise exchanges code for tokens**: Calls Dex token endpoint
5. **Rise redirects back to app**: With tokens in URL fragment (`#access_token=...`)
6. **JavaScript extracts tokens**: From fragment and stores in localStorage

## Security Features

- **Tokens in fragment**: Never sent to server - no server logs, no Referer leaks
- **CSRF protection**: State parameter validated before accepting tokens
- **Local storage**: Tokens stored client-side (could use sessionStorage for better security)

## Testing Locally

For local development without deploying:

```bash
# Serve the app locally
cd example/oauth-fragment-flow/public
python3 -m http.server 8000
```

Then update the JavaScript configuration in `index.html`:

```javascript
const CONFIG = {
    riseApiUrl: 'http://localhost:3000',
    projectName: 'oauth-demo',
    extensionName: 'oauth-dex',
    redirectUri: 'http://localhost:8000/'  // Local dev URL
};
```

And add `http://localhost:8000/` to Dex's redirect URIs.

## Default Dex Credentials

- **Email**: `admin@example.com`
- **Password**: `password`

## Troubleshooting

**"Extension not found"**: Ensure you created the OAuth extension with the correct name (`oauth-dex`)

**"Client authentication failed"**: Verify the client secret in the environment variable matches Dex configuration

**"Redirect URI mismatch"**: Ensure Dex's `redirectURIs` includes the OAuth callback URL

**Tokens not appearing**: Check browser console for errors, verify the fragment is in the URL
