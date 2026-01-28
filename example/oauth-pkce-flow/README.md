# OAuth PKCE Flow Example

This example demonstrates the **PKCE (Proof Key for Code Exchange) OAuth flow** for single-page applications (SPAs). PKCE provides better security than traditional OAuth flows by preventing authorization code interception attacks, without requiring client secrets.

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

The extension will automatically generate Rise client credentials (`rise_client_id` and `rise_client_secret`) that are stored as environment variables. These are used when your SPA exchanges the authorization code for tokens.

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
cd example/oauth-pkce-flow
rise deployment create oauth-demo
```

The app will be accessible at `https://oauth-demo.{your-domain}`.

**Note**: The app listens on port 8080 (Rise's default), as configured in `nginx.conf`.

## How It Works

### PKCE Flow Steps

1. **SPA generates code verifier**: Random 128-character string (base64url-safe)
2. **SPA generates code challenge**: SHA-256 hash of code verifier, base64url-encoded
3. **User clicks "Login"**: JavaScript redirects to Rise OAuth authorization endpoint with `code_challenge`
4. **Rise redirects to Dex**: User authenticates with Dex (username: `admin@example.com`, password: `password`)
5. **Dex redirects to Rise callback**: With authorization code
6. **Rise exchanges code for tokens**: Calls Dex token endpoint, stores tokens encrypted
7. **Rise redirects back to app**: With authorization code in query string (`?code=...`)
8. **SPA exchanges code for tokens**: Calls Rise `/token` endpoint with `code` and `code_verifier`
9. **Rise validates PKCE**: Verifies code verifier matches stored code challenge
10. **Rise returns tokens**: SPA receives access token, ID token, etc.

### Security Benefits

- **No Client Secret Required**: SPAs can't securely store secrets - PKCE eliminates this need
- **Code Interception Protection**: Even if authorization code is intercepted, attacker can't use it without the code verifier
- **Standards-Compliant**: RFC 7636 PKCE flow works with any compliant OAuth provider
- **Constant-Time Validation**: Rise uses constant-time comparison to prevent timing attacks

## Testing Locally

For local development without deploying:

```bash
# Serve the app locally
cd example/oauth-pkce-flow/public
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

## Default Dex Credentials

- **Email**: `admin@example.com`
- **Password**: `password`

## Troubleshooting

**"Extension not found"**: Ensure you created the OAuth extension with the correct name (`oauth-dex`)

**"Client authentication failed"**: Verify the client secret in the environment variable matches Dex configuration

**"Redirect URI mismatch"**: Ensure Dex's `redirectURIs` includes the OAuth callback URL

**"Invalid code verifier"**: The code verifier stored in sessionStorage doesn't match - restart the flow

**"No authorization code in callback"**: Check browser console for errors, verify Dex redirect is working

## PKCE Implementation Details

This example uses the **S256** code challenge method (SHA-256 hash):

```javascript
// Generate random code verifier (128 characters)
const codeVerifier = generateRandomString(128);

// Hash it with SHA-256 and base64url-encode
const codeChallenge = await sha256Base64Url(codeVerifier);

// Send challenge to authorization endpoint
authUrl.searchParams.set('code_challenge', codeChallenge);
authUrl.searchParams.set('code_challenge_method', 'S256');

// Later, send verifier to token endpoint
tokenRequest.code_verifier = codeVerifier;
```

The `plain` method (no hashing) is also supported but less secure.
