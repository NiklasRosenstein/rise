# Snowflake OAuth Integration

Rise can act as an OAuth proxy for Snowflake, handling user authentication and injecting access tokens into requests to your app via the `X-Snowflake-Token` header.

## Overview

When enabled for a project, Rise:
1. Redirects unauthenticated users to Snowflake OAuth login
2. Stores encrypted tokens in the database
3. Refreshes tokens automatically before expiry
4. Injects `X-Snowflake-Token` header into requests via ingress auth

## Prerequisites

- A Snowflake account with admin access
- Rise backend with encryption configured (required for token storage)

## Setup

### 1. Create Snowflake Security Integration

In Snowflake (requires ACCOUNTADMIN role):

```sql
CREATE SECURITY INTEGRATION rise_oauth
  TYPE = OAUTH
  ENABLED = TRUE
  OAUTH_CLIENT = CUSTOM
  OAUTH_CLIENT_TYPE = 'CONFIDENTIAL'
  OAUTH_REDIRECT_URI = 'https://your-rise-domain/.rise/oauth/snowflake/callback'
  OAUTH_ISSUE_REFRESH_TOKENS = TRUE
  OAUTH_REFRESH_TOKEN_VALIDITY = 86400;
```

Get the client credentials:

```sql
SELECT SYSTEM$SHOW_OAUTH_CLIENT_SECRETS('RISE_OAUTH');
```

### 2. Configure Rise Backend

Add to your Rise config (e.g., `config/local.yaml` or via environment variables):

```yaml
# Encryption is required for Snowflake token storage
encryption:
  type: "aes-gcm-256"
  key: "${RISE_ENCRYPTION_KEY}"  # Generate with: openssl rand -base64 32

# Snowflake OAuth configuration
snowflake:
  account: "xy12345.eu-west-1"  # Your Snowflake account identifier
  client_id: "${SNOWFLAKE_CLIENT_ID}"
  client_secret: "${SNOWFLAKE_CLIENT_SECRET}"
  redirect_uri: "https://your-rise-domain/.rise/oauth/snowflake/callback"
  scopes: "session:role:PUBLIC"  # Optional, see Scopes section
```

### 3. Enable for Projects

```bash
rise project create my-app --snowflake-enabled
# or
rise project update my-app --snowflake-enabled
```

## OAuth Scopes

The `scopes` configuration determines which Snowflake role the OAuth token can use:

| Scope | Description |
|-------|-------------|
| `session:role:PUBLIC` | Use the PUBLIC role (default) |
| `session:role:ANALYST` | Use a specific role named ANALYST |
| `session:role-any` | Use any role the user has access to |

**Note on Secondary Roles**: If you configure `session:role:PUBLIC` (or another primary role), users can still access secondary roles in Snowflake. The scope only sets the primary role for the session. This is usually sufficient for most use cases.

**Future Enhancement**: Per-project role configuration may be added if needed. For now, configure the platform-level scope to match your most common use case.

## How It Works

### Authentication Flow

1. User accesses an app with `snowflake_enabled=true`
2. Nginx ingress calls Rise's `/auth/ingress` endpoint
3. If no Snowflake session exists, user is prompted to authenticate
4. User visits `/.rise/oauth/snowflake/start?project=<name>&redirect=<url>`
5. Rise redirects to Snowflake OAuth login
6. User authenticates with Snowflake
7. Snowflake redirects back to Rise with authorization code
8. Rise exchanges code for tokens, encrypts and stores them
9. Rise sets `_rise_snowflake_session` cookie
10. User is redirected to original destination

### Token Injection

On subsequent requests:
1. Nginx calls `/auth/ingress?project=<name>`
2. Rise validates the Rise session (`_rise_ingress` cookie)
3. Rise looks up Snowflake token for this session + project
4. If token is expiring soon, Rise refreshes it automatically
5. Rise decrypts the access token
6. Rise returns `X-Snowflake-Token` header to Nginx
7. Nginx passes header to the app

### Background Refresh

The Snowflake Refresh Controller runs in the background and proactively refreshes tokens expiring within 10 minutes. This prevents users from experiencing authentication interruptions.

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /.rise/oauth/snowflake/start` | Start OAuth flow (requires `project` and `redirect` params) |
| `GET /.rise/oauth/snowflake/callback` | OAuth callback from Snowflake |
| `GET /.rise/oauth/snowflake/me` | Get current Snowflake session info |
| `POST /.rise/oauth/snowflake/logout` | Clear Snowflake session |

## Using the Token in Your App

Your app receives the Snowflake access token in the `X-Snowflake-Token` header. Use it to authenticate with Snowflake APIs:

```python
import requests

def get_snowflake_data(request):
    token = request.headers.get('X-Snowflake-Token')
    if not token:
        return {"error": "No Snowflake token"}, 401

    # Use token with Snowflake SQL API, Snowpark, etc.
    response = requests.post(
        "https://your-account.snowflakecomputing.com/api/v2/statements",
        headers={"Authorization": f"Bearer {token}"},
        json={"statement": "SELECT CURRENT_USER()"}
    )
    return response.json()
```

## Security Considerations

- **Token Storage**: Tokens are encrypted at rest using AES-256-GCM
- **Transport**: Always use HTTPS in production
- **Cookie Security**: Session cookies are HttpOnly and Secure (in production)
- **Credential Management**: Store OAuth client credentials in environment variables or secrets manager, never in code

## Troubleshooting

### "The requested scope is invalid"

The scope configured in Rise doesn't match what's allowed in Snowflake. Try:
- `session:role:PUBLIC` for the PUBLIC role
- `session:role:YOUR_ROLE` for a specific role you have access to
- Check `DESCRIBE SECURITY INTEGRATION rise_oauth;` in Snowflake

### "Redirect URI mismatch"

The `redirect_uri` in Rise config must exactly match `OAUTH_REDIRECT_URI` in Snowflake:
```
http://localhost:3000/.rise/oauth/snowflake/callback  # Local dev
https://rise.example.com/.rise/oauth/snowflake/callback  # Production
```

### "Encryption provider not configured"

Snowflake OAuth requires encryption for token storage. Add encryption config:
```yaml
encryption:
  type: "aes-gcm-256"
  key: "${RISE_ENCRYPTION_KEY}"
```

Generate a key: `openssl rand -base64 32`
