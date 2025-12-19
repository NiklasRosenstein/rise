# OAuth Exchange Flow Example

This example demonstrates the **exchange token flow** for server-rendered applications. The backend exchanges a temporary token for real OAuth credentials, enabling secure server-side token storage with HttpOnly cookies.

## Prerequisites

1. **Rise backend running** at `http://localhost:3000`
2. **Dex running** at `http://localhost:5556` (via `docker-compose up`)
3. **OAuth extension created** in Rise
4. **Node.js** installed (v14 or higher)

## Setup

### 1. Create the OAuth Extension

Same setup as the fragment flow example:

```bash
# Create a project (if you haven't already)
rise project create oauth-demo --visibility public

# Store the Dex client secret
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

1. **User clicks "Login"**: Server redirects to Rise OAuth endpoint with `flow=exchange`
2. **Rise redirects to Dex**: User authenticates (username: `admin@example.com`, password: `password`)
3. **Dex redirects to Rise callback**: With authorization code
4. **Rise exchanges code for tokens**: Calls Dex token endpoint, stores tokens in database
5. **Rise redirects to app callback**: With temporary exchange token in query param (`?exchange_token=...`)
6. **App backend exchanges token**: Calls Rise `/oauth/exchange` endpoint
7. **Rise returns real credentials**: Access token, refresh token, etc.
8. **App stores in session**: HttpOnly cookie (XSS-safe)

## Security Features

- **Exchange token**: Single-use, 5-minute TTL
- **HttpOnly cookies**: Tokens inaccessible to JavaScript (XSS protection)
- **Server-side storage**: Real credentials never exposed to browser
- **CSRF protection**: State parameter validated
- **No token leakage**: Tokens never in URL, browser history, or server logs

## Deploying to Rise

To deploy this example to Rise:

```bash
cd example/oauth-exchange-flow

# Deploy the application
rise deployment create oauth-demo

# Note: Update the server.js CONFIG to use environment variables:
# - RISE_API_URL: Your Rise API domain
# - PROJECT_NAME: oauth-demo
# - EXTENSION_NAME: oauth-dex
```

When deployed, the OAuth flow will redirect to your deployed app's domain.

## Environment Variables

Configure the application using environment variables:

- `PORT`: Server port (default: `8080`)
- `RISE_API_URL`: Rise API URL (default: `http://localhost:3000`)
- `PROJECT_NAME`: Rise project name (default: `oauth-demo`)
- `EXTENSION_NAME`: OAuth extension name (default: `oauth-dex`)
- `SESSION_SECRET`: Secret for session encryption (change in production!)

## Testing

1. Visit `http://localhost:8080`
2. Click "Login with OAuth"
3. Login to Dex with:
   - Email: `admin@example.com`
   - Password: `password`
4. You'll be redirected back with your session authenticated
5. Click "Test Protected API" to verify the tokens work

## Comparing with Fragment Flow

| Aspect | Fragment Flow | Exchange Flow |
|--------|---------------|---------------|
| Best for | SPAs (React, Vue, Angular) | Server-rendered apps (Rails, Django, Express) |
| Token delivery | URL fragment (`#access_token=...`) | Temporary exchange token → backend exchange |
| Token storage | localStorage, sessionStorage | HttpOnly cookies, backend session |
| XSS protection | Vulnerable if stored in localStorage | Protected with HttpOnly cookies |
| Server logs | Tokens never in logs | Tokens never in logs |
| Complexity | Simple | Slightly more complex |

## Troubleshooting

**"Extension not found"**: Create the OAuth extension first

**"Exchange token expired"**: Exchange token has 5-minute TTL, restart flow

**"Session not persisting"**: Check that cookies are enabled in browser

**"CORS errors"**: Ensure `RISE_API_URL` matches the actual API URL

## Default Dex Credentials

- **Email**: `admin@example.com`
- **Password**: `password`
